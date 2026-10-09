use super::*;
use ed25519_dalek::{Signer, SigningKey};
use novovm_adapter_api::unified_account::{UcaKeyProofType, UcaPrimaryKeyBinding};
use novovm_protocol::uca_delegation::UcaDelegationClaimsV1;
use novovm_protocol::{
    NovExecuteTxV1, NovExecutionModeV1, NovFeePolicyV1, NovPrivacyModeV1, NovVerificationModeV1,
};

fn fixture() -> (UcaAccount, NovNativeTxWireV1, SignedUcaDelegationV1) {
    // Public, isolated fixtures. No production UCA is registered by these tests.
    let root = SigningKey::from_bytes(&[31; 32]);
    let device = SigningKey::from_bytes(&[32; 32]);
    let public_key = root.verifying_key().to_bytes().to_vec();
    let account = UcaAccount {
        uca_id: "uca:isolated-delegation".into(),
        primary_key_ref: derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Ed25519, &public_key),
        primary_key_binding: Some(UcaPrimaryKeyBinding {
            key_algo: UcaKeyAlgo::Ed25519,
            public_key,
            proof_type: UcaKeyProofType::SignatureV1,
            proof_payload: vec![],
            verified_at: 1,
        }),
        status: UcaStatus::Active,
        created_at: 1,
        updated_at: 1,
    };
    let mut tx = NovNativeTxWireV1 {
        chain_id: 70001,
        signature: vec![],
        kind: NovTxKindV1::Execute(NovExecuteTxV1 {
            caller: device.verifying_key().to_bytes().to_vec(),
            account_id: Some(account.uca_id.clone()),
            fee_owner_account_id: Some(account.uca_id.clone()),
            nonce_owner_account_id: Some(account.uca_id.clone()),
            target: NovExecutionTargetV1::NativeModule("unified_account".into()),
            method: "authorize_device_v1".into(),
            args: b"isolated intent bytes; not a state command".to_vec(),
            execution_mode: NovExecutionModeV1::Standard,
            execution_policy: NovExecutionPolicyV1::Standard,
            privacy_mode: NovPrivacyModeV1::Public,
            verification_mode: NovVerificationModeV1::Standard,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "NOV".into(),
                max_pay_amount: 1,
                slippage_bps: 0,
            },
            gas_like_limit: Some(1000),
            nonce: 9,
        }),
    };
    let message = novovm_adapter_api::native_signing::tx_signing_message_v1(
        &crate::native_intent::nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap(),
    );
    tx.signature
        .extend_from_slice(&device.verifying_key().to_bytes());
    tx.signature
        .extend_from_slice(&device.sign(&message).to_bytes());
    let claims = UcaDelegationClaimsV1 {
        chain_id: tx.chain_id,
        genesis: [8; 32],
        account_id: account.uca_id.clone(),
        account_epoch: 4,
        app_scope: [9; 32],
        delegate_public_key: device.verifying_key().to_bytes(),
        native_intent: native_tx_unsigned_commitment_v3(&tx).unwrap(),
        not_before_height: 10,
        expires_at_height: 20,
        purpose: UcaDelegationPurposeV1::AuthorizeDevice,
    };
    let signature = root.sign(&claims.signing_bytes().unwrap()).to_bytes();
    (account, tx, SignedUcaDelegationV1 { claims, signature })
}

fn context(account: &UcaAccount) -> UcaDelegationContextV1<'_> {
    UcaDelegationContextV1 {
        account,
        delegation_epoch: 4,
        chain_id: 70001,
        genesis: [8; 32],
        parent_height: 12,
        next_nonce: 9,
        app_scope: [9; 32],
    }
}

#[test]
fn both_real_signatures_are_required_for_one_exact_intent() {
    let (account, tx, proof) = fixture();
    let context = context(&account);
    let bytes = proof.encode().unwrap();
    let verified = verify_uca_delegation_v1(&bytes, &tx, &context).unwrap();
    assert_eq!(
        verified.native_intent(),
        &native_tx_unsigned_commitment_v3(&tx).unwrap()
    );
    assert_eq!(verified.parent_height(), 12);
    let mut invalid = proof.clone();
    invalid.signature[12] ^= 1;
    assert!(verify_uca_delegation_v1(&invalid.encode().unwrap(), &tx, &context).is_err());
    let mut invalid = tx.clone();
    invalid.signature[60] ^= 1;
    assert!(verify_uca_delegation_v1(&bytes, &invalid, &context).is_err());
    invalid.signature[32..].fill(0);
    assert!(verify_uca_delegation_v1(&bytes, &invalid, &context).is_err());
    let attacker = SigningKey::from_bytes(&[33; 32]);
    let mut invalid = proof.clone();
    invalid.signature = attacker
        .sign(&invalid.claims.signing_bytes().unwrap())
        .to_bytes();
    assert!(verify_uca_delegation_v1(&invalid.encode().unwrap(), &tx, &context).is_err());
}

