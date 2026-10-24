use super::*;

fn intent() -> UcaSpendIntentV1 {
    UcaSpendIntentV1 {
        chain_id: 0x0102_0304_0506_0708,
        genesis: [0x22; 32],
        account_id: "uca:alice".into(),
        authorization_epoch: 9,
        spend_key_ref: [0x33; 32],
        recipient_uca: "uca:bob".into(),
        amount: 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10,
        max_fee: 17,
        nonce: 5,
        not_before_height: 100,
        expires_at_height: 200,
    }
}

fn signed() -> SignedUcaSpendV1 {
    let public_key = vec![0x42; MLDSA65_PUBLIC_KEY_BYTES];
    let mut intent = intent();
    intent.spend_key_ref = spend_key_ref_v1(&public_key).unwrap();
    SignedUcaSpendV1 {
        intent,
        public_key,
        // Deliberately not a real signature: codec acceptance proves no crypto.
        signature: vec![0x53; MLDSA65_SIGNATURE_BYTES],
    }
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0);
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn canonical_signing_vector_is_independent_of_encoder_and_external_pure_framing() {
    // Hand-assembled wire vector: literal protocol header, big-endian integers,
    // one-byte ASCII lengths, and exact identifiers. No production encoder or
    // serializer constructs the expected bytes. The crypto facade frames it.
    let expected = hex_bytes(concat!(
        "4e554153014101",
        "0102030405060708",
        "2222222222222222222222222222222222222222222222222222222222222222",
        "09",
        "7563613a616c696365",
        "0000000000000009",
        "3333333333333333333333333333333333333333333333333333333333333333",
        "07",
        "7563613a626f62",
        "0102030405060708090a0b0c0d0e0f10",
        "00000000000000000000000000000011",
        "0000000000000005",
        "0000000000000064",
        "00000000000000c8"
    ));
    assert_eq!(expected.len(), 161);
    assert_eq!(intent().signing_bytes().unwrap(), expected);
    assert_eq!(SPEND_CONTEXT_V1, b"NOVOVM/UCA/NOV-spend/v1");
}

#[test]
fn every_intent_field_changes_the_signed_message() {
    let base = intent();
    let bytes = base.signing_bytes().unwrap();
    for field in 0..12 {
        let mut changed = base.clone();
        match field {
            0 => changed.chain_id += 1,
            1 => changed.genesis[0] ^= 1,
            2 => changed.account_id.push('x'),
            3 => changed.authorization_epoch += 1,
            4 => changed.spend_key_ref[0] ^= 1,
            5 => changed.recipient_uca.push('x'),
            6 => changed.amount += 1,
            7 => changed.max_fee += 1,
            8 => changed.nonce += 1,
            9 => changed.not_before_height += 1,
            10 => changed.expires_at_height += 1,
            11 => std::mem::swap(&mut changed.account_id, &mut changed.recipient_uca),
            _ => unreachable!(),
        }
        assert_ne!(changed.signing_bytes().unwrap(), bytes, "field {field}");
    }
}

#[test]
fn signed_codec_round_trip_preserves_exact_bytes_but_does_not_verify_signature() {
    let original = signed();
    let bytes = original.encode().unwrap();
    let decoded = SignedUcaSpendV1::decode(&bytes).unwrap();
    assert!(decoded == original);
    assert_eq!(decoded.encode().unwrap(), bytes);
    let mut changed = bytes.clone();
    *changed.last_mut().unwrap() ^= 1;
    // Both signatures are dummy bytes; rejection belongs to the AOEM verifier.
    assert!(SignedUcaSpendV1::decode(&changed).is_ok());
}

#[test]
fn all_truncations_trailing_bytes_and_unknown_domains_are_rejected() {
    let bytes = signed().encode().unwrap();
    for length in 0..bytes.len() {
        assert!(
            SignedUcaSpendV1::decode(&bytes[..length]).is_err(),
            "length {length}"
        );
    }
    for extra in [1, 128, MAX_SIGNED_SPEND_BYTES] {
        let mut trailing = bytes.clone();
        trailing.resize(bytes.len() + extra, 0);
        assert!(SignedUcaSpendV1::decode(&trailing).is_err());
    }
    // Every framing byte is bound, including the explicit version, suite and
    // purpose. In particular, another ML-DSA level is not auto-negotiated.
    for index in 0..7 {
        let mut changed = bytes.clone();
        changed[index] ^= 0xff;
        assert!(SignedUcaSpendV1::decode(&changed).is_err());
    }
    for suite in [0, 44, 87] {
        let mut changed = bytes.clone();
        changed[5] = suite;
        assert!(SignedUcaSpendV1::decode(&changed).is_err());
    }
    for purpose in [0, 2, 255] {
        let mut changed = bytes.clone();
        changed[6] = purpose;
        assert!(SignedUcaSpendV1::decode(&changed).is_err());
    }
}

