//! Startup-only replay of the bounded journal dependency closure. No new
//! signature, timer, or execution capability is inferred from stored evidence.

use super::*;
use crate::consensus::journal::{ReplayEvidence, ReplayRecord, MAX_REPLAY_RECORDS};
use crate::persistence::io::IoTicket;
use crate::persistence::StoredCandidate;

pub(super) struct Recovery {
    pending: VecDeque<CandidateLocator>,
    active: Option<Active>,
    proposals: VecDeque<ReplayProposal>,
    offers: VecDeque<RestoreOffer>,
}

enum Active {
    Read {
        locator: CandidateLocator,
        ticket: Option<IoTicket<Option<StoredCandidate>>>,
    },
    Preparing {
        locator: CandidateLocator,
        bytes: usize,
    },
    Execute {
        locator: CandidateLocator,
        body_id: Hash,
        submitted: bool,
    },
}

struct ReplayProposal {
    proposal: Proposal,
    justification: Option<Quorum>,
    locator: CandidateLocator,
}

struct RestoreOffer {
    proposal: VerifiedProposal,
    justification: Option<VerifiedQuorum>,
    locator: CandidateLocator,
}

impl Recovery {
    pub(super) fn retained_bytes(&self) -> usize {
        match self.active {
            Some(Active::Preparing { bytes, .. }) => bytes,
            _ => 0,
        }
    }
}

impl Controller {
    /// Deterministic ingress timing only: the integration fixture supplies a
    /// REAL channel-owner-prepared, signature-verified small control message.
    /// The one slot consumes the ordinary event budget and receive/retirement
    /// path. It cannot fabricate a body, execution result, or signing authority.
    #[cfg(test)]
    pub(in crate::consensus) fn regression_recovery_control(
        &mut self,
        peer: &str,
        ready: &Ready,
    ) -> Result<bool> {
        ensure!(
            self.is_recovering(),
            "recovery ingress fixture used after recovery"
        );
        ensure!(
            matches!(ready.message.as_ref(), Message::Vote(_))
                && matches!(ready.evidence.as_ref(), VerifiedEvidence::Vote(_))
                && ready.body.is_none()
                && self.config.peers.values().any(|known| known == peer),
            "recovery ingress fixture requires verified control and configured route"
        );
        if self.recovery_test_ingress.is_some() {
            return Ok(false);
        }
        self.recovery_test_ingress = Some(crate::consensus::channel::Received {
            peer: peer.to_owned(),
            ready: Ready {
                message: ready.message.clone(),
                prepared: ready.prepared.clone(),
                evidence: ready.evidence.clone(),
                body: None,
            },
        });
        Ok(true)
    }

