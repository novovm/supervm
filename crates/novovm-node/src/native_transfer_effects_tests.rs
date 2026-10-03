//! Pure scheduling differentials, not AOEM/runtime throughput evidence.
use super::super::compute_outcome_v1;
use super::*;

#[derive(Clone)]
struct Work {
    intent: TransferIntent,
    snapshot: TransferSnapshot,
    fee_rejection: Option<&'static str>,
}

fn account(id: u8) -> Account {
    [id; 20].into()
}

fn intent(id: u8, from: Account, to: Account, nonce: u64, amount: u128) -> TransferIntent {
    TransferIntent {
        tx_hash: [id; 32],
        nonce_identity: from.to_hex_prefixed(),
        from,
        to,
        nonce,
        amount,
        approved_fee: 2,
        fee_cap: 2,
    }
}

fn work(intents: Vec<TransferIntent>, balances: &[(Account, u128)]) -> Vec<Work> {
    let balances: BTreeMap<_, _> = balances.iter().cloned().collect();
    intents
        .into_iter()
        .map(|intent| Work {
            snapshot: TransferSnapshot {
                payer_balance: balances[&intent.from],
                recipient_balance: balances[&intent.to],
                next_nonce: 0,
            },
            intent,
            fee_rejection: None,
        })
        .collect()
}

fn plan(work: &[Work]) -> Result<TransferEffectPlanV1> {
    TransferEffectPlanV1::build(
        &work
            .iter()
            .map(|item| item.intent.clone())
            .collect::<Vec<_>>(),
        &work.iter().map(|item| item.snapshot).collect::<Vec<_>>(),
    )
}

// Exactly the pre-existing arithmetic, with a local view per component. It is
// intentionally independent of the new effect reducer and does not fake a
// callback or parallel execution observation.
fn evaluate(work: &[Work], indices: &[usize]) -> Vec<TransferExecutionOutcomeV1> {
    let mut balances = BTreeMap::new();
    let mut nonces = BTreeMap::new();
    for &index in indices {
        let item = &work[index];
        balances.insert(item.intent.from.clone(), item.snapshot.payer_balance);
        balances.insert(item.intent.to.clone(), item.snapshot.recipient_balance);
        nonces.insert(item.intent.nonce_identity.clone(), item.snapshot.next_nonce);
    }
    indices
        .iter()
        .map(|&index| {
            let item = &work[index];
            let state = TransferSnapshot {
                payer_balance: balances[&item.intent.from],
                recipient_balance: balances[&item.intent.to],
                next_nonce: nonces[&item.intent.nonce_identity],
            };
            let outcome = compute_outcome_v1(&item.intent, &state, item.fee_rejection).unwrap();
            let delta = outcome.delta();
            balances.insert(delta.payer.account.clone(), delta.payer.after);
            balances.insert(delta.recipient.account.clone(), delta.recipient.after);
            nonces.insert(delta.nonce_identity.clone(), delta.nonce_after);
            outcome
        })
        .collect()
}

fn reduce_in_completion_order(
    work: &[Work],
    plan: &TransferEffectPlanV1,
    completion_order: &[usize],
) -> Vec<TransferExecutionOutcomeV1> {
    assert_eq!(completion_order.len(), plan.components.len());
    let mut outcomes = vec![None; work.len()];
    for &component in completion_order {
        let indices = &plan.components[component];
        for (&index, outcome) in indices.iter().zip(evaluate(work, indices)) {
            assert!(outcomes[index].replace(outcome).is_none());
        }
    }
    let mut outcomes: Vec<_> = outcomes.into_iter().map(Option::unwrap).collect();
    plan.reduce_ordered(&mut outcomes).unwrap();
    outcomes
}

fn assert_serial_parity(work: &[Work]) -> (TransferEffectPlanV1, Vec<TransferExecutionOutcomeV1>) {
    let plan = plan(work).unwrap();
    let expected = evaluate(work, &(0..work.len()).collect::<Vec<_>>());
    let forward: Vec<_> = (0..plan.components.len()).collect();
    let reverse: Vec<_> = forward.iter().rev().copied().collect();
    assert_eq!(reduce_in_completion_order(work, &plan, &forward), expected);
    assert_eq!(reduce_in_completion_order(work, &plan, &reverse), expected);
    (plan, expected)
}

