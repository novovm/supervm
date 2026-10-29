//! Existing input-gossip protocol on the native lanes. No second mempool,
//! authentication engine or delivery/finality acknowledgement is introduced.
use super::*;
use crate::native_pipeline::consensus::{
    statement::ParentPoint, transport::TransactionsScope, wire::Context as VoteContext,
};
use crate::native_pipeline::ingress::apfl::ApflTransferBatch;
use novovm_protocol::uca_spend_batch::AssetSpendBatchV2;

/// The cheap availability hint retains its existing conservative bounds. Only
/// this exact, prepared-message check grants the final local reservation; the
/// auxiliary/control split can be tighter than the half-budget hint.
fn fitting_recipients(
    mut reserved: Vec<bool>,
    mut fits: impl FnMut(usize) -> bool,
) -> Option<Vec<bool>> {
    for (peer, reserve) in reserved.iter_mut().enumerate() {
        *reserve = *reserve && fits(peer);
    }
    reserved.iter().any(|reserve| *reserve).then_some(reserved)
}

impl NativeLifecycle {
    pub fn take_client_query(
        &mut self,
        now: Instant,
    ) -> Option<(usize, novovm_protocol::native_query::Request)> {
        match self.channel.poll_query(now) {
            Ok(query) => query,
            Err(_) => {
                self.rejected_messages += 1;
                None
            }
        }
    }
    pub fn reply_client_query(
        &mut self,
        peer: usize,
        response: novovm_protocol::native_query::Response,
        now: Instant,
    ) -> Result<()> {
        self.channel.reply_query(peer, response, now)
    }
    /// Untrusted application input, not a validator message or signed vote.
    /// The product's existing admission owner must independently authenticate
    /// each transaction and disseminate accepted input to validator peers.
    pub fn take_client_transactions(&mut self) -> Option<Arc<Message>> {
        self.client_transactions.pop_front()
    }
    pub fn context(&self) -> VoteContext {
        self.progress.context()
    }
    pub fn parent(&self) -> ParentPoint {
        self.progress.parent()
    }
    pub fn head(&self) -> Option<ParentPoint> {
        self.progress.head()
    }
    pub fn round(&self) -> u64 {
        self.progress.round()
    }
    pub fn is_recovering(&self) -> bool {
        self.progress.is_recovering()
    }
    pub fn is_local_leader(&self) -> Result<bool> {
        self.progress.is_local_proposer()
    }

    pub fn apfl_transactions_message(
        &mut self,
        batch: Arc<ApflTransferBatch>,
    ) -> Result<Arc<Message>> {
        Ok(Arc::new(Message::ApflTransactions {
            scope: self.next_transactions_scope()?,
            batch,
        }))
    }

    pub fn uca_transactions_message(
        &mut self,
        batch: Arc<AssetSpendBatchV2>,
    ) -> Result<Arc<Message>> {
        Ok(Arc::new(Message::AssetTransactions {
            scope: self.next_transactions_scope()?,
            batch,
        }))
    }

    fn next_transactions_scope(&mut self) -> Result<TransactionsScope> {
        ensure!(!self.is_frozen(), "native lifecycle frozen");
        self.transaction_sequence = self
            .transaction_sequence
            .checked_add(1)
            .context("input sequence exhausted")?;
        let c = self.context();
        Ok(TransactionsScope {
            chain_id: c.chain_id,
            genesis: c.genesis_config_commitment,
            protocol: c.protocol_commitment,
            epoch: c.epoch,
            validator_set_hash: c.validator_set_hash,
            session: self.transaction_session,
            sequence: self.transaction_sequence,
        })
    }

    pub fn transactions_available_recipients(&self) -> Vec<bool> {
        self.deliveries
            .iter()
            .enumerate()
            .map(|(peer, queue)| {
                !self.is_frozen() && queue.iter().filter(|d| d.prepared.lane() == 2).count() < 2
                // Reserve delivery capacity for control and exact-body traffic.
                && queue.len() < self.limits.messages_per_peer.saturating_sub(4)
                && self.bytes[peer] + self.codec.message_bytes <= self.limits.bytes_per_peer / 2
            })
            .collect()
    }

