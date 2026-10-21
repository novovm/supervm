//! Node-wide native payload accounting, before ingress cryptography and after
//! successful egress, with held capacity for asynchronous carrier writes. This is an application admission bound, not an upstream
//! firewall: discarded incoming bytes still physically reach the socket.
//! 61 one-second buckets conservatively retain the oldest partial second, so
//! every rolling 60-second interval fits the limit without per-packet allocation.
//! Outstanding reservations remain charged separately until explicitly resolved.
//!
//! Large budgets are paced, not burst-then-blocked: one eighth of the limit is
//! available immediately; above it each second admits at most the remaining
//! seven eighths divided over 61 buckets. A window therefore never reaches the
//! rolling hard limit through admitted traffic, so a burst can no longer drain
//! the whole minute in seconds and then refuse every peer (including ACKs and
//! handshakes) until it ages out. Small budgets keep the plain rolling limit.

use super::carrier::{NativeDatagramIo, NativeRoute};
use anyhow::{ensure, Result};
use std::{io::ErrorKind, sync::Arc, time::Instant};

const MAX_PENDING_SENDS: usize = 64;

pub const MIN_BYTES_PER_MINUTE: u64 = 2400;
pub const MAX_BYTES_PER_MINUTE: u64 = 1024 * 1024 * 1024;
/// Pacing applies only when each second can admit at least this much; smaller
/// budgets could not carry one datagram per second and keep the rolling limit.
const MIN_PACED_BYTES_PER_SECOND: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeWireUsage {
    pub limit: u64,
    pub used: u64,
    pub remaining: u64,
    pub received_bytes: u64,
    pub admitted_receive_bytes: u64,
    pub sent_bytes: u64,
    /// Outstanding send bytes, included in used and never expired by the clock.
    pub reserved_send_bytes: u64,
    /// Bytes conservatively charged beyond the known actual write count.
    pub uncertain_send_bytes: u64,
    pub receive_dropped: u64,
    pub send_deferred: u64,
}

/// Single-use proof of capacity held by exactly one budget owner. Dropping this
/// token does not release capacity: the owner must resolve the actual write.
#[derive(Debug)]
pub(crate) struct NativeSendReservation {
    owner: Arc<()>,
    id: u64,
}

pub struct NativeWireBudget {
    origin: Instant,
    second: u64,
    buckets: [u64; 61],
    usage: NativeWireUsage,
    reservation_owner: Arc<()>,
    next_reservation: u64,
    pending_sends: [Option<(u64, usize)>; MAX_PENDING_SENDS],
}

impl NativeWireBudget {
    pub fn new(limit: u64, now: Instant) -> Result<Self> {
        ensure!(
            (MIN_BYTES_PER_MINUTE..=MAX_BYTES_PER_MINUTE).contains(&limit),
            "native UDP byte budget out of range"
        );
        Ok(Self {
            origin: now,
            second: 0,
            buckets: [0; 61],
            reservation_owner: Arc::new(()),
            next_reservation: 0,
            pending_sends: [None; MAX_PENDING_SENDS],
            usage: NativeWireUsage {
                limit,
                remaining: limit,
                ..NativeWireUsage::default()
            },
        })
    }

    fn advance(&mut self, now: Instant) {
        let second = now
            .saturating_duration_since(self.origin)
            .as_secs()
            .max(self.second);
        let elapsed = second - self.second;
        if elapsed >= 61 {
            self.buckets.fill(0);
            self.usage.used = self.usage.reserved_send_bytes;
        } else {
            for step in 1..=elapsed {
                let index = ((self.second + step) % 61) as usize;
                self.usage.used -= self.buckets[index];
                self.buckets[index] = 0;
            }
        }
        self.second = second;
        self.usage.remaining = self.usage.limit.saturating_sub(self.usage.used);
    }

    pub fn usage(&mut self, now: Instant) -> NativeWireUsage {
        self.advance(now);
        self.usage
    }

