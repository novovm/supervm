//! Arithmetic regression vectors migrated from the reviewed legacy operator.
//! These tests neither execute AOEM nor establish transaction authorization.

use super::*;

fn intent() -> TransferIntent {
    TransferIntent {
        tx_hash: [3; 32],
        from: [1; 20].into(),
        to: [2; 20].into(),
        nonce_identity: "caller-bound-signer".into(),
        nonce: 0,
        amount: 30,
        approved_fee: 7,
        fee_cap: 7,
    }
}

fn snapshot() -> TransferSnapshot {
    TransferSnapshot {
        payer_balance: 100,
        recipient_balance: 20,
        next_nonce: 0,
    }
}

#[test]
fn exact_account_width_and_owned_thread_bounds_are_preserved() {
    fn owned<T: Send + Sync + 'static>() {}
    owned::<TransferIntent>();
    owned::<TransferSnapshot>();
    owned::<TransferOutcome>();
    for length in [0, 1, 19, 21, 31, 33, 128] {
        let bytes = vec![7; length];
        assert!(Account::try_from(bytes.as_slice()).is_err());
        assert!(Account::try_from(bytes).is_err());
    }
    let short = Account::from([7; 20]);
    let long = Account::from([7; 32]);
    assert_ne!(short, long);
    for account in [short, long] {
        assert_eq!(Account::try_from(account.as_bytes()).unwrap(), account);
        assert_eq!(
            account.hex(),
            format!("0x{}", "07".repeat(account.as_bytes().len()))
        );
    }
}

#[test]
fn success_matches_known_vector_and_does_not_mutate_inputs() {
    let input = intent();
    let state = snapshot();
    let saved = (input.clone(), state);
    let outcome = compute_outcome(&input, &state, None).unwrap();
    let delta = outcome.delta();
    assert!(outcome.is_success());
    assert_eq!(delta, &compute_delta(&input, &state).unwrap());
    assert_eq!(delta.tx_hash, input.tx_hash);
    assert_eq!(delta.payer.account, input.from);
    assert_eq!(delta.recipient.account, input.to);
    assert_eq!((delta.payer.before, delta.payer.after), (100, 63));
    assert_eq!((delta.recipient.before, delta.recipient.after), (20, 50));
    assert_eq!((delta.nonce_before, delta.nonce_after), (0, 1));
    assert_eq!(delta.nonce_identity, input.nonce_identity);
    assert_eq!(delta.fee_funding_delta, 7);
    assert_eq!(
        delta.payer.after + delta.recipient.after + delta.fee_funding_delta,
        120
    );
    assert_eq!((input, state), saved);
}

#[test]
fn fee_quote_cap_and_unaffordable_fee_consume_nonce_without_payment() {
    let input = intent();
    let mut capped = input.clone();
    capped.fee_cap = 6;
    let short = TransferSnapshot {
        payer_balance: 6,
        ..snapshot()
    };
    let quote_reason = "fee.quote.max_pay_exceeded: required_with_slippage=8 max_pay_amount=7";
    let quote = compute_outcome(&input, &snapshot(), Some(quote_reason)).unwrap();
    assert_eq!(
        quote.failure(),
        Some(&TransferFailure::Fee(quote_reason.into()))
    );
    let cap = compute_outcome(&capped, &snapshot(), None).unwrap();
    assert_eq!(
        cap.failure(),
        Some(&TransferFailure::Fee(
            "fee.quote.max_pay_exceeded: approved_fee=7 max_pay_amount=6 pay_asset=NOV".into()
        ))
    );
    let unpaid = compute_outcome(&input, &short, None).unwrap();
    assert_eq!(unpaid.failure(), Some(&TransferFailure::Fee(format!("fee.clearing.insufficient_user_balance: nov_fee_asset_debit_failed: account={} requested=7 available=6", input.from.hex()))));
    for outcome in [quote, cap, unpaid] {
        let delta = outcome.delta();
        assert!(!outcome.is_success());
        assert_eq!(delta.payer.before, delta.payer.after);
        assert_eq!(delta.recipient.before, delta.recipient.after);
        assert_eq!(delta.fee_funding_delta, 0);
        assert_eq!(delta.nonce_after, 1);
    }
}

#[test]
fn economic_failures_charge_affordable_fee_and_consume_nonce() {
    let input = intent();
    for (state, failure) in [
        (
            TransferSnapshot {
                payer_balance: 7,
                ..snapshot()
            },
            TransferError::InsufficientFunds {
                available: 7,
                required: 37,
            },
        ),
        (
            TransferSnapshot {
                payer_balance: 36,
                ..snapshot()
            },
            TransferError::InsufficientFunds {
                available: 36,
                required: 37,
            },
        ),
        (
            TransferSnapshot {
                recipient_balance: u128::MAX,
                ..snapshot()
            },
            TransferError::RecipientOverflow,
        ),
    ] {
        let outcome = compute_outcome(&input, &state, None).unwrap();
        assert_eq!(outcome.failure(), Some(&TransferFailure::Business(failure)));
        assert_eq!(outcome.delta().payer.after, state.payer_balance - 7);
        assert_eq!(outcome.delta().recipient.after, state.recipient_balance);
        assert_eq!(outcome.delta().fee_funding_delta, 7);
        assert_eq!(outcome.delta().nonce_after, 1);
    }
    let mut overflow = input;
    overflow.amount = u128::MAX;
    let state = TransferSnapshot {
        payer_balance: u128::MAX,
        recipient_balance: 0,
        next_nonce: 0,
    };
    let outcome = compute_outcome(&overflow, &state, None).unwrap();
    assert_eq!(
        outcome.failure(),
        Some(&TransferFailure::Business(TransferError::DebitOverflow))
    );
    assert_eq!(outcome.delta().payer.after, u128::MAX - 7);
    assert_eq!(outcome.delta().fee_funding_delta, 7);
}

