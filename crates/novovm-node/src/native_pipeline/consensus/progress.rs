//! Bounded original-node orchestration of the EXISTING round-bft journal.
//! Source: controller::drive_consensus / poll_journal / controller_recovery at
//! 58e3aade. The safety machine, signing, timers and atomic publication remain
//! in novovm-consensus. No transport, execution loop or second chain head here.
//! Native I/O and AOEM work progress independently; only locally executed,
//! owner-bound durable candidates may authorize a value vote.

use super::collector::{CollectorLimits, VoteCollector};
use super::journal::{CandidateLocator, ReplayEvidence, ReplayRecord, MAX_REPLAY_RECORDS};
use super::message_evidence::{self, MessageDomain, VerifiedEvidence};
use super::pacemaker::{Pacemaker, TimeoutPolicy};
use super::statement::ParentPoint;
use super::transport::Message;
use super::wire::{
    Context as VoteContext, Hash, Phase, ValidatorSet, VerifiedProposal, VerifiedQuorum,
    VerifiedVote,
};
use super::{DurableMessage, TimeoutStep, ValidatorJournal};
use crate::native_pipeline::pipeline::{CandidatePipeline, DurableCandidate};
use anyhow::{ensure, Context, Result};
use novovm_consensus::round_bft::journal::backend::JournalBackend;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Instant,
};

mod recovery;

#[derive(Clone, Copy)]
pub struct ProgressLimits {
    pub candidates: usize,
    pub candidate_bytes: usize,
    /// Total vote custody: original collector plus the next-height ACK window.
    /// Two entries per validator are taken FROM this budget, not added to it.
    pub votes: CollectorLimits,
}

struct Candidate {
    body_id: Hash,
    candidate: DurableCandidate,
    local_round: Option<u64>,
}
// A disposable read-only view of THIS owner's acknowledged publication. Not
// another head, archive, signing capability or replay cache. At most one slot.
struct PublishedCandidate {
    point: ParentPoint,
    candidate: DurableCandidate,
    charge: usize,
}
struct Offered {
    proposal: VerifiedProposal,
    valid: Option<VerifiedQuorum>,
    decision: Option<VerifiedQuorum>,
    body_id: Hash,
}
struct ReplayOffer {
    proposal: VerifiedProposal,
    valid: Option<VerifiedQuorum>,
    locator: CandidateLocator,
    local: bool,
}

/// Extension of the original future-control custody, not a second collector.
/// Each authenticated signer owns one next-height tip per phase; no weight,
/// QC, timer, signing or publication decision is made before journal ACK.
#[derive(Default)]
struct FutureVotes {
    tips: BTreeMap<(Hash, u8), VerifiedVote>,
}

impl FutureVotes {
    fn retain(
        &mut self,
        current: VoteContext,
        max_round_span: u64,
        verified: &VerifiedVote,
    ) -> Result<()> {
        let vote = verified.vote();
        ensure!(
            current.height.checked_add(1) == Some(vote.context.height),
            "future vote is not at the immediately next height"
        );
        ensure!(
            vote.round <= max_round_span,
            "next-height vote exceeds retained-round span"
        );
        // Use the signed validator, never the relaying transport peer. A peer
        // cannot spend another signer's phase slot by relaying duplicates.
        let key = (vote.validator_id, u8::from(vote.phase == Phase::Precommit));
        if let Some(old) = self.tips.get(&key) {
            let old = old.vote();
            ensure!(
                old.context == vote.context,
                "next-height validator signed conflicting contexts"
            );
            ensure!(vote.round >= old.round, "stale next-height vote tip");
            if vote.round == old.round {
                ensure!(
                    old.value == vote.value,
                    "next-height validator equivocation"
                );
                return Ok(());
            }
        }
        self.tips.insert(key, verified.clone());
        Ok(())
    }

    /// Only the existing journal ACK calls this with its new full context.
    /// Removing custody first makes promotion one-shot. The original collector
    /// rechecks every exact-context vote and its local verification owner, with
    /// unchanged quorum rules and without repeating its signature arithmetic.
    fn promote(&mut self, collector: &mut VoteCollector) -> Result<()> {
        for (_, verified) in std::mem::take(&mut self.tips) {
            let vote = verified.vote();
            if vote.context == *collector.context() {
                collector.insert_verified(&verified)?;
                crate::native_pipeline::profile::critical_event(
                    crate::native_pipeline::profile::CriticalEvent::VoteReceived,
                    vote.context.height,
                    vote.round,
                    vote.value,
                    None,
                );
            }
        }
        Ok(())
    }
}

fn collector_limits_with_future_custody(
    total: CollectorLimits,
    validators: usize,
) -> Result<CollectorLimits> {
    let reserved = validators
        .checked_mul(2)
        .context("future vote budget overflow")?;
    let collector = total
        .max_votes
        .checked_sub(reserved)
        .context("vote budget cannot reserve next-height custody")?;
    ensure!(
        collector
            >= validators
                .checked_mul(5)
                .context("collector budget overflow")?,
        "vote budget cannot preserve current/future-round/witness reservations"
    );
    Ok(CollectorLimits {
        max_votes: collector,
        ..total
    })
}