#[test]
fn identifiers_have_exact_ascii_and_size_bounds_in_both_directions() {
    let mut max = signed();
    max.intent.account_id = "a".repeat(MAX_UCA_ID_BYTES);
    max.intent.recipient_uca = "b".repeat(MAX_UCA_ID_BYTES);
    let bytes = max.encode().unwrap();
    assert_eq!(bytes.len(), MAX_SIGNED_SPEND_BYTES);
    assert!(SignedUcaSpendV1::decode(&bytes).unwrap() == max);

    for invalid in [
        "".into(),
        "a".repeat(129),
        "uca:中".into(),
        "uca:a b".into(),
        "uca:a\0b".into(),
        "uca:a/b".into(),
    ] {
        for recipient in [false, true] {
            let mut changed = signed();
            if recipient {
                changed.intent.recipient_uca = invalid.clone();
            } else {
                changed.intent.account_id = invalid.clone();
            }
            assert!(changed.encode().is_err());
        }
    }
    let bytes = signed().encode().unwrap();
    // Fixed offsets in the independent vector above, not decoder-derived.
    for length_offset in [47, 97] {
        for length in [0, 129, 255] {
            let mut changed = bytes.clone();
            changed[length_offset] = length;
            assert!(SignedUcaSpendV1::decode(&changed).is_err());
        }
        for character in [0, b' ', b'/', 0xff] {
            let mut changed = bytes.clone();
            changed[length_offset + 1] = character;
            assert!(SignedUcaSpendV1::decode(&changed).is_err());
        }
    }
}

#[test]
fn key_reference_binds_exact_public_key_and_crypto_fields_have_fixed_lengths() {
    let original = signed();
    for length in [
        0,
        MLDSA65_PUBLIC_KEY_BYTES - 1,
        MLDSA65_PUBLIC_KEY_BYTES + 1,
    ] {
        let mut changed = original.clone();
        changed.public_key.resize(length, 0);
        assert!(changed.encode().is_err());
        assert!(spend_key_ref_v1(&changed.public_key).is_err());
    }
    for length in [0, MLDSA65_SIGNATURE_BYTES - 1, MLDSA65_SIGNATURE_BYTES + 1] {
        let mut changed = original.clone();
        changed.signature.resize(length, 0);
        assert!(changed.encode().is_err());
    }
    let mut changed = original.clone();
    changed.public_key[0] ^= 1;
    assert!(changed.encode().is_err());
    let mut bytes = original.encode().unwrap();
    bytes[161] ^= 1;
    assert!(SignedUcaSpendV1::decode(&bytes).is_err());
}

#[test]
fn replay_identity_survives_key_rotation_epoch_and_intent_changes() {
    let base = signed().intent;
    let nonce_identity = base.nonce_identity().unwrap();
    let mut rotated = base.clone();
    rotated.spend_key_ref = spend_key_ref_v1(&vec![0x43; MLDSA65_PUBLIC_KEY_BYTES]).unwrap();
    rotated.authorization_epoch += 1;
    rotated.recipient_uca = "uca:next".into();
    rotated.amount += 1;
    rotated.max_fee += 1;
    rotated.nonce += 1;
    rotated.not_before_height += 1;
    rotated.expires_at_height += 1;
    assert_ne!(
        rotated.signing_bytes().unwrap(),
        base.signing_bytes().unwrap()
    );
    assert_eq!(rotated.nonce_identity().unwrap(), nonce_identity);
    for field in 0..3 {
        let mut changed = base.clone();
        match field {
            0 => changed.account_id.push('x'),
            1 => changed.chain_id += 1,
            2 => changed.genesis[0] ^= 1,
            _ => unreachable!(),
        }
        assert_ne!(changed.nonce_identity().unwrap(), nonce_identity);
    }
}

#[test]
fn exhaustion_overflow_invalid_epoch_and_invalid_windows_fail_closed() {
    for field in 0..9 {
        let mut changed = intent();
        match field {
            0 => changed.chain_id = 0,
            1 => changed.genesis = [0; 32],
            2 => changed.authorization_epoch = 0,
            3 => changed.spend_key_ref = [0; 32],
            4 => changed.nonce = u64::MAX,
            5 => changed.amount = u128::MAX,
            6 => changed.expires_at_height = changed.not_before_height,
            7 => changed.expires_at_height = changed.not_before_height - 1,
            8 => changed.expires_at_height = changed.not_before_height + MAX_SPEND_WINDOW + 1,
            _ => unreachable!(),
        }
        assert!(changed.validate().is_err(), "field {field}");
        assert!(changed.signing_bytes().is_err());
        assert!(changed.nonce_identity().is_err());
    }
    let mut edge = intent();
    edge.amount = u128::MAX - edge.max_fee;
    edge.nonce = u64::MAX - 1;
    edge.not_before_height = u64::MAX - MAX_SPEND_WINDOW;
    edge.expires_at_height = u64::MAX;
    assert!(edge.validate().is_ok());
    edge.max_fee += 1;
    assert!(edge.validate().is_err());
}
