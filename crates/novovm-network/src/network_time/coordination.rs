//! Session-scoped relative time, never UTC or authorization authority. The
//! existing authenticated object owner transports these fixed binary messages.
use super::{measure_peer_clock_v1, ClockAnchorV1, PeerTimeSampleV1};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

pub const EXCHANGE_BYTES: usize = 90;
pub const MEASUREMENT_LIFETIME: Duration = Duration::from_secs(2);
pub const SAMPLE_INTERVAL: Duration = Duration::from_millis(250);
const REFERENCE_LIFETIME: Duration = Duration::from_secs(30);
const REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const DRIFT_PPM: u32 = 500;
const REQUIRED_SAMPLES: usize = 3;
const MAX_POLL_GAP: Duration = Duration::from_secs(2);
const CLOCK_JUMP_MS: u64 = 50;

/// Includes fixed chain/genesis/protocol commitment and both authenticated
/// identities, not an address. Role ordering is request source then target.
pub fn context(
    domain: [u8; 32],
    source: [u8; 32],
    target: [u8; 32],
    session: [u8; 16],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"novonet-relative-time/v1\0");
    hash.update(domain);
    hash.update(source);
    hash.update(target);
    hash.update(session);
    hash.finalize().into()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeExchangeV1 {
    pub response: bool,
    pub context: [u8; 32],
    pub challenge: [u8; 32],
    pub receive_ms: u64,
    pub send_ms: u64,
    pub residence_ns: u64,
}
impl TimeExchangeV1 {
    pub fn encode(&self) -> Result<[u8; EXCHANGE_BYTES]> {
        ensure!(self.challenge != [0; 32], "empty time challenge");
        ensure!(
            self.response || (self.receive_ms == 0 && self.send_ms == 0 && self.residence_ns == 0),
            "timestamps in time request"
        );
        let mut bytes = [0; EXCHANGE_BYTES];
        bytes[0] = 1;
        bytes[1] = u8::from(self.response);
        bytes[2..34].copy_from_slice(&self.context);
        bytes[34..66].copy_from_slice(&self.challenge);
        bytes[66..74].copy_from_slice(&self.receive_ms.to_be_bytes());
        bytes[74..82].copy_from_slice(&self.send_ms.to_be_bytes());
        bytes[82..90].copy_from_slice(&self.residence_ns.to_be_bytes());
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() == EXCHANGE_BYTES && bytes[0] == 1 && bytes[1] <= 1,
            "invalid time exchange version/length/kind"
        );
        let message = Self {
            response: bytes[1] == 1,
            context: bytes[2..34].try_into()?,
            challenge: bytes[34..66].try_into()?,
            receive_ms: u64::from_be_bytes(bytes[66..74].try_into()?),
            send_ms: u64::from_be_bytes(bytes[74..82].try_into()?),
            residence_ns: u64::from_be_bytes(bytes[82..90].try_into()?),
        };
        message.encode()?;
        Ok(message)
    }
    /// The owner captures at handoff, then queues this reply. Queueing delay
    /// remains in the measured uncertainty; it is not falsely subtracted.
    pub fn reply(&self, expected_context: [u8; 32], clock: ClockAnchorV1) -> Result<Self> {
        ensure!(
            !self.response && self.context == expected_context,
            "time request context mismatch"
        );
        self.encode()?;
        Ok(Self {
            response: true,
            receive_ms: clock.wall_ms(),
            send_ms: clock.wall_ms(),
            residence_ns: 0,
            ..self.clone()
        })
    }
}

struct Pending {
    challenge: [u8; 32],
    clock: ClockAnchorV1,
}

