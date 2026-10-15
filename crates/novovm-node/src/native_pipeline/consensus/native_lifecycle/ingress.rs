//! Existing input-gossip protocol on the native lanes. No second mempool,
//! authentication engine or delivery/finality acknowledgement is introduced.
use super::*;
use crate::native_pipeline::consensus::{
    statement::ParentPoint, transport::TransactionsScope, wire::Context as VoteContext,
};
use crate::native_pipeline::ingress::apfl::ApflTransferBatch;

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
        ensure!(!self.is_frozen(), "native lifecycle frozen");
        self.transaction_sequence = self
            .transaction_sequence
            .checked_add(1)
            .context("input sequence exhausted")?;
        let c = self.context();
        Ok(Arc::new(Message::ApflTransactions {
            scope: TransactionsScope {
                chain_id: c.chain_id,
                genesis: c.genesis_config_commitment,
                protocol: c.protocol_commitment,
                epoch: c.epoch,
                validator_set_hash: c.validator_set_hash,
                session: self.transaction_session,
                sequence: self.transaction_sequence,
            },
            batch,
        }))
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
                Message::Transactions { .. } | Message::ApflTransactions { .. }
            ),
            "expected signed-input dissemination"
        );
        let available = self.transactions_available_recipients();
        let reserved: Vec<_> = requested
            .iter()
            .zip(available)
            .map(|(r, a)| *r && a)
            .collect();
        if !reserved.iter().any(|r| *r) {
            return Ok(None);
        }
        let prepared = self.channel.prepare(message)?;
        ensure!(
            prepared.lane() == 2,
            "native input routed outside ingress lane"
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
