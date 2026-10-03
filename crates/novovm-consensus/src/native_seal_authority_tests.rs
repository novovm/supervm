use super::*;
use ed25519_dalek::SigningKey;

// Fixed before the move, from 7bfee630's native_block_seal.rs: the serde field
// order at 152, validator_id_v1/hash_parts_v1 at 2343/2917, and the big-endian
// validator_set_hash_v1 at 2347. The hash/JSON constants were independently
// calculated from that committed format, not emitted by the migrated module.
// RFC 8032 seeds give an independently pinned public-key fixture as well.
const SEED_A: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
const SEED_B: &str = "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb";
const PUBLIC_A: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
const PUBLIC_B: &str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
const ID_A: &str = "b7f1625294415ba0d16e5a69db002a7697dff9a2240c37f260708f9cc2f9dae2";
const ID_B: &str = "35f38de46577a2276b7df82c839562422bc72691d09441c3f060005976d0cbf2";
const SET_HASH: &str = "c75761be5b01be1ce62a371aa123a56348532b5300aa28ff426c80572f92e4ba";
const GOLDEN_JSON: &str = concat!(
    r#"{"schema":"novovm-native-block-seal-validator-set/v1","chain_id":52,"epoch":7,"activation_height":99,"validators":["#,
    r#"{"validator_id":[53,243,141,228,101,119,162,39,107,125,248,44,131,149,98,66,43,199,38,145,208,148,65,195,240,96,0,89,118,208,203,242],"#,
    r#""public_key":[61,64,23,195,232,67,137,90,146,183,10,167,77,27,126,188,156,152,44,207,46,196,150,140,192,205,85,241,42,244,102,12],"weight":4},"#,
    r#"{"validator_id":[183,241,98,82,148,65,91,160,209,110,90,105,219,0,42,118,151,223,249,162,36,12,55,242,96,112,143,156,194,249,218,226],"#,
    r#""public_key":[215,90,152,1,130,177,10,183,213,75,254,211,201,100,7,58,14,225,114,243,218,166,35,37,175,2,26,104,247,7,81,26],"weight":2}],"#,
    r#""total_weight":6,"quorum_weight":5,"validator_set_hash":[199,87,97,190,91,1,190,28,230,42,55,26,161,35,165,99,72,83,43,83,0,170,40,255,66,108,128,87,47,146,228,186]}"#,
);

fn hex32(text: &str) -> [u8; 32] {
    assert_eq!(text.len(), 64);
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap();
    }
    bytes
}

fn validator(seed: &str, weight: u64) -> NovNativeSealValidatorV1 {
    let key = SigningKey::from_bytes(&hex32(seed));
    NovNativeSealValidatorV1::new(key.verifying_key().to_bytes(), weight).unwrap()
}

fn fixture() -> NovNativeSealValidatorSetV1 {
    // Deliberately reverse the canonical ID order.
    NovNativeSealValidatorSetV1::new(52, 7, 99, vec![validator(SEED_A, 2), validator(SEED_B, 4)])
        .unwrap()
}

#[test]
fn native_seal_authority_preserves_pre_move_golden_bytes_and_commitments() {
    let first = validator(SEED_A, 2);
    let second = validator(SEED_B, 4);
    assert_eq!(first.public_key, hex32(PUBLIC_A));
    assert_eq!(second.public_key, hex32(PUBLIC_B));
    assert_eq!(first.validator_id, hex32(ID_A));
    assert_eq!(second.validator_id, hex32(ID_B));
    assert_eq!(validator_id_v1(&hex32(PUBLIC_A)), hex32(ID_A));

    let set = fixture();
    assert_eq!(set.validator_set_hash, hex32(SET_HASH));
    assert_eq!(serde_json::to_vec(&set).unwrap(), GOLDEN_JSON.as_bytes());
    let restored: NovNativeSealValidatorSetV1 = serde_json::from_str(GOLDEN_JSON).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, set);
}

#[test]
fn native_seal_authority_canonicalizes_constructor_but_rejects_unsorted_wire() {
    let set = fixture();
    let same = NovNativeSealValidatorSetV1::new(
        52,
        7,
        99,
        vec![validator(SEED_B, 4), validator(SEED_A, 2)],
    )
    .unwrap();
    assert_eq!(same, set);
    assert_eq!(set.validators[0].validator_id, hex32(ID_B));
    assert_eq!(set.validator(hex32(ID_A)).unwrap().weight, 2);
    assert_eq!(set.validator(hex32(ID_B)).unwrap().weight, 4);
    assert!(set.validator([0; 32]).is_none());

    let mut unsorted = set;
    unsorted.validators.reverse();
    assert!(unsorted
        .validate()
        .unwrap_err()
        .to_string()
        .contains("not strictly sorted and unique"));
}

#[test]
fn native_seal_authority_quorum_is_strict_even_at_three_divisibility_boundary() {
    // Fixed expectations: do not call the historical ceil(2W/3) implementation.
    // In particular W=3,6,9 require 3,5,7, not 2,4,6.
    for (weight, expected) in [
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 3),
        (5, 4),
        (6, 5),
        (7, 5),
        (9, 7),
        (10, 7),
        (u64::MAX, 12_297_829_382_473_034_411),
    ] {
        let set =
            NovNativeSealValidatorSetV1::new(52, 7, 99, vec![validator(SEED_A, weight)]).unwrap();
        assert_eq!(set.total_weight, weight);
        assert_eq!(set.quorum_weight, expected, "total weight {weight}");
        set.validate().unwrap();
    }
    assert_eq!((fixture().total_weight, fixture().quorum_weight), (6, 5));
}