#[test]
fn invalid_nonce_and_state_are_errors_even_when_quote_was_rejected() {
    let mut input = intent();
    input.nonce = 1;
    assert_eq!(
        compute_outcome(&input, &snapshot(), Some("quote rejection")),
        Err(TransferError::NonceMismatch {
            expected: 0,
            provided: 1
        })
    );
    input.nonce = u64::MAX;
    let state = TransferSnapshot {
        next_nonce: u64::MAX,
        ..snapshot()
    };
    assert_eq!(
        compute_outcome(&input, &state, Some("quote rejection")),
        Err(TransferError::NonceExhausted)
    );
    input.nonce_identity.clear();
    assert_eq!(
        compute_outcome(&input, &state, Some("quote rejection")),
        Err(TransferError::MissingNonceIdentity)
    );
    let mut same = intent();
    same.to = same.from.clone();
    assert_eq!(
        compute_outcome(&same, &snapshot(), Some("quote rejection")),
        Err(TransferError::InconsistentSelfBalance)
    );
}

#[test]
fn last_nonce_is_checked_and_shared_signer_does_not_alias_balance_accounts() {
    let mut input = intent();
    input.nonce = u64::MAX - 1;
    let state = TransferSnapshot {
        next_nonce: u64::MAX - 1,
        ..snapshot()
    };
    assert_eq!(
        compute_outcome(&input, &state, None)
            .unwrap()
            .delta()
            .nonce_after,
        u64::MAX
    );
    let first = intent();
    let one = compute_outcome(&first, &snapshot(), None).unwrap();
    let mut second = first.clone();
    second.from = [1; 32].into();
    second.nonce = one.delta().nonce_after;
    let two = compute_outcome(
        &second,
        &TransferSnapshot {
            next_nonce: 1,
            ..snapshot()
        },
        None,
    )
    .unwrap();
    assert_eq!(one.delta().nonce_identity, two.delta().nonce_identity);
    assert_ne!(one.delta().payer.account, two.delta().payer.account);
    assert_eq!(two.delta().nonce_after, 2);
    assert!(compute_outcome(&second, &snapshot(), None).is_err());
}

#[test]
fn self_transfer_has_one_balance_and_requires_full_amount_plus_fee() {
    let mut input = intent();
    input.to = input.from.clone();
    for (balance, success, after, fee) in [
        (6, false, 6, 0),
        (7, false, 0, 7),
        (36, false, 29, 7),
        (37, true, 30, 7),
        (100, true, 93, 7),
    ] {
        let state = TransferSnapshot {
            payer_balance: balance,
            recipient_balance: balance,
            next_nonce: 0,
        };
        let result = compute_outcome(&input, &state, None).unwrap();
        assert_eq!(result.is_success(), success);
        assert_eq!(result.delta().payer, result.delta().recipient);
        assert_eq!(result.delta().payer.after, after);
        assert_eq!(result.delta().fee_funding_delta, fee);
        assert_eq!(after + fee, balance);
        assert_eq!(result.delta().nonce_after, 1);
    }
}

#[test]
fn exact_u128_maximum_is_allowed_but_overflow_is_never_saturated() {
    let mut input = intent();
    input.amount = u128::MAX;
    input.approved_fee = 0;
    let state = TransferSnapshot {
        payer_balance: u128::MAX,
        recipient_balance: 0,
        next_nonce: 0,
    };
    let result = compute_outcome(&input, &state, None).unwrap();
    assert!(result.is_success());
    assert_eq!(
        (result.delta().payer.after, result.delta().recipient.after),
        (0, u128::MAX)
    );
    let overflowing = TransferSnapshot {
        recipient_balance: 1,
        ..state
    };
    assert_eq!(
        compute_delta(&input, &overflowing),
        Err(TransferError::RecipientOverflow)
    );
    let failed = compute_outcome(&input, &overflowing, None).unwrap();
    assert_eq!(
        (failed.delta().payer.after, failed.delta().recipient.after),
        (u128::MAX, 1)
    );
    input.to = input.from.clone();
    let same = TransferSnapshot {
        recipient_balance: u128::MAX,
        ..state
    };
    let result = compute_outcome(&input, &same, None).unwrap();
    assert!(result.is_success());
    assert_eq!(result.delta().payer, result.delta().recipient);
    assert_eq!(result.delta().payer.after, u128::MAX);
}

