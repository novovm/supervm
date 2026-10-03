//! Local monotonic, quorum-qualified timers for the durable round journal.
//! Adapted from Algorithm 1 (lines 34/47/55/57-67), 2019 revision:
//! https://arxiv.org/pdf/1807.04938 . This does NOT run a network, assemble
//! proposals, prove business execution or choose production block intervals.
//! Successful proposals/QCs need not wait for these FAILURE deadlines.
//!
//! Timers may stage at most one transition per poll. The journal alone owns
//! signing and persist-before-emit; the caller continues polling its AOEM I/O
//! alongside network/compute work. A phase threshold across different values
//! starts a wait; only a same-value QC can authorize a value precommit/decision.

use super::{
    collector::VoteCollector,
    journal::{JournalBackend, TimeoutStep, ValidatorJournal},
    wire::{Context, Phase},
};
use anyhow::{ensure, Context as _, Result};
use std::time::{Duration, Instant};

/// Explicit local operating choices, never a default mainnet timing policy.
#[derive(Clone, Copy, Debug)]
pub struct TimeoutPolicy {
    pub propose: Duration,
    pub prevote: Duration,
    pub precommit: Duration,
    pub round_increment: Duration,
}

impl TimeoutPolicy {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.propose.is_zero()
                && !self.prevote.is_zero()
                && !self.precommit.is_zero()
                && !self.round_increment.is_zero(),
            "zero consensus timeout"
        );
        Ok(())
    }
    fn duration(&self, step: TimeoutStep, round: u64) -> Result<Duration> {
        let base = match step {
            TimeoutStep::Propose => self.propose,
            TimeoutStep::Prevote => self.prevote,
            TimeoutStep::Precommit => self.precommit,
        };
        let nanos = self
            .round_increment
            .as_nanos()
            .checked_mul(u128::from(round))
            .and_then(|increment| base.as_nanos().checked_add(increment))
            .context("consensus timeout overflow")?;
        Ok(Duration::new(
            u64::try_from(nanos / 1_000_000_000).context("consensus timeout seconds overflow")?,
            (nanos % 1_000_000_000) as u32,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerAction {
    Idle,
    Pending,
    Decided,
    CatchUp(u64),
    Timeout(TimeoutStep),
}

struct Timers {
    context: Context,
    round: u64,
    propose: Instant,
    prevote: Option<Instant>,
    precommit: Option<Instant>,
}

impl Timers {
    fn new(context: Context, round: u64, now: Instant, policy: TimeoutPolicy) -> Result<Self> {
        Ok(Self {
            context,
            round,
            propose: deadline(now, policy.duration(TimeoutStep::Propose, round)?)?,
            prevote: None,
            precommit: None,
        })
    }

    fn observe(
        &mut self,
        now: Instant,
        step: TimeoutStep,
        prevotes: bool,
        precommits: bool,
        policy: TimeoutPolicy,
    ) -> Result<()> {
        if step == TimeoutStep::Prevote && prevotes && self.prevote.is_none() {
            self.prevote = Some(deadline(
                now,
                policy.duration(TimeoutStep::Prevote, self.round)?,
            )?);
        }
        // Can start before our proposal/prevote arrives; does not require a
        // locally executed value and cannot authorize any non-nil signature.
        if precommits && self.precommit.is_none() {
            self.precommit = Some(deadline(
                now,
                policy.duration(TimeoutStep::Precommit, self.round)?,
            )?);
        }
        Ok(())
    }

    fn elapsed(&self, now: Instant, step: TimeoutStep) -> Option<TimeoutStep> {
        if self.precommit.is_some_and(|at| now >= at) {
            return Some(TimeoutStep::Precommit);
        }
        match step {
            TimeoutStep::Propose if now >= self.propose => Some(TimeoutStep::Propose),
            TimeoutStep::Prevote if self.prevote.is_some_and(|at| now >= at) => {
                Some(TimeoutStep::Prevote)
            }
            _ => None,
        }
    }
}

fn deadline(now: Instant, duration: Duration) -> Result<Instant> {
    now.checked_add(duration)
        .context("monotonic consensus deadline overflow")
}

pub struct Pacemaker {
    policy: TimeoutPolicy,
    timers: Option<Timers>,
    last_now: Option<Instant>,
}

impl Pacemaker {
    pub fn new(policy: TimeoutPolicy) -> Result<Self> {
        policy.validate()?;
        Ok(Self {
            policy,
            timers: None,
            last_now: None,
        })
    }

    /// Call after processing eligible proposal/QC/decision inputs so a ready
    /// decision is not intentionally delayed by a local failure deadline.
    /// `now` must be the local monotonic Instant, never a peer timestamp.
    /// The collector must have caught up with ACKNOWLEDGED journal state.
    pub fn poll<B: JournalBackend>(
        &mut self,
        journal: &mut ValidatorJournal<B>,
        votes: &VoteCollector,
        now: Instant,
    ) -> Result<TimerAction> {
        ensure!(
            self.last_now.is_none_or(|last| now >= last),
            "pacemaker clock moved backwards"
        );
        self.last_now = Some(now);
        ensure!(!journal.is_frozen(), "pacemaker journal frozen");
        if journal.decided().is_some() {
            return Ok(TimerAction::Decided);
        }
        ensure!(
            votes.context() == &journal.context() && votes.current_round() == journal.round(),
            "pacemaker collector differs from acknowledged journal"
        );
        if self.timers.as_ref().is_none_or(|timers| {
            timers.context != journal.context() || timers.round != journal.round()
        }) {
            self.timers = Some(Timers::new(
                journal.context(),
                journal.round(),
                now,
                self.policy,
            )?);
        }
        let timers = self.timers.as_mut().context("pacemaker timers missing")?;
        timers.observe(
            now,
            journal.step(),
            votes.timeout_eligible(journal.round(), Phase::Prevote),
            votes.timeout_eligible(journal.round(), Phase::Precommit),
            self.policy,
        )?;
        if journal.is_pending() {
            return Ok(TimerAction::Pending);
        }
        if let Some(evidence) = votes.catch_up() {
            journal.catch_up(&evidence)?;
            return Ok(TimerAction::CatchUp(evidence.round()));
        }
        let Some(step) = timers.elapsed(now, journal.step()) else {
            return Ok(TimerAction::Idle);
        };
        match step {
            TimeoutStep::Precommit => journal.round_wait_elapsed(journal.round())?,
            _ => journal.timeout(journal.round(), step)?,
        }
        Ok(TimerAction::Timeout(step))
    }
}

#[cfg(test)]
mod tests;
