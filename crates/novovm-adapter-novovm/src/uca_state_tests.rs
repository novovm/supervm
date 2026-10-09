use super::*;
use ed25519_dalek::{Signer, SigningKey};
use novovm_adapter_api::unified_account::{UcaKeyProofType, UcaPrimaryKeyBinding};
use novovm_adapter_api::UcaStatus;
use novovm_protocol::uca_delegation::{
    SignedUcaDelegationV1, UcaDelegationClaimsV1, UcaDelegationPurposeV1,
};
use novovm_protocol::uca_transaction::encode_uca_transaction_v4;
use novovm_protocol::*;

fn parent() -> NativeUcaAccountRecordV1 {
    let public_key = SigningKey::from_bytes(&[41; 32])
        .verifying_key()
        .to_bytes()
        .to_vec();
    NativeUcaAccountRecordV1 {
        version: 1,
        delegation_epoch: 1,
        next_nonce: 0,
        authorizations: BTreeMap::new(),
        account: UcaAccount {
            uca_id: "uca:isolated-state".into(),
            primary_key_ref: derive_primary_key_ref_from_binding_v1(
                UcaKeyAlgo::Ed25519,
                &public_key,
            ),
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
        },
    }
}
fn domain() -> UcaParentDomainV1 {
    UcaParentDomainV1 {
        chain_id: 70001,
        genesis: [8; 32],
        parent_height: 10,
        app_scope: [9; 32],
    }
}
fn authorize(byte: u8) -> UcaDeviceCommandV1 {
    UcaDeviceCommandV1::Authorize {
        commitment: [byte; 32],
        app_scope: [9; 32],
        expires_at_height: 30,
    }
}
fn signed(
    parent: &NativeUcaAccountRecordV1,
    command: &UcaDeviceCommandV1,
    edit: impl FnOnce(&mut NovExecuteTxV1),
) -> Vec<u8> {
    let device = SigningKey::from_bytes(&[42; 32]);
    let mut execute = NovExecuteTxV1 {
        caller: device.verifying_key().to_bytes().to_vec(),
        account_id: Some(parent.account.uca_id.clone()),
        fee_owner_account_id: Some(parent.account.uca_id.clone()),
        nonce_owner_account_id: Some(parent.account.uca_id.clone()),
        target: NovExecutionTargetV1::NativeModule("unified_account".into()),
        method: command.method().into(),
        args: command.encode().unwrap(),
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
        nonce: parent.next_nonce,
    };
    edit(&mut execute);
    let purpose = if execute.method == "revoke_device_v1" {
        UcaDelegationPurposeV1::RevokeDevice
    } else {
        UcaDelegationPurposeV1::AuthorizeDevice
    };
    let mut tx = NovNativeTxWireV1 {
        chain_id: 70001,
        signature: vec![],
        kind: NovTxKindV1::Execute(execute),
    };
    let ir = crate::native_intent::nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
    tx.signature
        .extend_from_slice(&device.verifying_key().to_bytes());
    tx.signature.extend_from_slice(
        &device
            .sign(&novovm_adapter_api::native_signing::tx_signing_message_v1(
                &ir,
            ))
            .to_bytes(),
    );
    let claims = UcaDelegationClaimsV1 {
        chain_id: 70001,
        genesis: [8; 32],
        account_id: parent.account.uca_id.clone(),
        account_epoch: parent.delegation_epoch,
        app_scope: [9; 32],
        delegate_public_key: device.verifying_key().to_bytes(),
        native_intent: native_tx_unsigned_commitment_v3(&tx).unwrap(),
        not_before_height: 10,
        expires_at_height: 100,
        purpose,
    };
    let signature = SigningKey::from_bytes(&[41; 32])
        .sign(&claims.signing_bytes().unwrap())
        .to_bytes();
    encode_uca_transaction_v4(&tx, &SignedUcaDelegationV1 { claims, signature }).unwrap()
}

#[test]
fn command_codec_is_bounded_exact_and_purpose_specific() {
    for command in [
        authorize(1),
        UcaDeviceCommandV1::Revoke {
            commitment: [1; 32],
            app_scope: [9; 32],
        },
    ] {
        let bytes = command.encode().unwrap();
        assert_eq!(UcaDeviceCommandV1::decode(&bytes).unwrap(), command);
        for len in 0..bytes.len() {
            assert!(UcaDeviceCommandV1::decode(&bytes[..len]).is_err());
        }
        let mut invalid = bytes.clone();
        invalid.push(0);
        assert!(UcaDeviceCommandV1::decode(&invalid).is_err());
        for offset in [4, 5] {
            let mut invalid = bytes.clone();
            invalid[offset] = 255;
            assert!(UcaDeviceCommandV1::decode(&invalid).is_err());
        }
    }
    assert!(authorize(0).encode().is_err());
}

