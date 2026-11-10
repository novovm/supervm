use super::*;

fn clock(start: Instant, ms: u64, offset: i64) -> ClockAnchorV1 {
    ClockAnchorV1::from_parts(
        (1_000_000i64 + ms as i64 + offset) as u64,
        start + Duration::from_millis(ms),
    )
}
fn sample(state: &mut PeerTimeReferenceV1, start: Instant, ms: u64, offset: i64, nonce: u8) {
    let local = clock(start, ms, 0);
    state.observe([1; 32], local);
    let request = state.begin(local, [nonce; 32]).unwrap();
    let response = request
        .reply([1; 32], clock(start, ms + 5, offset))
        .unwrap();
    let received = clock(start, ms + 10, 0);
    state.observe([1; 32], received);
    state.accept(&response, received).unwrap();
}
fn ready(offset: i64) -> (PeerTimeReferenceV1, Instant) {
    let start = Instant::now();
    let mut state = PeerTimeReferenceV1::default();
    for i in 0..3 {
        sample(&mut state, start, i * 300, offset, i as u8 + 1);
    }
    (state, start)
}
#[test]
fn fast_and_slow_clocks_need_no_system_sync() {
    for offset in [-500, 444, 3600000] {
        let (mut state, start) = ready(offset);
        let now = start + Duration::from_millis(700);
        let actual = (1_000_700i64 + offset) as u64;
        let (lower, upper) = state.interval(now).unwrap();
        assert!(lower <= actual && actual <= upper);
        assert!(state
            .validity_deadline(actual - 100, actual + 9000, now)
            .is_ok());
        assert!(state
            .validity_deadline(actual + 100, actual + 9000, now)
            .is_err());
        assert!(state.validity_deadline(actual - 100, actual, now).is_err());
    }
}
#[test]
fn insufficient_samples_and_uncertain_boundary_fail_closed() {
    let mut state = PeerTimeReferenceV1::default();
    let start = Instant::now();
    sample(&mut state, start, 0, 444, 1);
    assert!(state.interval(start + Duration::from_millis(10)).is_err());
    let (mut state, start) = ready(444);
    let now = start + Duration::from_millis(700);
    let (_, upper) = state.interval(now).unwrap();
    assert!(state.validity_deadline(1, upper, now).is_err());
}
#[test]
fn session_domain_peer_and_direction_bindings_are_distinct() {
    let original = context([1; 32], [2; 32], [3; 32], [4; 16]);
    for changed in [
        context([9; 32], [2; 32], [3; 32], [4; 16]),
        context([1; 32], [9; 32], [3; 32], [4; 16]),
        context([1; 32], [2; 32], [9; 32], [4; 16]),
        context([1; 32], [2; 32], [3; 32], [9; 16]),
        context([1; 32], [3; 32], [2; 32], [4; 16]),
    ] {
        assert_ne!(original, changed);
    }
}
#[test]
fn response_replay_wrong_challenge_context_and_timeout_rejected() {
    let start = Instant::now();
    let mut state = PeerTimeReferenceV1::default();
    state.observe([1; 32], clock(start, 0, 0));
    let request = state.begin(clock(start, 0, 0), [2; 32]).unwrap();
    let response = request.reply([1; 32], clock(start, 5, 400)).unwrap();
    let mut wrong = response.clone();
    wrong.context = [9; 32];
    assert!(state.accept(&wrong, clock(start, 10, 0)).is_err());
    wrong = response.clone();
    wrong.challenge = [8; 32];
    assert!(state.accept(&wrong, clock(start, 10, 0)).is_err());
    state.accept(&response, clock(start, 10, 0)).unwrap();
    assert!(state.accept(&response, clock(start, 11, 0)).is_err());
    state.observe([1; 32], clock(start, 300, 0));
    let request = state.begin(clock(start, 300, 0), [3; 32]).unwrap();
    let response = request.reply([1; 32], clock(start, 305, 400)).unwrap();
    assert!(state.accept(&response, clock(start, 2300, 0)).is_err());
}
#[test]
fn wall_jumps_poll_gaps_and_new_sessions_invalidate() {
    for offset in [-1000, 1000] {
        let (mut state, start) = ready(444);
        state.observe([1; 32], clock(start, 700, offset));
        assert!(state.interval(start + Duration::from_millis(700)).is_err());
    }
    let (mut state, start) = ready(444);
    state.observe([9; 32], clock(start, 700, 0));
    assert!(state.interval(start + Duration::from_millis(700)).is_err());
    let (mut state, start) = ready(444);
    state.observe([1; 32], clock(start, 3000, 0));
    assert!(state.interval(start + Duration::from_millis(3000)).is_err());
    let (mut state, start) = ready(444);
    state.observe([1; 32], clock(start, 500, 0));
    assert!(state.interval(start + Duration::from_millis(500)).is_err());
}
#[test]
fn conflicting_samples_are_not_averaged_into_permission() {
    let start = Instant::now();
    let mut state = PeerTimeReferenceV1::default();
    sample(&mut state, start, 0, 444, 1);
    state.observe([1; 32], clock(start, 300, 0));
    let request = state.begin(clock(start, 300, 0), [2; 32]).unwrap();
    let response = request.reply([1; 32], clock(start, 305, 1444)).unwrap();
    assert!(state.accept(&response, clock(start, 310, 0)).is_err());
    assert!(state.interval(start + Duration::from_millis(310)).is_err());
}
#[test]
fn fixed_codec_rejects_truncation_unknown_version_and_request_timestamps() {
    let request = TimeExchangeV1 {
        response: false,
        context: [1; 32],
        challenge: [2; 32],
        receive_ms: 0,
        send_ms: 0,
        residence_ns: 0,
    };
    let bytes = request.encode().unwrap();
    assert_eq!(TimeExchangeV1::decode(&bytes).unwrap(), request);
    for len in 0..EXCHANGE_BYTES {
        assert!(TimeExchangeV1::decode(&bytes[..len]).is_err());
    }
    let mut invalid = bytes;
    invalid[0] = 2;
    assert!(TimeExchangeV1::decode(&invalid).is_err());
    invalid = bytes;
    invalid[89] = 1;
    assert!(TimeExchangeV1::decode(&invalid).is_err());
}
#[test]
fn reference_expires_and_deadline_cannot_be_extended_by_refresh() {
    let (mut state, start) = ready(444);
    let now = start + Duration::from_millis(700);
    let deadline = state.validity_deadline(1_000_000, 1_009_000, now).unwrap();
    assert!(deadline < start + Duration::from_millis(9000));
    assert!(state.interval(start + Duration::from_secs(31)).is_err());
}