    /// Follow the owner's current resource decision. Bytes already in the
    /// rolling window stay charged: lowering takes effect at once and raising
    /// never forgives past traffic. Outstanding reservations are unaffected.
    pub fn set_limit(&mut self, limit: u64, now: Instant) -> Result<()> {
        ensure!(
            (MIN_BYTES_PER_MINUTE..=MAX_BYTES_PER_MINUTE).contains(&limit),
            "native UDP byte budget out of range"
        );
        self.advance(now);
        self.usage.limit = limit;
        self.usage.remaining = limit.saturating_sub(self.usage.used);
        Ok(())
    }

    /// Admission rule shared by ingress, UDP sends and held writes. `advance`
    /// must already have run for `now`. Burst bytes in any window stay within
    /// limit/8 and paced bytes within 61 * second_cap = 7/8 * limit, so paced
    /// traffic always fits the rolling limit. Held reservations are covered by
    /// the rolling limit only; native UDP sends never hold one.
    fn fits(&self, bytes: u64) -> bool {
        if bytes > self.usage.remaining {
            return false;
        }
        let limit = self.usage.limit;
        let burst = limit / 8;
        let second_cap = (limit - burst) / 61;
        if second_cap < MIN_PACED_BYTES_PER_SECOND || self.usage.used + bytes <= burst {
            return true;
        }
        self.buckets[(self.second % 61) as usize] + bytes <= second_cap
    }

    fn charge(&mut self, bytes: usize) {
        self.buckets[(self.second % 61) as usize] += bytes as u64;
        self.usage.used += bytes as u64;
        self.usage.remaining = self.usage.limit.saturating_sub(self.usage.used);
    }

    /// Hold capacity before yielding to a carrier write. The same node-wide
    /// meter continues to account for ingress, UDP sends and other pending writes.
    pub(crate) fn reserve_send(
        &mut self,
        bytes: usize,
        now: Instant,
    ) -> Result<Option<NativeSendReservation>> {
        ensure!(bytes > 0, "empty native send reservation");
        self.advance(now);
        let slot = self.pending_sends.iter().position(Option::is_none);
        if !self.fits(bytes as u64) || slot.is_none() {
            self.usage.send_deferred = self.usage.send_deferred.saturating_add(1);
            return Ok(None);
        }
        let id = self
            .next_reservation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("native send reservation sequence exhausted"))?;
        self.next_reservation = id;
        self.pending_sends[slot.expect("available reservation slot")] = Some((id, bytes));
        self.usage.reserved_send_bytes += bytes as u64;
        self.usage.used += bytes as u64;
        self.usage.remaining = self.usage.limit.saturating_sub(self.usage.used);
        Ok(Some(NativeSendReservation {
            owner: Arc::clone(&self.reservation_owner),
            id,
        }))
    }

    /// Resolve a held write in the completion-time bucket. For unknown partial
    /// writes the whole reservation remains charged, but only the known actual
    /// bytes contribute to sent_bytes. This is accounting, not delivery success.
    /// Cancellation before any write uses actual_bytes=0 and uncertain=false.
    pub(crate) fn finish_send(
        &mut self,
        reservation: NativeSendReservation,
        actual_bytes: usize,
        uncertain: bool,
        now: Instant,
    ) -> Result<()> {
        ensure!(
            Arc::ptr_eq(&reservation.owner, &self.reservation_owner),
            "native send reservation belongs to another budget"
        );
        let slot = self
            .pending_sends
            .iter()
            .position(|entry| entry.is_some_and(|(id, _)| id == reservation.id))
            .ok_or_else(|| anyhow::anyhow!("native send reservation is no longer outstanding"))?;
        let (_, held) = self.pending_sends[slot].expect("outstanding reservation");
        ensure!(
            actual_bytes <= held,
            "native send count exceeds reservation"
        );
        self.advance(now);
        self.pending_sends[slot] = None;
        self.usage.reserved_send_bytes -= held as u64;
        self.usage.used -= held as u64;
        self.charge(if uncertain { held } else { actual_bytes });
        self.usage.sent_bytes = self.usage.sent_bytes.saturating_add(actual_bytes as u64);
        if uncertain {
            self.usage.uncertain_send_bytes = self
                .usage
                .uncertain_send_bytes
                .saturating_add((held - actual_bytes) as u64);
        }
        Ok(())
    }

    pub(crate) fn admit_receive(&mut self, bytes: usize, now: Instant) -> bool {
        self.advance(now);
        self.usage.received_bytes = self.usage.received_bytes.saturating_add(bytes as u64);
        if !self.fits(bytes as u64) {
            self.usage.receive_dropped = self.usage.receive_dropped.saturating_add(1);
            return false;
        }
        self.charge(bytes);
        self.usage.admitted_receive_bytes = self
            .usage
            .admitted_receive_bytes
            .saturating_add(bytes as u64);
        true
    }
}