#[test]
fn canonical_parent_changes_revoke_old_proof_without_consuming_state_in_verifier() {
    let (account, tx, proof) = fixture();
    let bytes = proof.encode().unwrap();
    for field in 0..7 {
        let mut ctx = context(&account);
        match field {
            0 => ctx.genesis[0] ^= 1,
            1 => ctx.chain_id += 1,
            2 => ctx.delegation_epoch += 1,
            3 => ctx.parent_height = 9,
            4 => ctx.parent_height = 20,
            5 => ctx.next_nonce += 1,
            _ => ctx.app_scope[0] ^= 1,
        }
        assert!(
            verify_uca_delegation_v1(&bytes, &tx, &ctx).is_err(),
            "field {field}"
        );
    }
    for status in [
        UcaStatus::Suspended,
        UcaStatus::Recovering,
        UcaStatus::Revoked,
    ] {
        let mut account = account.clone();
        account.status = status;
        assert!(verify_uca_delegation_v1(&bytes, &tx, &context(&account)).is_err());
    }
    // Validation is pure. Rechecking the SAME parent does not spend a nonce;
    // it must not be used as a replay-consumption or finalized receipt.
    assert!(verify_uca_delegation_v1(&bytes, &tx, &context(&account)).is_ok());
}

#[test]
fn signed_intent_mutations_and_other_owners_never_inherit_authority() {
    let (account, tx, proof) = fixture();
    let bytes = proof.encode().unwrap();
    for field in 0..12 {
        let mut tx = tx.clone();
        let NovTxKindV1::Execute(exec) = &mut tx.kind else {
            unreachable!()
        };
        match field {
            0 => exec.args.push(1),
            1 => exec.fee_policy.max_pay_amount += 1,
            2 => exec.nonce += 1,
            3 => exec.method = "revoke_device_v1".into(),
            4 => exec.target = NovExecutionTargetV1::NativeModule("treasury".into()),
            5 => exec.account_id = Some("victim".into()),
            6 => exec.fee_owner_account_id = Some("victim".into()),
            7 => exec.nonce_owner_account_id = None,
            8 => exec.caller[0] ^= 1,
            9 => exec.execution_policy = NovExecutionPolicyV1::PqRequired,
            10 => exec.privacy_mode = NovPrivacyModeV1::Private,
            _ => exec.verification_mode = NovVerificationModeV1::MandatoryZk,
        }
        assert!(
            verify_uca_delegation_v1(&bytes, &tx, &context(&account)).is_err(),
            "field {field}"
        );
    }
}

#[test]
fn missing_mismatched_and_unsupported_root_binding_fail_closed() {
    let (account, tx, proof) = fixture();
    let bytes = proof.encode().unwrap();
    let mut bad = account.clone();
    bad.primary_key_binding = None;
    assert!(verify_uca_delegation_v1(&bytes, &tx, &context(&bad)).is_err());
    let mut bad = account.clone();
    bad.primary_key_ref[0] ^= 1;
    assert!(verify_uca_delegation_v1(&bytes, &tx, &context(&bad)).is_err());
    let mut bad = account.clone();
    bad.primary_key_binding.as_mut().unwrap().key_algo = UcaKeyAlgo::Mldsa87;
    assert!(verify_uca_delegation_v1(&bytes, &tx, &context(&bad)).is_err());
}

#[test]
fn local_preparation_checks_device_and_policy_before_root_signer() {
    let (account, tx, proof) = fixture();
    let ctx = context(&account);
    let prepared = prepare_uca_delegation_v1(proof.claims.clone(), &tx, &ctx, |message| {
        Ok(SigningKey::from_bytes(&[31; 32]).sign(message).to_bytes())
    })
    .unwrap();
    assert!(prepared == proof);
    let mut bad_tx = tx.clone();
    bad_tx.signature[60] ^= 1;
    assert!(
        prepare_uca_delegation_v1(proof.claims.clone(), &bad_tx, &ctx, |_| panic!(
            "invalid intent must never reach root signer"
        ))
        .is_err()
    );
    assert!(prepare_uca_delegation_v1(proof.claims.clone(), &tx, &ctx, |_| Ok([0; 64])).is_err());
    assert!(
        prepare_uca_delegation_v1(proof.claims, &tx, &ctx, |_| anyhow::bail!("owner refused"))
            .is_err()
    );
}

