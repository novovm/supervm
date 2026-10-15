use super::*;

const NOW: u64 = 100_000;
const TTL: u64 = 10_000;
fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn public(seed: u8) -> [u8; 32] {
    key(seed).verifying_key().to_bytes()
}
fn binding(initiator: bool, exporter: u8) -> VerifiedTransportBinding {
    let (local, remote) = if initiator {
        (public(3), public(4))
    } else {
        (public(4), public(3))
    };
    VerifiedTransportBinding::from_authenticated_transport(
        local,
        remote,
        [exporter; 32],
        BOUND_TRANSPORT_ALPN_V1,
        initiator,
    )
    .unwrap()
}
fn start() -> BoundHandshakeInitiatorV1 {
    BoundHandshakeInitiatorV1::start(&key(1), public(2), binding(true, 7), NOW, TTL).unwrap()
}
fn respond(
    offer: &BoundNodeHandshakeOfferV1,
    exporter: u8,
    pin: u8,
    cache: &mut HandshakeReplayCacheV1,
) -> super::super::Result<BoundHandshakeResponderV1> {
    BoundHandshakeResponderV1::respond(
        offer,
        &key(2),
        public(pin),
        binding(false, exporter),
        NOW,
        TTL,
        cache,
    )
}
fn pending() -> (PendingBoundChannelV1, PendingBoundChannelV1) {
    let a = start();
    let offer = decode_offer(&encode_offer(a.offer()).unwrap()).unwrap();
    assert_eq!(&offer, a.offer());
    let b = respond(&offer, 7, 1, &mut HandshakeReplayCacheV1::new(8)).unwrap();
    let response = decode_response(&encode_response(b.response()).unwrap()).unwrap();
    assert_eq!(&response, b.response());
    assert_eq!(
        offer_signing_digest(&offer),
        offer_signing_digest(a.offer())
    );
    assert_eq!(
        response_signing_digest(&response),
        response_signing_digest(b.response())
    );
    let a = a
        .complete(&response, NOW, &mut HandshakeReplayCacheV1::new(8))
        .unwrap();
    (a, b.into_pending())
}

#[test]
fn structured_roundtrip_preserves_transcript_finished_and_bidirectional_channel() {
    let (mut a, mut b) = pending();
    assert_eq!(a.transcript_digest(), b.transcript_digest());
    let af = a.make_finished(NOW).unwrap();
    let bf = b.make_finished(NOW).unwrap();
    let af_wire = encode_finished(&af).unwrap();
    assert!(af_wire.starts_with(b"NVRLY002"));
    assert_eq!(decode_finished(&af_wire).unwrap(), af);
    a.verify_finished(
        &decode_finished(&encode_finished(&bf).unwrap()).unwrap(),
        NOW,
    )
    .unwrap();
    b.verify_finished(&decode_finished(&af_wire).unwrap(), NOW)
        .unwrap();
    let mut a = a.into_channel(NOW).unwrap();
    let mut b = b.into_channel(NOW).unwrap();
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        a.session_id(),
        7,
        2,
        0,
        0,
        vec![0, 1, 255],
    );
    let envelope = a.seal_novorudp_frame(&frame).unwrap();
    assert!(encode_finished(&envelope).is_err());
    assert_eq!(b.open_novorudp_frame(&envelope).unwrap(), frame);
    assert!(b.open_novorudp_frame(&envelope).is_err());
    let envelope = b.seal_novorudp_frame(&frame).unwrap();
    assert_eq!(a.open_novorudp_frame(&envelope).unwrap(), frame);
}

#[test]
fn structured_offer_does_not_grant_exporter_pin_signature_or_replay_authority() {
    let a = start();
    let offer = decode_offer(&encode_offer(a.offer()).unwrap()).unwrap();
    let mut cache = HandshakeReplayCacheV1::new(8);
    assert!(matches!(
        respond(&offer, 8, 1, &mut cache),
        Err(TransportBindingErrorV1::BindingMismatch)
    ));
    assert!(matches!(
        respond(&offer, 7, 5, &mut cache),
        Err(TransportBindingErrorV1::BindingMismatch)
    ));
    let mut bad = offer.clone();
    bad.signature[0] ^= 1;
    let bad = decode_offer(&encode_offer(&bad).unwrap()).unwrap();
    assert!(matches!(
        respond(&bad, 7, 1, &mut cache),
        Err(TransportBindingErrorV1::InvalidSignature)
    ));
    let responder = respond(&offer, 7, 1, &mut cache).unwrap();
    assert!(respond(&offer, 7, 1, &mut cache).is_err());
    let mut response = responder.response().clone();
    response.signature[0] ^= 1;
    let response = decode_response(&encode_response(&response).unwrap()).unwrap();
    assert!(matches!(
        a.complete(&response, NOW, &mut HandshakeReplayCacheV1::new(8)),
        Err(TransportBindingErrorV1::InvalidSignature)
    ));
}

fn rebuild(wire: &[u8], schema: u64, index: usize, replacement: &[u8]) -> Vec<u8> {
    let view = apfl_batch::decode(wire, schema, LIMITS).unwrap();
    let mut fields: Vec<_> = (0..view.columns())
        .map(|i| view.field(0, i).unwrap())
        .collect();
    fields[index] = replacement;
    encode(schema, view.context, &fields).unwrap()
}

