//! Node-wide UDP payload accounting, before ingress cryptography and after
//! successful egress. This is an application admission bound, not an upstream
//! firewall: discarded incoming bytes still physically reach the socket.
//! 61 one-second buckets conservatively retain the oldest partial second, so
//! every rolling 60-second interval fits the limit without per-packet allocation.

use super::carrier::{NativeDatagramIo, NativeRoute};
use anyhow::{ensure, Result};
use std::{io::ErrorKind, time::Instant};

pub const MIN_BYTES_PER_MINUTE: u64 = 2400;
pub const MAX_BYTES_PER_MINUTE: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeWireUsage {
    pub limit: u64,
    pub used: u64,
    pub remaining: u64,
    pub received_bytes: u64,
    pub admitted_receive_bytes: u64,
    pub sent_bytes: u64,
    pub receive_dropped: u64,
    pub send_deferred: u64,
}

pub struct NativeWireBudget {
    origin: Instant,
    second: u64,
    buckets: [u64; 61],
    usage: NativeWireUsage,
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
            self.usage.used = 0;
        } else {
            for step in 1..=elapsed {
                let index = ((self.second + step) % 61) as usize;
                self.usage.used -= self.buckets[index];
                self.buckets[index] = 0;
            }
        }
        self.second = second;
        self.usage.remaining = self.usage.limit - self.usage.used;
    }

    pub fn usage(&mut self, now: Instant) -> NativeWireUsage {
        self.advance(now);
        self.usage
    }

    fn charge(&mut self, bytes: usize) {
        self.buckets[(self.second % 61) as usize] += bytes as u64;
        self.usage.used += bytes as u64;
        self.usage.remaining = self.usage.limit - self.usage.used;
    }

    pub(crate) fn admit_receive(&mut self, bytes: usize, now: Instant) -> bool {
        self.advance(now);
        self.usage.received_bytes = self.usage.received_bytes.saturating_add(bytes as u64);
        if bytes as u64 > self.usage.remaining {
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
        if wire.len() as u64 > budget.usage.remaining {
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
}
