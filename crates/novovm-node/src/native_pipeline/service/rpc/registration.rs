//! Bounded registration intake on the existing native input and candidate owners.
use super::*;
use crate::native_pipeline::{
    business::uca_registration,
    pipeline::{PipelineTicket, Submission},
};
use novovm_protocol::{
    native_query::{RegistrationStatusV1, UcaIdentityIdV1},
    uca_registration::SignedUcaRegistrationV1,
};
use sha2::{Digest, Sha256};

pub(super) const MAX_IN_FLIGHT: usize = 16;
const MAX_RECENT_OUTCOMES: usize = 256;
const FANOUT_RETRY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct RegistrationQueue {
    entries: VecDeque<RegistrationIntake>,
    outcomes: BTreeMap<Hash, RegistrationStatusV1>,
    recent: VecDeque<Hash>,
}

struct RegistrationIntake {
    hash: Hash,
    identity: UcaIdentityIdV1,
    raw: Vec<u8>,
    valid_until_height: u64,
    broadcast: bool,
    fanout: Option<(Arc<Message>, Vec<bool>)>,
    last_fanout: Option<Instant>,
    parent: Option<ParentPoint>,
    ticket: Option<PipelineTicket>,
    verified: bool,
}

impl RegistrationQueue {
    pub(super) fn status(&self, hash: &Hash) -> RegistrationStatusV1 {
        if let Some(entry) = self.entries.iter().find(|entry| &entry.hash == hash) {
            return if entry.verified {
                RegistrationStatusV1::Verified
            } else {
                RegistrationStatusV1::Received
            };
        }
        self.outcomes
            .get(hash)
            .copied()
            .unwrap_or(RegistrationStatusV1::NotRecent)
    }

    fn record(&mut self, hash: Hash, status: RegistrationStatusV1) {
        if self.outcomes.insert(hash, status).is_none() {
            self.recent.push_back(hash);
        }
        if self.recent.len() > MAX_RECENT_OUTCOMES {
            if let Some(oldest) = self.recent.pop_front() {
                self.outcomes.remove(&oldest);
            }
        }
    }

    pub(super) fn publish(&mut self, raw: &[u8], point: ParentPoint) -> Result<()> {
        let signed = SignedUcaRegistrationV1::decode(raw)?;
        let hash = Sha256::digest(raw).into();
        for _ in 0..self.entries.len() {
            let entry = self.entries.pop_front().expect("registration entry");
            if entry.identity != signed.intent.identity {
                self.entries.push_back(entry);
            } else if entry.hash != hash {
                self.record(entry.hash, RegistrationStatusV1::IdentityAlreadyRegistered);
            }
        }
        self.record(
            hash,
            RegistrationStatusV1::Finalized {
                height: point.height,
                block_hash: point.block_hash,
            },
        );
        Ok(())
    }
}

impl RpcLifecycle {
    pub(super) fn receive_registration(
        &mut self,
        raw: &[u8],
        broadcast: bool,
        now: Instant,
    ) -> Result<()> {
        let vote = self.node.controller.context();
        ensure!(
            uca_registration::is_profile(&self.node.business_context(vote.height)),
            "native registration rules are not active"
        );
        let signed = SignedUcaRegistrationV1::decode(raw)?;
        ensure!(
            signed.intent.chain_id == vote.chain_id
                && signed.intent.genesis == vote.genesis_config_commitment,
            "registration command not valid for the current chain"
        );
        let hash = Sha256::digest(raw).into();
        let queue = &mut self.registrations;
        if matches!(
            queue.status(&hash),
            RegistrationStatusV1::Finalized { .. }
                | RegistrationStatusV1::IdentityAlreadyRegistered
        ) {
            return Ok(());
        }
        if !(signed.intent.valid_from_height..signed.intent.valid_until_height)
            .contains(&vote.height)
        {
            queue.record(hash, RegistrationStatusV1::OutsideValidityWindow);
            anyhow::bail!("registration command not valid for the current height");
        }
        if let Some(entry) = queue.entries.iter_mut().find(|entry| entry.hash == hash) {
            // A fresh client request can repair fanout lost to peer backpressure.
            entry.broadcast |= broadcast
                && entry.last_fanout.is_none_or(|last| {
                    now.saturating_duration_since(last) >= FANOUT_RETRY_INTERVAL
                });
            return Ok(());
        }
        if queue.entries.len() == MAX_IN_FLIGHT {
            queue.record(hash, RegistrationStatusV1::RetryQueueFull);
            anyhow::bail!("registration queue full; retry the same signed request");
        }
        queue.entries.push_back(RegistrationIntake {
            hash,
            identity: signed.intent.identity,
            raw: raw.to_vec(),
            valid_until_height: signed.intent.valid_until_height,
            broadcast,
            fanout: None,
            last_fanout: None,
            parent: None,
            ticket: None,
            verified: false,
        });
        Ok(())
    }