#[test]
fn shared_pure_credit_splits_components_and_preserves_complete_serial_outcomes() {
    let work = work(
        vec![
            intent(1, account(1), account(9), 0, 10),
            intent(2, account(2), account(9), 0, 20),
            intent(3, account(1), account(9), 1, 5),
        ],
        &[(account(1), 100), (account(2), 100), (account(9), 7)],
    );
    let (plan, outcomes) = assert_serial_parity(&work);
    assert_eq!(plan.components, [vec![0, 2], vec![1]]);
    assert!(plan.has_credit_reduction());
    assert_eq!(outcomes[1].delta().recipient.before, 17);
    assert_eq!(outcomes[2].delta().recipient.before, 37);
    assert_eq!(outcomes[2].delta().recipient.after, 42);
    assert_ne!(evaluate(&work, &[1])[0], outcomes[1]);
}

#[test]
fn every_three_component_completion_order_has_the_same_ordered_outcomes() {
    let work = work(
        (1..=3)
            .map(|id| intent(id, account(id), account(9), 0, u128::from(id)))
            .collect(),
        &[
            (account(1), 100),
            (account(2), 100),
            (account(3), 100),
            (account(9), 0),
        ],
    );
    let (plan, expected) = assert_serial_parity(&work);
    assert_eq!(plan.components.len(), 3);
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        assert_eq!(reduce_in_completion_order(&work, &plan, &order), expected);
    }
}

#[test]
fn exact_account_width_is_preserved_but_shared_signer_nonce_still_conflicts() {
    let short = account(1);
    let long: Account = [1; 32].into();
    let first = intent(1, short.clone(), account(9), 0, 10);
    let mut second = intent(2, long.clone(), account(9), 1, 20);
    second.nonce_identity.clone_from(&first.nonce_identity);
    let work = work(
        vec![first, second, intent(3, account(3), account(9), 0, 30)],
        &[
            (short.clone(), 100),
            (long.clone(), 200),
            (account(3), 100),
            (account(9), 0),
        ],
    );
    let (plan, outcomes) = assert_serial_parity(&work);
    assert_eq!(plan.components, [vec![0, 1], vec![2]]);
    assert!(plan.has_credit_reduction());
    assert_ne!(
        outcomes[0].delta().payer.account,
        outcomes[1].delta().payer.account
    );
    assert_eq!(outcomes[0].delta().payer.account, short);
    assert_eq!(outcomes[1].delta().payer.account, long);
    assert_eq!(outcomes[1].delta().nonce_after, 2);
    assert_eq!(outcomes[1].delta().payer.before, 200);
}

#[test]
fn later_payer_and_self_transfer_each_cancel_the_pure_credit_guard() {
    for to in [account(3), account(9)] {
        let work = work(
            vec![
                intent(1, account(1), account(9), 0, 10),
                intent(2, account(2), account(9), 0, 20),
                intent(3, account(9), to, 0, 35),
            ],
            &[
                (account(1), 100),
                (account(2), 100),
                (account(3), 0),
                (account(9), 7),
            ],
        );
        let (plan, outcomes) = assert_serial_parity(&work);
        assert_eq!(plan.components, [vec![0, 1, 2]]);
        assert!(!plan.has_credit_reduction());
        assert!(outcomes[2].is_success());
        if work[2].intent.from == work[2].intent.to {
            assert_eq!(outcomes[2].delta().payer, outcomes[2].delta().recipient);
            assert_eq!(outcomes[2].delta().payer.after, 35);
        }
    }
}

#[test]
fn inconsistent_parent_balances_and_shared_nonce_views_are_rejected() {
    let original = work(
        vec![
            intent(1, account(1), account(9), 0, 1),
            intent(2, account(2), account(9), 0, 1),
        ],
        &[(account(1), 100), (account(2), 100), (account(9), 0)],
    );
    let mut bad = original.clone();
    bad[1].snapshot.recipient_balance = 1;
    assert!(plan(&bad)
        .unwrap_err()
        .to_string()
        .contains("parent balance"));
    bad = original;
    bad[1].intent.nonce_identity = bad[0].intent.nonce_identity.clone();
    bad[1].snapshot.next_nonce = 1;
    assert!(plan(&bad).unwrap_err().to_string().contains("parent nonce"));
    assert!(TransferEffectPlanV1::build(&[], &[]).is_err());
    assert!(TransferEffectPlanV1::build(&[bad[0].intent.clone()], &[]).is_err());
    assert!(TransferEffectPlanV1::build(
        &vec![bad[0].intent.clone(); 1025],
        &vec![bad[0].snapshot; 1025]
    )
    .is_err());
}

