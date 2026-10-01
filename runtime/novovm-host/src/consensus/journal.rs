//! One pinned-height durable signing session over the SAME AOEM I/O owner.
//! All messages stay private until safety state + exact outbox bytes have been
//! atomically persisted and read back. This is not a canonical head manager,
//! network reactor or complete pacemaker. Opening pins operator-supplied parent
//! authority; a node must not reuse this session after changing that authority.

mod codec;

use super::round::{LocalTimeout, PreparedStep, State, Step};
use super::statement::{BlockStatement, ParentPoint};
use super::wire::{
    self, Context as ConsensusContext, Hash, Proposal, Quorum, Validator, ValidatorSet,
    VerifiedProposal, VerifiedQuorum, Vote,
};
use crate::persistence::io::IoTicket;
use crate::persistence::metadata::{
    MetaChange, MetaKey, MetaOutcome, MetaTransition, MetadataSnapshot,
};
use crate::pipeline::{CandidatePipeline, DurableCandidate};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use std::sync::Arc;

/// Durable protocol evidence, not a published block or execution validity proof.
#[derive(Clone, Debug)]
pub enum DurableMessage {
    Proposal(Proposal),
    Vote(Vote),
    Decision {
        proposal: Proposal,
        certificate: Quorum,
    },
}

#[derive(Clone, Copy, Debug)]
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

/// Nonblocking startup. A missing snapshot is fresh ONLY when its first outbox
/// slot is also absent. Existing state never silently resets after corruption.
pub struct JournalOpening {
    identity: Option<Identity>,
    initial: Option<IoTicket<MetadataSnapshot>>,
    recovered: Option<(Snapshot, Vec<u8>)>,
    last: Option<IoTicket<MetadataSnapshot>>,
    failed: bool,
}

impl JournalOpening {
    pub fn poll(&mut self, pipeline: &CandidatePipeline) -> Result<Option<ValidatorJournal>> {
        ensure!(
            !self.failed,
            "signing journal startup failed; explicit recovery required"
        );
        let result = self.poll_inner(pipeline);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn poll_inner(&mut self, pipeline: &CandidatePipeline) -> Result<Option<ValidatorJournal>> {
        let identity = self
            .identity
            .as_ref()
            .context("journal startup already consumed")?;
        identity.check_owner(pipeline)?;
        if let Some((snapshot, saved)) = self.recovered.take() {
            if self.last.is_none() {
                self.last =
                    pipeline.try_read_consensus_metadata(vec![MetaKey::ConsensusOutbox {
                        validator: identity.validator,
                        sequence: snapshot.revision,
                    }])?;
            }
            let result = self
                .last
                .as_mut()
                .map(IoTicket::try_take)
                .transpose()?
                .flatten();
            let Some(result) = result else {
                self.recovered = Some((snapshot, saved));
                return Ok(None);
            };
            ensure!(
                result.values.len() == 1,
                "journal outbox reply count mismatch"
            );
            let last = result.values[0]
                .as_deref()
                .context("durable signing outbox missing")?;
            let message = codec::decode_outbox(identity, &snapshot, &saved, last)?;
            return Ok(Some(ValidatorJournal {
                identity: self.identity.take().unwrap(),
                snapshot,
                saved: Some(saved),
                last_message: message,
                pending: None,
                frozen: false,
            }));
        }
        if self.initial.is_none() {
            self.initial = pipeline.try_read_consensus_metadata(vec![
                MetaKey::ConsensusState(identity.validator),
                MetaKey::ConsensusOutbox {
                    validator: identity.validator,
                    sequence: 1,
                },
            ])?;
        }
        let result = self
            .initial
            .as_mut()
            .map(IoTicket::try_take)
            .transpose()?
            .flatten();
        let Some(result) = result else {
            return Ok(None);
        };
        ensure!(
            result.values.len() == 2,
            "journal startup reply count mismatch"
        );
        self.initial.take();
        match result.values[0].as_ref() {
            None => {
                ensure!(
                    result.values[1].is_none(),
                    "signing snapshot disappeared; refuse reset"
                );
                let snapshot = Snapshot {
                    state: State::new(identity.context)?,
                    revision: 0,
                    proposed: None,
                    witness: None,
                };
                Ok(Some(ValidatorJournal {
                    identity: self.identity.take().unwrap(),
                    snapshot,
                    saved: None,
                    last_message: None,
                    pending: None,
                    frozen: false,
                }))
            }
            Some(bytes) => {
                ensure!(
                    result.values[1].is_some(),
                    "initial signing outbox disappeared"
                );
                let snapshot = codec::decode_snapshot(identity, bytes)?;
                self.recovered = Some((snapshot, bytes.clone()));
                Ok(None)
            }
        }
    }
}

struct Pending {
    snapshot: Snapshot,
    saved: Vec<u8>,
    outbox: Vec<u8>,
    message: Option<DurableMessage>,
    applying: Option<IoTicket<MetaOutcome>>,
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
}

impl ValidatorJournal {
    /// Explicit local configuration, not production genesis creation. The
    /// configured parent must come from independently verified chain authority.
    pub fn open(
        pipeline: &CandidatePipeline,
        context: ConsensusContext,
        parent: ParentPoint,
        set: Arc<ValidatorSet>,
        key: SigningKey,
    ) -> Result<JournalOpening> {
        context.validate(&set)?;
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
        Ok(JournalOpening {
            identity: Some(Identity {
                owner: pipeline.owner_identity(),
                context,
                parent,
                set,
                key,
                validator,
            }),
            initial: None,
            recovered: None,
            last: None,
            failed: false,
        })
    }

    pub fn round(&self) -> u64 {
        self.snapshot.state.round()
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

    /// Archive a locally checked decision, without changing the canonical head.
    pub fn observe_decision(
        &mut self,
        proposal: &VerifiedProposal,
        candidate: &DurableCandidate,
        certificate: &VerifiedQuorum,
    ) -> Result<()> {
        self.ready()?;
        let value = self.identity.statement(candidate)?.hash();
        let step = self
            .snapshot
            .state
            .prepare_decision(proposal, value, certificate)?;
        self.stage_step(
            step,
            Some(DurableMessage::Decision {
                proposal: proposal.proposal().clone(),
                certificate: certificate.quorum().clone(),
            }),
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

    fn stage(&mut self, mut next: Snapshot, message: Option<DurableMessage>) -> Result<()> {
        next.revision = self
            .snapshot
            .revision
            .checked_add(1)
            .context("signing revision exhausted")?;
        let saved = codec::encode_snapshot(&self.identity, &next)?;
        let outbox = codec::encode_outbox(&next, &saved, message.as_ref())?;
        // Validate the COMPLETE conditional batch before accepting the action.
        let _ = self.transition(&saved, &outbox, next.revision)?;
        self.pending = Some(Pending {
            snapshot: next,
            saved,
            outbox,
            message,
            applying: None,
        });
        Ok(())
    }

    fn transition(&self, saved: &[u8], outbox: &[u8], revision: u64) -> Result<MetaTransition> {
        MetaTransition::new(vec![
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
        ])
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
        Ok(Some(self.last_message.clone()))
    }

    fn submit_pending(&self, pending: &mut Pending, pipeline: &CandidatePipeline) -> Result<()> {
        if pending.applying.is_none() {
            pending.applying = pipeline.try_apply_consensus_metadata(self.transition(
                &pending.saved,
                &pending.outbox,
                pending.snapshot.revision,
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
