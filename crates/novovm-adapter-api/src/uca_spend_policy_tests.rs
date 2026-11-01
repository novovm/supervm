use super::*;

fn fixture() -> (AssetSpendIntentV3, Vec<u8>, SuppliedAssetSpendGrantV2) {
    (
        AssetSpendIntentV3 {
            authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
            chain_id: 7,
            genesis: [2; 32],
            payer: vec![1; 32],
            authorization_epoch: 3,
            spend_key_ref: [4; 32],
            recipient: vec![2; 32],
            asset: NATIVE_NOV_ASSET,
            amount: 100,
            max_fee: 20,
            nonce: 9,
            not_before_height: 10,
            expires_at_height: 20,
            authority_update: None,
            recipient_creation: None,
        },
        vec![1; 32],
        SuppliedAssetSpendGrantV2 {
            account: vec![1; 32],
            asset: NATIVE_NOV_ASSET,
            key_ref: [4; 32],
            authorization_epoch: 3,
            permits_nov_transfer: true,
            revoked: false,
            not_before_height: 5,
            expires_at_height: 30,
            max_debit_per_intent: 120,
            remaining_debit_budget: 120,
        },
    )
}

#[test]
fn absent_state_and_unresolved_nonce_never_become_fresh_account() {
    let (intent, account, grant) = fixture();
    assert_eq!(
        check_supplied_spend_state_v2(&intent, None, &[5; 32], 12),
        Err(SpendPolicyError::StateUnavailable)
    );
    let state = SuppliedAssetSpendStateV2 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        grant: Some(&grant),
        account_active: true,
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
        current_spend_epoch: Some(3),
        next_spend_nonce: None,
    };
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::StateUnavailable)
    );
}

#[test]
fn supplied_policy_rejects_every_authority_boundary_without_mutation() {
    use SpendPolicyError::*;
    for (case, expected) in [
        (0, WrongParent),
        (1, WrongDomain),
        (2, WrongDomain),
        (3, WrongAccount),
        (4, AccountInactive),
        (5, WrongKey),
        (6, WrongEpoch),
        (7, NotSpendPurpose),
        (8, Revoked),
        (9, Expired),
        (10, Expired),
        (11, NonceMismatch),
        (12, LimitExceeded),
        (13, LimitExceeded),
        (14, WrongAccount),
        (15, WrongEpoch),
    ] {
        let (intent, mut account, mut grant) = fixture();
        if case == 3 {
            account[0] ^= 1;
        }
        if case == 5 {
            grant.key_ref = [6; 32];
        }
        if case == 6 {
            grant.authorization_epoch += 1;
        }
        if case == 7 {
            grant.permits_nov_transfer = false;
        }
        if case == 8 {
            grant.revoked = true;
        }
        if case == 9 {
            grant.expires_at_height = 12;
        }
        if case == 10 {
            grant.not_before_height = 13;
        }
        if case == 12 {
            grant.max_debit_per_intent = 119;
        }
        if case == 13 {
            grant.remaining_debit_budget = 119;
        }
        if case == 14 {
            grant.account[0] ^= 1;
        }
        if case == 15 {
            grant.authorization_epoch = 0;
        }
        let mut state = SuppliedAssetSpendStateV2 {
            chain_id: 7,
            genesis: [2; 32],
            parent_root: [5; 32],
            account: &account,
            grant: Some(&grant),
            account_active: true,
            management_key_ref: [8; 32],
            recovery_key_ref: Some([9; 32]),
            current_spend_epoch: Some(3),
            next_spend_nonce: Some(9),
        };
        if case == 4 {
            state.account_active = false;
        }
        if case == 0 {
            state.parent_root = [6; 32];
        }
        if case == 1 {
            state.chain_id = 8;
        }
        if case == 2 {
            state.genesis = [3; 32];
        }
        if case == 11 {
            state.next_spend_nonce = Some(10);
        }
        assert_eq!(
            check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
            Err(expected),
            "case {case}"
        );
        let authority_expected = if matches!(case, 11..=13) {
            Ok(())
        } else {
            Err(expected)
        };
        assert_eq!(
            check_supplied_spend_authority_v2(&intent, Some(&state), &[5; 32], 12),
            authority_expected,
            "authority case {case}"
        );
        assert_eq!(intent.nonce, 9);
    }
}

