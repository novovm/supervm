//! A bounded scheduling hint for already-selected transactions, never a cached
//! body, authenticated parent, signing permission or finality decision.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ProposalContext {
    pub parent_workspace_id: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub authority_commitment: [u8; 32],
    pub height: u64,
    pub round: u64,
}

struct Active {
    context: ProposalContext,
    deadline: Instant,
    last_seen: Instant,
}

#[derive(Default)]
pub(super) struct ProposalWindow {
    active: Option<Active>,
    waiting: bool,
    current_selected_count: usize,
    wait_polls: u64,
    full_decisions: u64,
    deadline_decisions: u64,
    immediate_decisions: u64,
}

impl ProposalWindow {
    /// The caller freshly selects and validates on every poll, and clears this
    /// hint on lost eligibility, handoff, creation or halt. Ready does not consume
    /// the window: only the owner knows whether proposal creation succeeded.
    pub(super) fn ready(
        &mut self,
        context: ProposalContext,
        selected: usize,
        target: usize,
        now: Instant,
        delay: Duration,
    ) -> Result<bool> {
        if target == 0 || selected > target {
            bail!("proposal window requires a nonzero target and selected <= target");
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| now < active.last_seen)
        {
            bail!("proposal window monotonic clock moved backwards");
        }
        self.current_selected_count = selected;
        self.waiting = false;
        if selected == 0 {
            self.clear();
            return Ok(false);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.context != context)
        {
            self.active = None;
        }
        if delay.is_zero() {
            self.active = None;
            self.immediate_decisions = self.immediate_decisions.saturating_add(1);
            return Ok(true);
        }
        if selected == target {
            if let Some(active) = &mut self.active {
                active.last_seen = now;
            }
            self.full_decisions = self.full_decisions.saturating_add(1);
            return Ok(true);
        }
        if self.active.is_none() {
            self.active = Some(Active {
                context,
                deadline: now
                    .checked_add(delay)
                    .context("proposal window deadline overflow")?,
                last_seen: now,
            });
        }
        let active = self
            .active
            .as_mut()
            .expect("partial proposal window initialized");
        active.last_seen = now;
        if now >= active.deadline {
            self.deadline_decisions = self.deadline_decisions.saturating_add(1);
            Ok(true)
        } else {
            self.waiting = true;
            self.wait_polls = self.wait_polls.saturating_add(1);
            Ok(false)
        }
    }

    pub(super) fn clear(&mut self) {
        self.active = None;
        self.waiting = false;
        self.current_selected_count = 0;
    }

