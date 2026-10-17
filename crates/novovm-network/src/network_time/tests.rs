use super::*;

const LOCAL: u64 = 10_000_000;

fn measurement(
    offset: i128,
    forward_ms: u64,
    residence_ms: u64,
    reverse_ms: u64,
) -> PeerTimeSampleV1 {
    let start = Instant::now();
    let remote_receive = (i128::from(LOCAL) + offset + i128::from(forward_ms))
        .try_into()
        .unwrap();
    measure_peer_clock_v1(
        ClockAnchorV1::from_parts(LOCAL, start),
        start + Duration::from_millis(forward_ms + residence_ms + reverse_ms),
        remote_receive,
        remote_receive + residence_ms,
        residence_ms * 1_000_000,
    )
    .unwrap()
}

#[test]
fn anchor_advances_monotonically_with_checked_integer_milliseconds() {
    let start = Instant::now();
    let anchor = ClockAnchorV1::from_parts(1000, start);
    assert_eq!(anchor.instant(), start);
    assert_eq!(anchor.wall_ms(), 1000);
    assert_eq!(anchor.at(start).unwrap(), 1000);
    assert_eq!(
        anchor.at(start + Duration::from_nanos(999_999)).unwrap(),
        1000
    );
    assert_eq!(
        anchor.at(start + Duration::from_nanos(1_999_999)).unwrap(),
        1001
    );
    // Windows Instant may round a one-nanosecond subtraction to the same tick.
    let earlier = start - Duration::from_millis(1);
    assert!(earlier < start);
    assert!(anchor.at(earlier).is_err());
    assert!(ClockAnchorV1::from_parts(u64::MAX, start)
        .at(start + Duration::from_millis(1))
        .is_err());
}

#[test]
fn injected_anchor_preserves_platform_pair_without_sampling_or_utc_qualification() {
    // A platform supplies a pair under from_parts' <=1ms precision contract.
    // The pure constructor cannot measure that contract or certify the value.
    let at = Instant::now();
    let anchor = ClockAnchorV1::from_parts(123, at);
    assert_eq!(anchor.instant(), at);
    assert_eq!(anchor.wall_ms(), 123);
    assert_eq!(anchor.at(at + Duration::from_millis(7)).unwrap(), 130);
}

#[test]
fn hour_scale_positive_and_negative_offsets_remain_signed() {
    for offset in [-7_200_000i128, -3_600_000, 3_600_000, 86_400_000] {
        let sample = measurement(offset, 2, 3, 15);
        assert_eq!(sample.offset_bounds_ms(), (offset - 19, offset + 6));
        assert_eq!(sample.round_trip(), Duration::from_millis(17));
        let (lower, upper) = sample
            .peer_interval_at(sample.observed_at(), Duration::from_secs(1), 0)
            .unwrap();
        let actual = i128::from(LOCAL + 20) + offset;
        assert!(i128::from(lower) <= actual && actual <= i128::from(upper));
    }
}

#[test]
fn either_direction_can_contain_all_network_delay() {
    let offset = -2_000_000;
    for (forward, reverse) in [(0, 900), (900, 0), (1, 899), (899, 1), (450, 450)] {
        let sample = measurement(offset, forward, 4, reverse);
        let (lower, upper) = sample.offset_bounds_ms();
        assert_eq!(lower, offset - i128::from(reverse) - 4);
        assert_eq!(upper, offset + i128::from(forward) + 4);
        assert!(lower <= offset && offset <= upper);
        assert_eq!(upper - lower, 908);
        assert_eq!(sample.round_trip(), Duration::from_millis(900));
    }
}

#[test]
fn fractional_residence_uses_exact_nanoseconds_and_outward_quantization() {
    let start = Instant::now();
    let sample = measure_peer_clock_v1(
        ClockAnchorV1::from_parts(10_000, start),
        start + Duration::from_nanos(1_999_999),
        20_000,
        20_000,
        999_999,
    )
    .unwrap();
    assert_eq!(sample.round_trip(), Duration::from_millis(1));
    assert_eq!(sample.offset_bounds_ms(), (9995, 10_004));
    assert_eq!(
        sample
            .peer_interval_at(sample.observed_at(), Duration::from_secs(1), 0)
            .unwrap(),
        (19_996, 20_005)
    );
    let zero = measure_peer_clock_v1(
        ClockAnchorV1::from_parts(10_000, start),
        start,
        10_000,
        10_000,
        0,
    )
    .unwrap();
    assert_eq!(zero.round_trip(), Duration::ZERO);
    assert_eq!(zero.offset_bounds_ms(), (-4, 4));
}