    /// Opening has already checked the referenced event digests, roles, local
    /// signer, and original justification. This cold constructor consumes the
    /// typed records; it does not repeat signature verification in poll.
    pub(super) fn initialize_recovery(&mut self, mut records: Vec<ReplayRecord>) -> Result<()> {
        ensure!(
            records.len() <= MAX_REPLAY_RECORDS,
            "cold replay role budget"
        );
        if self.journal.decided().is_some() {
            // The head is already published. Normal advance/archive recovery
            // handles it; unfinished execution is not required for new height.
            return Ok(());
        }
        if records.is_empty() {
            ensure!(
                self.journal.valid_certificate().is_none(),
                "undecided valid candidate replay dependencies missing"
            );
            return Ok(());
        }
        ensure!(
            !self.journal.is_pending(),
            "cold journal has pending mutation"
        );
        records.sort_by_key(|record| record.revision);
        let mut roots = BTreeMap::new();
        let mut recovery = Recovery {
            pending: VecDeque::new(),
            active: None,
            proposals: VecDeque::new(),
            offers: VecDeque::new(),
        };
        for record in records {
            let mut local_proposal = None;
            match record.message {
                Some(DurableMessage::Vote(vote))
                    if vote.context == self.context() && vote.round == self.round() =>
                {
                    ensure!(
                        self.prepare(
                            PrepareInput::New(Arc::new(Message::Vote(vote))),
                            Purpose::Replay
                        )?,
                        "cold vote replay preparation budget"
                    );
                }
                Some(DurableMessage::Proposal(proposal))
                    if proposal.context == self.context() && proposal.round == self.round() =>
                {
                    ensure!(
                        proposal.proposer_id == self.config.local_validator,
                        "cold outbox proposal is not local"
                    );
                    self.proposed_round = Some(proposal.round);
                    local_proposal = Some(proposal);
                }
                _ => {}
            }
            let (proposal, justification, locator) = match record.evidence {
                ReplayEvidence::None => {
                    ensure!(
                        local_proposal.is_none(),
                        "cold proposal lacks original evidence"
                    );
                    continue;
                }
                ReplayEvidence::Proposal {
                    proposal,
                    justification,
                    candidate,
                } => (proposal, justification, candidate),
                ReplayEvidence::Certified {
                    proposal,
                    certificate: _,
                    candidate,
                } => {
                    // The journal retains the validated current valid QC. A
                    // certified event is not the original proposal justification.
                    ensure!(
                        local_proposal.is_none(),
                        "cold proposal has no original justification"
                    );
                    (proposal, None, candidate)
                }
            };
            ensure!(
                proposal.proposal().context == self.context(),
                "cold replay context changed"
            );
            ensure!(
                proposal.proposal().value == locator.value,
                "cold proposal locator mismatch"
            );
            if let Some(previous) = roots.insert(locator.value, locator) {
                ensure!(previous == locator, "conflicting cold candidate locators");
            }
            if let Some(local) = local_proposal {
                ensure!(
                    local == *proposal.proposal(),
                    "cold original proposal differs from outbox"
                );
                recovery.proposals.push_back(ReplayProposal {
                    proposal: proposal.into_proposal(),
                    justification: justification.map(VerifiedQuorum::into_quorum),
                    locator,
                });
            } else if proposal.proposal().round == self.round() {
                recovery.offers.push_back(RestoreOffer {
                    proposal,
                    justification,
                    locator,
                });
            }
        }
        ensure!(
            roots.len() <= self.config.limits.max_bodies,
            "cold replay roots exceed existing body slots"
        );
        recovery.pending.extend(roots.into_values());
        self.recovery = Some(recovery);
        Ok(())
    }

    pub(super) fn poll_recovery(&mut self, pipeline: &CandidatePipeline) -> Result<()> {
        if !self.retired.is_empty() {
            return Ok(());
        }
        let Some(mut recovery) = self.recovery.take() else {
            return Ok(());
        };
        let result = self.progress_recovery(&mut recovery, pipeline);
        // Preserve all remaining roots and pending owned inputs after failure.
        // The outer poll makes that failure sticky until explicit reopening.
        if !matches!(result, Ok(true)) {
            self.recovery = Some(recovery);
        }
        result.map(|_| ())
    }