#[test]
fn authorization_and_revocation_move_nonce_together_without_mutating_parent() {
    let parent = parent();
    let original = parent.clone();
    let bytes = signed(&parent, &authorize(1), |_| {});
    let update = prepare_uca_account_update_v1(&parent, &bytes, &domain()).unwrap();
    assert_eq!(parent, original);
    assert_eq!(update.next_record().next_nonce, 1);
    assert_eq!(update.next_record().authorizations.len(), 1);
    update.require_unchanged_parent(&parent).unwrap();
    assert!(update
        .require_unchanged_parent(update.next_record())
        .is_err());
    assert!(prepare_uca_account_update_v1(update.next_record(), &bytes, &domain()).is_err());
    let next = update.next_record();
    let command = UcaDeviceCommandV1::Revoke {
        commitment: [1; 32],
        app_scope: [9; 32],
    };
    let revoked =
        prepare_uca_account_update_v1(next, &signed(next, &command, |_| {}), &domain()).unwrap();
    assert_eq!(revoked.next_record().next_nonce, 2);
    assert!(revoked
        .next_record()
        .authorizations
        .values()
        .all(|g| g.revoked));
    // A new valid root signature cannot reuse a still-live revoked commitment.
    let next = revoked.next_record();
    assert!(
        prepare_uca_account_update_v1(next, &signed(next, &authorize(1), |_| {}), &domain())
            .is_err()
    );
    assert!(
        prepare_uca_account_update_v1(next, &signed(next, &command, |_| {}), &domain()).is_err()
    );
    assert!(
        prepare_uca_account_update_v1(next, &signed(next, &authorize(2), |_| {}), &domain())
            .is_ok()
    );
}

#[test]
fn signed_wrong_commands_and_expiry_leave_every_parent_field_unchanged() {
    let parent = parent();
    for case in 0..6 {
        let bytes = signed(&parent, &authorize(1), |tx| match case {
            0 => tx.args.push(0),
            1 => {
                tx.args = UcaDeviceCommandV1::Revoke {
                    commitment: [1; 32],
                    app_scope: [9; 32],
                }
                .encode()
                .unwrap()
            }
            2 => {
                tx.args = UcaDeviceCommandV1::Authorize {
                    commitment: [1; 32],
                    app_scope: [8; 32],
                    expires_at_height: 30,
                }
                .encode()
                .unwrap()
            }
            3 => {
                tx.args = UcaDeviceCommandV1::Authorize {
                    commitment: [1; 32],
                    app_scope: [9; 32],
                    expires_at_height: 11,
                }
                .encode()
                .unwrap()
            }
            4 => {
                tx.args = UcaDeviceCommandV1::Authorize {
                    commitment: [1; 32],
                    app_scope: [9; 32],
                    expires_at_height: u64::MAX,
                }
                .encode()
                .unwrap()
            }
            _ => tx.args.clear(),
        });
        let before = serde_json::to_vec(&parent).unwrap();
        assert!(
            prepare_uca_account_update_v1(&parent, &bytes, &domain()).is_err(),
            "case {case}"
        );
        assert_eq!(serde_json::to_vec(&parent).unwrap(), before);
    }
}

#[test]
fn account_state_nonce_epoch_root_and_status_cannot_be_substituted() {
    let parent = parent();
    let bytes = signed(&parent, &authorize(1), |_| {});
    let update = prepare_uca_account_update_v1(&parent, &bytes, &domain()).unwrap();
    for case in 0..7 {
        let mut other = parent.clone();
        match case {
            0 => other.next_nonce += 1,
            1 => other.delegation_epoch += 1,
            2 => other.account.status = UcaStatus::Suspended,
            3 => other.account.primary_key_ref[0] ^= 1,
            4 => {
                other
                    .account
                    .primary_key_binding
                    .as_mut()
                    .unwrap()
                    .public_key = SigningKey::from_bytes(&[43; 32])
                    .verifying_key()
                    .to_bytes()
                    .to_vec()
            }
            5 => other.version = 2,
            _ => other.account.uca_id.push('x'),
        }
        assert!(update.require_unchanged_parent(&other).is_err());
        assert!(
            prepare_uca_account_update_v1(&other, &bytes, &domain()).is_err(),
            "case {case}"
        );
    }
    let mut exhausted = parent.clone();
    exhausted.next_nonce = u64::MAX;
    assert!(prepare_uca_account_update_v1(
        &exhausted,
        &signed(&exhausted, &authorize(1), |_| {}),
        &domain()
    )
    .is_err());
}

#[test]
fn capacity_counts_revoked_entries_and_only_expired_entries_are_reclaimed() {
    let mut parent = parent();
    for n in 0..MAX_AUTHORIZATIONS_V1 {
        parent.authorizations.insert(
            format!("{:064x}", n + 1),
            UcaAuthorizationCommitmentV1 {
                app_scope: [9; 32],
                issued_at_height: 1,
                expires_at_height: 30,
                revoked: n % 2 == 0,
            },
        );
    }
    let command = authorize(4);
    let bytes = signed(&parent, &command, |_| {});
    assert!(prepare_uca_account_update_v1(&parent, &bytes, &domain()).is_err());
    parent
        .authorizations
        .values_mut()
        .next()
        .unwrap()
        .expires_at_height = 11;
    let update = prepare_uca_account_update_v1(&parent, &bytes, &domain()).unwrap();
    assert_eq!(
        update.next_record().authorizations.len(),
        MAX_AUTHORIZATIONS_V1
    );
    assert_eq!(parent.authorizations.len(), MAX_AUTHORIZATIONS_V1);
    parent.authorizations.insert(
        "F".repeat(64),
        UcaAuthorizationCommitmentV1 {
            app_scope: [9; 32],
            issued_at_height: 1,
            expires_at_height: 30,
            revoked: false,
        },
    );
    assert!(parent.validate(&parent.account.uca_id).is_err());
}
