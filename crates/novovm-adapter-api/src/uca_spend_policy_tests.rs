use super::*;

fn fixture() -> (UcaSpendIntentV1, UcaAccount, SuppliedSpendGrantV1) {
    (
        UcaSpendIntentV1 {
            chain_id: 7,
            genesis: [2; 32],
            account_id: "uca:alice".into(),
            authorization_epoch: 3,
            spend_key_ref: [4; 32],
            recipient_uca: "uca:bob".into(),
            amount: 100,
            max_fee: 20,
            nonce: 9,
            not_before_height: 10,
            expires_at_height: 20,
        },
        UcaAccount {
            uca_id: "uca:alice".into(),
            primary_key_ref: vec![8; 32],
            primary_key_binding: None,
            status: UcaStatus::Active,
            created_at: 1,
            updated_at: 2,
        },
        SuppliedSpendGrantV1 {
            account_id: "uca:alice".into(),
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
        check_supplied_spend_state_v1(&intent, None, &[5; 32], 12),
        Err(SpendPolicyError::StateUnavailable)
    );
    let state = SuppliedSpendStateV1 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        grant: &grant,
        current_spend_epoch: Some(3),
        next_spend_nonce: None,
    };
    assert_eq!(
        check_supplied_spend_state_v1(&intent, Some(&state), &[5; 32], 12),
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
            account.uca_id = "other".into();
        }
        if case == 4 {
            account.status = UcaStatus::Recovering;
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
            grant.account_id = "other".into();
        }
        if case == 15 {
            grant.authorization_epoch = 0;
        }
        let mut state = SuppliedSpendStateV1 {
            chain_id: 7,
            genesis: [2; 32],
            parent_root: [5; 32],
            account: &account,
            grant: &grant,
            current_spend_epoch: Some(3),
            next_spend_nonce: Some(9),
        };
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
            check_supplied_spend_state_v1(&intent, Some(&state), &[5; 32], 12),
            Err(expected),
            "case {case}"
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
    let mut state = SuppliedSpendStateV1 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        grant: &grant,
        current_spend_epoch: Some(4),
        next_spend_nonce: Some(9),
    };
    assert_eq!(
        check_supplied_spend_state_v1(&rotated, Some(&state), &[5; 32], 12),
        Ok(())
    );
    assert_eq!(
        check_supplied_spend_state_v1(&old, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongKey)
    );
    state.next_spend_nonce = Some(10);
    assert_eq!(
        check_supplied_spend_state_v1(&rotated, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::NonceMismatch)
    );
    rotated.nonce = 10;
    assert_eq!(
        check_supplied_spend_state_v1(&rotated, Some(&state), &[5; 32], 12),
        Ok(())
    );
    assert_eq!(
        check_supplied_spend_state_v1(&rotated, Some(&state), &[5; 32], 20),
        Err(SpendPolicyError::Expired)
    );
    assert_eq!(
        check_supplied_spend_state_v1(&rotated, Some(&state), &[5; 32], 9),
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
        let state = SuppliedSpendStateV1 {
            chain_id: 7,
            genesis: [2; 32],
            parent_root: [5; 32],
            account: &account,
            grant: &grant,
            current_spend_epoch: epoch,
            next_spend_nonce: Some(9),
        };
        assert_eq!(
            check_supplied_spend_state_v1(&intent, Some(&state), &[5; 32], 12),
            Err(expected)
        );
    }
}

#[test]
fn advancing_current_epoch_invalidates_retained_old_grant_without_resetting_nonce() {
    let (old, mut account, old_grant) = fixture();
    // Management timestamps may advance independently. Neither is a spend epoch.
    account.updated_at = 4;
    let mut state = SuppliedSpendStateV1 {
        chain_id: 7,
        genesis: [2; 32],
        parent_root: [5; 32],
        account: &account,
        grant: &old_grant,
        current_spend_epoch: Some(3),
        next_spend_nonce: Some(9),
    };
    assert_eq!(
        check_supplied_spend_state_v1(&old, Some(&state), &[5; 32], 12),
        Ok(())
    );
    state.current_spend_epoch = Some(4);
    assert!(!old_grant.revoked);
    assert_eq!(
        check_supplied_spend_state_v1(&old, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongEpoch)
    );
    let mut new_intent = old.clone();
    new_intent.authorization_epoch = 4;
    assert_eq!(
        check_supplied_spend_state_v1(&new_intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::WrongEpoch),
        "new intent cannot revive an old grant"
    );
    let (_, _, mut new_grant) = fixture();
    new_grant.authorization_epoch = 4;
    new_grant.key_ref = [6; 32];
    new_intent.spend_key_ref = new_grant.key_ref;
    state.grant = &new_grant;
    assert_eq!(
        check_supplied_spend_state_v1(&new_intent, Some(&state), &[5; 32], 12),
        Ok(())
    );
    assert_eq!(
        old.nonce_identity().unwrap(),
        new_intent.nonce_identity().unwrap()
    );
    assert_eq!(state.next_spend_nonce, Some(9));
    new_intent.nonce = 0;
    assert_eq!(
        check_supplied_spend_state_v1(&new_intent, Some(&state), &[5; 32], 12),
        Err(SpendPolicyError::NonceMismatch)
    );
}
