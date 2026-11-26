//! One in-flight registration on the existing native input and candidate owners.
use super::*;
use crate::native_pipeline::{
    business::uca_registration,
    pipeline::{PipelineTicket, Submission},
};
use novovm_protocol::uca_registration::SignedUcaRegistrationV1;

pub(super) struct RegistrationIntake {
    raw: Vec<u8>,
    valid_until_height: u64,
    broadcast: bool,
    fanout: Option<(Arc<Message>, Vec<bool>)>,
    parent: Option<ParentPoint>,
    ticket: Option<PipelineTicket>,
    verified: bool,
}

impl RpcLifecycle {
    pub(super) fn receive_registration(&mut self, raw: &[u8], broadcast: bool) -> Result<()> {
        ensure!(
            self.registration_intake.is_none(),
            "registration intake busy"
        );
        let vote = self.node.controller.context();
        ensure!(
            uca_registration::is_profile(&self.node.business_context(vote.height)),
            "native registration rules are not active"
        );
        let signed = SignedUcaRegistrationV1::decode(raw)?;
        ensure!(
            signed.intent.chain_id == vote.chain_id
                && signed.intent.genesis == vote.genesis_config_commitment
                && (signed.intent.valid_from_height..signed.intent.valid_until_height)
                    .contains(&vote.height),
            "registration command not valid for the current chain and height"
        );
        self.registration_intake = Some(RegistrationIntake {
            raw: raw.to_vec(),
            valid_until_height: signed.intent.valid_until_height,
            broadcast,
            fanout: None,
            parent: None,
            ticket: None,
            verified: false,
        });
        Ok(())
    }

    pub(super) fn poll_registration(&mut self) -> Result<bool> {
        let Some(mut work) = self.registration_intake.take() else {
            return Ok(false);
        };
        let vote = self.node.controller.context();
        let parent = self.node.controller.parent();
        if vote.height >= work.valid_until_height {
            return Ok(false);
        }
        if work.parent != Some(parent) {
            // Do not share a state check across parents. Leave an accepted
            // verification to drain before submitting for the next parent.
            if let Some(ticket) = work.ticket.as_mut() {
                if ticket.try_take().is_ok_and(|result| result.is_none()) {
                    self.registration_intake = Some(work);
                    return Ok(false);
                }
                work.ticket = None;
            }
            work.parent = Some(parent);
            work.verified = false;
        }
        if !work.verified {
            if let Some(ticket) = work.ticket.as_mut() {
                match ticket.try_take() {
                    Ok(None) => {
                        self.registration_intake = Some(work);
                        return Ok(false);
                    }
                    Ok(Some(_)) => {
                        work.ticket = None;
                        work.verified = true;
                        self.gossip_verified += 1;
                    }
                    Err(_) => {
                        self.gossip_rejected += 1;
                        return Ok(false);
                    }
                }
            } else {
                let context = self.registration_context(vote.height, parent)?;
                if !self
                    .node
                    .pipeline
                    .business_schedule
                    .as_ref()
                    .is_some_and(|s| s.matches(&context, vote, parent))
                {
                    return Ok(false);
                }
                let body = Message::Body {
                    context,
                    raw_transactions: vec![work.raw.clone()],
                };
                let request = self.node.controller.registration_candidate_request(&body)?;
                match self.node.pipeline.try_submit_background_owned(request) {
                    Ok(Submission::Accepted(ticket)) => work.ticket = Some(ticket),
                    Ok(Submission::Backpressured(_)) => (),
                    Err(_) => {
                        self.gossip_rejected += 1;
                        return Ok(false);
                    }
                }
                self.registration_intake = Some(work);
                return Ok(false);
            }
        }
        if work.broadcast {
            if work.fanout.is_none() {
                let peers = self
                    .node
                    .controller
                    .transactions_available_recipients()
                    .len();
                work.fanout = Some((
                    self.node
                        .controller
                        .transactions_message(vec![work.raw.clone()])?,
                    vec![true; peers],
                ));
            }
            let (message, requested) = work.fanout.as_mut().expect("prepared registration gossip");
            if requested.iter().any(|peer| *peer) {
                if let Some(accepted) = self
                    .node
                    .controller
                    .try_submit_transactions_to(message, requested)?
                {
                    for (wanted, accepted) in requested.iter_mut().zip(accepted) {
                        *wanted &= !accepted;
                    }
                }
            }
            if !requested.iter().any(|peer| *peer) {
                work.broadcast = false;
                work.fanout = None;
            }
        }

        let position = Position {
            height: vote.height,
            parent,
            round: self.node.controller.round(),
            source: None,
        };
        let mut offered = false;
        if self.offered != Some(position) && self.node.controller.is_local_leader()? {
            let message = Arc::new(Message::Body {
                context: self.registration_context(vote.height, parent)?,
                raw_transactions: vec![work.raw.clone()],
            });
            if self.node.controller.offer(&message)? {
                self.offer = Some(LocalOffer {
                    position,
                    message,
                    hashes: Vec::new(),
                });
                self.offered = Some(position);
                offered = true;
            }
        }
        self.registration_intake = Some(work);
        Ok(offered)
    }

    fn registration_context(
        &self,
        height: u64,
        parent: ParentPoint,
    ) -> Result<crate::native_pipeline::execution::plan::BatchContext> {
        let mut context = self.node.business_context(height);
        context.slot = height;
        context.timestamp_unix_ms = context
            .timestamp_unix_ms
            .checked_add(height)
            .context("resident proposed timestamp overflow")?;
        context.parent_height = parent.height;
        context.parent_block_hash = parent.block_hash;
        context.parent_state_root = parent.state_root;
        context.parent_receipt_root = parent.receipt_batch_commitment;
        context.parent_state_version = parent.state_version;
        Ok(context)
    }
}