#[test]
fn codec_rejects_all_truncations_trailing_bytes_versions_and_oversized_ids() {
    let (_, _, proof) = fixture();
    let bytes = proof.encode().unwrap();
    assert!(SignedUcaDelegationV1::decode(&bytes).unwrap() == proof);
    for n in 0..bytes.len() {
        assert!(SignedUcaDelegationV1::decode(&bytes[..n]).is_err());
    }
    let mut bad = bytes.clone();
    bad.push(0);
    assert!(SignedUcaDelegationV1::decode(&bad).is_err());
    let mut bad = bytes.clone();
    bad[4] = 2;
    assert!(SignedUcaDelegationV1::decode(&bad).is_err());
    let mut bad = bytes.clone();
    bad[5] = 3;
    assert!(SignedUcaDelegationV1::decode(&bad).is_err());
    for id in ["a\0b".to_owned(), "x".repeat(129), "".to_owned()] {
        let mut bad = proof.clone();
        bad.claims.account_id = id;
        assert!(bad.encode().is_err());
    }
    let mut bad = proof.clone();
    bad.claims.expires_at_height = 267;
    assert!(bad.encode().is_err());
}

#[test]
fn complete_v4_carrier_requires_both_signatures_and_current_parent() {
    use novovm_protocol::uca_transaction::encode_uca_transaction_v4;
    let (account, tx, proof) = fixture();
    let ctx = context(&account);
    let bytes = encode_uca_transaction_v4(&tx, &proof).unwrap();
    let verified = verify_uca_transaction_v4(&bytes, &ctx).unwrap();
    assert_eq!(verified.transaction(), &tx);
    assert_eq!(verified.native_intent(), &proof.claims.native_intent);
    assert_ne!(verified.transaction_id(), verified.native_intent());
    assert_eq!(verified.parent_height(), ctx.parent_height);
    let mut forged = bytes.clone();
    *forged.last_mut().unwrap() ^= 1;
    assert!(verify_uca_transaction_v4(&forged, &ctx).is_err());
    let mut forged = tx.clone();
    forged.signature[60] ^= 1;
    let forged_wire = encode_uca_transaction_v4(&forged, &proof).unwrap();
    assert!(verify_uca_transaction_v4(&forged_wire, &ctx).is_err());
    let mut stale = context(&account);
    stale.next_nonce += 1;
    assert!(verify_uca_transaction_v4(&bytes, &stale).is_err());
    let mut stale = context(&account);
    stale.delegation_epoch += 1;
    assert!(verify_uca_transaction_v4(&bytes, &stale).is_err());
    let mut wrong = context(&account);
    wrong.genesis[0] ^= 1;
    assert!(verify_uca_transaction_v4(&bytes, &wrong).is_err());
}

#[test]
fn complete_v4_revoke_requires_new_device_and_root_signatures() {
    use novovm_protocol::uca_transaction::encode_uca_transaction_v4;
    let (account, mut tx, mut proof) = fixture();
    let NovTxKindV1::Execute(exec) = &mut tx.kind else {
        unreachable!()
    };
    exec.method = "revoke_device_v1".into();
    proof.claims.purpose = UcaDelegationPurposeV1::RevokeDevice;
    proof.claims.native_intent = native_tx_unsigned_commitment_v3(&tx).unwrap();
    let ctx = context(&account);
    assert!(
        verify_uca_transaction_v4(&encode_uca_transaction_v4(&tx, &proof).unwrap(), &ctx).is_err()
    );
    let message = novovm_adapter_api::native_signing::tx_signing_message_v1(
        &crate::native_intent::nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap(),
    );
    tx.signature[32..]
        .copy_from_slice(&SigningKey::from_bytes(&[32; 32]).sign(&message).to_bytes());
    // Correct device signature alone is still insufficient for the new purpose.
    assert!(
        verify_uca_transaction_v4(&encode_uca_transaction_v4(&tx, &proof).unwrap(), &ctx).is_err()
    );
    proof.signature = SigningKey::from_bytes(&[31; 32])
        .sign(&proof.claims.signing_bytes().unwrap())
        .to_bytes();
    assert!(
        verify_uca_transaction_v4(&encode_uca_transaction_v4(&tx, &proof).unwrap(), &ctx).is_ok()
    );
}
