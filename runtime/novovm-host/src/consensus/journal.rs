//! Consecutive-height durable signing over the SAME AOEM I/O owner.
//! All messages stay private until safety state + exact outbox bytes have been
//! atomically persisted and read back. A decision advances the local chain head
//! in that same batch. No network reactor, full pacemaker or execution proof.

pub(crate) mod codec;
mod opening;
pub use opening::JournalOpening;

use super::chain::ChainRecord;
use super::round::{LocalTimeout, PreparedStep, State, Step};
use super::statement::{BlockStatement, ParentPoint};
use super::wire::{
    self, Context as ConsensusContext, Hash, Proposal, Quorum, Validator, ValidatorSet,
    VerifiedProposal, VerifiedQuorum, Vote,
};
use crate::persistence::io::IoTicket;
use crate::persistence::metadata::{MetaChange, MetaGuard, MetaKey, MetaOutcome, MetaTransition};
use crate::pipeline::{CandidatePipeline, DurableCandidate};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use std::sync::Arc;

/// Durable protocol evidence. An acknowledged Decision also publishes the
/// local head; none of these messages alone proves business execution validity.
#[derive(Clone, Debug)]
pub enum DurableMessage {
    Proposal(Proposal),
    Vote(Vote),
    Decision {
        proposal: Proposal,
        certificate: Quorum,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeoutStep {
    Propose,
    Prevote,
    Precommit,
}

#[derive(Clone)]
struct Snapshot {
    state: State,
    revision: u64,
    proposed: Option<(u64, Hash, Option<u64>)>,
    witness: Option<Quorum>,
}

struct Identity {
    owner: Arc<()>,
    context: ConsensusContext,
    parent: ParentPoint,
    set: Arc<ValidatorSet>,
    key: SigningKey,
    validator: Hash,
}

impl Identity {
    fn check_owner(&self, pipeline: &CandidatePipeline) -> Result<()> {
        ensure!(
            Arc::ptr_eq(&self.owner, &pipeline.owner_identity()),
            "signing journal belongs to another resident pipeline; reopen explicitly"
        );
        Ok(())
    }

    fn statement(&self, candidate: &DurableCandidate) -> Result<BlockStatement> {
        let packet = candidate.bind_to(&self.owner)?;
        BlockStatement::from_executed(packet, self.context, &self.set, &self.parent)
    }
}

struct Pending {
    snapshot: Snapshot,
    saved: Vec<u8>,
    outbox: Vec<u8>,
    message: Option<DurableMessage>,
    applying: Option<IoTicket<MetaOutcome>>,
    publication: Option<ChainRecord>,
    next_identity: Option<(ConsensusContext, ParentPoint)>,
}

/// Real signing is fail-closed on stale metadata, unknown writes or bad readback.
/// Only one in-flight transition is permitted per signer. No public method
/// exposes the pending message, raw private key or mutates the state in place.
pub struct ValidatorJournal {
    identity: Identity,
    snapshot: Snapshot,
    saved: Option<Vec<u8>>,
    last_message: Option<DurableMessage>,
    pending: Option<Pending>,
    frozen: bool,
    head_record: Option<ChainRecord>,
    head_bytes: Option<Vec<u8>>,
}

impl ValidatorJournal {
    /// Explicit cold recovery from the configured genesis anchor, not genesis
    /// creation or an arbitrary checkpoint supplied by a peer. One local signer
    /// owns each ledger; changing its key/set requires a separate protocol.
    pub fn open(
        pipeline: &CandidatePipeline,
        context: ConsensusContext,
        parent: ParentPoint,
        set: Arc<ValidatorSet>,
        key: SigningKey,
    ) -> Result<JournalOpening> {
        context.validate(&set)?;
        ensure!(
            context.height == 1 && parent.height == 0,
            "journal opening requires the configured genesis anchor, not an arbitrary checkpoint"
        );
        let domain = pipeline.storage_domain();
        ensure!(
            context.chain_id == domain.chain_id
                && context.genesis_config_commitment == domain.genesis_config_commitment
                && context.protocol_commitment == domain.protocol_commitment,
            "signing context differs from resident storage domain"
        );
        ensure!(
            parent.height.checked_add(1) == Some(context.height)
                && parent.block_hash == context.parent_block_hash
                && parent.decision_hash == context.parent_decision_hash
                && parent.state_root != [0; 32]
                && parent.receipt_batch_commitment != [0; 32],
            "signing parent does not bind configured context"
        );
        let validator = Validator::new(key.verifying_key().to_bytes(), 1)?.id();
        ensure!(
            set.member(&validator).is_some(),
            "local signer is not in validator set"
        );
        Ok(JournalOpening::new(Identity {
            owner: pipeline.owner_identity(),
            context,
            parent,
            set,
            key,
            validator,
        }))
    }

    pub fn context(&self) -> ConsensusContext {
        self.identity.context
    }
    /// Immutable configured signer identity, not a key or signing capability.
    pub fn local_validator(&self) -> Hash {
        self.identity.validator
    }
    pub fn parent(&self) -> ParentPoint {
        self.identity.parent
    }
    /// Last locally acknowledged head, not cached signing permission.
    pub fn head(&self) -> Option<ParentPoint> {
        self.head_record.as_ref().map(ChainRecord::point)
    }

    /// Advance only from our durably decided head; preserve the GLOBAL signer
    /// revision/outbox. Resetting keys by height would bypass anti-double-sign.
    pub fn advance_height(&mut self) -> Result<()> {
        ensure!(
            !self.frozen && self.pending.is_none(),
            "journal frozen or transition in flight"
        );
        let record = self
            .head_record
            .as_ref()
            .context("no decided head to advance")?;
        ensure!(
            self.decided() == Some(record.point().block_hash) && self.context() == record.context(),
            "signer has not decided the current head"
        );
        let parent = record.point();
        let context = successor_context(self.context(), parent)?;
        let next = Snapshot {
            state: State::new(context)?,
            revision: self.snapshot.revision,
            proposed: None,
            witness: None,
        };
        self.stage_full(next, None, None, Some((context, parent)))
    }

    pub fn round(&self) -> u64 {
        self.snapshot.state.round()
    }
    pub fn step(&self) -> TimeoutStep {
        match self.snapshot.state.step() {
            Step::Propose => TimeoutStep::Propose,
            Step::Prevote => TimeoutStep::Prevote,
            Step::Precommit => TimeoutStep::Precommit,
        }
    }
    pub fn decided(&self) -> Option<Hash> {
        self.snapshot.state.decided()
    }
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn valid_certificate(&self) -> Option<&Quorum> {
        self.snapshot.witness.as_ref()
    }

    /// Fixed historical bytes only. Resending does not create another vote or
    /// authorize a new round. Transport must still check its route/context.
    pub fn last_durable_message(&self) -> Option<&DurableMessage> {
        self.last_message.as_ref()
    }

    fn ready(&self) -> Result<()> {
        ensure!(
            !self.frozen,
            "signing journal frozen; explicit recovery required"
        );
        ensure!(
            self.pending.is_none(),
            "signing transition already in flight"
        );
        ensure!(self.decided().is_none(), "signing height already decided");
        Ok(())
    }

    /// Make a leader proposal from an executed packet, not arbitrary roots.
    /// Its signature is not observable until poll reports durable completion.
    pub fn propose(
        &mut self,
        candidate: &DurableCandidate,
        valid: Option<&VerifiedQuorum>,
    ) -> Result<()> {
        self.ready()?;
        ensure!(
            self.snapshot.state.step() == Step::Propose,
            "proposal step has passed"
        );
        let value = self.identity.statement(candidate)?.hash();
        let round = self.round();
        ensure!(
            self.snapshot.proposed.is_none_or(|(r, _, _)| r != round),
            "already proposed in this round"
        );
        let valid_round = match self.snapshot.state.valid() {
            None => {
                ensure!(valid.is_none(), "unsolicited proposal validity certificate");
                None
            }
            Some((vr, vv)) => {
                let qc =
                    valid.context("known valid value requires its exact prevote certificate")?;
                ensure!(
                    vv == value
                        && qc.context() == &self.identity.context
                        && qc.round() == vr
                        && qc.value() == Some(value)
                        && qc.phase() == wire::Phase::Prevote
                        && vr < round,
                    "proposal does not preserve highest known valid value/round"
                );
                Some(vr)
            }
        };
        let proposal = Proposal::sign(
            self.identity.context,
            round,
            value,
            valid_round,
            &self.identity.set,
            &self.identity.key,
        )?;
        let mut next = self.snapshot.clone();
        next.proposed = Some((round, value, valid_round));
        self.stage(next, Some(DurableMessage::Proposal(proposal)))
    }

    pub fn accept_proposal(
        &mut self,
        proposal: &VerifiedProposal,
        candidate: &DurableCandidate,
        valid: Option<&VerifiedQuorum>,
    ) -> Result<()> {
        self.ready()?;
        let statement = self.identity.statement(candidate)?;
        let step = self
            .snapshot
            .state
            .prepare_proposal(proposal, statement.hash(), valid)?;
        self.stage_step(step, None, None)
    }

    /// A nil quorum needs no packet; a value quorum requires the full locally
    /// executed proposal. A bare peer QC cannot cause a value precommit.
    pub fn observe_prevotes(
        &mut self,
        quorum: &VerifiedQuorum,
        candidate: Option<(&VerifiedProposal, &DurableCandidate)>,
    ) -> Result<()> {
        self.ready()?;
        let statement = candidate
            .as_ref()
            .map(|(_, packet)| self.identity.statement(packet))
            .transpose()?;
        let input = candidate
            .as_ref()
            .zip(statement.as_ref())
            .map(|((proposal, _), statement)| (*proposal, statement.hash()));
        let step = self.snapshot.state.prepare_prevote_quorum(quorum, input)?;
        self.stage_step(step, None, Some(quorum.quorum().clone()))
    }

    /// Explicit LOCAL timer event. A network message cannot manufacture one.
    /// Scheduling and quorum-observation timers belong to the future pacemaker.
    pub fn timeout(&mut self, expected_round: u64, step: TimeoutStep) -> Result<()> {
        self.ready()?;
        let step = match step {
            TimeoutStep::Propose => Step::Propose,
            TimeoutStep::Prevote => Step::Prevote,
            TimeoutStep::Precommit => Step::Precommit,
        };
        let prepared = self.snapshot.state.prepare_timeout(LocalTimeout {
            context: self.identity.context,
            round: expected_round,
            step,
        })?;
        self.stage_step(prepared, None, None)
    }

    /// Authenticated SAME-round >1/3 evidence only changes the local round;
    /// locked/valid value, durable signing history and current head survive.
    /// This stages metadata; callers must poll for durable acknowledgement.
    pub fn catch_up(&mut self, evidence: &super::collector::CatchUpEvidence) -> Result<()> {
        self.ready()?;
        ensure!(
            evidence.context() == &self.context()
                && u128::from(evidence.signed_weight()) * 3
                    > u128::from(self.identity.set.total_weight()),
            "round catch-up context/threshold mismatch"
        );
        let prepared = self.snapshot.state.prepare_round_change(evidence.round())?;
        self.stage_step(prepared, None, None)
    }

    /// Called only by the local pacemaker after the SAME-round precommit
    /// quorum wait expires (Algorithm 1 lines 47/67), even if a proposal/body
    /// is still missing locally. Never emits an invented intermediate vote.
    pub(super) fn round_wait_elapsed(&mut self, expected_round: u64) -> Result<()> {
        self.ready()?;
        ensure!(expected_round == self.round(), "stale round wait timeout");
        let target = expected_round
            .checked_add(1)
            .context("consensus round exhausted")?;
        let prepared = self.snapshot.state.prepare_round_change(target)?;
        self.stage_step(prepared, None, None)
    }

    /// Atomically archive the local decision and move the head. Candidate data
    /// is already immutable and durable; no second business state copy is made.
    pub fn observe_decision(
        &mut self,
        proposal: &VerifiedProposal,
        candidate: &DurableCandidate,
        certificate: &VerifiedQuorum,
    ) -> Result<()> {
        self.ready()?;
        let statement = self.identity.statement(candidate)?;
        let value = statement.hash();
        let step = self
            .snapshot
            .state
            .prepare_decision(proposal, value, certificate)?;
        ensure!(
            step.intent().is_none() && step.decision() == Some(value),
            "decision transition emitted unexpected vote/value"
        );
        let next = Snapshot {
            state: step.into_parts().0,
            ..self.snapshot.clone()
        };
        self.stage_full(
            next,
            Some(DurableMessage::Decision {
                proposal: proposal.proposal().clone(),
                certificate: certificate.quorum().clone(),
            }),
            Some((statement, candidate)),
            None,
        )
    }

    fn stage_step(
        &mut self,
        prepared: PreparedStep,
        decision: Option<DurableMessage>,
        witness: Option<Quorum>,
    ) -> Result<()> {
        let decision_value = match &decision {
            Some(DurableMessage::Decision { proposal, .. }) => Some(proposal.value),
            Some(_) => anyhow::bail!("non-decision message attached to a round transition"),
            None => None,
        };
        ensure!(
            prepared.decision() == decision_value
                && (decision.is_none() || prepared.intent().is_none()),
            "decision output differs from prepared round transition"
        );
        let changed_valid = prepared.next().valid() != self.snapshot.state.valid();
        let (state, intent, _) = prepared.into_parts();
        let message = match intent {
            Some(intent) => Some(DurableMessage::Vote(Vote::sign(
                intent.context,
                intent.round,
                intent.phase,
                intent.value,
                &self.identity.set,
                &self.identity.key,
            )?)),
            None => decision,
        };
        let mut next = Snapshot {
            state,
            ..self.snapshot.clone()
        };
        if changed_valid {
            next.witness =
                Some(witness.context("valid state transition missing durable quorum witness")?);
        }
        self.stage(next, message)
    }

    fn stage(&mut self, next: Snapshot, message: Option<DurableMessage>) -> Result<()> {
        self.stage_full(next, message, None, None)
    }

    fn stage_full(
        &mut self,
        mut next: Snapshot,
        message: Option<DurableMessage>,
        publication: Option<(BlockStatement, &DurableCandidate)>,
        next_identity: Option<(ConsensusContext, ParentPoint)>,
    ) -> Result<()> {
        next.revision = self
            .snapshot
            .revision
            .checked_add(1)
            .context("signing revision exhausted")?;
        let saved = match next_identity {
            Some((context, parent)) => {
                codec::encode_snapshot_at(&self.identity, context, parent, &next)?
            }
            None => codec::encode_snapshot(&self.identity, &next)?,
        };
        let outbox = codec::encode_outbox(&next, &saved, message.as_ref())?;
        let publication = publication
            .map(|(statement, candidate)| {
                ChainRecord::new(
                    &statement,
                    candidate.packet(),
                    self.identity.parent,
                    self.identity.validator,
                    next.revision,
                    &outbox,
                )
            })
            .transpose()?;
        // Validate the COMPLETE conditional batch before accepting the action.
        let _ = self.transition(&saved, &outbox, next.revision, publication.as_ref())?;
        self.pending = Some(Pending {
            snapshot: next,
            saved,
            outbox,
            message,
            applying: None,
            publication,
            next_identity,
        });
        Ok(())
    }

    fn transition(
        &self,
        saved: &[u8],
        outbox: &[u8],
        revision: u64,
        publication: Option<&ChainRecord>,
    ) -> Result<MetaTransition> {
        let mut changes = vec![
            MetaChange {
                key: MetaKey::ConsensusState(self.identity.validator),
                expected: self.saved.clone(),
                value: saved.to_vec(),
            },
            MetaChange {
                key: MetaKey::ConsensusOutbox {
                    validator: self.identity.validator,
                    sequence: revision,
                },
                expected: None,
                value: outbox.to_vec(),
            },
        ];
        if let Some(record) = publication {
            changes.push(MetaChange {
                key: MetaKey::ChainHead,
                expected: self.head_bytes.clone(),
                value: record.head_bytes()?,
            });
            changes.push(MetaChange {
                key: MetaKey::ChainBlock {
                    height: record.point().height,
                },
                expected: None,
                value: record.encode()?,
            });
            MetaTransition::new(changes)
        } else {
            MetaTransition::with_guards(
                changes,
                vec![MetaGuard {
                    key: MetaKey::ChainHead,
                    expected: self.head_bytes.clone(),
                }],
            )
        }
    }

    /// None means pending/backpressure. Some(None) is a durable state-only
    /// transition. Some(Some(message)) is the first point signed bytes escape.
    pub fn poll(&mut self, pipeline: &CandidatePipeline) -> Result<Option<Option<DurableMessage>>> {
        ensure!(
            !self.frozen,
            "signing journal frozen; explicit recovery required"
        );
        self.identity.check_owner(pipeline)?;
        let result = self.poll_inner(pipeline);
        if result.is_err() {
            self.frozen = true;
            self.pending.take();
        }
        result
    }

    fn poll_inner(
        &mut self,
        pipeline: &CandidatePipeline,
    ) -> Result<Option<Option<DurableMessage>>> {
        let Some(mut pending) = self.pending.take() else {
            return Ok(None);
        };
        self.submit_pending(&mut pending, pipeline)?;
        let done = pending
            .applying
            .as_mut()
            .map(IoTicket::try_take)
            .transpose()?
            .flatten();
        let Some(done) = done else {
            self.pending = Some(pending);
            return Ok(None);
        };
        ensure!(
            matches!(done, MetaOutcome::Applied | MetaOutcome::AlreadyPresent),
            "durable signing state changed; stale session cannot sign"
        );
        self.snapshot = pending.snapshot;
        self.saved = Some(pending.saved);
        self.last_message = pending.message;
        if let Some(record) = pending.publication {
            self.head_bytes = Some(record.head_bytes()?);
            self.head_record = Some(record);
        }
        if let Some((context, parent)) = pending.next_identity {
            self.identity.context = context;
            self.identity.parent = parent;
        }
        Ok(Some(self.last_message.clone()))
    }

    fn submit_pending(&self, pending: &mut Pending, pipeline: &CandidatePipeline) -> Result<()> {
        if pending.applying.is_none() {
            pending.applying = pipeline.try_apply_consensus_metadata(self.transition(
                &pending.saved,
                &pending.outbox,
                pending.snapshot.revision,
                pending.publication.as_ref(),
            )?)?;
        }
        Ok(())
    }

    /// Deterministic lost-ACK fixture: use the real enqueue path, but leave the
    /// reply unconsumed regardless of how quickly the native write completes.
    #[cfg(test)]
    pub(super) fn submit_pending_without_receiving_for_test(
        &mut self,
        pipeline: &CandidatePipeline,
    ) -> Result<bool> {
        ensure!(!self.frozen, "signing journal frozen");
        self.identity.check_owner(pipeline)?;
        let mut pending = self.pending.take().context("no pending transition")?;
        let result = self.submit_pending(&mut pending, pipeline);
        let accepted = pending.applying.is_some();
        self.pending = Some(pending);
        result?;
        Ok(accepted)
    }
}

fn successor_context(context: ConsensusContext, parent: ParentPoint) -> Result<ConsensusContext> {
    Ok(ConsensusContext {
        height: parent
            .height
            .checked_add(1)
            .context("chain height exhausted")?,
        parent_block_hash: parent.block_hash,
        parent_decision_hash: parent.decision_hash,
        ..context
    })
}
