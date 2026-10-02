use super::*;

fn policy() -> TimeoutPolicy {
    TimeoutPolicy {
        propose: Duration::from_millis(100),
        prevote: Duration::from_millis(20),
        precommit: Duration::from_millis(30),
        round_increment: Duration::from_millis(5),
    }
}
fn context() -> Context {
    Context {
        chain_id: 1,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        epoch: 1,
        validator_set_hash: [3; 32],
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    }
}

#[test]
fn no_phase_timeout_without_distinct_weight_threshold() {
    let now = Instant::now();
    let mut timers = Timers::new(context(), 0, now, policy()).unwrap();
    timers
        .observe(now, TimeoutStep::Prevote, false, false, policy())
        .unwrap();
    assert_eq!(
        timers.elapsed(now + Duration::from_secs(100), TimeoutStep::Prevote),
        None
    );
    assert_eq!(
        timers.elapsed(now + Duration::from_secs(100), TimeoutStep::Precommit),
        None
    );
    assert_eq!(
        timers.elapsed(now + Duration::from_millis(99), TimeoutStep::Propose),
        None
    );
    assert_eq!(
        timers.elapsed(now + Duration::from_millis(100), TimeoutStep::Propose),
        Some(TimeoutStep::Propose)
    );
}

#[test]
fn qualification_starts_once_and_repeated_votes_do_not_extend_deadline() {
    let now = Instant::now();
    let mut timers = Timers::new(context(), 0, now, policy()).unwrap();
    timers
        .observe(now, TimeoutStep::Propose, true, false, policy())
        .unwrap();
    assert!(timers.prevote.is_none());
    timers
        .observe(now, TimeoutStep::Prevote, true, false, policy())
        .unwrap();
    timers
        .observe(
            now + Duration::from_millis(19),
            TimeoutStep::Prevote,
            true,
            false,
            policy(),
        )
        .unwrap();
    assert_eq!(
        timers.elapsed(now + Duration::from_millis(19), TimeoutStep::Prevote),
        None
    );
    assert_eq!(
        timers.elapsed(now + Duration::from_millis(20), TimeoutStep::Prevote),
        Some(TimeoutStep::Prevote)
    );
    // Obsolete prevote timer does not fire after a durable precommit.
    assert_eq!(
        timers.elapsed(now + Duration::from_secs(100), TimeoutStep::Precommit),
        None
    );
}

#[test]
fn precommit_wait_can_expire_before_local_body_or_prevote_arrives() {
    let now = Instant::now();
    let mut timers = Timers::new(context(), 0, now, policy()).unwrap();
    timers
        .observe(now, TimeoutStep::Propose, false, true, policy())
        .unwrap();
    assert_eq!(
        timers.elapsed(now + Duration::from_millis(29), TimeoutStep::Propose),
        None
    );
    assert_eq!(
        timers.elapsed(now + Duration::from_millis(30), TimeoutStep::Propose),
        Some(TimeoutStep::Precommit)
    );
    assert_eq!(
        timers.elapsed(now + Duration::from_millis(30), TimeoutStep::Prevote),
        Some(TimeoutStep::Precommit)
    );
}

#[test]
fn timeout_grows_by_round_and_new_height_restarts_schedule() {
    let now = Instant::now();
    let timers = Timers::new(context(), 10, now, policy()).unwrap();
    assert_eq!(
        timers.propose.duration_since(now),
        Duration::from_millis(150)
    );
    let mut next = context();
    next.height = 2;
    let timers = Timers::new(next, 0, now, policy()).unwrap();
    assert_eq!(
        timers.propose.duration_since(now),
        Duration::from_millis(100)
    );
    assert!(timers.prevote.is_none() && timers.precommit.is_none());
}

#[test]
fn zero_and_unrepresentable_timeout_are_rejected_without_wrapping() {
    let mut invalid = policy();
    invalid.round_increment = Duration::ZERO;
    assert!(Pacemaker::new(invalid).is_err());
    invalid = policy();
    invalid.prevote = Duration::ZERO;
    assert!(Pacemaker::new(invalid).is_err());
    invalid = policy();
    invalid.round_increment = Duration::MAX;
    assert!(invalid.duration(TimeoutStep::Propose, 2).is_err());
}