/// A single retained outbox slot applies backpressure before observing another
/// journal ACK. The caller must retain a taken message until transport custody
/// or an explicit retry/error; take_message is NOT delivery acknowledgement.
pub struct RoundProgress {
    journal: ValidatorJournal,
    domain: MessageDomain,
    owner: Arc<()>,
    limits: ProgressLimits,
    collector: VoteCollector,
    pacemaker: Pacemaker,
    candidates: BTreeMap<Hash, Candidate>,
    candidate_bytes: usize,
    published: Option<PublishedCandidate>,
    offers: BTreeMap<(Hash, bool), Offered>,
    // Native object completion is acknowledged once. An authenticated future
    // control must survive a pending local round/height ACK, not rely on
    // repeating a completed transport object. Only current/next height,
    // one proposal and one decision per authenticated peer/height. Votes use
    // the signer's separate bounded phase slots below, never relay-peer slots.
    // These are evidence hints, not another execution or publication owner.
    future_offers: BTreeMap<(u64, Hash, bool), Offered>,
    future_votes: FutureVotes,
    proposed_round: Option<u64>,
    recovery_roots: BTreeMap<Hash, CandidateLocator>,
    recovery_offers: Vec<ReplayOffer>,
    recovery_work: Option<recovery::Work>,
    recovered_bodies: BTreeMap<Hash, Arc<Message>>,
    replay: VecDeque<Message>,
    recovering: bool,
    outbox: Option<Message>,
    frozen: bool,
}

impl RoundProgress {
    pub fn new(
        mut journal: ValidatorJournal,
        pipeline: &CandidatePipeline,
        validators: Arc<ValidatorSet>,
        limits: ProgressLimits,
        timeouts: TimeoutPolicy,
    ) -> Result<Self> {
        ensure!(
            limits.candidates > 0 && limits.candidates <= 64 && limits.candidate_bytes > 0,
            "invalid progress candidate budget"
        );
        ensure!(
            !journal.is_pending(),
            "progress requires completed journal opening"
        );
        // Checks the actual storage/compute owner, without making a transition.
        ensure!(
            journal.poll(pipeline)?.is_none(),
            "unexpected opening transition"
        );
        let context = journal.context();
        context.validate(&validators)?;
        let domain = MessageDomain {
            chain_id: context.chain_id,
            genesis: context.genesis_config_commitment,
            protocol: context.protocol_commitment,
            validators: validators.clone(),
        };
        let vote_limits =
            collector_limits_with_future_custody(limits.votes, validators.members().len())?;
        let records = journal.take_replay_records();
        let mut this = Self {
            collector: VoteCollector::new(context, validators, journal.round(), vote_limits)?,
            pacemaker: Pacemaker::new(timeouts)?,
            journal,
            domain,
            owner: pipeline.owner_identity(),
            limits,
            candidates: BTreeMap::new(),
            candidate_bytes: 0,
            published: None,
            offers: BTreeMap::new(),
            future_offers: BTreeMap::new(),
            future_votes: FutureVotes::default(),
            proposed_round: None,
            recovery_roots: BTreeMap::new(),
            recovery_offers: Vec::new(),
            recovery_work: None,
            recovered_bodies: BTreeMap::new(),
            replay: VecDeque::new(),
            recovering: false,
            outbox: None,
            frozen: false,
        };
        this.initialize_recovery(records)?;
        Ok(this)
    }

    pub fn context(&self) -> VoteContext {
        self.journal.context()
    }
    pub fn parent(&self) -> ParentPoint {
        self.journal.parent()
    }
    pub fn head(&self) -> Option<ParentPoint> {
        self.journal.head()
    }
    /// Available only after this owner's actual Decision ACK. Cold opening,
    /// remote evidence and speculative execution never populate this view.
    /// A miss uses the original checked ArchiveRead path, not relaxed finality.
    pub(crate) fn published_packet(
        &self,
        expected: ParentPoint,
    ) -> Option<&crate::native_pipeline::persistence::PreparedCandidate> {
        if self.is_frozen() || self.recovering || self.journal.head() != Some(expected) {
            return None;
        }
        self.published
            .as_ref()
            .filter(|p| p.point == expected)?
            .candidate
            .bind_to(&self.owner)
            .ok()
    }
    pub fn round(&self) -> u64 {
        self.journal.round()
    }
    pub fn step(&self) -> TimeoutStep {
        self.journal.step()
    }
    pub fn local_validator(&self) -> Hash {
        self.journal.local_validator()
    }
    pub fn is_local_proposer(&self) -> Result<bool> {
        Ok(self
            .domain
            .validators
            .leader(self.context().height, self.round())?
            == self.local_validator())
    }

