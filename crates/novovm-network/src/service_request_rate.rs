//! Windowed default request-rate feedback inside ServiceConcurrencyV1.
//! No clock task, queue, authorization or second resource owner.
use super::ResourcePressureV1;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(2);
const CONFIRM: u8 = 2;

#[derive(Clone, Copy, Default)]
pub(crate) struct RequestRateObservationV1 {
    pub admitted: u64,
    pub admission_us: u64,
    pub successful: [u64; 5], // Put, Fetch, Consume, other, Watch
    pub failed: u64,
    pub operation_us: u64,  // excludes idle Watch and policy waiting
    pub payload_bytes: u64, // encrypted Put/Fetch bytes, no payload retained
}
#[derive(Clone, Copy)]
pub(crate) struct RequestRateLimitV1 {
    pub per_second: u32,
    pub burst: u32,
}
#[derive(Clone, Copy)]
pub(super) struct RateSignals {
    pub pressure: ResourcePressureV1,
    pub memory_busy: bool,
    pub execution_backoffs: u64,
    pub wire_refusals: u64,
    pub service_us: u64,
    pub work: usize,
}
#[derive(Clone, Copy)]
struct Sample {
    at: Instant,
    observation: RequestRateObservationV1,
    signals: RateSignals,
}
#[derive(Clone, Copy)]
struct UsefulWindow {
    throughput: u64, // milli-requests per second
    operation_us: u64,
    payload_bytes: u64,
    service_us: u64,
    mix: [u64; 5],
}
struct GrowthProbe {
    previous_rate: u32,
    baseline: UsefulWindow,
    windows: u8,
    plateau: u8,
}
pub(super) struct RequestRateState {
    rate: u32,
    sample: Option<Sample>,
    good: u8,
    bad: u8,
    probe: Option<GrowthProbe>,
    cooldown: Option<Instant>,
}
impl RequestRateState {
    pub fn new() -> Self {
        // Preserve the working32/s bootstrap on small cloud and large mobile
        // hosts alike. It is not a permanent limit or throughput claim.
        Self {
            rate: 32,
            sample: None,
            good: 0,
            bad: 0,
            probe: None,
            cooldown: None,
        }
    }
    fn limits(&self, work: usize) -> RequestRateLimitV1 {
        RequestRateLimitV1 {
            per_second: self.rate,
            // Work-derived burst credit is independent of the refill rate. It
            // grows only with execution resources; changing it mints no tokens.
            // Existing work/body reservations remain authoritative.
            burst: work.saturating_mul(8).clamp(1, u32::MAX as usize) as u32,
        }
    }
    pub fn observe(
        &mut self,
        observation: RequestRateObservationV1,
        signals: RateSignals,
        now: Instant,
    ) -> RequestRateLimitV1 {
        let current = Sample {
            at: now,
            observation,
            signals,
        };
        let Some(previous) = self.sample else {
            self.sample = Some(current);
            return self.limits(signals.work);
        };
        if (previous.signals.pressure != ResourcePressureV1::Normal || previous.signals.memory_busy)
            && signals.pressure == ResourcePressureV1::Normal
            && !signals.memory_busy
        {
            // Resume the already-proven bootstrap after platform recovery,
            // not minutes of rate1 after a thermal pause. No bucket credit is
            // added; the original memory/execution gates still protect work.
            self.rate = self.rate.max(32);
            self.reset_learning();
            self.cooldown = None;
            self.sample = Some(current);
            return self.limits(signals.work);
        }
        let elapsed = now.saturating_duration_since(previous.at);
        if elapsed < WINDOW {
            return self.limits(signals.work);
        }
        self.sample = Some(current);
        let delta = |a: u64, b: u64| a.checked_sub(b);
        let mut successful = [0; 5];
        for (i, item) in successful.iter_mut().enumerate() {
            let Some(value) = delta(
                observation.successful[i],
                previous.observation.successful[i],
            ) else {
                self.reset_learning();
                return self.limits(signals.work);
            };
            *item = value;
        }
        let Some(admitted) = delta(observation.admitted, previous.observation.admitted) else {
            self.reset_learning();
            return self.limits(signals.work);
        };
        let Some(wait_us) = delta(observation.admission_us, previous.observation.admission_us)
        else {
            self.reset_learning();
            return self.limits(signals.work);
        };
        let Some(failed) = delta(observation.failed, previous.observation.failed) else {
            self.reset_learning();
            return self.limits(signals.work);
        };
        let Some(operation_us) = delta(observation.operation_us, previous.observation.operation_us)
        else {
            self.reset_learning();
            return self.limits(signals.work);
        };
        let Some(payload_bytes) = delta(
            observation.payload_bytes,
            previous.observation.payload_bytes,
        ) else {
            self.reset_learning();
            return self.limits(signals.work);
        };
        let pressured = signals.pressure != ResourcePressureV1::Normal || signals.memory_busy;
        let backoff = signals.execution_backoffs > previous.signals.execution_backoffs;
        // A lone execution outlier pauses growth. Two consecutive pressured
        // execution windows are needed for a rate reduction; platform pressure
        // is authoritative independently of those local elapsed samples.
        self.bad = if backoff {
            self.bad.saturating_add(1)
        } else {
            0
        };
        if pressured || self.bad >= CONFIRM {
            self.rate = self.rate.saturating_sub((self.rate / 4).max(1)).max(1);
            self.reset_learning();
            self.cooldown = Some(now + WINDOW * 2);
            return self.limits(signals.work);
        }
        let total = successful.iter().copied().fold(0u64, u64::saturating_add);
        // Sparse/idle intervals and counter restarts cannot supply a slope.
        if total < u64::from(self.rate.min(8)) || admitted == 0 || elapsed > WINDOW * 4 {
            self.reset_learning();
            return self.limits(signals.work);
        }
        let micros = elapsed.as_micros().max(1);
        let useful = UsefulWindow {
            throughput: ((u128::from(total) * 1_000_000_000) / micros).min(u128::from(u64::MAX))
                as u64,
            operation_us: operation_us / total.saturating_sub(successful[4]).max(1),
            payload_bytes: payload_bytes / total.saturating_sub(successful[4]).max(1),
            service_us: signals.service_us,
            mix: successful,
        };
        let clean =
            failed == 0 && !backoff && signals.wire_refusals == previous.signals.wire_refusals;
        if let Some(probe) = &mut self.probe {
            if clean && comparable(useful, probe.baseline) {
                probe.windows = probe.windows.saturating_add(1);
                let flat = u128::from(useful.throughput) * 100
                    <= u128::from(probe.baseline.throughput) * 105;
                let slower = u128::from(useful.operation_us) * 4
                    > u128::from(probe.baseline.operation_us.max(1)) * 5;
                probe.plateau = if flat && slower {
                    probe.plateau.saturating_add(1)
                } else {
                    0
                };
                if probe.plateau >= CONFIRM {
                    self.rate = probe.previous_rate;
                    self.reset_learning();
                    self.cooldown = Some(now + WINDOW * 4);
                    return self.limits(signals.work);
                }
                if probe.windows >= CONFIRM {
                    self.probe = None;
                }
            } else {
                // Different work mix/size (service cost), failures or a network
                // change invalidate the finite-difference comparison.
                self.probe = None;
                self.good = 0;
            }
        }
        let average_wait = wait_us / admitted;
        let quota_busy = average_wait > 1_000
            && u128::from(average_wait) * u128::from(self.rate) * 2 >= 1_000_000
            && u128::from(admitted) * 1_000_000 * 4 >= u128::from(self.rate) * micros * 3;
        self.good = if clean && quota_busy {
            self.good.saturating_add(1)
        } else {
            0
        };
        if self.good >= CONFIRM
            && self.probe.is_none()
            && self.cooldown.is_none_or(|until| now >= until)
        {
            let previous_rate = self.rate;
            // The numeric ceiling is nanosecond token-clock representation,
            // not a service/hardware target. There is no adaptive128 ceiling.
            self.rate = self
                .rate
                .saturating_add((self.rate / 4).max(1))
                .min(1_000_000_000);
            self.probe = Some(GrowthProbe {
                previous_rate,
                baseline: useful,
                windows: 0,
                plateau: 0,
            });
            self.good = 0;
        }
        self.limits(signals.work)
    }
    fn reset_learning(&mut self) {
        self.good = 0;
        self.bad = 0;
        self.probe = None;
    }
}
fn comparable(a: UsefulWindow, b: UsefulWindow) -> bool {
    let sum = |x: [u64; 5]| x.iter().copied().fold(0u64, u64::saturating_add) as u128;
    let (at, bt) = (sum(a.mix), sum(b.mix));
    let change = a.mix.iter().zip(b.mix).fold(0u128, |v, (a, b)| {
        v.saturating_add((u128::from(*a) * bt).abs_diff(u128::from(b) * at))
    });
    let comparable_cost = if a.service_us == 0 || b.service_us == 0 {
        a.service_us == b.service_us
    } else {
        u128::from(a.service_us) * 4 <= u128::from(b.service_us) * 5
            && u128::from(b.service_us) * 4 <= u128::from(a.service_us) * 5
    };
    let comparable_bytes = u128::from(a.payload_bytes) * 4 <= u128::from(b.payload_bytes) * 5
        && u128::from(b.payload_bytes) * 4 <= u128::from(a.payload_bytes) * 5;
    at > 0 && bt > 0 && change <= at * bt / 2 && comparable_cost && comparable_bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    fn signals() -> RateSignals {
        RateSignals {
            pressure: ResourcePressureV1::Normal,
            memory_busy: false,
            execution_backoffs: 0,
            wire_refusals: 0,
            service_us: 1_000,
            work: 8,
        }
    }
    fn advance(obs: &mut RequestRateObservationV1, requests: u64, wait_us: u64, cost_us: u64) {
        obs.admitted += requests;
        obs.admission_us += requests * wait_us;
        obs.successful[0] += requests / 2;
        obs.successful[1] += requests / 4;
        obs.successful[2] += requests - requests / 2 - requests / 4;
        obs.operation_us += requests * cost_us;
        obs.payload_bytes += requests * 32 * 1024;
    }
    #[test]
    fn default_rate_requires_two_busy_successful_windows_and_can_cross128() {
        let at = Instant::now();
        let mut s = RequestRateState::new();
        let mut o = RequestRateObservationV1::default();
        assert_eq!(s.observe(o, signals(), at).per_second, 32);
        for i in 1..=3 {
            advance(&mut o, 64, 0, 1000); // traffic alone never grows
            assert_eq!(s.observe(o, signals(), at + WINDOW * i).per_second, 32);
        }
        advance(&mut o, 64, 31_250, 1000);
        assert_eq!(s.observe(o, signals(), at + WINDOW * 4).per_second, 32);
        advance(&mut o, 64, 31_250, 1000);
        assert_eq!(s.observe(o, signals(), at + WINDOW * 5).per_second, 40);
        for i in 6..=24 {
            let rate = s.rate;
            advance(
                &mut o,
                u64::from(rate) * 2,
                1_000_000 / u64::from(rate),
                1000,
            );
            s.observe(o, signals(), at + WINDOW * i);
        }
        assert!(s.rate > 128, "128 is not an adaptive rate ceiling");
        assert_eq!(
            s.limits(8).burst,
            64,
            "rate growth alone does not grow burst credit"
        );
    }
    #[test]
    fn failures_network_pressure_and_single_execution_outlier_do_not_grow_or_cut_rate() {
        let at = Instant::now();
        let mut s = RequestRateState::new();
        let mut o = RequestRateObservationV1::default();
        let mut sig = signals();
        s.observe(o, sig, at);
        for i in 1..=2 {
            advance(&mut o, 64, 31_250, 1000);
            o.failed += 1;
            assert_eq!(s.observe(o, sig, at + WINDOW * i).per_second, 32);
        }
        sig.execution_backoffs = 1;
        advance(&mut o, 64, 31_250, 1000);
        assert_eq!(s.observe(o, sig, at + WINDOW * 3).per_second, 32);
        sig.wire_refusals = 1;
        advance(&mut o, 64, 31_250, 1000);
        assert_eq!(s.observe(o, sig, at + WINDOW * 4).per_second, 32);
        for i in 5..=6 {
            sig.execution_backoffs += 1;
            advance(&mut o, 64, 31_250, 1000);
            s.observe(o, sig, at + WINDOW * i);
        }
        assert_eq!(
            s.rate, 24,
            "persistent execution pressure reduces the default rate"
        );
    }
    #[test]
    fn real_resource_pressure_reduces_admission_without_needing_failures() {
        let at = Instant::now();
        let mut s = RequestRateState::new();
        let o = RequestRateObservationV1::default();
        s.observe(o, signals(), at);
        let pressure = RateSignals {
            pressure: ResourcePressureV1::Severe,
            work: 1,
            ..signals()
        };
        let limits = s.observe(o, pressure, at + WINDOW);
        assert_eq!((limits.per_second, limits.burst), (24, 8));
        let normal = s.observe(o, signals(), at + WINDOW * 2);
        assert_eq!(
            normal.per_second, 32,
            "platform recovery restores only the proven bootstrap"
        );
    }
    #[test]
    fn flat_throughput_with_queue_growth_rolls_back_one_probe_then_cools_down() {
        let at = Instant::now();
        let mut s = RequestRateState::new();
        let mut o = RequestRateObservationV1::default();
        s.observe(o, signals(), at);
        for i in 1..=2 {
            advance(&mut o, 64, 31_250, 1000);
            s.observe(o, signals(), at + WINDOW * i);
        }
        assert_eq!(s.rate, 40);
        advance(&mut o, 64, 25_000, 2000);
        assert_eq!(
            s.observe(o, signals(), at + WINDOW * 3).per_second,
            40,
            "one window cannot roll back"
        );
        advance(&mut o, 64, 25_000, 2000);
        assert_eq!(s.observe(o, signals(), at + WINDOW * 4).per_second, 32);
        for i in 5..=7 {
            advance(&mut o, 64, 31_250, 1000);
            assert_eq!(s.observe(o, signals(), at + WINDOW * i).per_second, 32);
        }
    }
    #[test]
    fn changed_work_and_idle_gaps_invalidate_a_marginal_comparison() {
        let at = Instant::now();
        let mut s = RequestRateState::new();
        let mut o = RequestRateObservationV1::default();
        s.observe(o, signals(), at);
        for i in 1..=2 {
            advance(&mut o, 64, 31_250, 1000);
            s.observe(o, signals(), at + WINDOW * i);
        }
        assert_eq!(s.rate, 40);
        advance(&mut o, 64, 25_000, 4000);
        let different = RateSignals {
            service_us: 4000,
            ..signals()
        };
        assert_eq!(s.observe(o, different, at + WINDOW * 3).per_second, 40);
        assert!(
            s.probe.is_none(),
            "larger service cost cannot supply the old workload slope"
        );
        assert_eq!(s.observe(o, different, at + WINDOW * 10).per_second, 40);
        assert!(s.probe.is_none());
        assert_eq!(
            s.observe(
                RequestRateObservationV1::default(),
                signals(),
                at + WINDOW * 11
            )
            .per_second,
            40
        );
    }
    #[test]
    fn cooldown_boundary_and_watch_only_traffic_do_not_manufacture_a_slope() {
        let at = Instant::now();
        let mut o = RequestRateObservationV1::default();
        advance(&mut o, 64, 31_250, 1000);
        for (offset, expected) in [(Duration::from_nanos(1), 32), (Duration::ZERO, 40)] {
            let mut s = RequestRateState::new();
            s.observe(
                RequestRateObservationV1::default(),
                signals(),
                at + WINDOW - Duration::from_millis(1),
            );
            s.good = 1;
            s.cooldown = Some(at + WINDOW * 2);
            assert_eq!(
                s.observe(o, signals(), at + WINDOW * 2 - offset).per_second,
                expected
            );
        }
        let mut s = RequestRateState::new();
        let mut watch = RequestRateObservationV1::default();
        s.observe(watch, signals(), at);
        for i in 1..=4 {
            watch.admitted += 64;
            watch.successful[4] += 64;
            assert_eq!(s.observe(watch, signals(), at + WINDOW * i).per_second, 32);
        }
    }
    #[test]
    fn payload_size_or_counter_restart_cannot_roll_back_the_old_workload_probe() {
        let at = Instant::now();
        let mut s = RequestRateState::new();
        let mut o = RequestRateObservationV1::default();
        s.observe(o, signals(), at);
        for i in 1..=2 {
            advance(&mut o, 64, 31_250, 1000);
            s.observe(o, signals(), at + WINDOW * i);
        }
        assert_eq!(s.rate, 40);
        advance(&mut o, 64, 25_000, 4000);
        o.payload_bytes += 64 * 1024 * 1024;
        assert_eq!(s.observe(o, signals(), at + WINDOW * 3).per_second, 40);
        assert!(
            s.probe.is_none(),
            "different object size is not a throughput slope"
        );
        s.observe(
            RequestRateObservationV1::default(),
            signals(),
            at + WINDOW * 4,
        );
        assert_eq!(s.rate, 40);
        assert!(s.probe.is_none());
        assert_eq!(s.good, 0);
    }
}
