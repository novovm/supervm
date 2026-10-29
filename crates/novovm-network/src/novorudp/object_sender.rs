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
    pending: Option<PendingSend>,
    last_plan: Option<NovoRudpRepairPlan>,
    last_missing: Option<u64>,
    retry_pending: bool,
    next_send: Instant,
    deferred_batch_sent: usize,
    feedback_deadline: Instant,
    deadline: Instant,
    attempts: u64,
    acknowledged_best: u64,
    diagnostic_progress: Option<diagnostics::NativeSendProgressV1>,
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
            pending: None,
            last_plan: None,
            last_missing: None,
            retry_pending: false,
            next_send: now,
            deferred_batch_sent: 0,
            feedback_deadline: now,
            deadline: now.checked_add(ttl).context("object TTL overflow")?,
            attempts: 0,
            acknowledged_best: 0,
            diagnostic_progress: None,
        })
    }
    pub(crate) fn acknowledged_best_v1(&self) -> u64 {
        self.acknowledged_best
    }
    pub(crate) fn enable_diagnostics_v1(&mut self) {
        self.diagnostic_progress = Some(diagnostics::NativeSendProgressV1::default());
    }
    pub(crate) fn diagnostic_progress_v1(&self) -> Option<diagnostics::NativeSendProgressV1> {
        self.diagnostic_progress
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
                if !same {
                    // Once ack_wait has armed a full tail repair, genuine ACK
                    // progress must not turn its remaining tail back into an
                    // unconfirmed/in-flight run. A trimmed internal-gap plan
                    // also has a final window, so check the actual last range.
                    let repairing_tail = self.initial == self.body.frame_count()
                        && self.repair.as_ref().is_some_and(|repair| {
                            repair
                                .plan
                                .window
                                .missing_ranges
                                .last()
                                .is_some_and(|range| {
                                    range.end_inclusive + 1 == self.body.frame_count() as u64
                                })
                        });
                    self.repair_generation = self
                        .repair_generation
                        .checked_add(1)
                        .context("native repair generation exhausted")?;
                    self.last_plan = Some(plan.clone());
                    // UDP receivers ACK after almost every fragment, so the
                    // final window's trailing missing run is usually still in
                    // flight or not yet sent. Repair only gaps behind the
                    // receiver's newest fragment now; the ack_wait retry
                    // repairs a genuinely lost tail from the full last_plan.
                    let next_plan = if repairing_tail {
                        Some(plan.clone())
                    } else {
                        trim_unconfirmed_tail(plan, self.body.frame_count() as u64)
                    };
                    self.repair = next_plan.map(|plan| RepairCursor {
                        index: plan.window.missing_ranges[0].start,
                        plan,
                        range: 0,
                        copy: 0,
                    });
                }
            }
        }
        // Diagnostic high-water only: do not alter existing retry/ACK behavior.
        self.acknowledged_best = self
            .acknowledged_best
            .max(self.body.frame_count() as u64 - ack.missing_count);
        self.last_missing = Some(ack.missing_count);
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
        ensure!(
            session.session_id() == self.session,
            "object sender session changed"
        );
        if self.terminal != ObjectSendState::Sending {
            return Ok(0);
        }
        if now >= self.deadline {
            self.terminal = ObjectSendState::Expired;
            return Ok(0);
        }
        if now < self.next_send {
            return Ok(0);
        }
        let initial = self.initial < self.body.frame_count();
        let retry = self.pending.is_none() && !initial && self.repair.is_none();
        if retry && now < self.feedback_deadline {
            return Ok(0);
        }
        if retry
            && self.attempts >= self.config.max_window_retries
            && (self.config.no_progress_backoff || self.pacing.no_progress_backoff)
        {
            // Suppress repeated no-progress work, not the retained intent.
            // Fresh progress reopens the cursor; only its explicit TTL expires it.
            return Ok(0);
        }
        if retry {
            self.retry_pending = true;
            if let Some(plan) = &self.last_plan {
                self.repair = Some(RepairCursor {
                    plan: plan.clone(),
                    range: 0,
                    index: plan.window.missing_ranges[0].start,
                    copy: 0,
                });
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
                let (index, copies, kind) = if self.initial < self.body.frame_count() {
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
                let cursor = if self.initial < self.body.frame_count() {
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
            if !session.try_send_limited(io, &pending.frame, now, wire_budget.as_deref_mut())? {
                break;
            }
            let pending = self.pending.take().expect("completed native frame");
            sent += 1;
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
            if self.retry_pending {
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
                    let repair = self.repair.as_mut().expect("current repair cursor");
                    ensure!(
                        repair.index == index && repair.copy == copy,
                        "native repair cursor changed during write"
                    );
                    repair.copy += 1;
                    if repair.copy == copies {
                        repair.copy = 0;
                        repair.index += 1;
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

/// In the object's final window, drop the trailing missing run: fragments after
/// the receiver's newest one are not yet known lost. Earlier gaps are kept, and
/// non-final windows are unchanged. `None` means no gap needs repair now.
fn trim_unconfirmed_tail(plan: &NovoRudpRepairPlan, total: u64) -> Option<NovoRudpRepairPlan> {
    let ranges = &plan.window.missing_ranges;
    let last = ranges.last()?;
    if plan.window.range.end_inclusive.saturating_add(1) != total
        || last.end_inclusive.saturating_add(1) != total
    {
        return Some(plan.clone());
    }
    let gaps = ranges[..ranges.len() - 1].to_vec();
    if gaps.is_empty() {
        return None;
    }
    let mut trimmed = plan.clone();
    trimmed.window.missing_count = missing_count(&gaps);
    trimmed.window.missing_ranges = gaps;
    Some(trimmed)
}

#[cfg(test)]
mod tests;
