use super::*;

// Codec fixtures contain no cryptographic proof. Real handshake/AOEM tests
// check authentication after this external shape boundary.
fn offer() -> NodeHandshakeOfferV1 {
    NodeHandshakeOfferV1 {
        version: PRODUCT_OVERLAY_PROTOCOL_VERSION_V1,
        session_id: [3; 16],
        initiator_peer_id: format!("novovm-mldsa65:{}", "a".repeat(64)),
        responder_peer_id: format!("novovm-mldsa65:{}", "b".repeat(64)),
        initiator_identity_public_key: vec![7; MLDSA65_PUBLIC_KEY_BYTES],
        initiator_ephemeral_public_key: vec![8; 33],
        challenge: [9; 32],
        issued_at_ms: 1000,
        expires_at_ms: 5000,
        signature: vec![10; MLDSA65_SIGNATURE_BYTES],
    }
}

fn response(value: &NodeHandshakeOfferV1) -> NodeHandshakeResponseV1 {
    NodeHandshakeResponseV1 {
        version: value.version,
        session_id: value.session_id,
        initiator_peer_id: value.initiator_peer_id.clone(),
        responder_peer_id: value.responder_peer_id.clone(),
        responder_identity_public_key: vec![11; MLDSA65_PUBLIC_KEY_BYTES],
        responder_ephemeral_public_key: vec![12; 33],
        challenge: value.challenge,
        response_nonce: [13; 32],
        offer_hash: [14; 32],
        issued_at_ms: 1100,
        expires_at_ms: 5000,
        signature: vec![15; MLDSA65_SIGNATURE_BYTES],
    }
}

#[test]
fn mldsa_native_carrier_preserves_all_signed_fields() {
    let offer = offer();
    let response = response(&offer);
    assert_eq!(OFFER_BYTES, 5529);
    assert_eq!(RESPONSE_BYTES, 5593);
    let wire = encode_offer(&offer).unwrap();
    assert_eq!(wire.len(), OFFER_BYTES);
    assert_eq!(&wire[9..13], &[0, 2, 0, 65]);
    assert_eq!(decode(&wire).unwrap(), HandshakeMessage::Offer(offer));
    let wire = encode_response(&response).unwrap();
    assert_eq!(wire.len(), RESPONSE_BYTES);
    assert_eq!(decode(&wire).unwrap(), HandshakeMessage::Response(response));
}

#[test]
fn rejects_legacy_unknown_suite_version_truncation_and_noncanonical_ids() {
    let offer = offer();
    let response = response(&offer);
    for original in [
        encode_offer(&offer).unwrap(),
        encode_response(&response).unwrap(),
    ] {
        for length in 0..original.len() {
            assert!(decode(&original[..length]).is_err());
        }
        let mut bad = original.clone();
        bad.push(0);
        assert!(decode(&bad).is_err());
        for (start, replacement) in [
            (0, b"NVHSHK01".as_slice()),
            (9, &[0, 1]),
            (9, &[0, 3]),
            (11, &[0, 1]),
            (11, &[0, 44]),
            (11, &[0, 87]),
            (11, &[0xff, 0xff]),
            (PEERS_OFFSET, b"novovm-ed25519:"),
            (PEERS_OFFSET + PEER_BYTES, b"novovm-ed25519:"),
            (PEERS_OFFSET + PEER_PREFIX.len(), b"G"),
            (PEERS_OFFSET + PEER_BYTES + PEER_PREFIX.len(), b"A"),
        ] {
            bad = original.clone();
            bad[start..start + replacement.len()].copy_from_slice(replacement);
            assert!(decode(&bad).is_err(), "accepted replacement at {start}");
        }
        bad = original;
        bad[8] = 9;
        assert!(decode(&bad).is_err());
    }
    assert!(decode(b"{\"kind\":\"offer\"}").is_err());
}

#[test]
fn encoder_rejects_wrong_key_signature_ephemeral_and_version_shapes() {
    for length in [
        0,
        32,
        MLDSA65_PUBLIC_KEY_BYTES - 1,
        MLDSA65_PUBLIC_KEY_BYTES + 1,
    ] {
        let mut value = offer();
        value.initiator_identity_public_key.resize(length, 1);
        assert!(encode_offer(&value).is_err());
        let mut response = response(&offer());
        response.responder_identity_public_key.resize(length, 1);
        assert!(encode_response(&response).is_err());
    }
    for length in [
        0,
        64,
        MLDSA65_SIGNATURE_BYTES - 1,
        MLDSA65_SIGNATURE_BYTES + 1,
    ] {
        let mut value = offer();
        value.signature.resize(length, 1);
        assert!(encode_offer(&value).is_err());
        let mut response = response(&offer());
        response.signature.resize(length, 1);
        assert!(encode_response(&response).is_err());
    }
    for length in [0, 32, 34] {
        let mut value = offer();
        value.initiator_ephemeral_public_key.resize(length, 1);
        assert!(encode_offer(&value).is_err());
        let mut response = response(&offer());
        response.responder_ephemeral_public_key.resize(length, 1);
        assert!(encode_response(&response).is_err());
    }
    let mut value = offer();
    value.version = 1;
    assert!(encode_offer(&value).is_err());
    let mut response = response(&offer());
    response.version = 1;
    assert!(encode_response(&response).is_err());
}