    fn progress_recovery(
        &mut self,
        recovery: &mut Recovery,
        pipeline: &CandidatePipeline,
    ) -> Result<bool> {
        if recovery.active.is_none() {
            if let Some(locator) = recovery.pending.pop_front() {
                recovery.active = Some(Active::Read {
                    locator,
                    ticket: None,
                });
            }
        }
        if let Some(active) = recovery.active.as_mut() {
            match active {
                Active::Read { locator, ticket } => {
                    if ticket.is_none() {
                        *ticket = pipeline.try_recover_consensus_candidate(locator.candidate_id)?;
                        return Ok(false);
                    }
                    let Some(stored) = ticket.as_mut().expect("read ticket").try_take()? else {
                        return Ok(false);
                    };
                    let stored = Arc::new(stored.context("cold replay candidate marker missing")?);
                    let checked = (|| -> Result<usize> {
                        ensure!(
                            stored.candidate_id() == locator.candidate_id
                                && stored.document_digest() == locator.document_digest
                                && self.matches_context(stored.context()),
                            "cold candidate locator/parent/business mismatch"
                        );
                        ensure!(
                            BlockStatement::from_stored(
                                &stored,
                                self.context(),
                                &self.config.validators,
                                &self.parent()
                            )?
                            .hash()
                                == locator.value,
                            "cold candidate statement differs from replay"
                        );
                        let bytes = self
                            .channel
                            .preparation_charge()
                            .checked_add(stored.record_bytes())
                            .context("cold candidate accounting overflow")?;
                        ensure!(
                            bytes
                                <= self
                                    .config
                                    .limits
                                    .max_body_bytes
                                    .saturating_sub(self.body_bytes()),
                            "cold candidate body budget"
                        );
                        Ok(bytes)
                    })();
                    let bytes = match checked {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            self.retire(Retirement::Input(PrepareInput::StoredBody(stored)));
                            return Err(error);
                        }
                    };
                    let locator = *locator;
                    ensure!(
                        self.prepare(
                            PrepareInput::StoredBody(stored),
                            Purpose::RecoveryBody(locator)
                        )?,
                        "cold body preparation budget"
                    );
                    recovery.active = Some(Active::Preparing { locator, bytes });
                }
                Active::Preparing { .. } => {}
                Active::Execute {
                    locator,
                    body_id,
                    submitted,
                } => {
                    let body = self
                        .bodies
                        .get_mut(body_id)
                        .context("cold body retired before execution")?;
                    if let Some((value, _)) = body.candidate.as_ref() {
                        ensure!(*value == locator.value, "cold executed value mismatch");
                        recovery.active = None;
                    } else if !*submitted && self.inflight.len() < self.config.limits.max_inflight {
                        let request = body
                            .request
                            .take()
                            .context("cold owner did not prepare execution request")?;
                        match pipeline.try_submit_owned(request) {
                            Ok(Submission::Accepted(ticket)) => {
                                self.inflight.push(Execution {
                                    id: *body_id,
                                    context: self.context(),
                                    parent: self.parent(),
                                    requester: self.local_peer.clone(),
                                    ticket,
                                    recovery: Some(*locator),
                                });
                                *submitted = true;
                            }
                            Ok(Submission::Backpressured(request)) => body.request = Some(request),
                            Err(rejected) => {
                                self.retire(Retirement::Request(rejected.request));
                                return Err(rejected
                                    .error
                                    .context("cold execution admission rejected"));
                            }
                        }
                    }
                }
            }
            return Ok(false);
        }
        if let Some(replay) = recovery.proposals.pop_front() {
            let (id, _) = self
                .candidate(replay.locator.value)
                .context("cold proposal lost executed candidate")?;
            let prepared = self
                .bodies
                .get(&id)
                .expect("candidate body")
                .prepared
                .clone();
            ensure!(self.cache(prepared, None), "cold body fanout budget");
            ensure!(
                self.prepare(
                    PrepareInput::New(Arc::new(Message::Proposal {
                        proposal: replay.proposal,
                        valid_quorum: replay.justification,
                        body_id: id,
                    })),
                    Purpose::Replay
                )?,
                "cold proposal replay preparation budget"
            );
            return Ok(false);
        }
        // Local replay must have passed the owner before recovered evidence is
        // installed. No remote delivery acknowledgement gates local recovery.
        if !self.preparing.is_empty() {
            return Ok(false);
        }
        if let Some(offer) = recovery.offers.pop_front() {
            let (body_id, _) = self
                .candidate(offer.locator.value)
                .context("cold offer lost executed candidate")?;
            let peer = self
                .peer_for(&offer.proposal.proposal().proposer_id)
                .context("cold proposer not configured")?
                .to_owned();
            self.offers.insert(
                (peer, false),
                Offered {
                    proposal: offer.proposal,
                    valid: offer.justification,
                    decision: None,
                    body_id,
                },
            );
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn recovered_body(&mut self, locator: CandidateLocator, ready: Ready) -> Result<()> {
        let stage = self
            .recovery
            .as_mut()
            .and_then(|recovery| recovery.active.take());
        if !matches!(stage, Some(Active::Preparing { locator: expected, .. }) if expected == locator)
        {
            self.retire(Retirement::Ready(ready));
            anyhow::bail!("cold body response does not match pending locator");
        }
        // StoredBody's large archive has been released on the channel owner;
        // replace its reservation with the actual prepared body's accounting.
        let Some(body_id) = ready.prepared.body_id() else {
            self.retire(Retirement::Ready(ready));
            anyhow::bail!("cold body response has no body id");
        };
        self.keep_body(self.local_peer.clone(), ready, None)?;
        let body = self
            .bodies
            .get_mut(&body_id)
            .context("cold body rejected by existing controller limits")?;
        body.recovered = true;
        self.recovery
            .as_mut()
            .context("cold recovery disappeared")?
            .active = Some(Active::Execute {
            locator,
            body_id,
            submitted: false,
        });
        Ok(())
    }
}
