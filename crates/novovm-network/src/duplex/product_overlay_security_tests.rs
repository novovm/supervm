use super::*;
use crate::duplex::novorudp::NovoRudpTransportFrameKindV0;

fn handshake() -> (NodeHandshakeInitiatorV1, NodeHandshakeResponderV1) {
    let a = SigningKey::from_bytes(&[3; 32]);
    let b = SigningKey::from_bytes(&[4; 32]);
    let peer = peer_id_from_ed25519_public_key_v1(&b.verifying_key().to_bytes());
    let initiator = NodeHandshakeInitiatorV1::start(&a, peer, 1_000, 10_000).unwrap();
    let responder = NodeHandshakeResponderV1::respond(
        initiator.offer(),
        &b,
        1_100,
        10_000,
        &mut HandshakeReplayCacheV1::default(),
    )
    .unwrap();
    (initiator, responder)
}

fn channels() -> (E2eSecureChannelV1, E2eSecureChannelV1) {
    let (initiator, responder) = handshake();
    let sender = initiator
        .complete(
            responder.response(),
            1_200,
            &mut HandshakeReplayCacheV1::default(),
        )
        .unwrap();
    (sender, responder.into_channel())
}

fn frame() -> NovoRudpTransportFrameV0 {
    NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        [8; 16],
        1,
        2,
        3,
        4,
        b"opaque bytes".to_vec(),
    )
}

#[test]
fn weak_identity_forgery_is_rejected_on_offer_and_response() {
    let (initiator, responder) = handshake();
    let mut weak_key = [0u8; 32];
    weak_key[0] = 1; // Compressed Ed25519 identity point, not a signing identity.
    let mut forged_signature = vec![0; 64];
    forged_signature[0] = 1;
    let mut offer = initiator.offer().clone();
    offer.initiator_identity_public_key = weak_key;
    offer.initiator_peer_id = peer_id_from_ed25519_public_key_v1(&weak_key);
    offer.signature = forged_signature.clone();
    assert!(matches!(
        validate_handshake_offer_v1(&offer, 1_200, &mut HandshakeReplayCacheV1::default()),
        Err(ProductOverlayErrorV1::InvalidHandshakeKey)
    ));
    let mut original_offer = initiator.offer().clone();
    original_offer.responder_peer_id = peer_id_from_ed25519_public_key_v1(&weak_key);
    let mut response = responder.response().clone();
    response.responder_identity_public_key = weak_key;
    response.responder_peer_id = original_offer.responder_peer_id.clone();
    response.offer_hash = handshake_offer_hash_v1(&original_offer);
    response.signature = forged_signature;
    assert!(matches!(
        validate_handshake_response_v1(
            &original_offer,
            &response,
            1_200,
            &mut HandshakeReplayCacheV1::default()
        ),
        Err(ProductOverlayErrorV1::InvalidHandshakeKey)
    ));
}

#[test]
fn response_transcript_replay_signature_and_offer_expiry_are_checked() {
    let (initiator, responder) = handshake();
    let response = responder.response();
    let mut cache = HandshakeReplayCacheV1::default();
    let mut tampered = response.clone();
    tampered.challenge[0] ^= 1;
    assert!(matches!(
        validate_handshake_response_v1(initiator.offer(), &tampered, 1_200, &mut cache),
        Err(ProductOverlayErrorV1::HandshakeTranscriptMismatch)
    ));
    tampered = response.clone();
    tampered.signature[0] ^= 1;
    assert!(matches!(
        validate_handshake_response_v1(initiator.offer(), &tampered, 1_200, &mut cache),
        Err(ProductOverlayErrorV1::InvalidHandshakeSignature)
    ));
    validate_handshake_response_v1(initiator.offer(), response, 1_200, &mut cache).unwrap();
    assert!(matches!(
        validate_handshake_response_v1(initiator.offer(), response, 1_200, &mut cache),
        Err(ProductOverlayErrorV1::HandshakeReplay)
    ));
    // A response can still be valid while the originating offer has expired.
    assert!(matches!(
        validate_handshake_response_v1(
            initiator.offer(),
            response,
            11_001,
            &mut HandshakeReplayCacheV1::default()
        ),
        Err(ProductOverlayErrorV1::HandshakeExpired)
    ));
}

