//! Source: `runtime/novovm-host/src/consensus/collector.rs` at `2da3583c`.
//! Migrated rules/tests; no production signing or publication is activated here.
//!
//! Bounded, single-context collection of real authenticated consensus votes.
//!
//! This is evidence aggregation, not a signer, journal, timer, or authority to
//! change a canonical head. In particular a mixed-value phase threshold only
//! permits the controller to START the corresponding timeout; it is not a QC
//! and does not mean the timeout has expired. Round/context changes are explicit.

use super::wire::{Context, Hash, Phase, Quorum, ValidatorSet, VerifiedQuorum, Vote};
use anyhow::{ensure, Context as _, Result};
use std::{collections::BTreeMap, sync::Arc};

/// Local memory/admission budgets, not negotiated protocol or production values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectorLimits {
    /// Current/past round buckets only; future per-validator tips are separate.
    pub max_retained_rounds: usize,
    pub max_future_round_span: u64,
    /// Logical retained vote entries, including the cached catch-up witness.
    /// Reserves current 2N + future tips 2N + pinned catch-up witness N.
    pub max_votes: usize,
}

impl CollectorLimits {
    fn validate(&self, set: &ValidatorSet) -> Result<()> {
        ensure!(
            self.max_retained_rounds > 0,
            "collector round budget is zero"
        );
        let reserved = set.members().len() * 5;
        let possible_votes = (self.max_retained_rounds - 1)
            .checked_mul(set.members().len())
            .and_then(|count| count.checked_mul(2))
            .and_then(|count| count.checked_add(reserved))
            .context("collector vote budget overflow")?;
        ensure!(
            self.max_votes >= reserved && self.max_votes <= possible_votes,
            "collector vote budget cannot reserve current/future/witness capacity or exceeds retained-round capacity"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteInsert {
    Inserted,
    Duplicate,
}

#[derive(Clone, Default)]
struct RoundVotes {
    prevotes: BTreeMap<Hash, Vote>,
    precommits: BTreeMap<Hash, Vote>,
}

impl RoundVotes {
    fn phase(&self, phase: Phase) -> &BTreeMap<Hash, Vote> {
        match phase {
            Phase::Prevote => &self.prevotes,
            Phase::Precommit => &self.precommits,
        }
    }

    fn phase_mut(&mut self, phase: Phase) -> &mut BTreeMap<Hash, Vote> {
        match phase {
            Phase::Prevote => &mut self.prevotes,
            Phase::Precommit => &mut self.precommits,
        }
    }

    fn count(&self) -> usize {
        self.prevotes.len() + self.precommits.len()
    }
}

/// More than one-third of distinct validator weight has authenticated votes at
/// ONE exact higher round. This recommends catch-up, never signs or updates a
/// durable round. The controller must reconcile the exact context/current round
/// with its journal. Private construction prevents caller-supplied weight claims.
#[derive(Clone, Debug)]
pub struct CatchUpEvidence {
    context: Context,
    round: u64,
    signed_weight: u64,
    votes: Vec<Vote>,
}

impl CatchUpEvidence {
    pub fn context(&self) -> &Context {
        &self.context
    }
    pub fn round(&self) -> u64 {
        self.round
    }
    pub fn signed_weight(&self) -> u64 {
        self.signed_weight
    }
    /// One vote per validator at one exact round; phases may differ. NOT a QC.
    pub fn votes(&self) -> &[Vote] {
        &self.votes
    }
}

pub struct VoteCollector {
    context: Context,
    set: Arc<ValidatorSet>,
    current_round: u64,
    first_round: u64,
    limits: CollectorLimits,
    rounds: BTreeMap<u64, RoundVotes>,
    future_tips: BTreeMap<(Hash, u8), Vote>,
    catch_up: Option<CatchUpEvidence>,
    vote_count: usize,
}

impl VoteCollector {
    /// `context` must come from the caller's verified local journal/head, not a
    /// network vote. Shape/set validation does not establish that authority.
    pub fn new(
        context: Context,
        set: Arc<ValidatorSet>,
        current_round: u64,
        limits: CollectorLimits,
    ) -> Result<Self> {
        context.validate(&set)?;
        limits.validate(&set)?;
        Ok(Self {
            context,
            set,
            current_round,
            first_round: current_round,
            limits,
            rounds: BTreeMap::new(),
            future_tips: BTreeMap::new(),
            catch_up: None,
            vote_count: 0,
        })
    }

    pub fn context(&self) -> &Context {
        &self.context
    }
    pub fn current_round(&self) -> u64 {
        self.current_round
    }
    pub fn lowest_retained_round(&self) -> u64 {
        self.first_round
    }
    pub fn vote_count(&self) -> usize {
        self.vote_count
    }
    pub fn retained_round_count(&self) -> usize {
        self.rounds.len()
    }
    pub fn future_tip_count(&self) -> usize {
        self.future_tips.len()
    }

    fn tip_key(vote: &Vote) -> (Hash, u8) {
        (vote.validator_id, u8::from(vote.phase == Phase::Precommit))
    }

    fn recount(&mut self) {
        self.vote_count = self.rounds.values().map(RoundVotes::count).sum::<usize>()
            + self.future_tips.len()
            + self.catch_up.as_ref().map_or(0, |proof| proof.votes.len());
    }

    /// Accept only this exact local context. Every accepted input, including a
    /// duplicate and a Vote extracted from someone else's VerifiedVote, is
    /// actually verified against OUR immutable set. No unchecked-weight entry
    /// point exists. Budget failures leave all existing evidence untouched.
    pub fn insert(&mut self, vote: &Vote) -> Result<VoteInsert> {
        ensure!(
            vote.context == self.context,
            "collector vote context mismatch"
        );
        ensure!(
            vote.round >= self.first_round,
            "collector vote round was retired"
        );
        ensure!(
            vote.round <= self.current_round
                || vote.round - self.current_round <= self.limits.max_future_round_span,
            "collector vote exceeds future-round span"
        );
        vote.verify(&self.set)?;
        let known = self
            .rounds
            .get(&vote.round)
            .and_then(|round| round.phase(vote.phase).get(&vote.validator_id))
            .or_else(|| {
                self.catch_up.as_ref().and_then(|proof| {
                    proof.votes.iter().find(|stored| {
                        stored.round == vote.round
                            && stored.phase == vote.phase
                            && stored.validator_id == vote.validator_id
                    })
                })
            })
            .or_else(|| {
                self.future_tips
                    .get(&Self::tip_key(vote))
                    .filter(|stored| stored.round == vote.round)
            });
        if let Some(existing) = known {
            ensure!(
                existing.value == vote.value,
                "collector validator equivocation"
            );
            return Ok(VoteInsert::Duplicate);
        }
        if vote.round > self.current_round {
            let key = Self::tip_key(vote);
            ensure!(
                self.future_tips
                    .get(&key)
                    .is_none_or(|old| vote.round > old.round),
                "collector stale future tip"
            );
            // Each validator owns two independent phase slots. Advancing one
            // tip explicitly replaces only that signer's older fragment; no
            // validator can consume another validator's future admission.
            self.future_tips.insert(key, vote.clone());
            self.refresh_catch_up();
            self.recount();
            return Ok(VoteInsert::Inserted);
        }
        ensure!(
            self.vote_count < self.limits.max_votes,
            "collector vote budget exhausted"
        );
        if vote.round != self.current_round {
            let current_count = self
                .rounds
                .get(&self.current_round)
                .map_or(0, RoundVotes::count);
            ensure!(
                self.rounds.values().map(RoundVotes::count).sum::<usize>() - current_count
                    < self.extra_vote_capacity(),
                "collector non-current vote budget exhausted; current round reserved"
            );
            let other_rounds =
                self.rounds.len() - usize::from(self.rounds.contains_key(&self.current_round));
            ensure!(
                self.rounds.contains_key(&vote.round)
                    || other_rounds < self.limits.max_retained_rounds - 1,
                "collector retained-round budget exhausted; current round reserved"
            );
        }
        ensure!(
            self.rounds.contains_key(&vote.round)
                || self.rounds.len() < self.limits.max_retained_rounds,
            "collector retained-round budget exhausted"
        );
        self.rounds
            .entry(vote.round)
            .or_default()
            .phase_mut(vote.phase)
            .insert(vote.validator_id, vote.clone());
        self.recount();
        Ok(VoteInsert::Inserted)
    }

    fn weight(&self, id: &Hash) -> u64 {
        self.set
            .member(id)
            .expect("only verified set members are stored")
            .weight()
    }

    fn extra_vote_capacity(&self) -> usize {
        self.limits.max_votes - self.set.members().len() * 5
    }

    fn future_phase_votes(&self, round: u64, phase: Phase) -> BTreeMap<Hash, &Vote> {
        let mut votes = BTreeMap::new();
        for vote in self
            .future_tips
            .values()
            .chain(self.catch_up.iter().flat_map(|proof| proof.votes.iter()))
        {
            if vote.round == round && vote.phase == phase {
                votes.entry(vote.validator_id).or_insert(vote);
            }
        }
        votes
    }

    /// Unique signers in ONE exact round/phase, independent of their values.
    /// Safe to sum: every map is unique and the set's total was checked at creation.
    pub fn phase_weight(&self, round: u64, phase: Phase) -> u64 {
        if round > self.current_round {
            return self
                .future_phase_votes(round, phase)
                .keys()
                .map(|id| self.weight(id))
                .sum();
        }
        self.rounds.get(&round).map_or(0, |votes| {
            votes.phase(phase).keys().map(|id| self.weight(id)).sum()
        })
    }

    /// Strictly more than two-thirds in the CURRENT round and exact phase.
    /// This starts eligibility only, not an elapsed-time assertion or nil vote.
    pub fn timeout_eligible(&self, round: u64, phase: Phase) -> bool {
        round == self.current_round && self.phase_weight(round, phase) >= self.set.quorum_weight()
    }

    /// Only same-context/round/phase/VALUE votes can produce a real certificate.
    /// Includes nil certificates; a nil certificate never proves a block.
    pub fn quorum(
        &self,
        round: u64,
        phase: Phase,
        value: Option<Hash>,
    ) -> Result<Option<VerifiedQuorum>> {
        if round > self.current_round {
            let votes = self.future_phase_votes(round, phase);
            let weight: u64 = votes
                .values()
                .filter(|vote| vote.value == value)
                .map(|vote| self.weight(&vote.validator_id))
                .sum();
            if weight < self.set.quorum_weight() {
                return Ok(None);
            }
            return Ok(Some(
                Quorum {
                    votes: votes
                        .into_values()
                        .filter(|vote| vote.value == value)
                        .cloned()
                        .collect(),
                }
                .verify(&self.set)?,
            ));
        }
        let Some(round) = self.rounds.get(&round) else {
            return Ok(None);
        };
        let matching = || {
            round
                .phase(phase)
                .values()
                .filter(|vote| vote.value == value)
        };
        let weight: u64 = matching().map(|vote| self.weight(&vote.validator_id)).sum();
        if weight < self.set.quorum_weight() {
            return Ok(None);
        }
        // Waiting for a quorum does not repeatedly allocate/copy partial votes.
        let votes = matching().cloned().collect();
        // BTreeMap iteration is the wire's strict sorted-validator order. The
        // public verification boundary checks signatures and context again.
        Ok(Some(Quorum { votes }.verify(&self.set)?))
    }

    /// Highest future round supported by >1/3 DISTINCT validator weight in that
    /// ONE exact round. The Algorithm 1 lines 55-56 higher-round rule must not
    /// aggregate fragments from different rounds. Prevote/precommit are deduped
    /// by signer. Proposals are not counted by this vote-only collector.
    pub fn catch_up(&self) -> Option<CatchUpEvidence> {
        self.catch_up.clone()
    }

    fn refresh_catch_up(&mut self) {
        let threshold = self.set.total_weight() / 3 + 1;
        let mut by_round: BTreeMap<u64, BTreeMap<Hash, &Vote>> = BTreeMap::new();
        for vote in self.future_tips.values() {
            by_round
                .entry(vote.round)
                .or_default()
                .entry(vote.validator_id)
                .or_insert(vote);
        }
        for (round, unique) in by_round.into_iter().rev() {
            let weight = unique.keys().map(|id| self.weight(id)).sum();
            if weight >= threshold && self.catch_up.as_ref().is_none_or(|old| round > old.round) {
                self.catch_up = Some(CatchUpEvidence {
                    context: self.context,
                    round,
                    signed_weight: weight,
                    votes: unique.into_values().cloned().collect(),
                });
                break;
            }
        }
    }

    /// Reflect an independently accepted local round transition. Never inferred
    /// from a single peer, never emits a vote, never drops old evidence silently.
    pub fn advance_round(&mut self, new_round: u64) -> Result<()> {
        self.advance_round_and_retire(new_round, self.first_round)
    }

    /// Explicit, atomic advancement plus caller-selected retirement. Required
    /// when keeping the whole old current round would exhaust the next round's
    /// reserved capacity. The caller must save needed QC/valid evidence first;
    /// an invalid request changes neither the round nor retained evidence.
    pub fn advance_round_and_retire(&mut self, new_round: u64, first_round: u64) -> Result<()> {
        ensure!(
            new_round >= self.current_round,
            "collector round cannot go backwards"
        );
        ensure!(
            first_round >= self.first_round && first_round <= new_round,
            "collector retirement is backwards or beyond new round"
        );
        let mut retained: BTreeMap<u64, RoundVotes> = self
            .rounds
            .iter()
            .filter(|(round, _)| **round >= first_round)
            .map(|(round, votes)| (*round, votes.clone()))
            .collect();
        let mut future = BTreeMap::new();
        for vote in self
            .future_tips
            .values()
            .chain(self.catch_up.iter().flat_map(|proof| proof.votes.iter()))
        {
            if vote.round < first_round {
                continue;
            }
            if vote.round <= new_round {
                let phase = retained
                    .entry(vote.round)
                    .or_default()
                    .phase_mut(vote.phase);
                if let Some(old) = phase.get(&vote.validator_id) {
                    ensure!(
                        old.value == vote.value,
                        "collector promotion found equivocation"
                    );
                } else {
                    phase.insert(vote.validator_id, vote.clone());
                }
            }
        }
        for (key, vote) in &self.future_tips {
            if vote.round > new_round {
                future.insert(*key, vote.clone());
            }
        }
        let mut other_votes = 0usize;
        let mut other_rounds = 0usize;
        for (round, votes) in &retained {
            if *round >= first_round && *round != new_round {
                other_votes += votes.count();
                other_rounds += 1;
            }
        }
        ensure!(
            other_votes <= self.extra_vote_capacity()
                && other_rounds < self.limits.max_retained_rounds,
            "collector advancement needs explicit retirement to reserve current round"
        );
        self.rounds = retained;
        self.future_tips = future;
        if self
            .catch_up
            .as_ref()
            .is_some_and(|proof| proof.round <= new_round)
        {
            self.catch_up = None;
        }
        self.first_round = first_round;
        self.current_round = new_round;
        self.refresh_catch_up();
        self.recount();
        Ok(())
    }

    /// Explicit retirement. Caller must preserve any still-needed valid-round or
    /// decision certificate elsewhere before discarding old collection buckets.
    /// Retired rounds cannot be inserted again, including equivocations.
    pub fn retire_before(&mut self, first_round: u64) -> Result<()> {
        ensure!(
            first_round >= self.first_round && first_round <= self.current_round,
            "collector retirement is backwards or beyond current round"
        );
        self.rounds.retain(|round, _| *round >= first_round);
        self.first_round = first_round;
        self.recount();
        Ok(())
    }

    /// Replace with a strictly higher, caller-verified local context in the SAME
    /// fixed chain/genesis/protocol/set. No implicit parent validation is claimed;
    /// the durable journal/head owns that check. Epoch/set changes need a new
    /// collector and their own authorized validator transition.
    pub fn reset_context(&mut self, next: Context, current_round: u64) -> Result<()> {
        next.validate(&self.set)?;
        ensure!(
            next.height > self.context.height
                && next.genesis_config_commitment == self.context.genesis_config_commitment
                && next.protocol_commitment == self.context.protocol_commitment,
            "collector context must advance height within the fixed chain domain"
        );
        self.context = next;
        self.current_round = current_round;
        self.first_round = current_round;
        self.rounds.clear();
        self.future_tips.clear();
        self.catch_up = None;
        self.vote_count = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
