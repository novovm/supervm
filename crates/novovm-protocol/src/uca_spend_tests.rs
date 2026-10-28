use super::*;

fn intent() -> AssetSpendIntentV2 {
    AssetSpendIntentV2 {
        chain_id: 0x0102_0304_0506_0708,
        genesis: [0x22; 32],
        payer: vec![0x11; 20],
        authorization_epoch: 9,
        spend_key_ref: [0x33; 32],
        recipient: vec![0x44; 32],
        asset: NATIVE_NOV_ASSET,
        amount: 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10,
        max_fee: 17,
        nonce: 5,
        not_before_height: 100,
        expires_at_height: 200,
        authority_update: None,
    }
}

fn update() -> AssetAuthorityUpdateV2 {
    AssetAuthorityUpdateV2 {
        use_recovery: false,
        next_epoch: 10,
        active: true,
        management_key_ref: [0x71; 32],
        recovery_key_ref: Some([0x72; 32]),
        grants: vec![AssetGrantConfigV2 {
            key_ref: [0x73; 32],
            permits_nov_transfer: true,
            not_before_height: 101,
            expires_at_height: 500,
            max_debit_per_intent: 100,
            remaining_debit_budget: 1000,
        }],
    }
}

fn signed() -> SignedAssetSpendV2 {
    let public_key = vec![0x42; MLDSA65_PUBLIC_KEY_BYTES];
    let mut intent = intent();
    intent.spend_key_ref = spend_key_ref_v1(&public_key).unwrap();
    SignedAssetSpendV2 {
        intent,
        public_key,
        // Codec acceptance does not establish cryptographic validity.
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
    let expected = hex_bytes(concat!(
        "4e554153024101",
        "0102030405060708",
        "2222222222222222222222222222222222222222222222222222222222222222",
        "14",
        "1111111111111111111111111111111111111111",
        "0000000000000009",
        "3333333333333333333333333333333333333333333333333333333333333333",
        "20",
        "4444444444444444444444444444444444444444444444444444444444444444",
        "01",
        "0102030405060708090a0b0c0d0e0f10",
        "00000000000000000000000000000011",
        "0000000000000005",
        "0000000000000064",
        "00000000000000c8",
        "00"
    ));
    assert_eq!(expected.len(), 199);
    assert_eq!(intent().signing_bytes().unwrap(), expected);
    assert_eq!(
        AssetSpendIntentV2::decode_signing_bytes(&expected).unwrap(),
        intent()
    );
    assert_eq!(SPEND_CONTEXT_V2, b"NOVOVM/asset/NOV-spend/v2");
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
            2 => changed.payer[0] ^= 1,
            3 => changed.authorization_epoch += 1,
            4 => changed.spend_key_ref[0] ^= 1,
            5 => changed.recipient[0] ^= 1,
            6 => changed.amount += 1,
            7 => changed.max_fee += 1,
            8 => changed.nonce += 1,
            9 => changed.not_before_height += 1,
            10 => changed.expires_at_height += 1,
            11 => std::mem::swap(&mut changed.payer, &mut changed.recipient),
            _ => unreachable!(),
        }
        assert_ne!(changed.signing_bytes().unwrap(), bytes, "field {field}");
    }
    let mut bad_asset = base;
    bad_asset.asset = 2;
    assert!(bad_asset.signing_bytes().is_err());
    let mut raw = bytes;
    raw[141] = 2;
    assert!(AssetSpendIntentV2::decode_signing_bytes(&raw).is_err());
}

#[test]
fn signed_codec_round_trip_preserves_exact_bytes_but_does_not_verify_signature() {
    let original = signed();
    let bytes = original.encode().unwrap();
    let decoded = SignedAssetSpendV2::decode(&bytes).unwrap();
    assert!(decoded == original);
    assert_eq!(decoded.encode().unwrap(), bytes);
    let mut changed = bytes;
    *changed.last_mut().unwrap() ^= 1;
    assert!(SignedAssetSpendV2::decode(&changed).is_ok());
}