#[test]
fn zero_cap_is_literal_and_zero_amount_does_not_invent_a_fee_policy() {
    let mut input = intent();
    input.fee_cap = 0;
    assert!(matches!(
        compute_delta(&input, &snapshot()),
        Err(TransferError::FeeCapExceeded { .. })
    ));
    input.approved_fee = 0;
    input.amount = 0;
    let result = compute_outcome(&input, &snapshot(), None).unwrap();
    assert!(result.is_success());
    assert_eq!(result.delta().payer.after, 100);
    assert_eq!(result.delta().recipient.after, 20);
    assert_eq!(result.delta().nonce_after, 1);
    input.approved_fee = 7;
    input.fee_cap = 7;
    input.amount = 1;
    let paid = compute_outcome(&input, &snapshot(), None).unwrap();
    assert!(paid.is_success());
    assert_eq!(
        (paid.delta().payer.after, paid.delta().recipient.after),
        (92, 21)
    );
}

#[test]
fn global_rejection_restores_pre_fee_balances_not_nonce_or_dependent_predictions() {
    for same in [false, true] {
        let mut input = intent();
        if same {
            input.to = input.from.clone();
        }
        for balance in [7, 100] {
            let state = TransferSnapshot {
                payer_balance: balance,
                recipient_balance: if same { balance } else { 20 },
                next_nonce: 0,
            };
            let outcome = compute_outcome(&input, &state, None).unwrap();
            let rejected = outcome.reject_fee("fee.settlement.amount_overflow".into());
            assert_eq!(rejected.delta().payer.after, state.payer_balance);
            assert_eq!(rejected.delta().recipient.after, state.recipient_balance);
            assert_eq!(rejected.delta().fee_funding_delta, 0);
            assert_eq!(rejected.delta().nonce_after, 1);
        }
    }
    let first = compute_outcome(&intent(), &snapshot(), None).unwrap();
    let mut dependent = intent();
    dependent.tx_hash = [4; 32];
    dependent.nonce_identity = "recipient-signer".into();
    dependent.from = dependent.to.clone();
    dependent.to = [4; 20].into();
    let predicted = TransferSnapshot {
        payer_balance: first.delta().recipient.after,
        recipient_balance: 0,
        next_nonce: 0,
    };
    assert!(compute_outcome(&dependent, &predicted, None)
        .unwrap()
        .is_success());
    let rejected = first.reject_fee("fee.settlement.amount_overflow".into());
    let corrected = TransferSnapshot {
        payer_balance: rejected.delta().recipient.after,
        ..predicted
    };
    let repaired = compute_outcome(&dependent, &corrected, None).unwrap();
    assert!(!repaired.is_success());
    assert_eq!(
        (
            repaired.delta().payer.after,
            repaired.delta().recipient.after
        ),
        (13, 0)
    );
}

#[test]
fn ordered_pending_funding_checks_overflow_without_mutating_deltas() {
    let delta = compute_delta(&intent(), &snapshot()).unwrap();
    let deltas = [delta.clone(), delta];
    let before = deltas.clone();
    assert_eq!(checked_fee_funding_after(3, &deltas).unwrap(), 17);
    assert_eq!(
        checked_fee_funding_after(u128::MAX - 14, &deltas).unwrap(),
        u128::MAX
    );
    assert_eq!(
        checked_fee_funding_after(u128::MAX - 13, &deltas),
        Err(TransferError::FeeFundingOverflow)
    );
    assert_eq!(
        checked_fee_funding_after(u128::MAX, &[]).unwrap(),
        u128::MAX
    );
    assert_eq!(deltas, before);
}

#[test]
fn bounded_exhaustive_small_values_preserve_failure_rules_and_conservation() {
    for same in [false, true] {
        for payer in 0..=12u128 {
            for recipient in 0..=4u128 {
                for amount in 0..=14u128 {
                    for fee in 0..=4u128 {
                        let mut input = intent();
                        input.amount = amount;
                        input.approved_fee = fee;
                        input.fee_cap = fee;
                        if same {
                            input.to = input.from.clone();
                        }
                        let recipient = if same { payer } else { recipient };
                        let state = TransferSnapshot {
                            payer_balance: payer,
                            recipient_balance: recipient,
                            next_nonce: 0,
                        };
                        let result = compute_outcome(&input, &state, None).unwrap();
                        let paid = if payer >= fee { fee } else { 0 };
                        let success = payer >= amount + fee;
                        let expected_payer =
                            payer - paid - if success && !same { amount } else { 0 };
                        let expected_recipient = if same {
                            expected_payer
                        } else {
                            recipient + if success { amount } else { 0 }
                        };
                        assert_eq!(result.is_success(), success);
                        assert_eq!(result.delta().payer.after, expected_payer);
                        assert_eq!(result.delta().recipient.after, expected_recipient);
                        assert_eq!(result.delta().fee_funding_delta, paid);
                        assert_eq!(result.delta().nonce_after, 1);
                        assert_eq!(
                            expected_payer + paid + if same { 0 } else { expected_recipient },
                            payer + if same { 0 } else { recipient }
                        );
                    }
                }
            }
        }
    }
}