    pub(super) fn poll_registration(&mut self, now: Instant) -> Result<bool> {
        if self.registrations.entries.is_empty() {
            return Ok(false);
        }
        let vote = self.node.controller.context();
        let parent = self.node.controller.parent();
        let context = self.registration_context(vote.height, parent)?;
        let position = Position {
            height: vote.height,
            parent,
            round: self.node.controller.round(),
            source: None,
        };
        let mut may_offer =
            self.offered != Some(position) && self.node.controller.is_local_leader()?;
        // The original pipeline offers exactly one background permit.
        let mut may_verify = !self
            .registrations
            .entries
            .iter()
            .any(|entry| entry.ticket.is_some());
        let mut offered = false;
        for _ in 0..self.registrations.entries.len() {
            let mut work = self
                .registrations
                .entries
                .pop_front()
                .expect("registration entry");
            if vote.height >= work.valid_until_height {
                self.registrations
                    .record(work.hash, RegistrationStatusV1::OutsideValidityWindow);
                continue;
            }
            if !self.poll_registration_verification(
                &mut work,
                context,
                vote,
                parent,
                &mut may_verify,
            )? {
                self.registrations.entries.push_back(work);
                continue;
            }
            if !work.verified {
                continue;
            }
            if work.broadcast {
                self.poll_registration_fanout(&mut work, now)?;
            }
            if may_offer {
                let message = Arc::new(Message::Body {
                    context,
                    raw_transactions: vec![work.raw.clone()],
                });
                if self.node.controller.offer(&message)? {
                    self.offer = Some(LocalOffer {
                        position,
                        message,
                        hashes: vec![work.hash],
                    });
                    self.offered = Some(position);
                    may_offer = false;
                    offered = true;
                }
            }
            self.registrations.entries.push_back(work);
        }
        Ok(offered)
    }

    /// Return true when verification finished, including an unsuccessful result.
    fn poll_registration_verification(
        &mut self,
        work: &mut RegistrationIntake,
        context: crate::native_pipeline::execution::plan::BatchContext,
        vote: VoteContext,
        parent: ParentPoint,
        may_verify: &mut bool,
    ) -> Result<bool> {
        if let Some(ticket) = work.ticket.as_mut() {
            let result = ticket.try_take();
            if matches!(result, Ok(None)) {
                return Ok(false);
            }
            work.ticket = None;
            if work.parent == Some(parent) {
                if result.is_err() {
                    self.gossip_rejected += 1;
                    self.registrations
                        .record(work.hash, RegistrationStatusV1::RetryVerification);
                    return Ok(true);
                }
                work.verified = true;
                self.gossip_verified += 1;
            }
        }
        if work.parent != Some(parent) {
            work.parent = Some(parent);
            work.verified = false;
        }
        if work.verified {
            return Ok(true);
        }
        if !*may_verify
            || !self
                .node
                .pipeline
                .business_schedule
                .as_ref()
                .is_some_and(|schedule| schedule.matches(&context, vote, parent))
        {
            return Ok(false);
        }
        *may_verify = false;
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
                self.registrations
                    .record(work.hash, RegistrationStatusV1::RetryVerification);
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn poll_registration_fanout(
        &mut self,
        work: &mut RegistrationIntake,
        now: Instant,
    ) -> Result<()> {
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
            work.last_fanout = Some(now);
        }
        Ok(())
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
