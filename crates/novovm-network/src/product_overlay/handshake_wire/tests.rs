use super::*;
use crate::product_overlay::*;
use ed25519_dalek::SigningKey;

#[test]
fn fixed_native_carrier_preserves_original_transcript_and_rejects_noncanonical_shapes() {
    let a = SigningKey::from_bytes(&[31; 32]);
    let b = SigningKey::from_bytes(&[47; 32]);
    let start = NodeHandshakeInitiatorV1::start(
        &a,
        peer_id_from_ed25519_public_key_v1(&b.verifying_key().to_bytes()),
        1000,
        5000,
    )
    .unwrap();
    let wire = encode_offer(start.offer()).unwrap();
    assert_eq!(wire.len(), OFFER_BYTES);
    assert_eq!(
        decode(&wire).unwrap(),
        HandshakeMessage::Offer(start.offer().clone())
    );
    let response = NodeHandshakeResponderV1::respond(
        start.offer(),
        &b,
        1100,
        5000,
        &mut HandshakeReplayCacheV1::default(),
    )
    .unwrap();
    let response_wire = encode_response(response.response()).unwrap();
    assert_eq!(response_wire.len(), RESPONSE_BYTES);
    let HandshakeMessage::Response(decoded) = decode(&response_wire).unwrap() else {
        panic!("wrong tag")
    };
    assert_eq!(&decoded, response.response());
    start
        .complete(&decoded, 1200, &mut HandshakeReplayCacheV1::default())
        .unwrap();
    for original in [&wire, &response_wire] {
        for length in 0..original.len() {
            assert!(decode(&original[..length]).is_err());
        }
        let mut bad = original.clone();
        bad.push(0);
        assert!(decode(&bad).is_err());
        bad = original.clone();
        bad[9] = 2; // Version stays in the signed message; original verifier rejects.
        assert!(decode(&bad).is_ok());
        bad = original.clone();
        bad[8] = 9;
        assert!(decode(&bad).is_err());
        bad = original.clone();
        bad[42] = b'G';
        assert!(decode(&bad).is_err());
    }
    let mut bad = response.response().clone();
    bad.signature.push(0);
    assert!(encode_response(&bad).is_err());
    assert!(decode(b"{\"kind\":\"offer\"}").is_err());
}
