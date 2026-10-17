use super::*;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn public(seed: u8) -> [u8; 32] {
    key(seed).verifying_key().to_bytes()
}
fn binding(initiator: bool, exporter: u8) -> VerifiedTransportBinding {
    VerifiedTransportBinding::from_authenticated_transport(
        public(if initiator { 3 } else { 4 }),
        public(if initiator { 4 } else { 3 }),
        [exporter; 32],
        BOUND_TRANSPORT_ALPN_V1,
        initiator,
    )
    .unwrap()
}
fn deadline() -> SessionClockDeadlineV2 {
    SessionClockDeadlineV2::new(Instant::now() + Duration::from_secs(30)).unwrap()
}
fn start() -> SessionClockInitiatorV2 {
    SessionClockInitiatorV2::start(&key(1), public(2), binding(true, 7), deadline()).unwrap()
}
fn respond(offer: &BoundNodeHandshakeOfferV1) -> SessionClockResponderV2 {
    SessionClockResponderV2::respond(
        offer,
        &key(2),
        public(1),
        binding(false, 7),
        deadline(),
        &mut HandshakeReplayCacheV1::new(8),
    )
    .unwrap()
}
fn pair() -> (PendingSessionClockV2, PendingSessionClockV2) {
    let a = start();
    let b = respond(a.offer());
    let a = a
        .complete(b.response(), &mut HandshakeReplayCacheV1::new(8))
        .unwrap();
    (a, b.into_pending())
}
fn complete_pair() -> (BoundSecureChannelV1, BoundSecureChannelV1) {
    let (mut a, mut b) = pair();
    let af = a.make_finished().unwrap();
    let bf = b.make_finished().unwrap();
    a.verify_finished(&bf).unwrap();
    b.verify_finished(&af).unwrap();
    (a.into_channel().unwrap(), b.into_channel().unwrap())
}

#[test]
fn v2_completed_channel_preserves_actual_carrier_and_encrypted_business() {
    let a = start();
    assert_eq!(a.offer().context.version, 2);
    assert_eq!(
        (a.offer().e2e.issued_at_ms, a.offer().e2e.expires_at_ms),
        (0, 30_000)
    );
    let (mut a, mut b) = complete_pair();
    a.verify_transport_binding(&binding(true, 7)).unwrap();
    b.verify_transport_binding(&binding(false, 7)).unwrap();
    assert!(a.verify_transport_binding(&binding(true, 8)).is_err());
    assert!(a.verify_transport_binding(&binding(false, 7)).is_err());
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        a.session_id(),
        1,
        1,
        1,
        0,
        b"original native channel".to_vec(),
    );
    let wire = a.seal_novorudp_frame(&frame).unwrap();
    assert_eq!(wire.sequence, 1);
    assert_eq!(b.open_novorudp_frame(&wire).unwrap(), frame);
    assert!(b.open_novorudp_frame(&wire).is_err());
}

#[test]
fn v2_keeps_v1_utc_semantics_and_never_accepts_mixed_modes() {
    let now = 1_800_000_000_000;
    let old =
        BoundHandshakeInitiatorV1::start(&key(1), public(2), binding(true, 7), now + 8_000, 30_000)
            .unwrap();
    assert!(matches!(
        BoundHandshakeResponderV1::respond(
            old.offer(),
            &key(2),
            public(1),
            binding(false, 7),
            now,
            30_000,
            &mut HandshakeReplayCacheV1::new(8)
        ),
        Err(TransportBindingErrorV1::Expired)
    ));
    assert!(SessionClockResponderV2::respond(
        old.offer(),
        &key(2),
        public(1),
        binding(false, 7),
        deadline(),
        &mut HandshakeReplayCacheV1::new(8)
    )
    .is_err());
    let new = start();
    let response = respond(new.offer());
    assert!(BoundHandshakeResponderV1::respond(
        new.offer(),
        &key(2),
        public(1),
        binding(false, 7),
        0,
        30_000,
        &mut HandshakeReplayCacheV1::new(8)
    )
    .is_err());
    assert!(old
        .complete(
            response.response(),
            now,
            &mut HandshakeReplayCacheV1::new(8)
        )
        .is_err());
}

