use super::*;
use crate::uca_delegation::UcaDelegationClaimsV1;
use crate::*;

fn fixture() -> (NovNativeTxWireV1, SignedUcaDelegationV1) {
    // Codec-only bytes, not authenticated signatures or production identities.
    let tx = NovNativeTxWireV1 {
        chain_id: 1,
        signature: vec![7; 96],
        kind: NovTxKindV1::Execute(NovExecuteTxV1 {
            caller: vec![7; 32],
            account_id: Some("uca:test".into()),
            fee_owner_account_id: Some("uca:test".into()),
            nonce_owner_account_id: Some("uca:test".into()),
            target: NovExecutionTargetV1::NativeModule("unified_account".into()),
            method: "authorize_device_v1".into(),
            args: vec![1, 2],
            execution_mode: NovExecutionModeV1::Standard,
            execution_policy: NovExecutionPolicyV1::Standard,
            privacy_mode: NovPrivacyModeV1::Public,
            verification_mode: NovVerificationModeV1::Standard,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "NOV".into(),
                max_pay_amount: 0,
                slippage_bps: 0,
            },
            gas_like_limit: Some(1000),
            nonce: 0,
        }),
    };
    let claims = UcaDelegationClaimsV1 {
        chain_id: 1,
        genesis: [2; 32],
        account_id: "uca:test".into(),
        account_epoch: 1,
        app_scope: [3; 32],
        delegate_public_key: [7; 32],
        native_intent: native_tx_unsigned_commitment_v3(&tx).unwrap(),
        not_before_height: 1,
        expires_at_height: 5,
        purpose: UcaDelegationPurposeV1::AuthorizeDevice,
    };
    (
        tx,
        SignedUcaDelegationV1 {
            claims,
            signature: [8; 64],
        },
    )
}

#[test]
fn v4_roundtrip_is_canonical_and_v3_never_strips_it() {
    let (tx, proof) = fixture();
    let raw = encode_uca_transaction_v4(&tx, &proof).unwrap();
    let decoded = decode_uca_transaction_v4(&raw).unwrap();
    assert_eq!(decoded.transaction(), &tx);
    assert!(decoded.delegation() == &proof);
    assert_eq!(
        encode_uca_transaction_v4(decoded.transaction(), decoded.delegation()).unwrap(),
        raw
    );
    assert!(decode_nov_native_tx_wire_v1(&raw).is_err());
    assert_ne!(decoded.transaction_id(), &proof.claims.native_intent);
    let mut changed = proof.clone();
    changed.signature[0] ^= 1;
    let other =
        decode_uca_transaction_v4(&encode_uca_transaction_v4(&tx, &changed).unwrap()).unwrap();
    assert_ne!(decoded.transaction_id(), other.transaction_id());
    // Decoding is structural only; altered root signatures require adapter rejection.
}

#[test]
fn v4_rejects_truncation_lengths_versions_trailing_and_inner_aliases() {
    let (tx, proof) = fixture();
    let raw = encode_uca_transaction_v4(&tx, &proof).unwrap();
    for n in 0..raw.len() {
        assert!(decode_uca_transaction_v4(&raw[..n]).is_err());
    }
    let inner_end = 9 + u32::from_be_bytes(raw[5..9].try_into().unwrap()) as usize;
    for variant in 0..8 {
        let mut bad = raw.clone();
        match variant {
            0 => bad.push(0),
            1 => bad[4] = 3,
            2 => bad[5..9].copy_from_slice(&u32::MAX.to_be_bytes()),
            3 => bad[inner_end..inner_end + 2].copy_from_slice(&u16::MAX.to_be_bytes()),
            4 => bad[13] = 4, // nested V4, never recursive parsing
            5 => bad[13] = 1, // legacy inner format disallowed
            6 => {
                bad[14] = 0x81;
                bad.insert(15, 0); // nonminimal chain ID
                bad[5..9].copy_from_slice(&((inner_end - 9 + 1) as u32).to_be_bytes());
            }
            _ => bad[inner_end + 2 + 4] = 2, // unknown delegation version
        }
        assert!(
            decode_uca_transaction_v4(&bad).is_err(),
            "variant {variant}"
        );
    }
    assert!(decode_uca_transaction_v4(&vec![0; MAX_TRANSACTION_BYTES + 1]).is_err());
}

#[test]
fn v4_rejects_mismatched_intent_owners_signer_and_purpose() {
    let (tx, proof) = fixture();
    for variant in 0..6 {
        let mut tx = tx.clone();
        let NovTxKindV1::Execute(exec) = &mut tx.kind else {
            unreachable!()
        };
        match variant {
            0 => exec.nonce += 1,
            1 => exec.account_id = Some("other".into()),
            2 => exec.fee_owner_account_id = None,
            3 => exec.method = "transfer".into(),
            4 => tx.signature[0] ^= 1,
            _ => exec.args.push(3),
        }
        assert!(encode_uca_transaction_v4(&tx, &proof).is_err());
    }
}

#[test]
fn v4_bounds_local_arguments_before_encoding() {
    let (mut tx, mut proof) = fixture();
    let NovTxKindV1::Execute(exec) = &mut tx.kind else {
        unreachable!()
    };
    exec.args = vec![0; 65_536];
    proof.claims.native_intent = native_tx_unsigned_commitment_v3(&tx).unwrap();
    let wire = encode_uca_transaction_v4(&tx, &proof).unwrap();
    assert!(wire.len() <= MAX_TRANSACTION_BYTES);
    assert!(decode_uca_transaction_v4(&wire).is_ok());
    let NovTxKindV1::Execute(exec) = &mut tx.kind else {
        unreachable!()
    };
    exec.args.push(0);
    assert!(encode_uca_transaction_v4(&tx, &proof).is_err());
}