/// Constant storage per original authenticated peer. Three distinct exchanges
/// establish consistency, NOT three independent trusted clock sources.
#[derive(Default)]
pub struct PeerTimeReferenceV1 {
    generation: u64,
    context: Option<[u8; 32]>,
    anchor: Option<ClockAnchorV1>,
    last_poll: Option<Instant>,
    next_sample: Option<Instant>,
    pending: Option<Pending>,
    samples: Vec<PeerTimeSampleV1>,
    // Retained across remeasurement/session changes for this pinned peer. A
    // backward peer-clock correction cannot make already elapsed time valid.
    validity_floor: Option<(u64, Instant)>,
}
impl PeerTimeReferenceV1 {
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        self.samples.clear();
        self.anchor = None;
        self.last_poll = None;
        self.next_sample = None;
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Call before sampling AND before consuming a reference. Detects local
    /// wall jumps and long polling gaps. OS suspend notification owners may
    /// additionally call invalidate; no claim of detecting a stopped dual clock.
    pub fn observe(&mut self, context: [u8; 32], clock: ClockAnchorV1) {
        let now = clock.instant();
        let changed = self.context != Some(context)
            || self.anchor.is_some_and(|anchor| {
                anchor
                    .at(now)
                    .map_or(true, |wall| wall.abs_diff(clock.wall_ms()) > CLOCK_JUMP_MS)
            })
            || self.last_poll.is_some_and(|last| {
                now.checked_duration_since(last)
                    .is_none_or(|gap| gap > MAX_POLL_GAP)
            });
        if changed {
            self.invalidate();
        }
        self.context = Some(context);
        self.anchor.get_or_insert(clock);
        self.last_poll = Some(now);
        if self.pending.as_ref().is_some_and(|p| {
            now.checked_duration_since(p.clock.instant())
                .is_none_or(|age| age >= MEASUREMENT_LIFETIME)
        }) {
            self.pending = None;
        }
        if self.samples.first().is_some_and(|s| {
            s.peer_interval_at(now, REFERENCE_LIFETIME, DRIFT_PPM)
                .is_err()
        }) {
            self.invalidate();
        }
    }
    pub fn due(&self, now: Instant) -> bool {
        self.context.is_some()
            && self.pending.is_none()
            && self.next_sample.is_none_or(|next| now >= next)
    }
    pub fn begin(&mut self, clock: ClockAnchorV1, challenge: [u8; 32]) -> Result<TimeExchangeV1> {
        ensure!(
            self.due(clock.instant()),
            "time measurement busy or rate limited"
        );
        ensure!(challenge != [0; 32], "empty time challenge");
        let context = self.context.context("time context unavailable")?;
        self.pending = Some(Pending { challenge, clock });
        self.next_sample = Some(clock.instant() + SAMPLE_INTERVAL);
        Ok(TimeExchangeV1 {
            response: false,
            context,
            challenge,
            receive_ms: 0,
            send_ms: 0,
            residence_ns: 0,
        })
    }
    pub fn accept(&mut self, response: &TimeExchangeV1, clock: ClockAnchorV1) -> Result<()> {
        ensure!(
            response.response && Some(response.context) == self.context,
            "time response context mismatch"
        );
        let pending = self
            .pending
            .as_ref()
            .context("unsolicited/replayed time response")?;
        ensure!(
            response.challenge == pending.challenge,
            "time challenge mismatch"
        );
        let pending = self.pending.take().expect("checked pending");
        let now = clock.instant();
        ensure!(
            now.checked_duration_since(pending.clock.instant())
                .is_some_and(|age| age < MEASUREMENT_LIFETIME),
            "time response expired"
        );
        let sample = measure_peer_clock_v1(
            pending.clock,
            now,
            response.receive_ms,
            response.send_ms,
            response.residence_ns,
        )?;
        let interval = sample.peer_interval_at(now, REFERENCE_LIFETIME, DRIFT_PPM)?;
        for previous in &self.samples {
            let prior = previous.peer_interval_at(now, REFERENCE_LIFETIME, DRIFT_PPM)?;
            if interval.0 > prior.1 || prior.0 > interval.1 {
                self.invalidate();
                anyhow::bail!("TIME_UNTRUSTED: inconsistent peer clock samples");
            }
        }
        if self.samples.len() == REQUIRED_SAMPLES {
            self.samples.remove(0);
        }
        self.samples.push(sample);
        if self.samples.len() == REQUIRED_SAMPLES {
            self.next_sample = Some(now + REFRESH_INTERVAL);
        }
        Ok(())
    }
    pub fn interval(&self, now: Instant) -> Result<(u64, u64)> {
        ensure!(
            self.samples.len() == REQUIRED_SAMPLES,
            "TIME_UNTRUSTED: insufficient samples"
        );
        // Hull, not midpoint or voting. Do not turn repeated measurements into
        // an unjustifiably narrow uncertainty interval.
        let mut lower = u64::MAX;
        let mut upper = 0;
        for sample in &self.samples {
            let bounds = sample.peer_interval_at(now, REFERENCE_LIFETIME, DRIFT_PPM)?;
            lower = lower.min(bounds.0);
            upper = upper.max(bounds.1);
        }
        Ok((lower, upper))
    }
    /// Entire possible time interval must lie within the ORIGINAL message
    /// lifetime. Returned deadline is immutable and conservative (upper end).
    pub fn validity_deadline(
        &mut self,
        issued: u64,
        expires: u64,
        now: Instant,
    ) -> Result<Instant> {
        let (ready, deadline) = self.validity_window(issued, expires, now)?;
        ensure!(ready <= now, "TIME_UNTRUSTED: validity not established");
        Ok(deadline)
    }
    /// Freeze one bounded waiting window from an ALREADY established reference.
    /// A consumer may wait for the lower bound to reach issued, but must never
    /// refresh this window or wait past its original upper-bound deadline.
    pub fn validity_window(
        &mut self,
        issued: u64,
        expires: u64,
        now: Instant,
    ) -> Result<(Instant, Instant)> {
        let (lower, upper) = self.interval(now)?;
        let mut next_floor = lower;
        if let Some((floor, at)) = self.validity_floor {
            let elapsed = now
                .checked_duration_since(at)
                .context("validity monotonic rollback")?;
            // Advance the floor conservatively under the same rate bound.
            let elapsed_ms = elapsed.as_millis();
            let advance =
                elapsed_ms.saturating_sub((elapsed_ms * u128::from(DRIFT_PPM)).div_ceil(1_000_000));
            let floor = floor
                .checked_add(advance.try_into()?)
                .context("validity floor overflow")?;
            ensure!(
                upper >= floor && expires > floor,
                "TIME_UNTRUSTED: peer time regressed"
            );
            next_floor = next_floor.max(floor);
        }
        self.validity_floor = Some((next_floor, now));
        ensure!(
            issued <= upper && upper < expires,
            "TIME_UNTRUSTED: validity not established"
        );
        let wait = (u128::from(issued.saturating_sub(lower)) * 1_000_000)
            .div_ceil(1_000_000 - u128::from(DRIFT_PPM));
        let remaining =
            u128::from(expires - upper) * 1_000_000 / (1_000_000 + u128::from(DRIFT_PPM));
        let ready = now
            .checked_add(Duration::from_millis(wait.try_into()?))
            .context("validity start overflow")?;
        let deadline = now
            .checked_add(Duration::from_millis(remaining.try_into()?))
            .context("validity deadline overflow")?;
        ensure!(
            ready < deadline,
            "TIME_UNTRUSTED: no certain validity window"
        );
        Ok((ready, deadline))
    }
}

#[cfg(test)]
mod tests;