#[test]
fn backward_remeasurement_cannot_revive_expired_message() {
    let (mut state, start) = ready(444);
    let now = start + Duration::from_millis(700);
    assert!(state.validity_deadline(1_000_000, 1_000_500, now).is_err());
    state.invalidate();
    for i in 0..3 {
        sample(&mut state, start, 1000 + i * 300, -1500, i as u8 + 20);
    }
    assert!(state
        .validity_deadline(999_000, 1_000_500, start + Duration::from_millis(1700))
        .is_err());
}

#[test]
fn asymmetric_delay_is_uncertainty_not_midpoint_authority() {
    let start = Instant::now();
    let mut state = PeerTimeReferenceV1::default();
    for i in 0..3 {
        let ms = i * 500;
        let local = clock(start, ms, 0);
        state.observe([1; 32], local);
        let request = state.begin(local, [i as u8 + 1; 32]).unwrap();
        let response = request.reply([1; 32], clock(start, ms + 1, 444)).unwrap();
        let end = clock(start, ms + 201, 0);
        state.observe([1; 32], end);
        state.accept(&response, end).unwrap();
    }
    let now = start + Duration::from_millis(1250);
    let (lower, upper) = state.interval(now).unwrap();
    assert!(upper - lower >= 201);
    assert!(lower <= 1_001_694 && upper >= 1_001_694);
}

#[test]
fn fresh_boundary_wait_is_frozen_and_jump_invalidates_generation() {
    let (mut state, start) = ready(444);
    let now = start + Duration::from_millis(700);
    let (lower, upper) = state.interval(now).unwrap();
    let (ready_at, deadline) = state.validity_window(upper, upper + 1000, now).unwrap();
    assert!(ready_at > now);
    assert!(ready_at.duration_since(now).as_millis() >= u128::from(upper - lower));
    assert!(deadline < now + Duration::from_secs(1));
    assert!(state
        .validity_window(upper + 1000, upper + 1001, now)
        .is_err());
    // Do not retain arbitrary future-dated probes beyond the existing budget.
    assert!(state
        .validity_window(upper + 60_000, upper + 70_000, now)
        .is_err());
    let generation = state.generation();
    state.observe([1; 32], clock(start, 800, 1000));
    assert_ne!(generation, state.generation());
    assert!(state.interval(ready_at).is_err());
}
