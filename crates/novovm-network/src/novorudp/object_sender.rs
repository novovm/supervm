//! Bounded object cursor adapting the original window repair/pacing functions
//! to native encrypted UDP. One cursor retains one prepared body; its owner must
//! reserve global/per-peer slots before construction. No consensus or ledger.
//! A repair plan is drained over polls WITHOUT requiring a fresh ACK each batch.

use super::{
    datagram::NativeUdpSession,
    fragments::{FragmentProfile, OutgoingMessage},
    object_ack::*,
    *,
};
use anyhow::{ensure, Context, Result};
use std::{
    net::UdpSocket,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectSendState {
    Sending,
    Delivered,
    Expired,
    /// The owner authenticated a replacement channel. Business outcome unknown.
    SessionReplaced,
    /// Local owner stopped further transmission. Remote outcome remains unknown.
    Cancelled,
}

struct RepairCursor {
    plan: NovoRudpRepairPlan,
    range: usize,
    index: u64,
    copy: u64,
}

enum PendingCursor {
    Initial {
        index: usize,
        copy: u64,
        copies: u64,
    },
    Repair {
        generation: u64,
        index: u64,
        copy: u64,
        copies: u64,
    },
    Probe,
}
struct PendingSend {
    frame: NovoRudpTransportFrameV0,
    cursor: PendingCursor,
}

pub struct NativeObjectSender {
    body: Arc<OutgoingMessage>,
    session: [u8; 16],
    stream: u64,
    config: NovoRudpWindowConfig,
    pacing: NovoRudpPacingProfile,
    state: NovoRudpSenderState,
    terminal: ObjectSendState,
    initial: usize,
    copy: u64,
    repair: Option<RepairCursor>,
    repair_generation: u64,
    // Keep successful writes in the current repair pass across ACK snapshots.
    // The original retry wait, not a changing ACK, reopens already sent gaps.
    repair_floor: u64,
    retry_tail_end: Option<u64>,
    repair_turn: bool,
    pending: Option<PendingSend>,
    last_plan: Option<NovoRudpRepairPlan>,
    last_missing: Option<u64>,
    retry_pending: bool,
    next_send: Instant,
    deferred_batch_sent: usize,
    feedback_deadline: Instant,
    // Already written repairs keep their original feedback wait even while
    // unrelated initial fragments are still making progress.
    repair_feedback_deadline: Option<Instant>,
    deadline: Instant,
    attempts: u64,
    acknowledged_best: u64,
    diagnostic_progress: Option<diagnostics::NativeSendProgressV1>,
    traffic: Option<diagnostics::NativeObjectTraffic>,
}

impl NativeObjectSender {
    pub fn new(
        body: Arc<OutgoingMessage>,
        session: [u8; 16],
        stream: u64,
        config: NovoRudpWindowConfig,
        pacing: NovoRudpPacingProfile,
        now: Instant,
        ttl: Duration,
    ) -> Result<Self> {
        ensure!(
            body.profile() == FragmentProfile::NativeUdpV2,
            "native sender requires UDP fragments"
        );
        ensure!(
            config.window_size == OBJECT_WINDOW
                && (1..=64).contains(&config.batch_size)
                && (1..=64).contains(&pacing.batch_size)
                && (1..=16).contains(&config.packet_copies)
                && (1..=16).contains(&config.tail_packet_copies)
                && (1..=16).contains(&pacing.packet_copies)
                && (1..=16).contains(&pacing.tail_packet_copies)
                && config.ack_wait_ms > 0
                && pacing.ack_wait_ms > 0
                && config.max_window_retries > 0
                && !ttl.is_zero(),
            "invalid bounded native pacing"
        );
        Ok(Self {
            body,
            session,
            stream,
            config,
            pacing,
            state: NovoRudpSenderState::new(),
            terminal: ObjectSendState::Sending,
            initial: 0,
            copy: 0,
            repair: None,
            repair_generation: 0,
            repair_floor: 0,
            retry_tail_end: None,
            repair_turn: false,
            pending: None,
            last_plan: None,
            last_missing: None,
            retry_pending: false,
            next_send: now,
            deferred_batch_sent: 0,
            feedback_deadline: now,
            repair_feedback_deadline: None,
            deadline: now.checked_add(ttl).context("object TTL overflow")?,
            attempts: 0,
            acknowledged_best: 0,
            diagnostic_progress: None,
            traffic: diagnostics::io_cost_enabled().then(diagnostics::NativeObjectTraffic::default),
        })
    }
    pub(crate) fn acknowledged_best_v1(&self) -> u64 {
        self.acknowledged_best
    }
    pub(crate) fn feedback_state_v1(&self) -> (u64, u64, Option<u64>) {
        (
            self.state.latest_ack_epoch,
            self.last_missing.unwrap_or(self.body.frame_count() as u64),
            self.state.active_window.map(|w| w.start),
        )
    }
    pub(crate) fn enable_diagnostics_v1(&mut self) {
        self.diagnostic_progress = Some(diagnostics::NativeSendProgressV1::default());
    }
    pub(crate) fn diagnostic_progress_v1(&self) -> Option<diagnostics::NativeSendProgressV1> {
        self.diagnostic_progress
    }
    pub(crate) fn traffic(&self) -> Option<diagnostics::NativeObjectTraffic> {
        self.traffic
    }
    pub fn status(&self) -> ObjectSendState {
        self.terminal
    }
    pub(crate) fn session_id(&self) -> [u8; 16] {
        self.session
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn retire_session(&mut self) {
        if self.terminal == ObjectSendState::Sending {
            self.terminal = ObjectSendState::SessionReplaced;
            self.repair = None;
            self.last_plan = None;
        }
    }
    pub(crate) fn cancel(&mut self) {
        if self.terminal == ObjectSendState::Sending {
            self.terminal = ObjectSendState::Cancelled;
            self.repair = None;
            self.last_plan = None;
        }
    }
    pub fn body(&self) -> &Arc<OutgoingMessage> {
        &self.body
    }
    /// Ownership is preserved even on timeout; caller can report/retry the same
    /// intent. Releasing transport retention is NOT permission to discard an
    /// accepted wallet transaction or treat it as finalized.
    pub fn into_body(self) -> Arc<OutgoingMessage> {
        self.body
    }

    pub fn on_ack(
        &mut self,
        frame: &NovoRudpTransportFrameV0,
        now: Instant,
    ) -> Result<NovoRudpSenderRepairDecision> {
        ensure!(
            self.terminal == ObjectSendState::Sending && now < self.deadline,
            "object sender is terminal/expired"
        );
        let ack = decode_object_ack(
            frame,
            self.session,
            self.stream,
            self.body.id(),
            self.body.frame_count() as u64,
        )?;
        let progress = self
            .last_missing
            .is_none_or(|previous| ack.missing_count < previous);
        let decision =
            sender_repair_decision_from_ack(&mut self.state, &ack, &self.config, &self.pacing);
        match &decision {
            NovoRudpSenderRepairDecision::StaleAck { .. } => return Ok(decision),
            NovoRudpSenderRepairDecision::ReceiverDone => {
                self.terminal = ObjectSendState::Delivered;
                self.repair = None;
                self.last_plan = None;
            }
            NovoRudpSenderRepairDecision::WindowComplete => {
                self.repair = None;
                self.last_plan = None;
            }
            NovoRudpSenderRepairDecision::Repair(plan) => {
                // Identical snapshots do not rewind a partially sent repair.
                let same = self
                    .last_plan
                    .as_ref()
                    .is_some_and(|old| old.window == plan.window);
                if !same || progress {
                    let previous = self.repair.as_ref().map(|r| (r.index, r.copy));
                    if let Some((index, copy)) = previous {
                        if copy >= plan.packet_copies {
                            self.repair_floor = self.repair_floor.max(index + 1);
                        }
                    }
                    let previous_write = self
                        .repair
                        .as_ref()
                        .map(|r| (r.index, r.copy, r.plan.packet_copies));
                    self.last_plan = Some(plan.clone());
                    self.repair = trim_unconfirmed_tail(
                        plan,
                        self.body.frame_count() as u64,
                        ack.missing_count,
                        self.retry_tail_end,
                    )
                    .and_then(|plan| repair_cursor(plan, self.repair_floor, previous));
                    let next_write = self
                        .repair
                        .as_ref()
                        .map(|r| (r.index, r.copy, r.plan.packet_copies));
                    // A held carrier frame still completes this exact cursor if
                    // only later gaps changed. Do not discard its completion.
                    if next_write != previous_write {
                        self.repair_generation = self
                            .repair_generation
                            .checked_add(1)
                            .context("native repair generation exhausted")?;
                    }
                }
            }
        }
        // Diagnostic high-water only: do not alter existing retry/ACK behavior.
        self.acknowledged_best = self
            .acknowledged_best
            .max(self.body.frame_count() as u64 - ack.missing_count);
        self.last_missing = Some(ack.missing_count);
        if ack
            .current_window_missing_ranges
            .first()
            .is_none_or(|range| range.start >= self.repair_floor)
        {
            self.repair_feedback_deadline = None;
        }
        if progress {
            self.attempts = 0;
            self.feedback_deadline = self.feedback_due(now);
        }
        Ok(decision)
    }

    /// Bounded work quantum; no read/ACK wait here. Caller drains ACKs separately
    /// and fairly polls other peers/objects. Failed sends never advance cursors.
    pub fn poll_send(
        &mut self,
        socket: &UdpSocket,
        session: &mut NativeUdpSession,
        now: Instant,
        budget: usize,
    ) -> Result<usize> {
        self.poll_send_limited(socket, session, now, budget, None)
    }

    pub(crate) fn poll_send_limited(
        &mut self,
        io: &dyn super::carrier::NativeDatagramIo,
        session: &mut NativeUdpSession,
        now: Instant,
        budget: usize,
        mut wire_budget: Option<&mut wire_budget::NativeWireBudget>,
    ) -> Result<usize> {
        ensure!((1..=64).contains(&budget), "invalid datagram quantum");
        let _cost = super::diagnostics::io_span(super::diagnostics::IoPhase::SenderPoll);
        ensure!(
            session.session_id() == self.session,
            "object sender session changed"
        );
        if let Some(p) = &mut self.diagnostic_progress {
            p.polls.calls = p.polls.calls.saturating_add(1);
            p.polls.offered_budget = p.polls.offered_budget.saturating_add(budget as u64);
            if self.initial < self.body.frame_count() {
                p.polls.initial_calls = p.polls.initial_calls.saturating_add(1);
                if let Some(last) = p.last_poll {
                    p.polls.initial_max_gap_us = p.polls.initial_max_gap_us.max(
                        now.saturating_duration_since(last)
                            .as_micros()
                            .min(u128::from(u64::MAX)) as u64,
                    );
                }
            }
            p.last_poll = Some(now);
        }
        if self.terminal != ObjectSendState::Sending {
            return Ok(0);
        }
        if now >= self.deadline {
            self.terminal = ObjectSendState::Expired;
            return Ok(0);
        }
        if now < self.next_send {
            if let Some(p) = &mut self.diagnostic_progress {
                p.polls.pacing_wait = p.polls.pacing_wait.saturating_add(1);
            }
            return Ok(0);
        }
        let initial = self.initial < self.body.frame_count();
        let idle_cursor = self.pending.is_none() && self.repair.is_none();
        let ordinary_retry = idle_cursor && !initial && now >= self.feedback_deadline;
        let gap_plan = if !ordinary_retry
            && idle_cursor
            && self.repair_feedback_deadline.is_some_and(|due| now >= due)
        {
            self.last_plan
                .as_ref()
                .and_then(|plan| {
                    trim_unconfirmed_tail(
                        plan,
                        self.body.frame_count() as u64,
                        self.last_missing.expect("repair ACK retained"),
                        None,
                    )
                })
                .filter(|plan| plan.window.missing_ranges[0].start < self.repair_floor)
        } else {
            None
        };
        let retry = ordinary_retry || gap_plan.is_some();
        if idle_cursor && !initial && !retry {
            if let Some(p) = &mut self.diagnostic_progress {
                p.polls.feedback_wait = p.polls.feedback_wait.saturating_add(1);
            }
            return Ok(0);
        }
        let backoff = self.attempts >= self.config.max_window_retries
            && (self.config.no_progress_backoff || self.pacing.no_progress_backoff);
        if retry && backoff && !initial {
            if let Some(p) = &mut self.diagnostic_progress {
                p.polls.backoff_wait = p.polls.backoff_wait.saturating_add(1);
            }
            // Suppress repeated no-progress work, not the retained intent.
            // Fresh progress reopens the cursor; only its explicit TTL expires it.
            return Ok(0);
        }
        if retry && !backoff {
            self.retry_pending = true;
            self.repair_floor = 0;
            self.retry_tail_end = None;
            self.repair_feedback_deadline = None;
            if let Some(plan) = &self.last_plan {
                self.repair_generation = self
                    .repair_generation
                    .checked_add(1)
                    .context("native repair generation exhausted")?;
                let retry_plan = if ordinary_retry {
                    self.retry_tail_end =
                        plan.window.missing_ranges.last().map(|r| r.end_inclusive);
                    Some(plan.clone())
                } else {
                    // A lost early repair may retry before initial completion;
                    // that is not permission to send the unconfirmed tail early.
                    gap_plan
                };
                self.repair = retry_plan.and_then(|plan| repair_cursor(plan, 0, None));
            }
        }
        let batch =
            self.repair
                .as_ref()
                .map(|r| r.plan.batch_size)
                .unwrap_or(self.config.batch_size.max(self.pacing.batch_size)) as usize;
        let pause = self
            .repair
            .as_ref()
            .map(|r| r.plan.batch_pause_ms)
            .unwrap_or(self.config.batch_pause_ms.max(self.pacing.batch_pause_ms));
        let mut sent = 0;
        for _ in 0..budget.min(batch) {
            if self.pending.is_none() {
                let _cost = super::diagnostics::io_span(super::diagnostics::IoPhase::FramePrepare);
                // Proven gaps need not wait for the whole multi-MiB initial
                // body. Alternate the existing cursors within the same quantum.
                let send_initial = self.initial < self.body.frame_count()
                    && (!self.repair_turn || self.repair.is_none());
                let (index, copies, kind) = if send_initial {
                    (
                        self.initial,
                        self.config.packet_copies.max(self.pacing.packet_copies),
                        NovoRudpTransportFrameKindV0::Data,
                    )
                } else if let Some(repair) = &self.repair {
                    (
                        repair.index as usize,
                        repair.plan.packet_copies,
                        NovoRudpTransportFrameKindV0::Repair,
                    )
                } else if sent == 0 {
                    // Lost feedback/all initial packets: a bounded tail probe lets
                    // the receiver reconstruct its missing snapshot. No full resend.
                    (
                        self.body.frame_count() - 1,
                        1,
                        NovoRudpTransportFrameKindV0::Repair,
                    )
                } else {
                    break;
                };
                let frame = NovoRudpTransportFrameV0::new(
                    kind,
                    self.session,
                    self.stream,
                    u64::from_be_bytes(self.body.id()[..8].try_into()?),
                    index as u64,
                    self.state.latest_ack_epoch,
                    self.body.frame(index)?,
                );
                let cursor = if send_initial {
                    PendingCursor::Initial {
                        index: self.initial,
                        copy: self.copy,
                        copies,
                    }
                } else if let Some(repair) = &self.repair {
                    PendingCursor::Repair {
                        generation: self.repair_generation,
                        index: repair.index,
                        copy: repair.copy,
                        copies,
                    }
                } else {
                    PendingCursor::Probe
                };
                self.pending = Some(PendingSend { frame, cursor });
            }
            let pending = self.pending.as_ref().expect("prepared native frame");
            let Some(wire_bytes) =
                session.try_send_limited(io, &pending.frame, now, wire_budget.as_deref_mut())?
            else {
                if let Some(p) = &mut self.diagnostic_progress {
                    p.polls.send_blocked = p.polls.send_blocked.saturating_add(1);
                    if wire_budget.is_none() && !io.deferred_writes() {
                        p.polls.udp_would_block = p.polls.udp_would_block.saturating_add(1);
                    }
                }
                break;
            };
            let pending = self.pending.take().expect("completed native frame");
            sent += 1;
            if let Some(traffic) = &mut self.traffic {
                let (counter, event) = match pending.cursor {
                    PendingCursor::Initial { .. } => {
                        (&mut traffic.initial, diagnostics::Traffic::InitialSent)
                    }
                    PendingCursor::Repair { .. } => {
                        (&mut traffic.repair, diagnostics::Traffic::RepairSent)
                    }
                    PendingCursor::Probe => (&mut traffic.probe, diagnostics::Traffic::ProbeSent),
                };
                counter.record(wire_bytes);
                diagnostics::frame_traffic(event, &pending.frame, wire_bytes);
            }
            self.repair_turn = matches!(pending.cursor, PendingCursor::Initial { .. });
            if let Some(progress) = &mut self.diagnostic_progress {
                let observed_at = Instant::now();
                match &pending.cursor {
                    PendingCursor::Initial { .. } => {
                        progress.initial_packets = progress.initial_packets.saturating_add(1);
                        progress.first_initial.get_or_insert(observed_at);
                        progress.last_initial = Some(observed_at);
                    }
                    PendingCursor::Repair { index, .. } => {
                        progress.repair_packets = progress.repair_packets.saturating_add(1);
                        progress.first_repair.get_or_insert(observed_at);
                        progress.first_repair_index.get_or_insert(*index);
                        if self.retry_pending {
                            progress.repair_retry_passes =
                                progress.repair_retry_passes.saturating_add(1);
                            progress.first_repair_retry.get_or_insert(observed_at);
                            progress.first_repair_retry_index.get_or_insert(*index);
                        }
                        progress.last_repair = Some(observed_at);
                        progress.last_repair_index = Some(*index);
                    }
                    PendingCursor::Probe => {
                        progress.probe_packets = progress.probe_packets.saturating_add(1);
                        progress.first_probe.get_or_insert(observed_at);
                        progress.last_probe = Some(observed_at);
                    }
                }
            }
            if self.retry_pending && !matches!(pending.cursor, PendingCursor::Initial { .. }) {
                self.attempts = self.attempts.saturating_add(1);
                self.retry_pending = false;
            }
            match pending.cursor {
                PendingCursor::Initial {
                    index,
                    copy,
                    copies,
                } => {
                    ensure!(
                        self.initial == index && self.copy == copy,
                        "native initial cursor changed during write"
                    );
                    self.copy += 1;
                    if self.copy == copies {
                        self.copy = 0;
                        self.initial += 1;
                        if self.initial == self.body.frame_count() {
                            if let Some(progress) = &mut self.diagnostic_progress {
                                progress.initial_complete = progress.last_initial;
                            }
                        }
                    }
                }
                PendingCursor::Repair {
                    generation,
                    index,
                    copy,
                    copies,
                } => {
                    if generation != self.repair_generation || self.repair.is_none() {
                        if io.deferred_writes() {
                            break;
                        }
                        continue;
                    }
                    // Wait after the last actual repair write in this bounded
                    // pass; unrelated initial writes/ACKs cannot extend it.
                    self.repair_feedback_deadline = Some(self.feedback_due(now));
                    let repair = self.repair.as_mut().expect("current repair cursor");
                    ensure!(
                        repair.index == index && repair.copy == copy,
                        "native repair cursor changed during write"
                    );
                    repair.copy += 1;
                    if repair.copy == copies {
                        repair.copy = 0;
                        repair.index += 1;
                        self.repair_floor = repair.index;
                        if repair.index
                            > repair.plan.window.missing_ranges[repair.range].end_inclusive
                        {
                            repair.range += 1;
                            if repair.range == repair.plan.window.missing_ranges.len() {
                                self.repair = None;
                            } else {
                                repair.index =
                                    repair.plan.window.missing_ranges[repair.range].start;
                            }
                        }
                    }
                }
                PendingCursor::Probe => break,
            }
            // Return the sole reliable-stream write slot to the same owner so
            // pending feedback can run before this object queues another frame.
            if io.deferred_writes() {
                break;
            }
        }
        if sent == batch && batch < budget {
            if let Some(p) = &mut self.diagnostic_progress {
                p.polls.batch_limited = p.polls.batch_limited.saturating_add(1);
            }
        }
        if sent > 0 {
            // A deferred carrier yields its one write slot after each real
            // completion for ACK/data fairness. Count those completions across
            // polls: slot handoff is not an entire pacing batch. WouldBlock
            // contributes nothing; byte budgets and poll quanta remain intact.
            let batch_finished = if io.deferred_writes() {
                self.deferred_batch_sent += sent;
                if self.deferred_batch_sent >= batch {
                    self.deferred_batch_sent = 0;
                    true
                } else {
                    false
                }
            } else {
                true
            };
            self.next_send = if batch_finished {
                now.checked_add(Duration::from_millis(pause))
                    .unwrap_or(self.deadline)
                    .min(self.deadline)
            } else {
                now
            };
            self.feedback_deadline = self.feedback_due(now);
        }
        Ok(sent)
    }

    fn feedback_due(&self, now: Instant) -> Instant {
        now.checked_add(Duration::from_millis(
            self.config.ack_wait_ms.max(self.pacing.ack_wait_ms),
        ))
        .unwrap_or(self.deadline)
        .min(self.deadline)
    }
}

/// Trim unconfirmed trailing data in any window, not just the object's final
/// window. Preserve the unsent tail already armed by the original retry wait.
fn trim_unconfirmed_tail(
    plan: &NovoRudpRepairPlan,
    total: u64,
    missing: u64,
    armed_end: Option<u64>,
) -> Option<NovoRudpRepairPlan> {
    let ranges = &plan.window.missing_ranges;
    let last = ranges.last()?;
    let after_window = total - plan.window.range.end_inclusive - 1;
    if last.end_inclusive != plan.window.range.end_inclusive
        || missing.saturating_sub(plan.window.missing_count) != after_window
    {
        return Some(plan.clone());
    }
    let mut gaps = ranges[..ranges.len() - 1].to_vec();
    if let Some(end) = armed_end.filter(|end| *end >= last.start) {
        gaps.push(NovoRudpRange::new(last.start, last.end_inclusive.min(end)));
    }
    if gaps.is_empty() {
        return None;
    }
    let mut trimmed = plan.clone();
    trimmed.window.missing_count = missing_count(&gaps);
    trimmed.window.missing_ranges = gaps;
    Some(trimmed)
}

fn repair_cursor(
    mut plan: NovoRudpRepairPlan,
    floor: u64,
    previous: Option<(u64, u64)>,
) -> Option<RepairCursor> {
    plan.window.missing_ranges.retain_mut(|range| {
        range.start = range.start.max(floor);
        range.start <= range.end_inclusive
    });
    let index = plan.window.missing_ranges.first()?.start;
    plan.window.missing_count = missing_count(&plan.window.missing_ranges);
    let copy = previous
        .filter(|(old, _)| *old == index)
        .map_or(0, |(_, copy)| copy);
    Some(RepairCursor {
        plan,
        range: 0,
        index,
        copy,
    })
}

#[cfg(test)]
mod tests;