/// A mutable borrow makes admission/send/accounting one synchronous operation;
/// failed socket writes consume no budget. No mutex or separate queue is needed.
pub(crate) fn try_send_datagram(
    io: &dyn NativeDatagramIo,
    wire: &[u8],
    remote: impl Into<NativeRoute>,
    now: Instant,
    mut budget: Option<&mut NativeWireBudget>,
) -> Result<bool> {
    if let Some(budget) = budget.as_deref_mut() {
        budget.advance(now);
        if !budget.fits(wire.len() as u64) {
            budget.usage.send_deferred = budget.usage.send_deferred.saturating_add(1);
            return Ok(false);
        }
    }
    match io.try_send(wire, remote.into()) {
        Ok(count) => {
            ensure!(
                count <= wire.len(),
                "native carrier write count exceeds input"
            );
            if let Some(budget) = budget {
                budget.charge(count);
                budget.usage.sent_bytes = budget.usage.sent_bytes.saturating_add(count as u64);
            }
            ensure!(count == wire.len(), "partial native UDP datagram");
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::WouldBlock => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;
    use std::time::Duration;

    // Internal forgery only exercises defensive validation; callers cannot
    // construct or clone the private token.
    fn duplicate_for_test(token: &NativeSendReservation) -> NativeSendReservation {
        NativeSendReservation {
            owner: Arc::clone(&token.owner),
            id: token.id,
        }
    }

    #[test]
    fn held_send_survives_window_expiry_and_completion_starts_new_window() -> Result<()> {
        let start = Instant::now();
        let later = start + Duration::from_secs(600);
        let mut budget = NativeWireBudget::new(2400, start)?;
        assert!(budget.admit_receive(1200, start));
        let pending = budget.reserve_send(1200, start)?.unwrap();
        assert_eq!(budget.usage(start).remaining, 0);
        let usage = budget.usage(later);
        assert_eq!((usage.used, usage.reserved_send_bytes), (1200, 1200));
        assert_eq!(usage.sent_bytes, 0);
        assert!(budget.admit_receive(1200, later));
        assert!(!budget.admit_receive(1, later));
        assert!(budget.reserve_send(1, later)?.is_none());
        budget.finish_send(pending, 1200, false, later)?;
        let usage = budget.usage(later);
        assert_eq!(
            (usage.used, usage.reserved_send_bytes, usage.sent_bytes),
            (2400, 0, 1200)
        );
        assert_eq!(budget.usage(start).remaining, 0); // clock regression cannot undo it
        assert_eq!(budget.usage(later + Duration::from_secs(60)).remaining, 0);
        assert_eq!(
            budget.usage(later + Duration::from_secs(61)).remaining,
            2400
        );
        Ok(())
    }

    #[test]
    fn cancel_partial_and_uncertain_completion_do_not_report_false_bytes() -> Result<()> {
        let now = Instant::now();
        let mut budget = NativeWireBudget::new(2400, now)?;
        let cancel = budget.reserve_send(1200, now)?.unwrap();
        budget.finish_send(cancel, 0, false, now)?;
        assert_eq!(budget.usage(now).remaining, 2400);
        let partial = budget.reserve_send(1200, now)?.unwrap();
        budget.finish_send(partial, 250, false, now)?;
        assert_eq!(budget.usage(now).used, 250);
        let uncertain = budget.reserve_send(1200, now)?.unwrap();
        budget.finish_send(uncertain, 200, true, now)?;
        let usage = budget.usage(now);
        assert_eq!(
            (usage.used, usage.sent_bytes, usage.uncertain_send_bytes),
            (1450, 450, 1000)
        );
        assert_eq!(usage.reserved_send_bytes, 0);
        let unknown = budget.reserve_send(950, now)?.unwrap();
        budget.finish_send(unknown, 0, true, now)?;
        let usage = budget.usage(now);
        assert_eq!(
            (
                usage.remaining,
                usage.sent_bytes,
                usage.uncertain_send_bytes
            ),
            (0, 450, 1950)
        );
        Ok(())
    }

    #[test]
    fn reservations_are_owner_bound_single_use_and_bounded() -> Result<()> {
        let now = Instant::now();
        let mut budget = NativeWireBudget::new(2400, now)?;
        let mut other = NativeWireBudget::new(2400, now)?;
        assert!(budget.reserve_send(0, now).is_err());
        let pending = budget.reserve_send(10, now)?.unwrap();
        let duplicate = duplicate_for_test(&pending);
        assert!(other
            .finish_send(duplicate_for_test(&pending), 10, false, now)
            .is_err());
        assert!(budget
            .finish_send(duplicate_for_test(&pending), 11, false, now)
            .is_err());
        assert_eq!(budget.usage(now).reserved_send_bytes, 10);
        assert_eq!(other.usage(now).used, 0);
        budget.finish_send(pending, 10, false, now)?;
        assert!(budget.finish_send(duplicate, 10, false, now).is_err());
        let mut tokens = Vec::new();
        for _ in 0..MAX_PENDING_SENDS {
            tokens.push(budget.reserve_send(1, now)?.unwrap());
        }
        assert!(budget.reserve_send(1, now)?.is_none());
        assert!(budget
            .reserve_send(1, now + Duration::from_secs(600))?
            .is_none());
        assert_eq!(budget.usage(now).reserved_send_bytes, 64);
        for token in tokens {
            budget.finish_send(token, 0, false, now)?;
        }
        assert_eq!(budget.usage(now).remaining, 2400);
        assert!(budget.reserve_send(2400, now)?.is_some());
        Ok(())
    }

    #[test]
    fn deferred_write_capacity_is_shared_with_udp_and_ingress() -> Result<()> {
        let now = Instant::now();
        let mut budget = NativeWireBudget::new(2400, now)?;
        let pending = budget.reserve_send(1200, now)?.unwrap();
        assert!(budget.admit_receive(600, now));
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let receiver = UdpSocket::bind("127.0.0.1:0")?;
        receiver.set_read_timeout(Some(Duration::from_secs(1)))?;
        assert!(try_send_datagram(
            &socket,
            &[7; 600],
            receiver.local_addr()?,
            now,
            Some(&mut budget)
        )?);
        let mut buffer = [0; 1200];
        assert_eq!(receiver.recv_from(&mut buffer)?.0, 600);
        assert!(!try_send_datagram(
            &socket,
            &[8],
            receiver.local_addr()?,
            now,
            Some(&mut budget)
        )?);
        assert!(!budget.admit_receive(1, now));
        budget.finish_send(pending, 0, false, now)?;
        assert!(budget.admit_receive(1200, now));
        let usage = budget.usage(now);
        assert_eq!(
            (usage.used, usage.sent_bytes, usage.admitted_receive_bytes),
            (2400, 600, 1800)
        );
        assert_eq!(usage.reserved_send_bytes, 0);
        assert_eq!((usage.send_deferred, usage.receive_dropped), (1, 1));
        Ok(())
    }

    #[test]
    fn rolling_window_has_no_boundary_burst_or_clock_regression_reset() -> Result<()> {
        let start = Instant::now();
        let mut budget = NativeWireBudget::new(2400, start)?;
        assert!(budget.admit_receive(2400, start + Duration::from_millis(999)));
        assert!(!budget.admit_receive(1, start + Duration::from_secs(60)));
        assert_eq!(budget.usage(start).remaining, 0);
        assert_eq!(
            budget.usage(start + Duration::from_secs(61)).remaining,
            2400
        );
        assert!(budget.admit_receive(1200, start + Duration::from_secs(61)));
        assert_eq!(
            budget.usage(start + Duration::from_secs(600)).remaining,
            2400
        );
        assert_eq!(budget.usage(start).received_bytes, 3601);
        assert!(NativeWireBudget::new(2399, start).is_err());
        assert!(NativeWireBudget::new(MAX_BYTES_PER_MINUTE + 1, start).is_err());
        Ok(())
    }

    #[test]
    fn actual_udp_sends_and_ingress_share_budget_without_false_success() -> Result<()> {
        let now = Instant::now();
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let receiver = UdpSocket::bind("127.0.0.1:0")?;
        receiver.set_read_timeout(Some(Duration::from_secs(1)))?;
        let mut budget = NativeWireBudget::new(2400, now)?;
        assert!(budget.admit_receive(1200, now));
        assert!(try_send_datagram(
            &socket,
            &[7; 1200],
            receiver.local_addr()?,
            now,
            Some(&mut budget)
        )?);
        let mut bytes = [0; 2048];
        assert_eq!(receiver.recv_from(&mut bytes)?.0, 1200);
        assert!(!try_send_datagram(
            &socket,
            &[8],
            receiver.local_addr()?,
            now,
            Some(&mut budget)
        )?);
        receiver.set_nonblocking(true)?;
        assert_eq!(
            receiver.recv_from(&mut bytes).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        let usage = budget.usage(now);
        assert_eq!(
            (usage.used, usage.sent_bytes, usage.admitted_receive_bytes),
            (2400, 1200, 1200)
        );
        assert_eq!(usage.send_deferred, 1);
        Ok(())
    }

    #[test]
    fn large_budget_paces_greedy_traffic_without_rolling_blackout() -> Result<()> {
        // Phone L4 allowance with greedy 1200-byte datagrams polled every 10 ms.
        // Unpaced, this drained the minute in ~20 s and then refused everything.
        let limit = 64 * 1024 * 1024;
        let start = Instant::now();
        let mut budget = NativeWireBudget::new(limit, start)?;
        let mut per_second = vec![0u64; 180];
        for tick in 0..18_000u64 {
            let now = start + Duration::from_millis(tick * 10);
            while budget.admit_receive(1200, now) {
                per_second[(tick / 100) as usize] += 1200;
            }
        }
        let second_cap = (limit - limit / 8) / 61;
        assert!(per_second[0] + 1200 > limit / 8, "burst is immediate");
        for (second, bytes) in per_second.iter().enumerate().skip(1) {
            assert!(
                *bytes + 1200 > second_cap,
                "second {second} admitted only {bytes} bytes"
            );
        }
        for window in per_second.windows(60) {
            assert!(window.iter().sum::<u64>() <= limit);
        }
        assert!(
            budget
                .usage(start + Duration::from_secs(180))
                .receive_dropped
                > 0
        );
        Ok(())
    }

    #[test]
    fn budgets_below_pacing_floor_keep_plain_rolling_limit() -> Result<()> {
        let now = Instant::now();
        let limit = 1024 * 1024;
        let mut budget = NativeWireBudget::new(limit, now)?;
        let mut admitted = 0;
        while budget.admit_receive(1200, now) {
            admitted += 1200;
        }
        assert!(admitted <= limit && admitted + 1200 > limit);
        assert!(!budget.admit_receive(1200, now + Duration::from_secs(60)));
        assert!(budget.admit_receive(1200, now + Duration::from_secs(61)));
        Ok(())
    }
}