#[test]
fn exact_maximum_credit_bound_is_allowed_without_saturation() {
    let work = work(
        vec![
            intent(1, account(1), account(9), 0, 5),
            intent(2, account(2), account(9), 0, 7),
        ],
        &[
            (account(1), 100),
            (account(2), 100),
            (account(9), u128::MAX - 12),
        ],
    );
    let (plan, outcomes) = assert_serial_parity(&work);
    assert!(plan.has_credit_reduction());
    assert_eq!(plan.components.len(), 2);
    assert!(outcomes.iter().all(TransferExecutionOutcomeV1::is_success));
    assert_eq!(outcomes[1].delta().recipient.after, u128::MAX);
}

#[test]
fn parent_plus_credit_overflow_falls_back_instead_of_rejecting_the_batch() {
    // Arithmetic defense; this synthetic state is not a claim of a reachable
    // globally supply-conserving genesis allocation.
    let work = work(
        vec![
            intent(1, account(1), account(9), 0, 7),
            intent(2, account(2), account(9), 0, 7),
        ],
        &[
            (account(1), 100),
            (account(2), 100),
            (account(9), u128::MAX - 10),
        ],
    );
    let (plan, outcomes) = assert_serial_parity(&work);
    assert!(!plan.has_credit_reduction());
    assert_eq!(plan.components, [vec![0, 1]]);
    assert!(outcomes[0].is_success());
    assert_eq!(
        outcomes[1].failure(),
        Some(&TransferExecutionFailureV1::Business(
            TransferError::RecipientOverflow
        ))
    );
    assert_eq!(outcomes[1].delta().fee_funding_delta, 2);
    assert_eq!(outcomes[1].delta().nonce_after, 1);
    assert_eq!(outcomes[1].delta().recipient.after, u128::MAX - 3);
}

#[test]
fn requested_amount_sum_overflow_keeps_the_conservative_reference_path() {
    let mut first = intent(1, account(1), account(9), 0, u128::MAX);
    first.approved_fee = 0;
    let work = work(
        vec![first, intent(2, account(2), account(9), 0, 1)],
        &[(account(1), u128::MAX), (account(2), 100), (account(9), 0)],
    );
    let (plan, outcomes) = assert_serial_parity(&work);
    assert!(!plan.has_credit_reduction());
    assert_eq!(plan.components, [vec![0, 1]]);
    assert!(outcomes[0].is_success());
    assert_eq!(
        outcomes[1].failure(),
        Some(&TransferExecutionFailureV1::Business(
            TransferError::RecipientOverflow
        ))
    );
}

#[test]
fn mixed_failures_and_zero_amount_preserve_fee_nonce_and_recipient_prefixes() {
    let mut work = work(
        vec![
            intent(1, account(1), account(9), 0, 3),
            intent(2, account(2), account(9), 0, 3),
            intent(3, account(3), account(9), 0, 99),
            intent(4, account(4), account(9), 0, 10),
            intent(5, account(5), account(9), 0, 0),
        ],
        &[
            (account(1), 10),
            (account(2), 1),
            (account(3), 6),
            (account(4), 100),
            (account(5), 10),
            (account(9), 7),
        ],
    );
    work[3].fee_rejection = Some("fee.quote.test_refusal");
    let (plan, outcomes) = assert_serial_parity(&work);
    assert!(plan.has_credit_reduction());
    assert_eq!(
        outcomes
            .iter()
            .map(TransferExecutionOutcomeV1::is_success)
            .collect::<Vec<_>>(),
        [true, false, false, false, true]
    );
    assert_eq!(
        outcomes
            .iter()
            .map(|outcome| outcome.delta().fee_funding_delta)
            .collect::<Vec<_>>(),
        [2, 0, 2, 0, 2]
    );
    assert!(outcomes
        .iter()
        .all(|outcome| outcome.delta().nonce_after == 1));
    for outcome in &outcomes[1..] {
        assert_eq!(outcome.delta().recipient.before, 10);
        assert_eq!(outcome.delta().recipient.after, 10);
    }
}

