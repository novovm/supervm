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
    // proposal must survive a pending local round ACK, not rely on repeating
    // the same completed transport object. One tip per proposer; never votes.
    future_offers: BTreeMap<Hash, Offered>,
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
        let records = journal.take_replay_records();
        let mut this = Self {
            collector: VoteCollector::new(context, validators, journal.round(), limits.votes)?,
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
        ensure!(!self.is_frozen(), "progress frozen");
        ensure!(
            self.domain
                .validators
                .members()
                .iter()
                .any(|v| v.id() == peer),
            "progress source is outside validator set"
        );
        match (message, message_evidence::verify(message, &self.domain)?) {
            (Message::Vote(vote), VerifiedEvidence::Vote(_)) => {
                if vote.context == self.context() {
                    self.collector.insert(vote)?;
                    crate::native_pipeline::profile::critical_event(
                        crate::native_pipeline::profile::CriticalEvent::VoteReceived,
                        vote.context.height,
                        vote.round,
                        vote.value,
                        None,
                    );
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
                    if self
                        .future_offers
                        .get(&peer)
                        .is_none_or(|old| old.proposal.proposal().round <= p.round)
                    {
                        self.future_offers.insert(
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
                }
            }
            _ => {
                anyhow::bail!("progress expects qualified consensus control, not body/network work")
            }
        }
        Ok(())
    }

    /// Never blocks on transport or AOEM completion. A local persistence error
    /// latches until explicit reopening/recovery; it cannot be retried as a new
    /// signer. Peer rejection is intentionally outside this failure boundary.
    pub fn poll(&mut self, pipeline: &CandidatePipeline, now: Instant) -> Result<()> {
        self.poll_acknowledgements(pipeline)?;
        self.poll_after_receive(now)
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
    pub(crate) fn poll_after_receive(&mut self, now: Instant) -> Result<()> {
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
                self.candidates.clear();
                let height = self.context().height;
                self.recovered_bodies.retain(|_, body| match body.as_ref() {
                    Message::Body { context, .. }
                    | Message::ApflBody { context, .. }
                    | Message::UcaBody { context, .. } => {
                        context.height.saturating_add(1) >= height
                    }
                    _ => false,
                });
                self.candidate_bytes = 0;
                self.offers.clear();
                self.future_offers.clear();
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
                for (peer, offer) in std::mem::take(&mut self.future_offers) {
                    match offer.proposal.proposal().round.cmp(&current) {
                        std::cmp::Ordering::Equal => {
                            self.offers.insert((peer, false), offer);
                        }
                        std::cmp::Ordering::Greater => {
                            self.future_offers.insert(peer, offer);
                        }
                        std::cmp::Ordering::Less => {}
                    }
                }
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