#[test]
fn all_truncations_trailing_bytes_and_old_or_unknown_domains_are_rejected() {
    let bytes = signed().encode().unwrap();
    for length in 0..bytes.len() {
        assert!(
            SignedAssetSpendV2::decode(&bytes[..length]).is_err(),
            "length {length}"
        );
    }
    for extra in [1, 128, MAX_SIGNED_SPEND_BYTES] {
        let mut trailing = bytes.clone();
        trailing.resize(bytes.len() + extra, 0);
        assert!(SignedAssetSpendV2::decode(&trailing).is_err());
    }
    for index in 0..7 {
        let mut changed = bytes.clone();
        changed[index] ^= 0xff;
        assert!(SignedAssetSpendV2::decode(&changed).is_err());
    }
    for version in [0, 1, 3] {
        let mut changed = bytes.clone();
        changed[4] = version;
        assert!(SignedAssetSpendV2::decode(&changed).is_err());
    }
    for suite in [0, 44, 87] {
        let mut changed = bytes.clone();
        changed[5] = suite;
        assert!(SignedAssetSpendV2::decode(&changed).is_err());
    }
    for purpose in [0, 2, 255] {
        let mut changed = bytes.clone();
        changed[6] = purpose;
        assert!(SignedAssetSpendV2::decode(&changed).is_err());
    }
}

#[test]
fn concrete_accounts_have_exact_binary_length_bounds_without_aliases() {
    for payer_len in [20, 32] {
        for recipient_len in [20, 32] {
            let mut value = signed();
            value.intent.payer = vec![0xff; payer_len];
            value.intent.recipient = vec![0; recipient_len];
            assert!(SignedAssetSpendV2::decode(&value.encode().unwrap()).unwrap() == value);
        }
    }
    for length in [0, 1, 19, 21, 31, 33, 128, 256] {
        for recipient in [false, true] {
            let mut changed = signed();
            if recipient {
                changed.intent.recipient.resize(length, 1);
            } else {
                changed.intent.payer.resize(length, 1);
            }
            assert!(changed.encode().is_err());
        }
    }
    let bytes = signed().encode().unwrap();
    for offset in [47, 108] {
        for length in [0, 19, 21, 31, 33, 255] {
            let mut changed = bytes.clone();
            changed[offset] = length;
            assert!(SignedAssetSpendV2::decode(&changed).is_err());
        }
    }
    let a = intent();
    let mut b = a.clone();
    b.payer.resize(32, 0);
    assert_ne!(a.nonce_identity().unwrap(), b.nonce_identity().unwrap());
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
    bytes[199] ^= 1;
    assert!(SignedAssetSpendV2::decode(&bytes).is_err());
}

#[test]
fn account_replay_identity_survives_key_rotation_epoch_and_intent_changes() {
    let base = signed().intent;
    let identity = base.nonce_identity().unwrap();
    let mut rotated = base.clone();
    rotated.spend_key_ref = spend_key_ref_v1(&vec![0x43; MLDSA65_PUBLIC_KEY_BYTES]).unwrap();
    rotated.authorization_epoch += 1;
    rotated.recipient[0] ^= 1;
    rotated.amount += 1;
    rotated.max_fee += 1;
    rotated.nonce += 1;
    rotated.not_before_height += 1;
    rotated.expires_at_height += 1;
    assert_ne!(
        rotated.signing_bytes().unwrap(),
        base.signing_bytes().unwrap()
    );
    assert_eq!(rotated.nonce_identity().unwrap(), identity);
    for field in 0..3 {
        let mut changed = base.clone();
        match field {
            0 => changed.payer[0] ^= 1,
            1 => changed.chain_id += 1,
            2 => changed.genesis[0] ^= 1,
            _ => unreachable!(),
        }
        assert_ne!(changed.nonce_identity().unwrap(), identity);
    }
}