#[test]
fn invalid_key_signature_length_and_wrong_responder_are_rejected() {
    let (initiator, _) = handshake();
    let mut cache = HandshakeReplayCacheV1::default();
    let mut offer = initiator.offer().clone();
    offer.initiator_ephemeral_public_key = vec![0; 33];
    assert!(matches!(
        validate_handshake_offer_v1(&offer, 1_200, &mut cache),
        Err(ProductOverlayErrorV1::InvalidHandshakeKey)
    ));
    offer = initiator.offer().clone();
    offer.signature.pop();
    assert!(matches!(
        validate_handshake_offer_v1(&offer, 1_200, &mut cache),
        Err(ProductOverlayErrorV1::InvalidHandshakeSignature)
    ));
    assert!(matches!(
        NodeHandshakeResponderV1::respond(
            initiator.offer(),
            &SigningKey::from_bytes(&[5; 32]),
            1_200,
            1_000,
            &mut cache
        ),
        Err(ProductOverlayErrorV1::HandshakeTargetMismatch)
    ));
}

#[test]
fn unauthenticated_future_sequence_does_not_poison_the_replay_window() {
    let (mut sender, mut receiver) = channels();
    let first = sender.seal_novorudp_frame(&frame()).unwrap();
    let second = sender.seal_novorudp_frame(&frame()).unwrap();
    let mut forged = first.clone();
    forged.sequence = u64::MAX;
    forged.nonce = secure_frame_nonce_v1(receiver.inbound_nonce_prefix, forged.sequence);
    assert!(matches!(
        receiver.open_novorudp_frame(&forged),
        Err(ProductOverlayErrorV1::SecureFrameAuthenticationFailed)
    ));
    assert_eq!(receiver.open_novorudp_frame(&second).unwrap(), frame());
    assert_eq!(receiver.open_novorudp_frame(&first).unwrap(), frame());
    assert!(matches!(
        receiver.open_novorudp_frame(&first),
        Err(ProductOverlayErrorV1::SecureFrameReplay)
    ));
}

#[test]
fn outbound_sequence_exhaustion_never_wraps_or_reuses_a_nonce() {
    let (mut sender, mut receiver) = channels();
    sender.outbound_sequence = u64::MAX - 1;
    let final_envelope = sender.seal_novorudp_frame(&frame()).unwrap();
    assert_eq!(final_envelope.sequence, u64::MAX - 1);
    assert_eq!(
        receiver.open_novorudp_frame(&final_envelope).unwrap(),
        frame()
    );
    for _ in 0..2 {
        assert!(matches!(
            sender.seal_novorudp_frame(&frame()),
            Err(ProductOverlayErrorV1::SecureFrameSequenceExhausted)
        ));
        assert_eq!(sender.outbound_sequence, u64::MAX);
    }
}

#[test]
fn authenticated_malformed_frame_is_rejected_and_cannot_be_replayed() {
    let (mut sender, mut receiver) = channels();
    let mut envelope = sender.seal_novorudp_frame(&frame()).unwrap();
    let mut plaintext = frame().encode();
    plaintext[11] = 1;
    envelope.ciphertext = ChaCha20Poly1305::new((&sender.outbound_key).into())
        .encrypt(
            Nonce::from_slice(&envelope.nonce),
            Payload {
                msg: &plaintext,
                aad: &secure_frame_aad_v1(&envelope),
            },
        )
        .unwrap();
    assert!(matches!(
        receiver.open_novorudp_frame(&envelope),
        Err(ProductOverlayErrorV1::NovoRudpDecode(
            NovoRudpTransportFrameDecodeErrorV0::NonzeroReserved { value: 1 }
        ))
    ));
    assert!(matches!(
        receiver.open_novorudp_frame(&envelope),
        Err(ProductOverlayErrorV1::SecureFrameReplay)
    ));
    let valid = sender.seal_novorudp_frame(&frame()).unwrap();
    assert_eq!(receiver.open_novorudp_frame(&valid).unwrap(), frame());
}
