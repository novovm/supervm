//! Native binary reads of the existing verified projection and AOEM state owner.
//! No JSON conversion, new authoritative store, or transport-finality inference.
use super::*;
use novovm_protocol::native_query::{
    Answer, ConsensusProposalStatus, ProposalObservation, Query, QueryError, Response, Status,
};
mod identity;
mod receipt_proofs;
mod state_nodes;
mod uca;
pub(super) use receipt_proofs::ArchiveQueries;
use receipt_proofs::ProofKind;
pub(super) use state_nodes::StateNodeQueries;
pub(super) use uca::{UcaBatchStateRead, UcaCaptureStats, UcaQueries, UcaReadResult};

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
        if let Some(receipt) = self.application_receipts.get(&hash) {
            return receipt.clone();
        }
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

    pub(super) fn consensus_proposal_status(
        &self,
        height: u64,
        round: u64,
        hash: Hash,
        include_progress: bool,
    ) -> Result<Answer> {
        let core = &self.node.controller;
        let current_height = core.context().height;
        let current_round = core.round();
        let matches = |position: Position| {
            position.height == height
                && position.round == round
                && position.parent == core.parent()
                && position.source.is_none()
        };
        let observation = if (height, round) == (current_height, current_round) {
            Some(ProposalObservation {
                in_pending_pool: self.pending.contains_key(&hash),
                selection_gate: self
                    .selection_diagnostic
                    .filter(|s| matches(s.position))
                    .map(|s| s.reason.to_owned()),
                in_local_offer: self
                    .offer
                    .as_ref()
                    .filter(|offer| matches(offer.position))
                    .map(|offer| offer.hashes.contains(&hash)),
                candidate: core.current_local_proposal(hash)?,
            })
        } else {
            None
        };
        let status = ConsensusProposalStatus {
            current_height,
            current_round,
            observation,
        };
        Ok(if include_progress {
            Answer::ConsensusProposalProgress {
                progress: status
                    .observation
                    .as_ref()
                    .map(|_| core.proposal_progress()),
                status,
            }
        } else {
            Answer::ConsensusProposal(status)
        })
    }

    /// At most one request/turn; bounded replies share the original socket.
    /// During recovery only status is observable, never an unverified head's
    /// balance/receipt projection. Balance None means pending, NOT zero/absence.
    pub(super) fn poll_native_query(&mut self, now: Instant) -> Result<()> {
        let _span = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::NativeQuery,
        );
        self.uca_queries.poll(&self.node.pipeline, now);
        self.archive_queries.poll(&self.node, now);
        self.state_node_queries.poll(now);
        let Some((peer, request)) = self.node.controller.take_client_query(now) else {
            return Ok(());
        };
        let answer = self.answer_native_query(&request.query, now)?;
        self.node
            .controller
            .reply_client_query(peer, Response { request, answer }, now)
    }

    #[cfg(feature = "iroh-transport")]
    pub(crate) fn answer_carrier_query(&mut self, query: &Query, now: Instant) -> Result<Answer> {
        match query {
            Query::Status
            | Query::ReceiptProof(_)
            | Query::FinalizedHeader(_)
            | Query::StateNode(_) => self.answer_native_query(query, now),
            Query::Receipt { .. } => {
                // Expose the location of a durable receipt, never the admission or mempool state.
                match self.answer_native_query(query, now)? {
                    answer @ Answer::Receipt {
                        state: ReceiptState::Finalized { .. },
                        ..
                    } => Ok(answer),
                    _ => Ok(Answer::Error(QueryError::Unavailable)),
                }
            }
            _ => Ok(Answer::Error(QueryError::Unavailable)),
        }
    }

    fn answer_native_query(&mut self, query: &Query, now: Instant) -> Result<Answer> {
        Ok(if *query == Query::Status {
            Answer::Status(self.native_status())
        } else if self.node.controller.is_recovering() || self.projection_error.is_some() {
            Answer::Error(QueryError::Unavailable)
        } else {
            match query {
                Query::Status => unreachable!(),
                Query::UcaRegistration { request_hash } => {
                    Answer::UcaRegistration(self.registrations.status(request_hash))
                }
                Query::ConsensusProposal {
                    height,
                    round,
                    hash,
                }
                | Query::ConsensusProposalProgress {
                    height,
                    round,
                    hash,
                } => self
                    .consensus_proposal_status(
                        *height,
                        *round,
                        *hash,
                        matches!(query, Query::ConsensusProposalProgress { .. }),
                    )
                    .unwrap_or(Answer::Error(QueryError::Unavailable)),
                Query::Receipt { hash } if self.application_receipts.contains_key(hash) => {
                    // Binary receipt v1 has asset nonce/fee fields, not NUAR.
                    Answer::Error(QueryError::Unavailable)
                }
                Query::Receipts { hashes }
                    if hashes
                        .iter()
                        .any(|hash| self.application_receipts.contains_key(hash)) =>
                {
                    Answer::Error(QueryError::Unavailable)
                }
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
                Query::ReceiptProof(selector) => {
                    self.archive_queries
                        .answer(&self.node, *selector, ProofKind::AssetReceipt)
                }
                Query::FinalizedHeader(selector) => {
                    self.archive_queries
                        .answer(&self.node, *selector, ProofKind::FinalizedHeader)
                }
                Query::StateNode(selector) => {
                    self.state_node_queries.answer(&self.node, selector, now)
                }
                Query::NovBalance { account } => {
                    let account = Account::try_from(account.as_bytes().to_vec())
                        .map_err(anyhow::Error::msg)?;
                    match self.balance_read(account) {
                        Ok(balance) => Answer::NovBalance(balance),
                        Err(_) => Answer::Error(QueryError::Busy),
                    }
                }
                Query::UcaIdentity {
                    chain_id, genesis, ..
                } => {
                    if *chain_id != self.node.template.chain_id
                        || *genesis != self.node.template.genesis_config_commitment
                        || !self.is_uca()
                    {
                        Answer::Error(QueryError::Unavailable)
                    } else {
                        let parent = self.node.controller.parent();
                        self.uca_queries.read_identity(
                            query.clone(),
                            parent.state_root,
                            parent.height,
                            now,
                            self.node.pipeline.capture_budget(),
                        )
                    }
                }
                Query::UcaApplicationGrant { selector } => {
                    if selector.chain_id != self.node.template.chain_id
                        || selector.genesis != self.node.template.genesis_config_commitment
                        || !self.is_uca()
                    {
                        Answer::Error(QueryError::Unavailable)
                    } else {
                        let parent = self.node.controller.parent();
                        self.uca_queries.read_application(
                            query.clone(),
                            parent.state_root,
                            parent.height,
                            now,
                            self.node.pipeline.capture_budget(),
                        )
                    }
                }
                Query::AssetSpendState { intent, .. } => {
                    let intent_value = intent.intent();
                    if intent_value.chain_id != self.node.template.chain_id
                        || intent_value.genesis != self.node.template.genesis_config_commitment
                        || !self.is_uca()
                    {
                        Answer::Error(QueryError::Unavailable)
                    } else {
                        let parent = self.node.controller.parent();
                        self.uca_queries
                            .read(query.clone(), parent.state_root, parent.height, now)
                    }
                }
            }
        })
    }
}