    pub(super) fn status_json(&self) -> Value {
        json!({
            "waiting":self.waiting,"current_selected_count":self.current_selected_count,
            "wait_polls":self.wait_polls,"full_decisions":self.full_decisions,
            "deadline_decisions":self.deadline_decisions,"immediate_decisions":self.immediate_decisions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ProposalContext {
        ProposalContext {
            parent_workspace_id: [1; 32],
            parent_block_hash: [2; 32],
            authority_commitment: [3; 32],
            height: 4,
            round: 0,
        }
    }

    #[test]
    fn zero_delay_preserves_immediate_behavior_but_empty_selection_never_proposes() {
        let mut window = ProposalWindow::default();
        let now = Instant::now();
        assert!(!window.ready(context(), 0, 32, now, Duration::ZERO).unwrap());
        assert!(window.ready(context(), 1, 32, now, Duration::ZERO).unwrap());
        assert!(window
            .ready(context(), 32, 32, now, Duration::ZERO)
            .unwrap());
        assert!(window.active.is_none());
        assert_eq!(window.status_json()["immediate_decisions"], 2);
        assert_eq!(window.status_json()["full_decisions"], 0);
        assert_eq!(window.status_json()["waiting"], false);
    }

    #[test]
    fn exact_deadline_uses_current_count_without_consuming_or_restarting_the_window() {
        let mut window = ProposalWindow::default();
        let now = Instant::now();
        let delay = Duration::from_millis(1000);
        assert!(!window.ready(context(), 3, 32, now, delay).unwrap());
        assert!(!window
            .ready(
                context(),
                7,
                32,
                now + delay - Duration::from_nanos(1),
                delay
            )
            .unwrap());
        assert!(window.ready(context(), 2, 32, now + delay, delay).unwrap());
        assert_eq!(window.status_json()["current_selected_count"], 2);
        assert_eq!(window.status_json()["waiting"], false);
        assert!(window.ready(context(), 1, 32, now + delay, delay).unwrap());
        assert_eq!(window.status_json()["deadline_decisions"], 2);
        assert_eq!(window.status_json()["wait_polls"], 2);
    }

    #[test]
    fn full_selection_is_immediate_and_does_not_replace_an_existing_deadline() {
        let mut window = ProposalWindow::default();
        let now = Instant::now();
        let delay = Duration::from_secs(1);
        assert!(!window.ready(context(), 1, 32, now, delay).unwrap());
        assert!(window
            .ready(context(), 32, 32, now + Duration::from_millis(1), delay)
            .unwrap());
        assert_eq!(window.status_json()["full_decisions"], 1);
        assert!(!window
            .ready(context(), 2, 32, now + Duration::from_millis(2), delay)
            .unwrap());
        assert!(window.ready(context(), 2, 32, now + delay, delay).unwrap());
        let mut fresh = ProposalWindow::default();
        assert!(fresh.ready(context(), 32, 32, now, delay).unwrap());
        assert_eq!(fresh.status_json()["full_decisions"], 1);
    }

    #[test]
    fn sustained_partial_arrivals_and_count_decreases_never_extend_deadline() {
        let mut window = ProposalWindow::default();
        let now = Instant::now();
        let delay = Duration::from_secs(1);
        for tick in 0..40 {
            assert!(!window
                .ready(
                    context(),
                    tick % 7 + 1,
                    32,
                    now + Duration::from_millis(tick as u64 * 25),
                    delay
                )
                .unwrap());
        }
        assert!(window.ready(context(), 4, 32, now + delay, delay).unwrap());
        assert_eq!(window.status_json()["wait_polls"], 40);
    }

    #[test]
    fn each_context_field_creates_a_fresh_window() {
        let now = Instant::now();
        let delay = Duration::from_secs(1);
        for field in 0..5 {
            let mut window = ProposalWindow::default();
            assert!(!window.ready(context(), 1, 32, now, delay).unwrap());
            let mut changed = context();
            match field {
                0 => changed.parent_workspace_id[0] ^= 1,
                1 => changed.parent_block_hash[0] ^= 1,
                2 => changed.authority_commitment[0] ^= 1,
                3 => changed.height += 1,
                _ => changed.round += 1,
            }
            let shifted = now + Duration::from_millis(900);
            assert!(!window.ready(changed, 1, 32, shifted, delay).unwrap());
            assert!(!window.ready(changed, 1, 32, now + delay, delay).unwrap());
            assert!(window
                .ready(changed, 1, 32, shifted + delay, delay)
                .unwrap());
        }
    }

    #[test]
    fn no_eligible_selection_clear_and_restart_discard_only_active_scheduling_state() {
        let mut window = ProposalWindow::default();
        let now = Instant::now();
        let delay = Duration::from_secs(1);
        assert!(!window.ready(context(), 0, 32, now, delay).unwrap());
        assert!(window.active.is_none());
        assert!(!window.ready(context(), 1, 32, now, delay).unwrap());
        assert!(!window.ready(context(), 0, 32, now + delay, delay).unwrap());
        assert!(window.active.is_none());
        assert_eq!(window.status_json()["current_selected_count"], 0);
        assert!(!window.ready(context(), 1, 32, now + delay, delay).unwrap());
        window.clear();
        assert_eq!(window.status_json()["waiting"], false);
        assert_eq!(window.status_json()["wait_polls"], 2);
        assert!(!window
            .ready(context(), 1, 32, now + delay + delay, delay)
            .unwrap());
        let restarted = ProposalWindow::default();
        assert!(restarted.active.is_none());
        assert_eq!(restarted.status_json()["wait_polls"], 0);
        assert_eq!(restarted.status_json()["current_selected_count"], 0);
    }

    #[test]
    fn invalid_counts_backwards_active_clock_and_deadline_overflow_fail_closed() {
        let mut window = ProposalWindow::default();
        let now = Instant::now();
        let delay = Duration::from_secs(1);
        assert!(window.ready(context(), 0, 0, now, delay).is_err());
        assert!(window.ready(context(), 33, 32, now, delay).is_err());
        assert!(window.ready(context(), 1, 32, now, Duration::MAX).is_err());
        assert!(window.active.is_none());
        let started = now + delay;
        assert!(!window.ready(context(), 1, 32, started, delay).unwrap());
        for selected in [0, 1, 32] {
            assert!(window.ready(context(), selected, 32, now, delay).is_err());
        }
        let mut changed = context();
        changed.round += 1;
        assert!(window.ready(changed, 1, 32, now, delay).is_err());
        assert!(window
            .ready(context(), 1, 32, started + delay, delay)
            .unwrap());
        assert_eq!(window.status_json()["full_decisions"], 0);
    }

    #[test]
    fn diagnostic_counters_saturate() {
        let mut window = ProposalWindow {
            wait_polls: u64::MAX,
            full_decisions: u64::MAX,
            deadline_decisions: u64::MAX,
            immediate_decisions: u64::MAX,
            ..ProposalWindow::default()
        };
        let now = Instant::now();
        let delay = Duration::from_secs(1);
        assert!(!window.ready(context(), 1, 32, now, delay).unwrap());
        assert!(window.ready(context(), 32, 32, now, delay).unwrap());
        assert!(window.ready(context(), 1, 32, now + delay, delay).unwrap());
        assert!(window
            .ready(context(), 1, 32, now + delay, Duration::ZERO)
            .unwrap());
        assert_eq!(
            (
                window.wait_polls,
                window.full_decisions,
                window.deadline_decisions,
                window.immediate_decisions
            ),
            (u64::MAX, u64::MAX, u64::MAX, u64::MAX)
        );
    }
}
