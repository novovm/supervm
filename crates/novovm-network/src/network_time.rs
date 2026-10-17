//! Bounded relative clock candidates from an authenticated four-timestamp
//! exchange. Authentication of a peer does not make its clock trusted UTC.
//! This module owns no transport, scheduler, source policy or system-clock IO
//! beyond capturing a local candidate anchor. It never changes a business TTL.

use anyhow::{ensure, Context, Result};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const NETWORK_TIME_MAX_MEASUREMENT_V1: Duration = Duration::from_secs(30);
// Each endpoint has at most 1ms capture uncertainty plus whole-millisecond
// quantization. Cover both endpoints and never round an interval inward.
const QUANTIZATION_MARGIN_MS: i128 = 4;

/// A local UTC candidate advanced only by elapsed monotonic time. This is not
/// evidence of UTC accuracy or permission to use a signed business credential.
#[derive(Clone, Copy, Debug)]
pub struct ClockAnchorV1 {
    wall_ms: u64,
    monotonic: Instant,
}

impl ClockAnchorV1 {
    pub fn capture() -> Result<Self> {
        let before = Instant::now();
        let wall = SystemTime::now();
        let after = Instant::now();
        ensure!(
            after
                .checked_duration_since(before)
                .is_some_and(|elapsed| elapsed <= Duration::from_millis(1)),
            "local clock capture precision exceeds 1 ms"
        );
        let wall_ms = wall
            .duration_since(UNIX_EPOCH)
            .context("local clock precedes UTC epoch")?
            .as_millis()
            .try_into()
            .context("local clock milliseconds overflow")?;
        // Use the later monotonic reading. The wall reading precedes it by at
        // most 1ms; an interrupted capture fails instead of hiding that delay.
        Ok(Self::from_parts(wall_ms, after))
    }

    /// Platform-injected candidate. The platform must pair the wall reading
    /// with `at` within 1ms, matching capture's precision contract. This method
    /// preserves the supplied pair exactly; it cannot verify platform sampling
    /// precision and implies no source qualification.
    pub fn from_parts(wall_ms: u64, at: Instant) -> Self {
        Self {
            wall_ms,
            monotonic: at,
        }
    }

    pub fn instant(&self) -> Instant {
        self.monotonic
    }

    pub fn wall_ms(&self) -> u64 {
        self.wall_ms
    }

    pub fn at(&self, now: Instant) -> Result<u64> {
        let elapsed = now
            .checked_duration_since(self.monotonic)
            .context("local monotonic clock moved backwards")?;
        let elapsed_ms: u64 = elapsed
            .as_millis()
            .try_into()
            .context("local elapsed milliseconds overflow")?;
        self.wall_ms
            .checked_add(elapsed_ms)
            .context("local UTC candidate overflow")
    }
}

/// A relative peer-clock measurement, not a UTC source qualification. The
/// offset interval permits either direction to contain all network delay.
#[derive(Clone, Copy, Debug)]
pub struct PeerTimeSampleV1 {
    client: ClockAnchorV1,
    observed_at: Instant,
    offset_lower_ms: i128,
    offset_upper_ms: i128,
    round_trip: Duration,
}

impl PeerTimeSampleV1 {
    pub fn offset_bounds_ms(&self) -> (i128, i128) {
        (self.offset_lower_ms, self.offset_upper_ms)
    }

    /// Local elapsed time minus the peer's independently measured residence
    /// duration. Residence spans its receive/send events and may include
    /// handshake network waits; it is not CPU processing time. Neither elapsed
    /// duration comes from subtracting wall clocks.
    pub fn round_trip(&self) -> Duration {
        self.round_trip
    }

    pub fn observed_at(&self) -> Instant {
        self.observed_at
    }