#[test]
fn rotation_keeps_account_and_nonce_and_rejects_old_key_and_replay() {
    let (old, account, mut grant) = fixture();
    let mut rotated = old.clone();
    rotated.spend_key_ref = [6; 32];
    rotated.authorization_epoch = 4;
    grant.key_ref = rotated.spend_key_ref;
    grant.authorization_epoch = 4;
    assert_eq!(
        old.nonce_identity().unwrap(),
        rotated.nonce_identity().unwrap()
    );
    let mut state = SuppliedAssetSpendStateV2 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        grant: Some(&grant),
        account_active: true,
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
        current_spend_epoch: Some(4),
        next_spend_nonce: Some(9),
    };
    assert_eq!(
        check_supplied_spend_state_v2(&rotated, Some(&state), &[5; 32], 12),
        Ok(())
    );
    assert_eq!(
        check_supplied_spend_state_v2(&old, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongKey)
    );
    state.next_spend_nonce = Some(10);
    assert_eq!(
        check_supplied_spend_state_v2(&rotated, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::NonceMismatch)
    );
    rotated.nonce = 10;
    assert_eq!(
        check_supplied_spend_state_v2(&rotated, Some(&state), &[5; 32], 12),
        Ok(())
    );
    assert_eq!(
        check_supplied_spend_state_v2(&rotated, Some(&state), &[5; 32], 20),
        Err(SpendPolicyError::Expired)
    );
    assert_eq!(
        check_supplied_spend_state_v2(&rotated, Some(&state), &[5; 32], 9),
        Err(SpendPolicyError::Expired)
    );
}

#[test]
fn unresolved_or_zero_current_epoch_cannot_be_inferred_from_matching_grant() {
    let (intent, account, grant) = fixture();
    for (epoch, expected) in [
        (None, SpendPolicyError::StateUnavailable),
        (Some(0), SpendPolicyError::WrongEpoch),
        (Some(2), SpendPolicyError::WrongEpoch),
        (Some(4), SpendPolicyError::WrongEpoch),
    ] {
        let state = SuppliedAssetSpendStateV2 {
            chain_id: 7,
            genesis: [2; 32],
            parent_root: [5; 32],
            account: &account,
            grant: Some(&grant),
            account_active: true,
            management_key_ref: [8; 32],
            recovery_key_ref: Some([9; 32]),
            current_spend_epoch: epoch,
            next_spend_nonce: Some(9),
        };
        assert_eq!(
            check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
            Err(expected)
        );
        assert_eq!(
            check_supplied_spend_authority_v2(&intent, Some(&state), &[5; 32], 12),
            Err(expected)
        );
    }
}

#[test]
fn authority_preflight_does_not_substitute_for_dynamic_prefix_checks() {
    let (mut intent, account, mut grant) = fixture();
    intent.nonce = 10; // A preceding same-account intent may consume nonce 9.
    grant.max_debit_per_intent = 0;
    grant.remaining_debit_budget = 0;
    let mut state = SuppliedAssetSpendStateV2 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        grant: Some(&grant),
        account_active: true,
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
        current_spend_epoch: Some(3),
        next_spend_nonce: Some(9),
    };
    assert_eq!(
        check_supplied_spend_authority_v2(&intent, Some(&state), &[5; 32], 12),
        Ok(())
    );
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::NonceMismatch),
        "original nonce rejection takes priority over debit limits"
    );
    state.next_spend_nonce = None;
    assert_eq!(
        check_supplied_spend_authority_v2(&intent, Some(&state), &[5; 32], 12),
        Ok(())
    );
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::StateUnavailable)
    );
    state.next_spend_nonce = Some(10);
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::LimitExceeded),
        "passing the authority-only check never waives debit limits"
    );
    assert_eq!(grant.remaining_debit_budget, 0);
    assert_eq!(intent.nonce, 10);
}

