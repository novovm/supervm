//! Native binary reads of the existing verified projection and AOEM state owner.
//! No JSON conversion, new authoritative store, or transport-finality inference.
use super::*;
use novovm_protocol::native_query::{Answer, Query, QueryError, Response, Status};

impl RpcLifecycle {
    fn recent_receipt(&self, hash: Hash) -> ReceiptState {
        self.receipts.get(&hash).copied().unwrap_or_else(|| {
            if self.pending.contains_key(&hash) {
                ReceiptState::Received
            } else {
                ReceiptState::NotRecent
            }
        })
    }

    pub(super) fn receipt_status_json(&self, hash: Hash) -> Value {
        match self.recent_receipt(hash) {
            ReceiptState::Received => self.pending_status(hash),
            ReceiptState::NotRecent => json!({"tx_hash":hex(&hash),"state":"not_in_recent_index",
                "finalized":false,"lookup_complete":false,"indexed_height":self.indexed_height}),
            ReceiptState::RejectedNonce => json!({"tx_hash":hex(&hash),"state":"rejected_nonce",
                "finalized":false,"executed":false}),
            ReceiptState::Finalized {
                success,
                nonce_after,
                charged_fee,
                height,
                block_hash,
            } => {
                json!({"tx_hash":hex(&hash),"state":if success {"finalized_success"} else {"finalized_business_failure"},
                    "executed":true,"success":success,"nonce_after":nonce_after,
                    "charged_fee":charged_fee.to_string(),"block_height":height,"block_hash":hex(&block_hash),
                    "finalized":true,"finality_kind":"BFT_durable","proof_verified":false})
            }
        }
    }

    fn native_status(&self) -> Status {
        let core = &self.node.controller;
        let parent = core.parent();
        Status {
            height: parent.height,
            block_hash: parent.block_hash,
            state_root: parent.state_root,
            receipt_batch_commitment: parent.receipt_batch_commitment,
            state_version: parent.state_version,
            indexed_height: self.indexed_height,
            current_height: core.context().height,
            round: core.round(),
            recovering: core.is_recovering(),
            projection_failed: self.projection_error.is_some(),
            pending: self.pending.len() as u64,
            apfl_rows: core.apfl_execution_rows(),
            integer_rows: core.integer_execution().0,
            integer_plans: core.integer_execution().1,
            rejected_inputs: self.gossip_rejected,
        }
    }

    /// At most one request/turn; bounded replies share the original socket.
    /// During recovery only status is observable, never an unverified head's
    /// balance/receipt projection. Balance None means pending, NOT zero/absence.
    pub(super) fn poll_native_query(&mut self, now: Instant) -> Result<()> {
        let Some((peer, request)) = self.node.controller.take_client_query(now) else {
            return Ok(());
        };
        let answer = if request.query == Query::Status {
            Answer::Status(self.native_status())
        } else if self.node.controller.is_recovering() || self.projection_error.is_some() {
            Answer::Error(QueryError::Unavailable)
        } else {
            match &request.query {
                Query::Status => unreachable!(),
                Query::Receipt { hash } => Answer::Receipt {
                    state: self.recent_receipt(*hash),
                    indexed_height: self.indexed_height,
                },
                Query::Receipts { hashes } => Answer::Receipts {
                    states: hashes
                        .iter()
                        .map(|hash| self.recent_receipt(*hash))
                        .collect::<Vec<_>>()
                        .try_into()
                        .map_err(anyhow::Error::msg)?,
                    indexed_height: self.indexed_height,
                },
                Query::NovBalance { account } => {
                    let account = Account::try_from(account.as_bytes().to_vec())
                        .map_err(anyhow::Error::msg)?;
                    match self.balance_read(account) {
                        Ok(balance) => Answer::NovBalance(balance),
                        Err(_) => Answer::Error(QueryError::Busy),
                    }
                }
            }
        };
        self.node
            .controller
            .reply_client_query(peer, Response { request, answer }, now)
    }
}