#[test]
fn reduction_rejects_wrong_count_identity_and_noncredit_effect() {
    let work = work(
        vec![
            intent(1, account(1), account(9), 0, 5),
            intent(2, account(2), account(9), 0, 7),
        ],
        &[(account(1), 100), (account(2), 100), (account(9), 0)],
    );
    let plan = plan(&work).unwrap();
    let original = vec![
        evaluate(&work, &[0]).remove(0),
        evaluate(&work, &[1]).remove(0),
    ];
    assert!(plan.reduce_ordered(&mut original[..1].to_vec()).is_err());
    for mutation in 0..4 {
        let mut outcomes = original.clone();
        match mutation {
            0 => outcomes[0].delta.tx_hash = [99; 32],
            1 => outcomes[0].delta.payer.account = account(7),
            2 => outcomes[0].delta.recipient.account = account(7),
            _ => outcomes[0].delta.recipient.after += 1,
        }
        assert!(plan.reduce_ordered(&mut outcomes).is_err());
    }
}

#[test]
fn bounded_deterministic_differentials_match_serial_for_192_mixed_batches() {
    fn next(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *seed >> 19
    }
    let mut seed = 0x6e6f_765f_6372_6564;
    let accounts: Vec<Account> = (1..=8)
        .map(|id| {
            if id % 2 == 0 {
                [id; 32].into()
            } else {
                account(id)
            }
        })
        .collect();
    let mut reduced_batches = 0;
    let mut fallback_batches = 0;
    for case in 0..192 {
        let balances: Vec<_> = accounts
            .iter()
            .enumerate()
            .map(|(index, account)| {
                let balance = if case % 11 == 0 && index == 7 {
                    u128::MAX - 20
                } else {
                    u128::from(next(&mut seed) % 500)
                };
                (account.clone(), balance)
            })
            .collect();
        let count = 3 + (next(&mut seed) % 14) as usize;
        let mut nonces = BTreeMap::<String, u64>::new();
        let mut intents = Vec::new();
        for index in 0..count {
            let from = (next(&mut seed) % 6) as usize;
            let to = if case % 4 == 0 {
                7
            } else {
                (next(&mut seed) % 8) as usize
            };
            let amount = match next(&mut seed) % 19 {
                0 => u128::MAX,
                1 => 0,
                _ => u128::from(next(&mut seed) % 100),
            };
            let mut input = intent(
                (index + 1) as u8,
                accounts[from].clone(),
                accounts[to].clone(),
                0,
                amount,
            );
            // Distinct 20/32 byte balance keys may have the same signer nonce.
            input.nonce_identity = format!("signer-{}", from / 2);
            let nonce = nonces.entry(input.nonce_identity.clone()).or_default();
            input.nonce = *nonce;
            *nonce += 1;
            input.approved_fee = u128::from(next(&mut seed) % 8);
            input.fee_cap = if next(&mut seed).is_multiple_of(13) {
                0
            } else {
                input.approved_fee
            };
            intents.push(input);
        }
        let mut work = work(intents, &balances);
        for item in &mut work {
            if next(&mut seed).is_multiple_of(17) {
                item.fee_rejection = Some("fee.quote.differential_refusal");
            }
        }
        let (plan, expected) = assert_serial_parity(&work);
        if plan.has_credit_reduction() {
            reduced_batches += 1;
        } else {
            fallback_batches += 1;
        }
        let mut order: Vec<_> = (0..plan.components.len()).collect();
        for index in (1..order.len()).rev() {
            let other = (next(&mut seed) % (index as u64 + 1)) as usize;
            order.swap(index, other);
        }
        assert_eq!(
            reduce_in_completion_order(&work, &plan, &order),
            expected,
            "differential case {case}"
        );
    }
    assert!(
        reduced_batches > 0,
        "must actually exercise the checked reduction"
    );
    assert!(
        fallback_batches > 0,
        "must also exercise conservative dependencies"
    );
}