#[test]
fn advancing_current_epoch_invalidates_retained_old_grant_without_resetting_nonce() {
    let (old, account, old_grant) = fixture();
    let mut state = SuppliedAssetSpendStateV2 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        grant: Some(&old_grant),
        account_active: true,
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
        current_spend_epoch: Some(3),
        next_spend_nonce: Some(9),
    };
    assert_eq!(
        check_supplied_spend_state_v2(&old, Some(&state), &[5; 32], 12),
        Ok(())
    );
    state.current_spend_epoch = Some(4);
    assert!(!old_grant.revoked);
    assert_eq!(
        check_supplied_spend_state_v2(&old, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongEpoch)
    );
    let mut new_intent = old.clone();
    new_intent.authorization_epoch = 4;
    assert_eq!(
        check_supplied_spend_state_v2(&new_intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongEpoch),
        "new intent cannot revive an old grant"
    );
    let (_, _, mut new_grant) = fixture();
    new_grant.authorization_epoch = 4;
    new_grant.key_ref = [6; 32];
    new_intent.spend_key_ref = new_grant.key_ref;
    state.grant = Some(&new_grant);
    assert_eq!(
        check_supplied_spend_state_v2(&new_intent, Some(&state), &[5; 32], 12),
        Ok(())
    );
    assert_eq!(
        old.nonce_identity().unwrap(),
        new_intent.nonce_identity().unwrap()
    );
    assert_eq!(state.next_spend_nonce, Some(9));
    new_intent.nonce = 0;
    assert_eq!(
        check_supplied_spend_state_v2(&new_intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::NonceMismatch)
    );
}

#[test]
fn concrete_accounts_isolate_keys_grants_and_nonce_even_with_shared_identity() {
    let (quick, account, grant) = fixture();
    let state = SuppliedAssetSpendStateV2 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        account_active: true,
        grant: Some(&grant),
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
        current_spend_epoch: Some(3),
        next_spend_nonce: Some(9),
    };
    assert!(check_supplied_spend_state_v2(&quick, Some(&state), &[5; 32], 12).is_ok());
    let mut savings = quick.clone();
    savings.payer = vec![7; 32];
    assert_ne!(
        savings.nonce_identity().unwrap(),
        quick.nonce_identity().unwrap()
    );
    assert_eq!(
        check_supplied_spend_state_v2(&savings, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongAccount)
    );
    // Re-pointing the account alone does not transplant the other account's grant.
    let savings_account = savings.payer.clone();
    let savings_state = SuppliedAssetSpendStateV2 {
        account: &savings_account,
        ..state
    };
    assert_eq!(
        check_supplied_spend_state_v2(&savings, Some(&savings_state), &[5; 32], 12),
        Err(SpendPolicyError::WrongAccount)
    );
    let mut cross_asset = grant.clone();
    cross_asset.asset = 2;
    let state = SuppliedAssetSpendStateV2 {
        account: &account,
        grant: Some(&cross_asset),
        ..savings_state
    };
    assert_eq!(
        check_supplied_spend_state_v2(&quick, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongDomain)
    );
}

#[test]
fn manager_or_explicit_recovery_can_update_inactive_account_without_spend_grant() {
    use novovm_protocol::uca_spend::AssetAuthorityUpdateV2;
    let (mut intent, account, _) = fixture();
    intent.authority = AssetSpendAuthorityV3::Controller;
    intent.amount = 0;
    intent.recipient = intent.payer.clone();
    intent.spend_key_ref = [8; 32];
    intent.authority_update = Some(AssetAuthorityUpdateV2 {
        use_recovery: false,
        next_epoch: 4,
        active: true,
        management_key_ref: [10; 32],
        recovery_key_ref: Some([11; 32]),
        grants: Vec::new(),
    });
    let mut state = SuppliedAssetSpendStateV2 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        account_active: false,
        grant: None,
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
        current_spend_epoch: Some(3),
        next_spend_nonce: Some(9),
    };
    assert!(check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12).is_ok());
    intent.spend_key_ref = [4; 32]; // The quick-payment key is not a manager.
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongKey)
    );
    intent.spend_key_ref = [10; 32]; // A proposed new manager cannot approve itself.
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongKey)
    );
    intent.spend_key_ref = [9; 32];
    intent.authority_update.as_mut().unwrap().use_recovery = true;
    assert!(check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12).is_ok());
    state.recovery_key_ref = None;
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongKey)
    );
    state.recovery_key_ref = Some([9; 32]);
    state.next_spend_nonce = Some(10);
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::NonceMismatch)
    );
    state.next_spend_nonce = Some(9);
    state.current_spend_epoch = Some(4);
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongEpoch)
    );
}