#[test]
fn negative_rtt_inconsistent_residence_and_timestamp_overflow_are_rejected() {
    let start = Instant::now();
    let client = ClockAnchorV1::from_parts(LOCAL, start);
    assert!(measure_peer_clock_v1(
        client,
        start + Duration::from_millis(1),
        100,
        102,
        2_000_000
    )
    .is_err());
    assert!(measure_peer_clock_v1(
        client,
        start + Duration::from_millis(2),
        100,
        102,
        1_999_999
    )
    .is_err());
    assert!(measure_peer_clock_v1(
        client,
        start + Duration::from_millis(2),
        100,
        100,
        1_000_000
    )
    .is_err());
    assert!(measure_peer_clock_v1(client, start, 100, 99, 0).is_err());
    assert!(measure_peer_clock_v1(
        client,
        start + Duration::from_millis(1),
        u64::MAX,
        0,
        1_000_000
    )
    .is_err());
    assert!(measure_peer_clock_v1(client, start - Duration::from_millis(1), 100, 100, 0).is_err());
}

#[test]
fn original_measurement_budget_is_exclusive_and_includes_peer_residence() {
    let start = Instant::now();
    let client = ClockAnchorV1::from_parts(LOCAL, start);
    let at_limit = start + NETWORK_TIME_MAX_MEASUREMENT_V1;
    assert!(measure_peer_clock_v1(
        client,
        at_limit - Duration::from_nanos(1),
        100,
        30_099,
        29_999_999_999,
    )
    .is_ok());
    assert!(measure_peer_clock_v1(client, at_limit, 100, 30_100, 30_000_000_000).is_err());
    assert!(measure_peer_clock_v1(
        client,
        at_limit + Duration::from_nanos(1),
        100,
        30_100,
        30_000_000_000,
    )
    .is_err());
}

#[test]
fn monotonic_age_advances_candidate_and_drift_widens_both_sides() {
    let sample = measurement(3_600_000, 2, 3, 15);
    let observed = sample.observed_at();
    let base = sample
        .peer_interval_at(observed, Duration::from_secs(61), 100)
        .unwrap();
    // 60s at 100 ppm is exactly 6ms additional relative uncertainty.
    assert_eq!(
        sample
            .peer_interval_at(
                observed + Duration::from_secs(60),
                Duration::from_secs(61),
                100
            )
            .unwrap(),
        (base.0 + 60_000 - 6, base.1 + 60_000 + 6)
    );
    // Even a fractional-millisecond uncertainty rounds out to a full millisecond.
    assert_eq!(
        sample
            .peer_interval_at(
                observed + Duration::from_nanos(1),
                Duration::from_secs(1),
                1
            )
            .unwrap(),
        (base.0 - 1, base.1 + 1)
    );
}

#[test]
fn stale_zero_age_budget_and_monotonic_rollback_are_rejected() {
    let sample = measurement(0, 1, 1, 1);
    let observed = sample.observed_at();
    assert!(sample
        .peer_interval_at(observed, Duration::ZERO, 0)
        .is_err());
    assert!(sample
        .peer_interval_at(observed + Duration::from_secs(1), Duration::from_secs(1), 0)
        .is_err());
    assert!(sample
        .peer_interval_at(
            observed - Duration::from_millis(1),
            Duration::from_secs(1),
            0
        )
        .is_err());
}

#[test]
fn utc_bound_underflow_overflow_and_later_anchor_overflow_fail_closed() {
    let start = Instant::now();
    let client = ClockAnchorV1::from_parts(LOCAL, start);
    let below_epoch = measure_peer_clock_v1(client, start, 0, 0, 0).unwrap();
    assert!(below_epoch
        .peer_interval_at(start, Duration::from_secs(1), 0)
        .is_err());
    let above_range = measure_peer_clock_v1(client, start, u64::MAX, u64::MAX, 0).unwrap();
    assert!(above_range
        .peer_interval_at(start, Duration::from_secs(1), 0)
        .is_err());
    let near_end = measure_peer_clock_v1(
        ClockAnchorV1::from_parts(u64::MAX - 10, start),
        start,
        100,
        100,
        0,
    )
    .unwrap();
    assert!(near_end
        .peer_interval_at(start + Duration::from_millis(11), Duration::from_secs(1), 0)
        .is_err());
    let huge_drift = measure_peer_clock_v1(client, start, 100, 100, 0).unwrap();
    assert!(huge_drift
        .peer_interval_at(
            start + Duration::from_secs(1),
            Duration::from_secs(2),
            u32::MAX
        )
        .is_err());
}