    /// One observation per retained caller offer, not one per failed poll.
    pub(crate) fn profile_transactions_wait(&self, message: &Message, requested: &[bool]) {
        if crate::native_pipeline::profile::asset_enabled() {
            self.profile_transactions_reservation(
                "input_fanout_wait",
                message,
                requested,
                &self.transactions_available_recipients(),
                None,
                None,
            );
        }
    }

    fn profile_transactions_reservation(
        &self,
        event: &'static str,
        message: &Message,
        requested: &[bool],
        available: &[bool],
        reserved: Option<&[bool]>,
        fragment: Option<Hash>,
    ) {
        let Message::AssetTransactions { scope, .. } = message else {
            return;
        };
        crate::native_pipeline::profile::asset_event(event, self.context().height, || {
            let peers: Vec<_> = self
                .deliveries
                .iter()
                .enumerate()
                .map(|(peer, queue)| {
                    let input_count = queue.iter().filter(|d| d.prepared.lane() == 2).count();
                    let is_reserved = reserved.is_some_and(|mask| mask[peer]);
                    let handles: Vec<_> = queue
                        .iter()
                        .map(|delivery| {
                            serde_json::json!({
                                "fragment":delivery.prepared.fragment_id(),
                                "lane":delivery.prepared.lane(),
                                "bytes":delivery.prepared.byte_len(),
                                "admitted":delivery.admitted,"retiring":delivery.retiring,
                                "realtime":delivery.realtime,"archive":delivery.archive,
                                "auxiliary":delivery.auxiliary
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "peer":peer,"validator":self.peers[peer],
                        "requested":requested[peer],"available":available[peer],
                        "reserved":is_reserved,"frozen":self.is_frozen(),
                        "input_slots_full":input_count>=2,
                        "delivery_count_full":queue.len()>=self.limits.messages_per_peer.saturating_sub(4),
                        "delivery_bytes_full":self.bytes[peer]+self.codec.message_bytes>self.limits.bytes_per_peer/2,
                        "exact_fit_omitted":reserved.map(|mask| requested[peer]&&available[peer]&&!mask[peer]),
                        "charged_bytes":self.bytes[peer],"max_message_bytes":self.codec.message_bytes,
                        "peer_byte_limit":self.limits.bytes_per_peer,
                        "control_reserve_messages":self.control_reserve_messages,
                        "control_reserve_bytes":self.control_reserve_bytes,"handles":handles
                    })
                })
                .collect();
            serde_json::json!({"session":scope.session,"sequence":scope.sequence,
                "fragment":fragment,"peers":peers})
        });
    }

    /// Return an exact local reservation mask. Full peers are omitted from this
    /// best-effort fanout; existing RPC hints retain their original signed input.
    /// None transfers no ownership. Completed transport is not wallet admission.
    pub fn try_submit_transactions_to(
        &mut self,
        message: &Arc<Message>,
        requested: &[bool],
    ) -> Result<Option<Vec<bool>>> {
        ensure!(!self.is_frozen(), "native lifecycle frozen");
        ensure!(
            requested.len() == self.peers.len(),
            "native recipient mask mismatch"
        );
        ensure!(
            matches!(
                message.as_ref(),
                Message::Transactions { .. }
                    | Message::ApflTransactions { .. }
                    | Message::AssetTransactions { .. }
            ),
            "expected signed-input dissemination"
        );
        let available = self.transactions_available_recipients();
        let reserved: Vec<_> = requested
            .iter()
            .zip(&available)
            .map(|(r, a)| *r && *a)
            .collect();
        if !reserved.iter().any(|r| *r) {
            return Ok(None);
        }
        let prepared = self.channel.prepare(message)?;
        ensure!(
            prepared.lane() == 2,
            "native input routed outside ingress lane"
        );
        // The signed-control/drain reservation can leave less auxiliary room
        // than the cheap half-budget hint. Backpressure retains the caller's
        // original Arc, not a fatal reservation mismatch. Other independently
        // available peers continue.
        let Some(reserved) = fitting_recipients(reserved, |peer| self.fits(peer, &prepared, false))
        else {
            return Ok(None);
        };
        self.profile_transactions_reservation(
            "input_fanout_reserved",
            message,
            requested,
            &available,
            Some(&reserved),
            Some(prepared.fragment_id()),
        );
        for (peer, reserve) in reserved.iter().enumerate() {
            if *reserve {
                ensure!(
                    self.enqueue(peer, prepared.clone()),
                    "native input reservation lost"
                );
            }
        }
        Ok(Some(reserved))
    }

    /// Fair, bounded handoff to the existing independent authentication owner.
    pub fn take_transactions(&mut self) -> Option<Arc<Message>> {
        for _ in 0..self.transactions.len() {
            let peer = self.transaction_turn;
            self.transaction_turn = (peer + 1) % self.transactions.len();
            if let Some(input) = self.transactions[peer].pop_front() {
                return Some(input);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::consensus::tests::progress_integration::{
        with_delivery_live_body_for_test, with_delivery_peer_lifecycle_for_test,
    };

    #[test]
    #[ignore = "real AOEM lifecycle; per-peer input reservation and height retirement"]
    fn native_input_fanout_omits_only_full_peer_and_survives_height_advance() -> Result<()> {
        with_delivery_live_body_for_test(|node, _, body| {
            let Message::ApflBody { batch, .. } = body.as_ref() else {
                anyhow::bail!("expected original APFL delivery fixture");
            };
            let only_first = [true, false, false];
            for _ in 0..2 {
                let message = node.apfl_transactions_message(batch.clone())?;
                ensure!(
                    node.try_submit_transactions_to(&message, &only_first)?
                        == Some(only_first.to_vec())
                );
            }
            let retained: Vec<_> = node.deliveries[0]
                .iter()
                .map(|delivery| delivery.prepared.fragment_id())
                .collect();
            let charged = node.bytes[0];
            let third = node.apfl_transactions_message(batch.clone())?;
            ensure!(
                node.try_submit_transactions_to(&third, &[true; 3])?
                    == Some(vec![false, true, true]),
                "one full peer prevented independently available fanout"
            );
            ensure!(node.deliveries[0].len() == 2);
            ensure!(node.deliveries[1].len() == 1 && node.deliveries[2].len() == 1);
            node.height += 1;
            node.round += 1;
            node.retire_realtime()?;
            ensure!(node.bytes[0] == charged);
            ensure!(
                node.deliveries[0]
                    .iter()
                    .map(|delivery| delivery.prepared.fragment_id())
                    .eq(retained),
                "input reservation vanished on control-context advance"
            );
            ensure!(node.deliveries[0]
                .iter()
                .all(|delivery| delivery.realtime && !delivery.retiring));
            ensure!(node
                .try_submit_transactions_to(&third, &only_first)?
                .is_none());
            Ok(())
        })
    }

    #[test]
    #[ignore = "real native UDP/DONE releases input reservation; no transaction authorization claim"]
    fn native_input_full_peer_reopens_only_after_original_transport_done() -> Result<()> {
        with_delivery_peer_lifecycle_for_test(|node, remote, _| {
            let only_first = [true, false, false];
            // Transport-only payloads never enter signature or spend admission.
            // The original scope, lane, fragmentation and DONE owner are real.
            let mut messages = Vec::new();
            for value in 1..=3 {
                messages.push(Arc::new(Message::Transactions {
                    scope: node.next_transactions_scope()?,
                    raw_transactions: vec![vec![value; 512]; 3],
                }));
            }
            for message in &messages[..2] {
                ensure!(
                    node.try_submit_transactions_to(message, &only_first)?
                        == Some(only_first.to_vec())
                );
            }
            ensure!(node
                .try_submit_transactions_to(&messages[2], &only_first)?
                .is_none());
            let start = Instant::now();
            node.poll_io(1000, start)?;
            ensure!(node.deliveries[0].iter().any(|delivery| delivery.admitted));
            node.height += 1;
            node.retire_realtime()?;
            ensure!(node.deliveries[0].len() == 2);
            ensure!(node.deliveries[0]
                .iter()
                .all(|delivery| delivery.realtime && !delivery.retiring));
            let deadline = start + Duration::from_secs(3);
            let mut received = BTreeSet::new();
            while received.len() < 2 || !node.deliveries[0].is_empty() {
                ensure!(
                    Instant::now() < deadline,
                    "original input delivery did not complete"
                );
                node.poll_io(1001 + start.elapsed().as_millis() as u64, Instant::now())?;
                let report = remote.channel.poll_io(1001, Instant::now(), 64)?;
                ensure!(report.rejected == 0 && report.backpressured == 0);
                if let Some(event) = remote.channel.poll_receive_lane(2, Instant::now(), 64)? {
                    match event {
                        ReceiveEvent::Message(value) => {
                            ensure!(messages[..2]
                                .iter()
                                .any(|message| **message == value.message));
                            ensure!(
                                received.insert(value.fragment_id),
                                "duplicate application handoff"
                            );
                        }
                        ReceiveEvent::Rejected { error, .. } => anyhow::bail!(error),
                    }
                }
                std::thread::yield_now();
            }
            ensure!(node.bytes[0] == 0 && node.expired_sends == 0);
            ensure!(
                node.try_submit_transactions_to(&messages[2], &only_first)?
                    == Some(only_first.to_vec()),
                "returned DONE did not release the original peer reservation"
            );
            Ok(())
        })
    }

    #[test]
    fn auxiliary_full_is_backpressure_not_a_reservation() {
        let maximum_message = 1024usize;
        let signed_control = maximum_message / 2;
        let control_reserve = 8 * signed_control;
        let budget = 5 * maximum_message;
        let auxiliary_used = maximum_message;
        let prepared_bytes = maximum_message / 4;
        // This is the old cheap preflight: it can legitimately report true
        // while fits()'s reserved-control auxiliary ceiling is already full.
        let available = auxiliary_used + maximum_message <= budget / 2;
        assert!(available);
        let auxiliary_budget = budget - control_reserve;
        let requested = [true];
        let reserved = requested.iter().map(|r| *r && available).collect();
        let mut checked = 0;
        let result = fitting_recipients(reserved, |_| {
            checked += 1;
            prepared_bytes <= auxiliary_budget.saturating_sub(auxiliary_used)
        });
        assert_eq!(
            result, None,
            "full auxiliary budget must retain caller input"
        );
        assert_eq!(checked, 1);
        assert_eq!(
            requested,
            [true],
            "caller request was not consumed or changed"
        );
    }

    #[test]
    fn exact_auxiliary_backpressure_isolated_to_that_recipient() {
        let maximum_message = 1024usize;
        let signed_control = maximum_message / 2;
        let control_reserve = 8 * signed_control;
        let budget = 5 * maximum_message;
        let used = [maximum_message, 0];
        let available: Vec<_> = used
            .iter()
            .map(|bytes| bytes + maximum_message <= budget / 2)
            .collect();
        assert_eq!(available, [true, true]);
        let reserved = fitting_recipients(available, |peer| {
            maximum_message / 4 <= (budget - control_reserve).saturating_sub(used[peer])
        });
        assert_eq!(reserved, Some(vec![false, true]));
        assert_eq!(
            fitting_recipients(vec![false], |_| panic!("unrequested peer checked")),
            None
        );
    }
}
