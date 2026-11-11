use super::*;

fn probe() -> NetworkProbe {
    NetworkProbe {
        nonce: [9; 32],
        sender: [1; 32],
        recipient: [3; 32],
        issued_at_ms: 100,
        expires_at_ms: 10100,
    }
}
#[test]
fn relay_diagnostic_commands_are_bounded_and_do_not_authorize_business() -> Result<()> {
    let p = probe();
    for command in [
        ProbeCommand::RelayArm(p.clone(), [2; 32]),
        ProbeCommand::RelaySend(p.clone(), [2; 32]),
        ProbeCommand::RelayCancel(p.clone(), [2; 32]),
    ] {
        let wire = command.encode()?;
        assert_eq!(
            wire.len(),
            crate::native_pipeline::consensus::native_lifecycle::probe::RELAY_COMMAND_BYTES
        );
        assert_eq!(ProbeCommand::decode(&wire)?.encode()?, wire);
        for length in 0..wire.len() {
            assert!(ProbeCommand::decode(&wire[..length]).is_err());
        }
        let mut extra = wire.clone();
        extra.push(0);
        assert!(ProbeCommand::decode(&extra).is_err());
        let mut wrong = wire.clone();
        wrong[8] = 1;
        assert!(ProbeCommand::decode(&wrong).is_err());
        assert!(ProbeCommand::RelayArm(p.clone(), p.sender)
            .encode()
            .is_err());
    }
    let now = Instant::now();
    let mut state = State::default();
    assert!(state
        .command(ProbeCommand::RelayArm(p.clone(), [2; 32]), 100, now)
        .is_err());
    state.identities = Some(([1; 32], vec![[2; 32], [3; 32]]));
    assert!(state
        .command(ProbeCommand::RelayArm(p.clone(), [4; 32]), 100, now)
        .is_err());
    assert!(state
        .command(ProbeCommand::RelayArm(p.clone(), [2; 32]), 10101, now)
        .is_err());
    state.command(ProbeCommand::RelayArm(p.clone(), [2; 32]), 100, now)?;
    assert!(state
        .command(ProbeCommand::RelayArm(p.clone(), [2; 32]), 100, now)
        .is_err());
    assert!(state
        .command(ProbeCommand::RelaySend(p.clone(), [2; 32]), 100, now)
        .is_err());
    state.request.as_mut().unwrap().ready = true;
    state.command(ProbeCommand::RelaySend(p.clone(), [2; 32]), 100, now)?;
    assert!(state
        .command(ProbeCommand::RelaySend(p.clone(), [2; 32]), 100, now)
        .is_err());
    state.command(ProbeCommand::RelayCancel(p.clone(), [2; 32]), 100, now)?;
    assert!(state.request.as_ref().unwrap().cancelled);
    assert!(state
        .command(ProbeCommand::RelaySend(p, [2; 32]), 100, now)
        .is_err());
    assert_ne!(
        context((1, [2; 32], [3; 32])),
        context((2, [2; 32], [3; 32]))
    );
    assert_ne!(
        context((1, [2; 32], [3; 32])),
        super::super::relay::context((1, [2; 32], [3; 32]))
    );
    Ok(())
}

#[test]
fn relay_diagnostic_history_cancel_expiry_and_occupancy_are_bounded() -> Result<()> {
    let now = Instant::now();
    let shared = Arc::new(Mutex::new(State {
        identities: Some(([1; 32], vec![[2; 32], [3; 32]])),
        ..Default::default()
    }));
    let guard = Occupancy::acquire(&shared)?;
    assert!(Occupancy::acquire(&shared).is_err());
    assert!(shared
        .lock()
        .unwrap()
        .command(ProbeCommand::RelayArm(probe(), [2; 32]), 100, now)
        .is_err());
    drop(guard);
    shared
        .lock()
        .unwrap()
        .command(ProbeCommand::RelayArm(probe(), [2; 32]), 100, now)?;
    assert!(take_request(&shared).is_some());
    assert!(take_request(&shared).is_none());
    for _ in 0..HISTORY + 5 {
        note(&shared, "test", None, None, None, serde_json::json!({}));
    }
    assert_eq!(shared.lock().unwrap().events.len(), HISTORY);
    assert_eq!(shared.lock().unwrap().evicted, 5);
    let mut state = shared.lock().unwrap();
    state.request.as_mut().unwrap().ready = true;
    assert!(state
        .command(ProbeCommand::RelaySend(probe(), [2; 32]), 100, now + LEASE)
        .is_err());
    for i in 1..HISTORY {
        let mut p = probe();
        p.nonce = [i as u8 + 20; 32];
        state.remember(&p, now)?;
    }
    let mut p = probe();
    p.nonce = [200; 32];
    assert!(state.remember(&p, now).is_err());
    state.remember(&p, now + LEASE)?;
    assert_eq!(state.seen.len(), 1);
    Ok(())
}

#[test]
fn relay_diagnostic_receiver_rejects_business_wrong_peer_and_duplicate() -> Result<()> {
    let now = Instant::now();
    let mut state = State {
        identities: Some(([3; 32], vec![[1; 32], [2; 32]])),
        ..Default::default()
    };
    let p = probe();
    assert!(state.receive(&p, [2; 32], now).is_err());
    let mut wrong = p.clone();
    wrong.recipient = [2; 32];
    assert!(state.receive(&wrong, [1; 32], now).is_err());
    state.receive(&p, [1; 32], now)?;
    assert!(state.receive(&p, [1; 32], now).is_err());
    let bytes = transport::encode(&Message::RequestBody { body_id: [4; 32] }, LIMITS)?;
    assert!(decode_message(&bytes).is_err());
    let mut bytes = transport::encode(&Message::NetworkProbe(p), LIMITS)?;
    assert!(matches!(decode_message(&bytes)?, Message::NetworkProbe(_)));
    *bytes.last_mut().unwrap() ^= 1;
    assert!(decode_message(&bytes).is_err());
    Ok(())
}
