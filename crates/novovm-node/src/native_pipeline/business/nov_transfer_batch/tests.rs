use super::*;

fn request(payer: u8, recipient: u8, amount: u128) -> TransferFeeRequest {
    TransferFeeRequest {
        tx_hash: [payer; 32],
        payer: [payer; 20].into(),
        recipient: [recipient; 20].into(),
        asset: "NOV".into(),
        amount,
        pay_asset: "NOV".into(),
        max_pay_amount: 0,
        slippage_bps: 0,
    }
}

fn plan(
    requests: &[TransferFeeRequest],
    identities: &[[u8; 32]],
    recipient_balance: u128,
) -> (Vec<Vec<usize>>, BTreeSet<Account>) {
    let mut balances = BTreeMap::new();
    for request in requests {
        balances.insert(request.payer.clone(), Some(1000));
        balances.insert(request.recipient.clone(), Some(recipient_balance));
    }
    components(requests, identities, &balances)
}

#[test]
fn shared_credit_is_not_a_conflict_but_payer_dependencies_are() {
    let requests: Vec<_> = (1..=32).map(|payer| request(payer, 200, 100)).collect();
    let identities: Vec<_> = (1..=32).map(|payer| [payer; 32]).collect();
    let (groups, credits) = plan(&requests, &identities, 0);
    assert_eq!(groups.len(), 32);
    assert_eq!(credits, BTreeSet::from([Account::from([200; 20])]));
    let mut spends = requests.clone();
    spends.push(request(200, 201, 1));
    let mut with_signer = identities;
    with_signer.push([200; 32]);
    let (groups, credits) = plan(&spends, &with_signer, 0);
    assert_eq!(groups.len(), 1);
    assert!(!credits.contains(&[200; 20].into()));
}

#[test]
fn credit_guard_checks_entire_possible_sum_before_relaxing_dependencies() {
    let transactions = [request(1, 200, 1), request(2, 200, 1)];
    let ids = [[1; 32], [2; 32]];
    assert_eq!(plan(&transactions, &ids, u128::MAX - 2).0.len(), 2);
    let (groups, credits) = plan(&transactions, &ids, u128::MAX - 1);
    assert_eq!(groups, vec![vec![0, 1]]);
    assert!(credits.is_empty());
    let requests = [request(1, 200, u128::MAX), request(2, 200, 1)];
    assert_eq!(plan(&requests, &ids, 0).0.len(), 1);
}

#[test]
fn signer_alias_dependency_is_independent_of_balance_account_width() {
    let first = request(1, 200, 10);
    let mut second = request(2, 200, 10);
    second.payer = [3; 32].into();
    let (groups, credits) = plan(&[first, second], &[[8; 32], [8; 32]], 0);
    assert_eq!(groups, vec![vec![0, 1]]);
    assert_eq!(credits.len(), 1);
}

#[test]
fn transitive_dependencies_keep_original_order_without_assuming_contiguity() {
    let requests = [
        request(1, 200, 10),
        request(2, 201, 10),
        request(1, 201, 10),
        request(201, 202, 10),
    ];
    let (groups, _) = plan(&requests, &[[1; 32], [2; 32], [1; 32], [3; 32]], 0);
    assert_eq!(groups, vec![vec![0, 1, 2, 3]]);
}

#[test]
fn zero_and_self_transfers_have_no_unsafe_pure_credit_classification() {
    let (groups, credits) = plan(
        &[request(1, 1, 0), request(2, 200, 0)],
        &[[1; 32], [2; 32]],
        u128::MAX,
    );
    assert_eq!(groups.len(), 2);
    assert!(!credits.contains(&[1; 20].into()));
    assert!(credits.contains(&[200; 20].into()));
}

#[test]
fn serialized_accounts_cannot_bypass_exact_identity_width() {
    for length in [0, 19, 21, 31, 33, 512] {
        let bytes = postcard::to_allocvec(&vec![1u8; length]).unwrap();
        assert!(postcard::from_bytes::<Account>(&bytes).is_err());
    }
    for account in [Account::from([1; 20]), Account::from([2; 32])] {
        let bytes = postcard::to_allocvec(&account).unwrap();
        assert_eq!(postcard::from_bytes::<Account>(&bytes).unwrap(), account);
    }
}