#[test]
fn structured_wire_rejects_wrong_schema_version_width_extra_fields_trailing_and_json() {
    let a = start();
    let wire = encode_offer(a.offer()).unwrap();
    assert!(wire.starts_with(b"NVAPFLC1"));
    assert!(decode_response(&wire).is_err());
    assert!(decode_offer(&a.offer().encode_json().unwrap()).is_err());
    assert!(decode_offer(&rebuild(&wire, OFFER_SCHEMA, 0, &2u16.to_le_bytes())).is_err());
    assert!(decode_offer(&rebuild(&wire, OFFER_SCHEMA, 6, &2u16.to_le_bytes())).is_err());
    assert!(decode_offer(&rebuild(&wire, OFFER_SCHEMA, 8, &[8; 32])).is_err());
    assert!(decode_offer(&rebuild(&wire, OFFER_SCHEMA, 12, &[8; 63])).is_err());
    assert!(decode_offer(&rebuild(&wire, OFFER_SCHEMA, 11, &(NOW - 1).to_le_bytes())).is_err());
    let mut trailing = wire.clone();
    trailing.push(0);
    assert!(decode_offer(&trailing).is_err());
    let mut zero_binding = wire.clone();
    zero_binding[24..56].fill(0);
    assert!(decode_offer(&zero_binding).is_err());
    let mut bad_version = wire.clone();
    bad_version[8..10].copy_from_slice(&2u16.to_le_bytes());
    assert!(decode_offer(&bad_version).is_err());
    let view = apfl_batch::decode(&wire, OFFER_SCHEMA, LIMITS).unwrap();
    let mut fields: Vec<_> = (0..view.columns())
        .map(|i| view.field(0, i).unwrap())
        .collect();
    fields.push(&[]);
    assert!(decode_offer(&encode(OFFER_SCHEMA, view.context, &fields).unwrap()).is_err());
    fields.pop();
    fields.pop();
    assert!(decode_offer(&encode(OFFER_SCHEMA, view.context, &fields).unwrap()).is_err());
    for end in 0..wire.len() {
        assert!(decode_offer(&wire[..end]).is_err());
    }
    assert!(decode_offer(&vec![0; BOUND_HANDSHAKE_MAX_WIRE_BYTES_V1 + 1]).is_err());
}

#[test]
fn response_and_offer_reject_noncanonical_redundant_identity_fields() {
    let a = start();
    let mut offer = a.offer().clone();
    offer.e2e.initiator_peer_id.push('x');
    assert!(encode_offer(&offer).is_err());
    let b = respond(a.offer(), 7, 1, &mut HandshakeReplayCacheV1::new(8)).unwrap();
    let wire = encode_response(b.response()).unwrap();
    assert!(decode_offer(&wire).is_err());
    assert!(decode_response(&b.response().encode_json().unwrap()).is_err());
    assert!(decode_response(&rebuild(&wire, RESPONSE_SCHEMA, 16, &[8; 31])).is_err());
    let mut response = b.response().clone();
    response.e2e.responder_identity_public_key = public(6);
    assert!(encode_response(&response).is_err());
    for end in 0..wire.len() {
        assert!(decode_response(&wire[..end]).is_err());
    }
    let mut trailing = wire;
    trailing.push(0);
    assert!(decode_response(&trailing).is_err());
}

#[test]
fn finished_binary_rejects_wrong_kind_version_sequence_bounds_and_trailing() {
    let (mut a, _) = pending();
    let envelope = a.make_finished(NOW).unwrap();
    let wire = encode_finished(&envelope).unwrap();
    assert!(decode_finished(&serde_json::to_vec(&envelope).unwrap()).is_err());
    let mut kind = wire.clone();
    kind[8] = 2;
    assert!(decode_finished(&kind).is_err());
    let mut trailing = wire.clone();
    trailing.push(0);
    assert!(decode_finished(&trailing).is_err());
    assert!(decode_finished(&vec![0; FINISHED_WIRE_LIMIT + 1]).is_err());
    for end in 0..wire.len() {
        assert!(decode_finished(&wire[..end]).is_err());
    }
    for field in 0..4 {
        let mut bad = envelope.clone();
        match field {
            0 => bad.version = 2,
            1 => bad.sequence = 1,
            2 => bad.ciphertext = vec![0; 513],
            _ => bad.sender_peer_id.clear(),
        }
        assert!(encode_finished(&bad).is_err());
        let binary = crate::duplex::encode_iroh_envelope_v1(&bad).unwrap();
        assert!(decode_finished(&binary).is_err());
    }
}

#[test]
fn finished_still_requires_both_roles_and_rejects_tampering_or_replay() {
    let (mut a, b) = pending();
    let af = decode_finished(&encode_finished(&a.make_finished(NOW).unwrap()).unwrap()).unwrap();
    assert!(a.into_channel(NOW).is_err());
    assert!(b.into_channel(NOW).is_err());
    let (mut a, mut b) = pending();
    let fresh = decode_finished(&encode_finished(&a.make_finished(NOW).unwrap()).unwrap()).unwrap();
    assert!(b.verify_finished(&af, NOW).is_err()); // Different signed session.
    assert!(b.into_channel(NOW).is_err());
    let (mut a, mut b) = pending();
    let mut broken = a.make_finished(NOW).unwrap();
    broken.ciphertext[0] ^= 1;
    assert!(b
        .verify_finished(
            &decode_finished(&encode_finished(&broken).unwrap()).unwrap(),
            NOW
        )
        .is_err());
    let (mut a, mut b) = pending();
    let af = a.make_finished(NOW).unwrap();
    b.make_finished(NOW).unwrap();
    b.verify_finished(
        &decode_finished(&encode_finished(&af).unwrap()).unwrap(),
        NOW,
    )
    .unwrap();
    assert!(b.verify_finished(&af, NOW).is_err());
    assert!(b.into_channel(NOW).is_err());
    assert_ne!(fresh.session_id, af.session_id);
}