#[test]
fn exhaustion_overflow_invalid_epoch_and_invalid_windows_fail_closed() {
    for field in 0..10 {
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
            9 => changed.asset = 0,
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

#[test]
fn authority_update_roundtrip_binds_every_management_effect_and_shares_nonce() {
    let mut managed = signed();
    let spend_nonce = managed.intent.nonce_identity().unwrap();
    managed.intent.amount = 0;
    managed.intent.recipient = managed.intent.payer.clone();
    managed.intent.authority_update = Some(update());
    let bytes = managed.encode().unwrap();
    assert!(SignedAssetSpendV2::decode(&bytes).unwrap() == managed);
    assert_eq!(managed.intent.nonce_identity().unwrap(), spend_nonce);
    let original = managed.intent.signing_bytes().unwrap();
    for field in 0..11 {
        let mut changed = managed.intent.clone();
        let update = changed.authority_update.as_mut().unwrap();
        match field {
            0 => update.use_recovery = true,
            1 => update.active = false,
            2 => update.management_key_ref[0] ^= 1,
            3 => update.recovery_key_ref = None,
            4 => update.grants[0].key_ref[0] ^= 1,
            5 => update.grants[0].permits_nov_transfer = false,
            6 => update.grants[0].not_before_height += 1,
            7 => update.grants[0].expires_at_height += 1,
            8 => update.grants[0].max_debit_per_intent += 1,
            9 => update.grants[0].remaining_debit_budget += 1,
            10 => update.grants.clear(),
            _ => unreachable!(),
        }
        assert_ne!(
            changed.signing_bytes().unwrap(),
            original,
            "management field {field}"
        );
    }
    for length in 0..bytes.len() {
        assert!(SignedAssetSpendV2::decode(&bytes[..length]).is_err());
    }
}

#[test]
fn management_rejects_wrong_shape_duplicate_keys_bounds_and_epoch_overflow() {
    for field in 0..12 {
        let mut value = intent();
        value.amount = 0;
        value.recipient = value.payer.clone();
        value.authority_update = Some(update());
        let update = value.authority_update.as_mut().unwrap();
        match field {
            0 => value.amount = 1,
            1 => value.recipient[0] ^= 1,
            2 => update.next_epoch = value.authorization_epoch,
            3 => {
                value.authorization_epoch = u64::MAX;
                update.next_epoch = 1;
            }
            4 => update.management_key_ref = [0; 32],
            5 => update.recovery_key_ref = Some([0; 32]),
            6 => update.grants.push(update.grants[0].clone()),
            7 => update.grants[0].expires_at_height = update.grants[0].not_before_height,
            8 => update.grants[0].remaining_debit_budget = 0,
            9 => update.recovery_key_ref = Some(update.management_key_ref),
            10 => update.grants[0].key_ref = update.management_key_ref,
            11 => update.grants[0].key_ref = update.recovery_key_ref.unwrap(),
            _ => unreachable!(),
        }
        assert!(value.validate().is_err(), "case {field}");
    }
    let mut max = signed();
    max.intent.payer.resize(32, 1);
    max.intent.recipient = max.intent.payer.clone();
    max.intent.amount = 0;
    let mut authority = update();
    authority.grants = (0..MAX_ASSET_GRANTS)
        .map(|index| AssetGrantConfigV2 {
            key_ref: [index as u8 + 1; 32],
            ..authority.grants[0].clone()
        })
        .collect();
    max.intent.authority_update = Some(authority);
    assert_eq!(max.encode().unwrap().len(), MAX_SIGNED_SPEND_BYTES);
    assert!(SignedAssetSpendV2::decode(&max.encode().unwrap()).unwrap() == max);
    assert!(
        max.intent.retained_heap_bytes()
            >= 64 + MAX_ASSET_GRANTS * std::mem::size_of::<AssetGrantConfigV2>()
    );
    let mut too_many = max.clone();
    too_many
        .intent
        .authority_update
        .as_mut()
        .unwrap()
        .grants
        .push(update().grants[0].clone());
    assert!(too_many.encode().is_err());
    for bad in [2, 255] {
        let mut bytes = max.intent.signing_bytes().unwrap();
        // Explicit option tag follows the 210-byte max-account fixed intent.
        bytes[210] = bad;
        assert!(AssetSpendIntentV2::decode_signing_bytes(&bytes).is_err());
    }
}