#[test]
fn controller_spends_without_a_grant_but_still_requires_current_account_authority() {
    use SpendPolicyError::*;
    let cases = [
        (0, WrongParent),
        (1, WrongDomain),
        (2, WrongAccount),
        (3, AccountInactive),
        (4, WrongKey),
        (5, WrongKey),
        (6, WrongEpoch),
        (7, StateUnavailable),
        (8, StateUnavailable),
        (9, NonceMismatch),
        (10, Expired),
        (11, Expired),
        (12, WrongKey),
    ];
    for (case, expected) in cases {
        let (mut intent, account, _) = fixture();
        intent.authority = AssetSpendAuthorityV3::Controller;
        intent.spend_key_ref = [8; 32];
        let other_account = vec![2; 32];
        let mut state = SuppliedAssetSpendStateV2 {
            chain_id: 7,
            genesis: [2; 32],
            parent_root: [5; 32],
            account: &account,
            account_active: true,
            grant: None,
            management_key_ref: [8; 32],
            recovery_key_ref: Some([9; 32]),
            current_spend_epoch: Some(3),
            next_spend_nonce: Some(9),
        };
        assert!(check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12).is_ok());
        let mut height = 12;
        match case {
            0 => state.parent_root = [6; 32],
            1 => state.chain_id += 1,
            2 => state.account = &other_account,
            3 => state.account_active = false,
            4 => intent.spend_key_ref = [4; 32],
            5 => intent.spend_key_ref = [9; 32],
            6 => intent.authorization_epoch += 1,
            7 => state.current_spend_epoch = None,
            8 => state.next_spend_nonce = None,
            9 => state.next_spend_nonce = Some(10),
            10 => height = 9,
            11 => height = 20,
            12 => state.management_key_ref = [0; 32],
            _ => unreachable!(),
        }
        assert_eq!(
            check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], height),
            Err(expected),
            "controller boundary {case}"
        );
    }
}

#[test]
fn old_restricted_grant_is_not_promoted_and_controller_does_not_consume_its_budget() {
    let (mut intent, account, grant) = fixture();
    let original_grant = grant.clone();
    let mut state = SuppliedAssetSpendStateV2 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        account_active: true,
        grant: Some(&grant),
        management_key_ref: [8; 32],
        recovery_key_ref: Some([9; 32]),
        current_spend_epoch: Some(3),
        next_spend_nonce: Some(9),
    };
    assert!(check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12).is_ok());
    intent.amount += 1;
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::LimitExceeded)
    );
    intent.authority = AssetSpendAuthorityV3::Controller;
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongKey),
        "restricted key cannot choose unrestricted mode"
    );
    state.grant = None;
    intent.authority = AssetSpendAuthorityV3::DelegatedGrant;
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::StateUnavailable),
        "missing grant never becomes controller permission"
    );
    assert_eq!(grant, original_grant);

    intent.authority = AssetSpendAuthorityV3::Controller;
    intent.spend_key_ref = [8; 32];
    assert!(check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12).is_ok());
    // Grant state is separate from the controller's balance authority. Even an
    // unusable old delegation is neither consulted nor reset by this preflight.
    let mut unusable_grant = grant.clone();
    unusable_grant.revoked = true;
    unusable_grant.expires_at_height = 6;
    unusable_grant.remaining_debit_budget = 0;
    let state = SuppliedAssetSpendStateV2 {
        grant: Some(&unusable_grant),
        ..state
    };
    assert!(check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12).is_ok());
    assert!(unusable_grant.revoked);
    assert_eq!(unusable_grant.remaining_debit_budget, 0);
    intent.authority = AssetSpendAuthorityV3::DelegatedGrant;
    intent.spend_key_ref = [4; 32];
    assert_eq!(
        check_supplied_spend_state_v2(&intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::Revoked)
    );
}
