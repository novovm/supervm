//! Single-height safety transitions, adapted from Tendermint Algorithm 1,
//! https://arxiv.org/pdf/1807.04938 (lines 22-67, 2019 revision).
//!
//! This is NOT a complete Tendermint implementation, pacemaker, vote collector,
//! execution proof, network or finality publication service. In particular the
//! caller owns timeout scheduling/aggregation and independently validates local
//! execution. A value hash alone is not an executed/durable capability.
//!
//! Every method is a pure preparation: persist the expected-state CAS and exact
//! outbox intent BEFORE adopting `next` or releasing a signature. Cloning an old
//! state cannot grant a second signing permission. This module has no keys and
//! never signs. Unlike the archived seal's proposal/first-vote height lock,
//! only a current-round prevote quorum can create or migrate a non-nil lock.

use super::wire::{Context, Hash, Phase, VerifiedProposal, VerifiedQuorum};
use anyhow::{ensure, Context as _, Result};
use serde::{Deserialize, Serialize};

const SNAPSHOT_PREFIX: &[u8; 8] = b"NVRND001";
const MAX_SNAPSHOT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Step {
    Propose,
    Prevote,
    Precommit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CertifiedValue {
    round: u64,
    value: Hash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct State {
    context: Context,
    round: u64,
    step: Step,
    locked: Option<CertifiedValue>,
    valid: Option<CertifiedValue>,
    decided: Option<Hash>,
}

// A decoder alone cannot construct a signing capability. Only the bounded,
// canonical restore entry point creates a structurally valid State, and even
// that must be reconciled with durable statement/certificate/outbox records.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    context: Context,
    round: u64,
    step: Step,
    locked: Option<CertifiedValue>,
    valid: Option<CertifiedValue>,
    decided: Option<Hash>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VoteIntent {
    pub(crate) context: Context,
    pub(crate) round: u64,
    pub(crate) phase: Phase,
    pub(crate) value: Option<Hash>,
}

/// Explicit LOCAL event, not a remotely supplied round-change certificate.
/// The pacemaker must establish when this timer is eligible and has expired;
/// this pure kernel checks only exact context/current round/current step.
pub(crate) struct LocalTimeout {
    pub(crate) context: Context,
    pub(crate) round: u64,
    pub(crate) step: Step,
}

pub(crate) struct PreparedStep {
    next: State,
    intent: Option<VoteIntent>,
    decision: Option<Hash>,
}

impl PreparedStep {
    pub(crate) fn next(&self) -> &State {
        &self.next
    }
    pub(crate) fn intent(&self) -> Option<&VoteIntent> {
        self.intent.as_ref()
    }
    pub(crate) fn decision(&self) -> Option<Hash> {
        self.decision
    }
    pub(crate) fn into_parts(self) -> (State, Option<VoteIntent>, Option<Hash>) {
        (self.next, self.intent, self.decision)
    }
}

impl State {
    pub(crate) fn new(context: Context) -> Result<Self> {
        let state = Self {
            context,
            round: 0,
            step: Step::Propose,
            locked: None,
            valid: None,
            decided: None,
        };
        state.validate()?;
        Ok(state)
    }

    pub(crate) fn context(&self) -> &Context {
        &self.context
    }
    pub(crate) fn round(&self) -> u64 {
        self.round
    }
    pub(crate) fn step(&self) -> Step {
        self.step
    }
    pub(crate) fn locked(&self) -> Option<(u64, Hash)> {
        self.locked.map(|v| (v.round, v.value))
    }
    pub(crate) fn valid(&self) -> Option<(u64, Hash)> {
        self.valid.map(|v| (v.round, v.value))
    }
    pub(crate) fn decided(&self) -> Option<Hash> {
        self.decided
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let snapshot = Snapshot {
            context: self.context,
            round: self.round,
            step: self.step,
            locked: self.locked,
            valid: self.valid,
            decided: self.decided,
        };
        let mut bytes = SNAPSHOT_PREFIX.to_vec();
        bytes.extend_from_slice(&postcard::to_allocvec(&snapshot)?);
        ensure!(
            bytes.len() <= MAX_SNAPSHOT_BYTES,
            "round snapshot exceeds byte bound"
        );
        Ok(bytes)
    }

    /// Structural local recovery only. This checks no durable certificate,
    /// statement, previous state or outbox. The storage/signing wrapper MUST
    /// verify those records before using this snapshot to prepare a signature.
    pub(crate) fn restore(bytes: &[u8], expected_context: &Context) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_SNAPSHOT_BYTES && bytes.starts_with(SNAPSHOT_PREFIX),
            "unsupported or oversized round snapshot"
        );
        let (snapshot, trailing): (Snapshot, _) =
            postcard::take_from_bytes(&bytes[SNAPSHOT_PREFIX.len()..])?;
        ensure!(trailing.is_empty(), "round snapshot has trailing bytes");
        ensure!(
            &snapshot.context == expected_context,
            "round snapshot context mismatch"
        );
        let state = Self {
            context: snapshot.context,
            round: snapshot.round,
            step: snapshot.step,
            locked: snapshot.locked,
            valid: snapshot.valid,
            decided: snapshot.decided,
        };
        state.validate()?;
        ensure!(state.encode()? == bytes, "round snapshot is not canonical");
        Ok(state)
    }

    fn validate(&self) -> Result<()> {
        self.context.validate_shape()?;
        for checkpoint in [self.locked, self.valid].into_iter().flatten() {
            ensure!(
                checkpoint.value != [0; 32] && checkpoint.round <= self.round,
                "invalid certified round/value checkpoint"
            );
            ensure!(
                self.step == Step::Precommit || checkpoint.round < self.round,
                "current-round certified value before precommit step"
            );
        }
        if let Some(locked) = self.locked {
            let valid = self
                .valid
                .context("locked state missing valid checkpoint")?;
            ensure!(
                valid.round >= locked.round
                    && (valid.round != locked.round || valid.value == locked.value),
                "round snapshot valid/locked checkpoints conflict"
            );
        }
        ensure!(
            self.decided != Some([0; 32]),
            "round snapshot has zero decision value"
        );
        Ok(())
    }

    fn active(&self) -> Result<()> {
        self.validate()?;
        ensure!(self.decided.is_none(), "height already decided");
        Ok(())
    }

    fn proposal_value(&self, proposal: &VerifiedProposal, executed_value: Hash) -> Result<Hash> {
        let proposal = proposal.proposal();
        ensure!(
            proposal.context == self.context,
            "proposal context mismatch"
        );
        ensure!(
            executed_value != [0; 32] && proposal.value == executed_value,
            "proposal differs from locally executed value"
        );
        Ok(proposal.value)
    }

    fn quorum(
        &self,
        quorum: &VerifiedQuorum,
        round: u64,
        phase: Phase,
        value: Option<Hash>,
    ) -> Result<()> {
        ensure!(
            quorum.context() == &self.context
                && quorum.round() == round
                && quorum.phase() == phase
                && quorum.value() == value,
            "quorum domain/round/phase/value mismatch"
        );
        Ok(())
    }

    fn prepared(
        &self,
        next: State,
        vote: Option<(Phase, Option<Hash>)>,
        decision: Option<Hash>,
    ) -> Result<PreparedStep> {
        next.validate()?;
        Ok(PreparedStep {
            next,
            intent: vote.map(|(phase, value)| VoteIntent {
                context: self.context,
                round: self.round,
                phase,
                value,
            }),
            decision,
        })
    }

    /// Algorithm 1 lines 22-33: a justification is a verified PREVOTE quorum,
    /// never merely the proposer's claimed valid_round or a precommit message.
    pub(crate) fn prepare_proposal(
        &self,
        proposal: &VerifiedProposal,
        executed_value: Hash,
        justification: Option<&VerifiedQuorum>,
    ) -> Result<PreparedStep> {
        self.active()?;
        ensure!(
            self.step == Step::Propose && proposal.proposal().round == self.round,
            "proposal is stale or prevote was already prepared"
        );
        let value = self.proposal_value(proposal, executed_value)?;
        let valid_round = proposal.proposal().valid_round;
        match (valid_round, justification) {
            (None, None) => {}
            (Some(round), Some(proof)) => {
                ensure!(
                    round < self.round,
                    "proposal valid round must precede current round"
                );
                self.quorum(proof, round, Phase::Prevote, Some(value))?;
                for known in [self.locked, self.valid].into_iter().flatten() {
                    ensure!(
                        known.round != round || known.value == value,
                        "conflicting certified values at the same round"
                    );
                }
            }
            _ => anyhow::bail!("proposal justification missing or unexpected"),
        }
        let accepted = self.locked.is_none_or(|locked| {
            locked.value == value || valid_round.is_some_and(|round| locked.round <= round)
        });
        let mut next = self.clone();
        next.step = Step::Prevote;
        self.prepared(
            next,
            Some((Phase::Prevote, accepted.then_some(value))),
            None,
        )
    }

    /// Algorithm 1 lines 36-46. A late non-nil QC after nil precommit updates
    /// valid, not locked, and produces NO second precommit. Quorum alone never
    /// substitutes for the current proposal's locally checked business value.
    pub(crate) fn prepare_prevote_quorum(
        &self,
        quorum: &VerifiedQuorum,
        proposal: Option<(&VerifiedProposal, Hash)>,
    ) -> Result<PreparedStep> {
        self.active()?;
        ensure!(
            matches!(self.step, Step::Prevote | Step::Precommit),
            "prevote step not reached"
        );
        self.quorum(quorum, self.round, Phase::Prevote, quorum.value())?;
        let mut next = self.clone();
        match quorum.value() {
            None => {
                ensure!(self.step == Step::Prevote, "precommit was already prepared");
                next.step = Step::Precommit;
                self.prepared(next, Some((Phase::Precommit, None)), None)
            }
            Some(value) => {
                let (proposal, executed) =
                    proposal.context("non-nil prevote quorum needs a valid local proposal")?;
                ensure!(
                    proposal.proposal().round == self.round
                        && self.proposal_value(proposal, executed)? == value,
                    "prevote quorum differs from current locally valid proposal"
                );
                ensure!(
                    self.valid.is_none_or(|known| known.round != self.round),
                    "current-round prevote quorum already applied or conflicts"
                );
                let checkpoint = CertifiedValue {
                    round: self.round,
                    value,
                };
                next.valid = Some(checkpoint);
                if self.step == Step::Prevote {
                    next.locked = Some(checkpoint);
                    next.step = Step::Precommit;
                    self.prepared(next, Some((Phase::Precommit, Some(value))), None)
                } else {
                    self.prepared(next, None, None)
                }
            }
        }
    }

    /// Local timeout only. Never clear a lock. No f+1 remote round-jump rule or
    /// timeout scheduling is implemented here; a precommit timeout advances by
    /// exactly one round and is checked against this exact height/round/step.
    pub(crate) fn prepare_timeout(&self, timeout: LocalTimeout) -> Result<PreparedStep> {
        self.active()?;
        ensure!(
            timeout.context == self.context
                && timeout.round == self.round
                && timeout.step == self.step,
            "stale or mismatched local timeout"
        );
        let mut next = self.clone();
        match self.step {
            Step::Propose => {
                next.step = Step::Prevote;
                self.prepared(next, Some((Phase::Prevote, None)), None)
            }
            Step::Prevote => {
                next.step = Step::Precommit;
                self.prepared(next, Some((Phase::Precommit, None)), None)
            }
            Step::Precommit => {
                next.round = self
                    .round
                    .checked_add(1)
                    .context("consensus round exhausted")?;
                next.step = Step::Propose;
                self.prepared(next, None, None)
            }
        }
    }

    /// Algorithm 1 lines 49-54: a non-nil PRECOMMIT certificate plus its exact
    /// locally valid proposal is a decision input. The certificate may arrive
    /// after moving beyond its round; no old-round vote is emitted. This only
    /// records a terminal decision intention, not durable canonical promotion.
    pub(crate) fn prepare_decision(
        &self,
        proposal: &VerifiedProposal,
        executed_value: Hash,
        quorum: &VerifiedQuorum,
    ) -> Result<PreparedStep> {
        self.active()?;
        let value = self.proposal_value(proposal, executed_value)?;
        self.quorum(
            quorum,
            proposal.proposal().round,
            Phase::Precommit,
            Some(value),
        )?;
        let mut next = self.clone();
        next.decided = Some(value);
        self.prepared(next, None, Some(value))
    }
}

#[cfg(test)]
mod tests;