#[test]
fn native_seal_authority_rejects_weight_overflow_in_constructor_and_readback() {
    let maximum = validator(SEED_A, u64::MAX);
    let extra = validator(SEED_B, 1);
    let error = NovNativeSealValidatorSetV1::new(52, 7, 99, vec![maximum, extra]).unwrap_err();
    assert!(error.to_string().contains("weight overflow"));

    let mut malformed = fixture();
    malformed.validators[0].weight = u64::MAX;
    malformed.validators[1].weight = 1;
    assert!(malformed
        .validate()
        .unwrap_err()
        .to_string()
        .contains("weight overflow"));
}

#[test]
fn native_seal_authority_rejects_invalid_metadata_and_member_counts() {
    for (chain, epoch, activation) in [(0, 7, 99), (52, 0, 99), (52, 7, 0)] {
        assert!(NovNativeSealValidatorSetV1::new(
            chain,
            epoch,
            activation,
            vec![validator(SEED_A, 1)]
        )
        .is_err());
    }
    assert!(NovNativeSealValidatorSetV1::new(52, 7, 99, vec![]).is_err());
    let too_many = vec![validator(SEED_A, 1); NOV_NATIVE_BLOCK_SEAL_MAX_VALIDATORS_V1 + 1];
    assert!(
        NovNativeSealValidatorSetV1::new(52, 7, 99, too_many.clone())
            .unwrap_err()
            .to_string()
            .contains("size is invalid")
    );
    let mut set = fixture();
    set.validators = too_many;
    assert!(set.validate().is_err());
    set.validators.clear();
    assert!(set.validate().is_err());
}

#[test]
fn native_seal_authority_rejects_tampered_serialized_set_fields() {
    let original: serde_json::Value = serde_json::from_str(GOLDEN_JSON).unwrap();
    for (field, value) in [
        ("schema", serde_json::json!("wrong-schema")),
        ("chain_id", serde_json::json!(0)),
        ("chain_id", serde_json::json!(53)),
        ("epoch", serde_json::json!(0)),
        ("epoch", serde_json::json!(8)),
        ("activation_height", serde_json::json!(0)),
        ("activation_height", serde_json::json!(100)),
        ("total_weight", serde_json::json!(7)),
        ("quorum_weight", serde_json::json!(4)),
        ("validator_set_hash", serde_json::json!(vec![0u8; 32])),
    ] {
        let mut altered = original.clone();
        altered[field] = value;
        let set: NovNativeSealValidatorSetV1 = serde_json::from_value(altered).unwrap();
        assert!(set.validate().is_err(), "altered field {field}");
    }
    for (field, value) in [
        ("weight", serde_json::json!(0)),
        ("weight", serde_json::json!(5)),
        ("validator_id", serde_json::json!(vec![0u8; 32])),
        ("public_key", serde_json::json!(hex32(PUBLIC_A))),
    ] {
        let mut altered = original.clone();
        altered["validators"][0][field] = value;
        let set: NovNativeSealValidatorSetV1 = serde_json::from_value(altered).unwrap();
        assert!(set.validate().is_err(), "altered member field {field}");
    }
}

#[test]
fn native_seal_authority_rejects_duplicate_identity_even_with_different_weight() {
    let first = validator(SEED_A, 1);
    let second = validator(SEED_A, 2);
    assert!(
        NovNativeSealValidatorSetV1::new(52, 7, 99, vec![first.clone(), second.clone()])
            .unwrap_err()
            .to_string()
            .contains("duplicate validator")
    );
    let mut set = fixture();
    set.validators = vec![first, second];
    assert!(set
        .validate()
        .unwrap_err()
        .to_string()
        .contains("not strictly sorted and unique"));
}

#[test]
fn native_seal_authority_rejects_zero_weight_invalid_and_weak_keys() {
    assert!(NovNativeSealValidatorV1::new(hex32(PUBLIC_A), 0).is_err());
    let mut identity_point = [0; 32];
    identity_point[0] = 1;
    for weak_key in [[0; 32], identity_point] {
        assert!(NovNativeSealValidatorV1::new(weak_key, 1)
            .unwrap_err()
            .to_string()
            .contains("public key is weak"));
        let forged = NovNativeSealValidatorV1 {
            validator_id: validator_id_v1(&weak_key),
            public_key: weak_key,
            weight: 1,
        };
        assert!(forged.validate().is_err());
    }
    // y=2 has no Edwards25519 x-coordinate; this is not merely a weak point.
    let mut invalid_key = [0; 32];
    invalid_key[0] = 2;
    assert!(NovNativeSealValidatorV1::new(invalid_key, 1)
        .unwrap_err()
        .to_string()
        .contains("public key is invalid"));
    let forged = NovNativeSealValidatorV1 {
        validator_id: validator_id_v1(&invalid_key),
        public_key: invalid_key,
        weight: 1,
    };
    assert!(forged.validate().is_err());
}