    /// Assembly-side export of ORIGINAL input, never a remote execution permit.
    /// Also serves replayed candidates whose input was recovered from the same
    /// storage owner. Do not recompute business results to answer a body request.
    pub fn candidate_body(&self, body_id: Hash) -> Option<Arc<Message>> {
        self.recovered_bodies.get(&body_id).cloned()
    }
    pub fn retains_body(&self, body_id: Hash) -> bool {
        self.candidates
            .values()
            .any(|candidate| candidate.body_id == body_id)
    }
    pub fn is_recovering(&self) -> bool {
        self.recovering
    }
    pub fn is_frozen(&self) -> bool {
        self.frozen || self.journal.is_frozen()
    }
    pub fn pending_message(&self) -> Option<&Message> {
        self.outbox.as_ref()
    }
    pub fn take_message(&mut self) -> Option<Message> {
        self.outbox.take()
    }
    pub fn recovery_candidates(&self) -> impl Iterator<Item = &CandidateLocator> {
        self.recovery_roots.values()
    }
    /// Exact unsigned hints, bounded to two per configured validator. Fetching
    /// one grants no vote: the returned body must independently execute.
    pub fn missing_bodies(&self) -> impl Iterator<Item = (Hash, Hash)> + '_ {
        self.offers.iter().filter_map(|((peer, _), offer)| {
            (!self
                .candidates
                .contains_key(&offer.proposal.proposal().value))
            .then_some((*peer, offer.body_id))
        })
    }

    /// Admission borrows the completion: a full local budget leaves the exact
    /// candidate with its caller, without discarding/re-executing accepted work.
    pub fn register_candidate(
        &mut self,
        body_id: Hash,
        candidate: &DurableCandidate,
        locally_selected: bool,
    ) -> Result<bool> {
        let _span = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::CandidateRegistration,
        );
        ensure!(!self.is_frozen(), "progress frozen");
        ensure!(body_id != [0; 32], "empty candidate body identity");
        let statement = CandidatePipeline::checked_statement(
            candidate,
            &self.owner,
            self.context(),
            &self.domain.validators,
            &self.parent(),
        )?;
        let value = statement.hash();
        ensure!(
            !self.recovering
                || self.recovery_roots.contains_key(&value)
                || self.candidates.contains_key(&value),
            "unrelated candidate during cold recovery"
        );
        if let Some(locator) = self.recovery_roots.get(&value) {
            ensure!(
                *locator == CandidatePipeline::candidate_locator(candidate, value),
                "re-executed recovery locator differs"
            );
        }
        if let Some(known) = self.candidates.get_mut(&value) {
            ensure!(known.body_id == body_id, "candidate body identity changed");
            if locally_selected {
                known.local_round = Some(self.journal.round());
            }
            return Ok(true);
        }
        let bytes = candidate.packet().retained_bytes();
        if self.candidates.len() == self.limits.candidates
            || bytes
                > self
                    .limits
                    .candidate_bytes
                    .saturating_sub(self.candidate_bytes)
        {
            return Ok(false);
        }
        // This optional query view may never take capacity from consensus.
        // Account its full packet conservatively even while sharing the Arc.
        if self.published.as_ref().is_some_and(|p| {
            self.candidates.len().saturating_add(2) > self.limits.candidates
                || bytes.checked_add(p.charge).is_none_or(|charge| {
                    charge
                        > self
                            .limits
                            .candidate_bytes
                            .saturating_sub(self.candidate_bytes)
                })
        }) {
            self.published = None;
        }
        self.candidates.insert(
            value,
            Candidate {
                body_id,
                candidate: candidate.clone(),
                local_round: locally_selected.then_some(self.round()),
            },
        );
        self.candidate_bytes += bytes;
        self.recovery_roots.remove(&value);
        crate::native_pipeline::profile::critical_event(
            crate::native_pipeline::profile::CriticalEvent::CandidateRegistered,
            self.context().height,
            self.round(),
            Some(value),
            Some(body_id),
        );
        Ok(true)
    }

    /// Verify before mutation. Peer is the validator identity authenticated by
    /// the existing native session, NOT an identity claimed in the payload.
    /// Invalid peer input is returned as an error, never freezes local signing.
    pub fn receive(&mut self, peer: Hash, message: &Message) -> Result<()> {
        let evidence = message_evidence::verify(message, &self.domain)?;
        self.receive_verified(peer, message, evidence)
    }

    /// NativeChannel already qualified the exact message. Reuse that fact, not
    /// a prior consensus decision: peer, domain, context, round, equivocation,
    /// custody budgets and execution/publication authority remain local checks.
    pub(super) fn receive_verified(
        &mut self,
        peer: Hash,
        message: &Message,
        evidence: VerifiedEvidence,
    ) -> Result<()> {
        ensure!(!self.is_frozen(), "progress frozen");
        ensure!(
            self.domain
                .validators
                .members()
                .iter()
                .any(|v| v.id() == peer),
            "progress source is outside validator set"
        );
        evidence.ensure_matches(message, &self.domain)?;
        match (message, evidence) {
            (Message::Vote(vote), VerifiedEvidence::Vote(verified)) => {
                if vote.context == self.context() {
                    self.collector.insert_verified(&verified)?;
                    crate::native_pipeline::profile::critical_event(
                        crate::native_pipeline::profile::CriticalEvent::VoteReceived,
                        vote.context.height,
                        vote.round,
                        vote.value,
                        None,
                    );
                } else if self.context().height.checked_add(1) == Some(vote.context.height) {
                    self.future_votes.retain(
                        self.context(),
                        self.limits.votes.max_future_round_span,
                        &verified,
                    )?;
                }
            }
            (
                Message::Proposal { body_id, .. },
                VerifiedEvidence::Proposal {
                    proposal,
                    valid_quorum,
                },
            ) => {
                let p = proposal.proposal();
                ensure!(
                    peer == p.proposer_id,
                    "proposal body hint is not from proposer"
                );
                if p.context == self.context() && p.round == self.round() {
                    crate::native_pipeline::profile::critical_event(
                        crate::native_pipeline::profile::CriticalEvent::ProposalReceived,
                        p.context.height,
                        p.round,
                        Some(p.value),
                        Some(*body_id),
                    );
                    self.offers.insert(
                        (peer, false),
                        Offered {
                            proposal,
                            valid: valid_quorum,
                            decision: None,
                            body_id: *body_id,
                        },
                    );
                } else if p.context == self.context() && p.round > self.round() {
                    ensure!(
                        p.round - self.round() <= self.limits.votes.max_future_round_span,
                        "future proposal exceeds retained-round span"
                    );
                    self.retain_future_offer(
                        peer,
                        Offered {
                            proposal,
                            valid: valid_quorum,
                            decision: None,
                            body_id: *body_id,
                        },
                    );
                } else if self.context().height.checked_add(1) == Some(p.context.height) {
                    ensure!(
                        p.round <= self.limits.votes.max_future_round_span,
                        "next-height proposal exceeds retained-round span"
                    );
                    self.retain_future_offer(
                        peer,
                        Offered {
                            proposal,
                            valid: valid_quorum,
                            decision: None,
                            body_id: *body_id,
                        },
                    );
                }
            }
            (
                Message::Decision { body_id, .. },
                VerifiedEvidence::Decision {
                    proposal,
                    certificate,
                },
            ) => {
                if proposal.proposal().context == self.context() {
                    self.offers.insert(
                        (peer, true),
                        Offered {
                            proposal,
                            valid: None,
                            decision: Some(certificate),
                            body_id: *body_id,
                        },
                    );
                } else if self.context().height.checked_add(1)
                    == Some(proposal.proposal().context.height)
                {
                    self.retain_future_offer(
                        peer,
                        Offered {
                            proposal,
                            valid: None,
                            decision: Some(certificate),
                            body_id: *body_id,
                        },
                    );
                }
            }
            _ => {
                anyhow::bail!("progress expects qualified consensus control, not body/network work")
            }
        }
        Ok(())
    }

    fn retain_future_offer(&mut self, peer: Hash, offer: Offered) {
        let proposal = offer.proposal.proposal();
        let key = (proposal.context.height, peer, offer.decision.is_some());
        if self
            .future_offers
            .get(&key)
            .is_none_or(|old| old.proposal.proposal().round <= proposal.round)
        {
            self.future_offers.insert(key, offer);
        }
    }

    // Called only after the ORIGINAL journal acknowledges a local transition.
    // A matching height is insufficient: parent block/decision and the entire
    // domain must match the newly authoritative context before any body demand.
    fn promote_future_offers(&mut self) {
        let context = self.context();
        let round = self.round();
        for (key, offer) in std::mem::take(&mut self.future_offers) {
            let proposal = offer.proposal.proposal();
            if proposal.context == context {
                if offer.decision.is_some() || proposal.round == round {
                    self.offers.insert((key.1, key.2), offer);
                } else if proposal.round > round {
                    self.future_offers.insert(key, offer);
                }
            } else if context.height.checked_add(1) == Some(proposal.context.height) {
                self.future_offers.insert(key, offer);
            }
        }
    }

    /// Never blocks on transport or AOEM completion. A local persistence error
    /// latches until explicit reopening/recovery; it cannot be retried as a new
    /// signer. Peer rejection is intentionally outside this failure boundary.
    pub fn poll(&mut self, pipeline: &CandidatePipeline, now: Instant) -> Result<()> {
        self.poll_acknowledgements(pipeline)?;
        self.poll_after_receive(pipeline, now)
    }

    /// Advance durable ACK/context and recovery before receiving control for
    /// the new context. Native ingress must get its bounded turn before any
    /// failure deadline; receiving after a staged nil vote is too late.
    pub(crate) fn poll_acknowledgements(&mut self, pipeline: &CandidatePipeline) -> Result<()> {
        let _phase = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::Consensus,
        );
        ensure!(!self.is_frozen(), "progress frozen");
        let result = self.poll_inner(pipeline);
        if result.is_err() {
            self.frozen = true;
        }
        result
    }

    /// Process already qualified proposals/QCs before the unchanged local
    /// timer. Outbox/recovery backpressure still prevents another transition.
    pub(crate) fn poll_after_receive(
        &mut self,
        pipeline: &CandidatePipeline,
        now: Instant,
    ) -> Result<()> {
        let _phase = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::Consensus,
        );
        ensure!(!self.is_frozen(), "progress frozen");
        if self.outbox.is_some() || self.recovering {
            return Ok(());
        }
        let result = (|| {
            self.drive_consensus()?;
            self.pacemaker
                .poll(&mut self.journal, &self.collector, now)?;
            if self.journal.is_pending() {
                // Submit the existing transition before the rest of this Host
                // turn. Enqueue is NOT an ACK: only poll_acknowledgements may
                // adopt durable state and expose the original signed outbox.
                // Backpressure retains that same private ticket/transition.
                self.journal.enqueue_pending(pipeline)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.frozen = true;
        }
        result
    }

    fn poll_inner(&mut self, pipeline: &CandidatePipeline) -> Result<()> {
        ensure!(
            Arc::ptr_eq(&self.owner, &pipeline.owner_identity()),
            "progress owner changed"
        );
        if self.outbox.is_some() {
            return Ok(());
        }
        if self.recovering {
            if !self.recovery_roots.is_empty() || self.recovery_work.is_some() {
                return Ok(());
            }
            self.finish_recovery()?;
        }
        if let Some(message) = self.replay.pop_front() {
            self.emit(message)?;
            return Ok(());
        }
        let context = self.context();
        let parent = self.parent();
        let round = self.round();
        // A QC may arrive while a round transition is waiting for its ACK.
        // Capture it before that ACK retires the old collector buckets.
        self.pin_decisions()?;
        if let Some(message) = self.journal.poll(pipeline)? {
            match &message {
                Some(DurableMessage::Vote(vote)) => {
                    crate::native_pipeline::profile::critical_event(
                        crate::native_pipeline::profile::CriticalEvent::LocalVoteDurable,
                        vote.context.height,
                        vote.round,
                        vote.value,
                        None,
                    );
                }
                Some(DurableMessage::Decision { proposal, .. }) => {
                    let body = self.candidate(proposal.value).map(|candidate| candidate.0);
                    crate::native_pipeline::profile::critical_event(
                        crate::native_pipeline::profile::CriticalEvent::DecisionDurableAck,
                        proposal.context.height,
                        proposal.round,
                        Some(proposal.value),
                        body,
                    );
                }
                Some(DurableMessage::Proposal(_)) | None => {}
            }
            if let Some(DurableMessage::Decision { proposal, .. }) = &message {
                self.retain_published(proposal, context, parent)?;
            }
            if self.context() != context {
                self.collector.reset_context(self.context(), self.round())?;
                self.future_votes.promote(&mut self.collector)?;
                self.candidates.clear();
                let height = self.context().height;
                self.recovered_bodies.retain(|_, body| match body.as_ref() {
                    Message::Body { context, .. }
                    | Message::ApflBody { context, .. }
                    | Message::AssetBody { context, .. } => {
                        context.height.saturating_add(1) >= height
                    }
                    _ => false,
                });
                self.candidate_bytes = 0;
                self.offers.clear();
                self.promote_future_offers();
                self.proposed_round = None;
            } else if self.round() != round {
                self.collector
                    .advance_round_and_retire(self.round(), self.round())?;
                self.proposed_round = None;
                let current = self.round();
                self.offers
                    .retain(|_, o| o.decision.is_some() || o.proposal.proposal().round == current);
                // Promote only after the same durable journal acknowledges
                // this round. Retaining a proposal does not grant catch-up,
                // an execution capability, a lock change or signing authority.
                self.promote_future_offers();
                let valid = self
                    .journal
                    .verified_valid_certificate()
                    .and_then(|qc| qc.value());
                self.candidates.retain(|value, _| {
                    Some(*value) == valid
                        || self
                            .offers
                            .values()
                            .any(|o| o.proposal.proposal().value == *value)
                });
                self.candidate_bytes = self
                    .candidates
                    .values()
                    .map(|c| c.candidate.packet().retained_bytes())
                    .sum();
            }
            if let Some(message) = message {
                let message = self.materialize(message)?;
                self.emit(message)?;
                return Ok(());
            }
        }
        Ok(())
    }
    fn emit(&mut self, message: Message) -> Result<()> {
        self.receive(self.journal.local_validator(), &message)?;
        ensure!(self.outbox.is_none(), "unconsumed durable outbox");
        self.outbox = Some(message);
        Ok(())
    }
    // Called ONLY by the actual journal.poll ACK branch above, never emit,
    // receive, replay, registration, or candidate persistence completion.
    fn retain_published(
        &mut self,
        proposal: &super::wire::Proposal,
        context: VoteContext,
        parent: ParentPoint,
    ) -> Result<()> {
        self.published = None;
        ensure!(
            !self.recovering,
            "publication ACK during unresolved recovery"
        );
        ensure!(proposal.context == context, "publication context mismatch");
        let candidate = &self
            .candidates
            .get(&proposal.value)
            .context("acknowledged publication lost its local candidate")?
            .candidate;
        let packet = candidate.bind_to(&self.owner)?;
        let statement = CandidatePipeline::checked_statement(
            candidate,
            &self.owner,
            context,
            &self.domain.validators,
            &parent,
        )?;
        let expected = ParentPoint {
            height: context.height,
            block_hash: statement.hash(),
            state_root: packet.state_root(),
            receipt_batch_commitment: packet.receipt_batch_commitment(),
            state_version: statement.state_version(),
            decision_hash: super::chain::decision_id(&context, statement.hash()),
        };
        ensure!(
            statement.hash() == proposal.value && self.journal.head() == Some(expected),
            "publication ACK differs from exact owner-bound executed candidate"
        );
        let Some(charge) = packet
            .retained_bytes()
            .checked_add(std::mem::size_of::<PublishedCandidate>())
        else {
            return Ok(());
        };
        if self.candidates.len() < self.limits.candidates
            && charge
                <= self
                    .limits
                    .candidate_bytes
                    .saturating_sub(self.candidate_bytes)
        {
            self.published = Some(PublishedCandidate {
                point: expected,
                candidate: candidate.clone(),
                charge,
            });
        }
        Ok(())
    }
    fn materialize(&mut self, message: DurableMessage) -> Result<Message> {
        Ok(match message {
            DurableMessage::Vote(vote) => Message::Vote(vote),
            DurableMessage::Proposal(proposal) => {
                self.proposed_round = Some(proposal.round);
                let body_id = self
                    .candidate(proposal.value)
                    .context("durable proposal lost local candidate")?
                    .0;
                Message::Proposal {
                    proposal,
                    body_id,
                    valid_quorum: self.journal.valid_certificate().cloned(),
                }
            }
            DurableMessage::Decision {
                proposal,
                certificate,
            } => {
                let body_id = self
                    .candidate(proposal.value)
                    .context("durable decision lost local candidate")?
                    .0;
                Message::Decision {
                    proposal,
                    certificate,
                    body_id,
                }
            }
        })
    }
    fn candidate(&self, value: Hash) -> Option<(Hash, DurableCandidate)> {
        self.candidates
            .get(&value)
            .map(|c| (c.body_id, c.candidate.clone()))
    }
    fn is_local_leader(&self) -> Result<bool> {
        Ok(self
            .domain
            .validators
            .leader(self.context().height, self.round())?
            == self.journal.local_validator())
    }

    fn initialize_recovery(&mut self, mut records: Vec<ReplayRecord>) -> Result<()> {
        ensure!(
            records.len() <= MAX_REPLAY_RECORDS,
            "cold replay role budget"
        );
        if self.journal.decided().is_some() {
            return Ok(());
        }
        if records.is_empty() {
            ensure!(
                self.journal.valid_certificate().is_none(),
                "undecided valid candidate dependencies missing"
            );
            // A warm mutated journal may not have cold replay records. Do not
            // silently accept it and discard unresolved signing dependencies.
            match self.journal.last_durable_message() {
                None => {}
                Some(DurableMessage::Vote(vote)) if vote.value.is_none() => {
                    self.replay.push_back(Message::Vote(vote.clone()));
                }
                _ => anyhow::bail!("nonempty journal must be explicitly reopened for recovery"),
            }
            return Ok(());
        }
        records.sort_by_key(|r| r.revision);
        self.recovering = true;
        for record in records {
            let local = match record.message {
                Some(DurableMessage::Vote(vote))
                    if vote.context == self.context() && vote.round == self.round() =>
                {
                    self.replay.push_back(Message::Vote(vote));
                    None
                }
                Some(DurableMessage::Proposal(p))
                    if p.context == self.context() && p.round == self.round() =>
                {
                    ensure!(
                        p.proposer_id == self.journal.local_validator(),
                        "nonlocal replay"
                    );
                    self.proposed_round = Some(p.round);
                    Some(p)
                }
                _ => None,
            };
            let (proposal, valid, locator) = match record.evidence {
                ReplayEvidence::None => {
                    ensure!(local.is_none(), "replay proposal lacks evidence");
                    continue;
                }
                ReplayEvidence::Proposal {
                    proposal,
                    justification,
                    candidate,
                } => (proposal, justification, candidate),
                ReplayEvidence::Certified {
                    proposal,
                    candidate,
                    ..
                } => {
                    ensure!(
                        local.is_none(),
                        "replay proposal lacks original justification"
                    );
                    (proposal, None, candidate)
                }
            };
            ensure!(
                proposal.proposal().context == self.context()
                    && proposal.proposal().value == locator.value,
                "replay locator/context mismatch"
            );
            if let Some(previous) = self.recovery_roots.insert(locator.value, locator) {
                ensure!(previous == locator, "conflicting recovery locators");
            }
            if let Some(original) = &local {
                ensure!(
                    original == proposal.proposal(),
                    "replay proposal bytes changed"
                );
            }
            if local.is_some() || proposal.proposal().round == self.round() {
                self.recovery_offers.push(ReplayOffer {
                    proposal,
                    valid,
                    locator,
                    local: local.is_some(),
                });
            }
        }
        ensure!(
            self.recovery_roots.len() <= self.limits.candidates,
            "cold replay exceeds candidate slots"
        );
        Ok(())
    }
    fn finish_recovery(&mut self) -> Result<()> {
        for item in std::mem::take(&mut self.recovery_offers) {
            let body_id = self
                .candidate(item.locator.value)
                .context("replay candidate missing after execution")?
                .0;
            if item.local {
                self.replay.push_back(Message::Proposal {
                    proposal: item.proposal.clone().into_proposal(),
                    valid_quorum: item.valid.clone().map(VerifiedQuorum::into_quorum),
                    body_id,
                });
            }
            let key = (item.proposal.proposal().proposer_id, false);
            // A later Certified record is NOT the proposal's original valid QC.
            // Never erase a retained original justification with that record.
            if let Some(known) = self.offers.get(&key) {
                if known.proposal.proposal() == item.proposal.proposal()
                    && known.valid.is_some()
                    && item.valid.is_none()
                {
                    continue;
                }
            }
            self.offers.insert(
                key,
                Offered {
                    proposal: item.proposal,
                    valid: item.valid,
                    decision: None,
                    body_id,
                },
            );
        }
        self.recovering = false;
        Ok(())
    }

    fn pin_decisions(&mut self) -> Result<()> {
        // Pin a completed old-round decision certificate BEFORE retiring vote
        // buckets, even when the corresponding body has not executed yet.
        let mut proofs = Vec::new();
        for ((peer, decision), offer) in &self.offers {
            if !decision {
                let p = offer.proposal.proposal();
                if self.offers.get(&(*peer, true)).is_some_and(|known| {
                    known.proposal.proposal().round == p.round
                        && known.proposal.proposal().value == p.value
                }) {
                    continue;
                }
                if let Some(qc) = self
                    .collector
                    .quorum(p.round, Phase::Precommit, Some(p.value))?
                {
                    proofs.push((
                        *peer,
                        Offered {
                            proposal: offer.proposal.clone(),
                            valid: None,
                            decision: Some(qc),
                            body_id: offer.body_id,
                        },
                    ));
                }
            }
        }
        for (peer, proof) in proofs {
            self.offers.insert((peer, true), proof);
        }
        Ok(())
    }

    fn drive_consensus(&mut self) -> Result<()> {
        if self.journal.is_pending() {
            return Ok(());
        }
        if self.journal.decided().is_some() {
            self.journal.advance_height()?;
            return Ok(());
        }
        // An old-round decision remains eligible after a round change. No
        // body hint or QC bypasses the current local execution capability.
        for offer in self.offers.values() {
            let p = offer.proposal.proposal();
            let Some((_, candidate)) = self.candidate(p.value) else {
                continue;
            };
            let decision = match &offer.decision {
                Some(qc) => Some(qc.clone()),
                None => self
                    .collector
                    .quorum(p.round, Phase::Precommit, Some(p.value))?,
            };
            if let Some(qc) = decision {
                crate::native_pipeline::profile::critical_event(
                    crate::native_pipeline::profile::CriticalEvent::PrecommitQcAvailable,
                    p.context.height,
                    p.round,
                    Some(p.value),
                    Some(offer.body_id),
                );
                self.journal
                    .observe_decision(&offer.proposal, &candidate, &qc)?;
                crate::native_pipeline::profile::critical_event(
                    crate::native_pipeline::profile::CriticalEvent::DecisionStaged,
                    p.context.height,
                    p.round,
                    Some(p.value),
                    Some(offer.body_id),
                );

                return Ok(());
            }
        }
        if self.journal.step() == TimeoutStep::Propose {
            if let Some(offer) = self.offers.values().find(|offer| {
                offer.proposal.proposal().round == self.round()
                    && offer.decision.is_none()
                    && self.candidate(offer.proposal.proposal().value).is_some()
            }) {
                let candidate = self
                    .candidate(offer.proposal.proposal().value)
                    .expect("checked candidate")
                    .1;
                self.journal
                    .accept_proposal(&offer.proposal, &candidate, offer.valid.as_ref())?;

                return Ok(());
            }
            if self.is_local_leader()? && self.proposed_round != Some(self.round()) {
                let valid = self.journal.verified_valid_certificate();
                let candidate = match &valid {
                    Some(qc) => qc.value().and_then(|value| self.candidate(value)),
                    None => self.candidates.values().find_map(|body| {
                        (body.local_round == Some(self.round()))
                            .then(|| (body.body_id, body.candidate.clone()))
                    }),
                };
                if let Some((_, candidate)) = candidate {
                    self.journal.propose(&candidate, valid.as_deref())?;

                    return Ok(());
                }
            }
        } else {
            let round = self.round();
            let already_valid = self
                .journal
                .valid_certificate()
                .is_some_and(|qc| qc.votes.first().is_some_and(|vote| vote.round == round));
            if !already_valid {
                for offer in self
                    .offers
                    .values()
                    .filter(|offer| offer.proposal.proposal().round == round)
                {
                    let value = offer.proposal.proposal().value;
                    let Some((_, candidate)) = self.candidate(value) else {
                        continue;
                    };
                    if let Some(qc) = self.collector.quorum(round, Phase::Prevote, Some(value))? {
                        crate::native_pipeline::profile::critical_event(
                            crate::native_pipeline::profile::CriticalEvent::PrevoteQcAvailable,
                            offer.proposal.proposal().context.height,
                            round,
                            Some(value),
                            Some(offer.body_id),
                        );
                        self.journal
                            .observe_prevotes(&qc, Some((&offer.proposal, &candidate)))?;

                        return Ok(());
                    }
                }
            }
            if self.journal.step() == TimeoutStep::Prevote {
                if let Some(qc) = self.collector.quorum(round, Phase::Prevote, None)? {
                    self.journal.observe_prevotes(&qc, None)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod future_vote_tests {
    use super::*;
    use novovm_consensus::round_bft::{
        test_vectors::{sign_vote, FixtureSigningKey},
        wire::{Validator, Vote},
    };

    struct Fixture {
        keys: Vec<FixtureSigningKey>,
        domain: MessageDomain,
        parent: VoteContext,
        child: VoteContext,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let keys: Vec<_> = (1..=4)
                .map(|seed| FixtureSigningKey::from_bytes(&[seed; 32]))
                .collect();
            let members = keys
                .iter()
                .map(|key| Validator::new(key.verifying_key().to_bytes(), 1))
                .collect::<Result<Vec<_>>>()?;
            let set = Arc::new(ValidatorSet::new_for_test(7, 1, 1, members)?);
            let parent = VoteContext {
                chain_id: 7,
                genesis_config_commitment: [1; 32],
                protocol_commitment: [2; 32],
                epoch: 1,
                validator_set_hash: set.hash(),
                height: 1,
                parent_block_hash: [0; 32],
                parent_decision_hash: [0; 32],
            };
            Ok(Self {
                keys,
                domain: MessageDomain {
                    chain_id: 7,
                    genesis: parent.genesis_config_commitment,
                    protocol: parent.protocol_commitment,
                    validators: set,
                },
                parent,
                child: VoteContext {
                    height: 2,
                    parent_block_hash: [3; 32],
                    parent_decision_hash: [4; 32],
                    ..parent
                },
            })
        }

        fn vote(&self, signer: usize, context: VoteContext, round: u64, phase: Phase) -> Vote {
            // Explicit consensus rule fixture, not actual ML-DSA qualification.
            sign_vote(
                context,
                round,
                phase,
                Some([9; 32]),
                &self.domain.validators,
                &self.keys[signer],
            )
            .unwrap()
        }

        fn receive(&self, retained: &mut FutureVotes, vote: Vote) -> Result<()> {
            let VerifiedEvidence::Vote(verified) =
                message_evidence::verify(&Message::Vote(vote), &self.domain)?
            else {
                anyhow::bail!("vote fixture evidence mismatch");
            };
            retained.retain(self.parent, 32, &verified)
        }

        fn collector(&self, context: VoteContext) -> VoteCollector {
            VoteCollector::new(
                context,
                self.domain.validators.clone(),
                0,
                collector_limits_with_future_custody(
                    CollectorLimits {
                        max_retained_rounds: 2,
                        max_future_round_span: 32,
                        max_votes: 28,
                    },
                    4,
                )
                .unwrap(),
            )
            .unwrap()
        }
    }

    #[test]
    fn future_vote_custody_rejects_invalid_domain_signer_signature_height_and_round() -> Result<()>
    {
        let f = Fixture::new()?;
        let original = f.vote(0, f.child, 0, Phase::Prevote);
        let mut retained = FutureVotes::default();
        for field in 0..4 {
            let mut vote = original.clone();
            match field {
                0 => vote.context.genesis_config_commitment = [17; 32],
                1 => vote.context.protocol_commitment = [18; 32],
                2 => vote.validator_id = [19; 32],
                _ => vote.signature[0] ^= 1,
            }
            assert!(f.receive(&mut retained, vote).is_err());
            assert!(retained.tips.is_empty());
        }
        for context in [
            VoteContext {
                genesis_config_commitment: [17; 32],
                ..f.child
            },
            VoteContext {
                protocol_commitment: [18; 32],
                ..f.child
            },
        ] {
            // A genuine signature under the same set cannot change the pinned
            // local genesis/protocol domain at the existing receive boundary.
            let foreign = f.vote(0, context, 0, Phase::Prevote);
            assert!(foreign.verify(&f.domain.validators).is_ok());
            assert!(f.receive(&mut retained, foreign).is_err());
            assert!(retained.tips.is_empty());
        }
        // Even validly signed inputs cannot expand the one-height/round bound.
        for (context, round) in [
            (f.parent, 0),
            (
                VoteContext {
                    height: 3,
                    ..f.child
                },
                0,
            ),
            (f.child, 33),
        ] {
            assert!(f
                .receive(&mut retained, f.vote(0, context, round, Phase::Prevote))
                .is_err());
            assert!(retained.tips.is_empty());
        }
        Ok(())
    }

    #[test]
    fn future_vote_custody_deduplicates_by_signed_context_signer_phase() -> Result<()> {
        let f = Fixture::new()?;
        let mut retained = FutureVotes::default();
        let vote = f.vote(0, f.child, 0, Phase::Prevote);
        for _ in 0..8 {
            f.receive(&mut retained, vote.clone())?;
        }
        assert_eq!(retained.tips.len(), 1);
        let conflict = sign_vote(
            f.child,
            0,
            Phase::Prevote,
            Some([10; 32]),
            &f.domain.validators,
            &f.keys[0],
        )?;
        assert!(f.receive(&mut retained, conflict).is_err());
        assert!(f
            .receive(
                &mut retained,
                f.vote(
                    0,
                    VoteContext {
                        parent_decision_hash: [11; 32],
                        ..f.child
                    },
                    1,
                    Phase::Prevote,
                )
            )
            .is_err());
        assert_eq!(
            retained.tips.values().next().map(VerifiedVote::vote),
            Some(&vote)
        );
        f.receive(&mut retained, f.vote(0, f.child, 1, Phase::Prevote))?;
        assert!(f.receive(&mut retained, vote).is_err());
        f.receive(&mut retained, f.vote(0, f.child, 0, Phase::Precommit))?;
        assert_eq!(retained.tips.len(), 2);
        Ok(())
    }

    #[test]
    fn future_vote_custody_preserves_current_quorum_within_original_total_budget() -> Result<()> {
        let f = Fixture::new()?;
        let total = CollectorLimits {
            max_retained_rounds: 2,
            max_future_round_span: 32,
            max_votes: 28,
        };
        assert_eq!(
            collector_limits_with_future_custody(total, 4)?.max_votes,
            20
        );
        assert!(collector_limits_with_future_custody(
            CollectorLimits {
                max_votes: 27,
                ..total
            },
            4
        )
        .is_err());
        let mut retained = FutureVotes::default();
        let mut current = f.collector(f.parent);
        for round in 0..=32 {
            for signer in 0..4 {
                for phase in [Phase::Prevote, Phase::Precommit] {
                    f.receive(&mut retained, f.vote(signer, f.child, round, phase))?;
                }
            }
            assert_eq!(retained.tips.len(), 8);
            assert!(current.vote_count() + retained.tips.len() <= total.max_votes);
        }
        for signer in 0..4 {
            for phase in [Phase::Prevote, Phase::Precommit] {
                current.insert(&f.vote(signer, f.parent, 0, phase))?;
            }
        }
        assert!(current.quorum(0, Phase::Prevote, Some([9; 32]))?.is_some());
        assert!(current
            .quorum(0, Phase::Precommit, Some([9; 32]))?
            .is_some());
        assert!(current.vote_count() + retained.tips.len() <= total.max_votes);
        Ok(())
    }

    #[test]
    fn future_vote_custody_promotes_exact_parent_once_and_retires_other_branches() -> Result<()> {
        let f = Fixture::new()?;
        let mut retained = FutureVotes::default();
        for signer in 0..3 {
            f.receive(&mut retained, f.vote(signer, f.child, 0, Phase::Prevote))?;
        }
        let wrong = VoteContext {
            parent_block_hash: [77; 32],
            ..f.child
        };
        f.receive(&mut retained, f.vote(3, wrong, 0, Phase::Prevote))?;
        let mut child = f.collector(f.child);
        assert_eq!(child.vote_count(), 0);
        retained.promote(&mut child)?;
        assert_eq!(child.vote_count(), 3);
        assert!(retained.tips.is_empty());
        assert!(child.quorum(0, Phase::Prevote, Some([9; 32]))?.is_some());
        retained.promote(&mut child)?;
        assert_eq!(child.vote_count(), 3);
        child.reset_context(
            VoteContext {
                height: 3,
                ..f.child
            },
            0,
        )?;
        retained.promote(&mut child)?;
        assert_eq!(child.vote_count(), 0);
        Ok(())
    }
}