    /// Advance this peer candidate without reading either system clock again.
    /// `max_age` is exclusive; refreshing a sample cannot extend an existing
    /// authorization or operation deadline. `drift_ppm` bounds additional
    /// relative-rate uncertainty since receipt, and is rounded outward.
    pub fn peer_interval_at(
        &self,
        now: Instant,
        max_age: Duration,
        drift_ppm: u32,
    ) -> Result<(u64, u64)> {
        let age = now
            .checked_duration_since(self.observed_at)
            .context("peer sample monotonic clock moved backwards")?;
        ensure!(age < max_age, "peer clock sample expired");
        // age_ns * ppm / 1e6 gives uncertainty in nanoseconds; divide by
        // another 1e6 to obtain milliseconds, retaining the full remainder.
        let numerator = age
            .as_nanos()
            .checked_mul(u128::from(drift_ppm))
            .context("peer clock drift multiplication overflow")?;
        const DRIFT_DENOMINATOR: u128 = 1_000_000_000_000;
        let drift_ms =
            numerator / DRIFT_DENOMINATOR + u128::from(numerator % DRIFT_DENOMINATOR != 0);
        let drift_ms: i128 = drift_ms
            .try_into()
            .context("peer clock drift milliseconds overflow")?;
        let local_now = i128::from(self.client.at(now)?);
        let lower = local_now
            .checked_add(self.offset_lower_ms)
            .and_then(|value| value.checked_sub(drift_ms))
            .context("peer clock lower bound overflow")?;
        let upper = local_now
            .checked_add(self.offset_upper_ms)
            .and_then(|value| value.checked_add(drift_ms))
            .context("peer clock upper bound overflow")?;
        ensure!(lower <= upper, "peer clock interval reversed");
        Ok((
            lower
                .try_into()
                .context("peer clock lower bound outside UTC range")?,
            upper
                .try_into()
                .context("peer clock upper bound outside UTC range")?,
        ))
    }
}

/// Bound peer minus local clock offset using the four-timestamp ordering:
/// `t3 - t4 <= offset <= t2 - t1`, widened for millisecond quantization.
/// Unlike the midpoint estimator, this does not assume symmetric path delays.
///
/// The caller must authenticate and bind the exchange's nonce, session, peer
/// and source generation. This pure calculation cannot verify those facts or
/// confer UTC authority on the peer's claimed receive/send timestamps.
pub fn measure_peer_clock_v1(
    client: ClockAnchorV1,
    received_at: Instant,
    remote_receive_ms: u64,
    remote_send_ms: u64,
    remote_elapsed_ns: u64,
) -> Result<PeerTimeSampleV1> {
    let elapsed = received_at
        .checked_duration_since(client.instant())
        .context("measurement monotonic clock moved backwards")?;
    ensure!(
        elapsed < NETWORK_TIME_MAX_MEASUREMENT_V1,
        "peer clock measurement exceeded original budget"
    );
    let residence = Duration::from_nanos(remote_elapsed_ns);
    ensure!(
        residence <= elapsed,
        "peer clock measurement has negative network RTT"
    );
    ensure!(
        remote_receive_ms.checked_add(remote_elapsed_ns / 1_000_000) == Some(remote_send_ms),
        "peer clock send timestamp disagrees with monotonic residence"
    );
    let local_receive_ms = client.at(received_at)?;
    let offset_lower_ms = i128::from(remote_send_ms)
        .checked_sub(i128::from(local_receive_ms))
        .and_then(|value| value.checked_sub(QUANTIZATION_MARGIN_MS))
        .context("peer clock lower offset overflow")?;
    let offset_upper_ms = i128::from(remote_receive_ms)
        .checked_sub(i128::from(client.wall_ms()))
        .and_then(|value| value.checked_add(QUANTIZATION_MARGIN_MS))
        .context("peer clock upper offset overflow")?;
    ensure!(
        offset_lower_ms <= offset_upper_ms,
        "peer clock offset interval reversed"
    );
    Ok(PeerTimeSampleV1 {
        client,
        observed_at: received_at,
        offset_lower_ms,
        offset_upper_ms,
        round_trip: elapsed
            .checked_sub(residence)
            .context("negative network RTT")?,
    })
}

#[cfg(test)]
mod tests;