#[test]
fn v2_signatures_identity_exporter_and_canonical_epoch_are_mandatory() {
    let a = start();
    for kind in 0..7 {
        let mut offer = a.offer().clone();
        match kind {
            0 => offer.signature[0] ^= 1,
            1 => offer.e2e.signature[0] ^= 1,
            2 => offer.e2e.issued_at_ms = 1,
            3 => offer.e2e.expires_at_ms = 29_999,
            4 => offer.context.purpose[0] ^= 1,
            5 => offer.context.connection_binding[0] ^= 1,
            _ => offer.context.responder_identity_public_key = public(6),
        }
        assert!(SessionClockResponderV2::respond(
            &offer,
            &key(2),
            public(1),
            binding(false, 7),
            deadline(),
            &mut HandshakeReplayCacheV1::new(8)
        )
        .is_err());
    }
    assert!(SessionClockResponderV2::respond(
        a.offer(),
        &key(2),
        public(6),
        binding(false, 7),
        deadline(),
        &mut HandshakeReplayCacheV1::new(8)
    )
    .is_err());
    assert!(SessionClockResponderV2::respond(
        a.offer(),
        &key(2),
        public(1),
        binding(false, 8),
        deadline(),
        &mut HandshakeReplayCacheV1::new(8)
    )
    .is_err());
    for kind in 0..5 {
        let a = start();
        let b = respond(a.offer());
        let mut response = b.response().clone();
        match kind {
            0 => response.signature[0] ^= 1,
            1 => response.e2e.signature[0] ^= 1,
            2 => response.offer_hash[0] ^= 1,
            3 => response.context.purpose[0] ^= 1,
            _ => response.e2e.expires_at_ms = 30_001,
        }
        assert!(a
            .complete(&response, &mut HandshakeReplayCacheV1::new(8))
            .is_err());
    }
}

#[test]
fn v2_fresh_challenges_and_finished_reject_replays_without_relying_on_utc() {
    let a = start();
    let b = respond(a.offer());
    let fresh = start();
    assert_ne!(a.offer().e2e.challenge, fresh.offer().e2e.challenge);
    assert!(fresh
        .complete(b.response(), &mut HandshakeReplayCacheV1::new(8))
        .is_err());
    let mut replay = HandshakeReplayCacheV1::new(8);
    SessionClockResponderV2::respond(
        a.offer(),
        &key(2),
        public(1),
        binding(false, 7),
        deadline(),
        &mut replay,
    )
    .unwrap();
    assert!(SessionClockResponderV2::respond(
        a.offer(),
        &key(2),
        public(1),
        binding(false, 7),
        deadline(),
        &mut replay
    )
    .is_err());
    let (mut x, mut y) = pair();
    let xf = x.make_finished().unwrap();
    assert!(x.verify_finished(&xf).is_err()); // own role cannot masquerade as remote
    let (mut x, mut y2) = pair();
    let wrong = y.make_finished().unwrap();
    assert!(x.verify_finished(&wrong).is_err()); // another transcript on same carrier
    assert!(x.verify_finished(&y2.make_finished().unwrap()).is_err()); // terminal failure
}

#[test]
fn v2_original_deadline_survives_response_finished_and_channel_return() {
    assert!(SessionClockDeadlineV2::new(Instant::now() - Duration::from_millis(1)).is_err());
    let max = SessionClockDeadlineV2::new(Instant::now() + Duration::from_secs(300)).unwrap();
    assert!(max.instant() <= Instant::now() + Duration::from_secs(30));
    let mut a = start();
    let b = respond(a.offer());
    a.deadline.until = Instant::now() - Duration::from_millis(1);
    assert!(a
        .complete(b.response(), &mut HandshakeReplayCacheV1::new(8))
        .is_err());
    let (mut a, mut b) = pair();
    let finished = b.make_finished().unwrap();
    a.deadline.until = Instant::now() - Duration::from_millis(1);
    assert!(a.make_finished().is_err());
    assert!(a.verify_finished(&finished).is_err());
    let (mut a, mut b) = pair();
    let af = a.make_finished().unwrap();
    let bf = b.make_finished().unwrap();
    a.verify_finished(&bf).unwrap();
    b.verify_finished(&af).unwrap();
    a.deadline.until = Instant::now() - Duration::from_millis(1);
    assert!(a.into_channel().is_err());
    let (a, _) = pair();
    assert!(a.into_channel().is_err()); // Finished cannot be skipped
}
