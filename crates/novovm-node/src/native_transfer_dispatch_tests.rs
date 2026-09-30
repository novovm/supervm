use super::*;

fn transfer() -> NovNativeTxWireV1 {
    NovNativeTxWireV1 {
        chain_id: 81_742,
        kind: NovTxKindV1::Transfer(novovm_protocol::NovTransferTxV1 {
            from: vec![1; 20],
            to: vec![2; 32],
            asset: "NOV".into(),
            amount: u128::MAX,
            nonce: 0,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "NOV".into(),
                max_pay_amount: 0,
                slippage_bps: 0,
            },
        }),
        signature: Vec::new(),
    }
}

#[test]
fn transfer_fee_projection_preserves_wire_and_reuses_the_unified_quote() {
    let tx = transfer();
    let original = tx.clone();
    let request = fee_request_v1(&tx, [9; 32]).unwrap();
    assert_eq!(tx, original);
    assert_eq!(request.caller, vec![1; 20]);
    assert_eq!(request.gas_like_limit, Some(21_000));
    let args: serde_json::Value = serde_json::from_slice(&request.args).unwrap();
    assert_eq!(args["amount"], u128::MAX.to_string());
    assert_eq!(args["to"], format!("0x{}", "02".repeat(32)));
    let pure = quote_v1(&request, 123).unwrap();
    assert_eq!(pure.nov_amount, estimate_execution_fee_nov_v1(&request));
    let mut store = NovNativeExecutionStoreV1::default();
    let ordered = quote_fee_policy_from_execution_request_v1(&request, &mut store, 123).unwrap();
    assert_eq!(pure, ordered);
    assert_eq!(store.module_state.last_fee_quote, Some(pure));
}

#[test]
fn transfer_quote_checks_slippage_inclusive_cap_and_zero_auto_cap() {
    let mut request = fee_request_v1(&transfer(), [9; 32]).unwrap();
    request.fee_slippage_bps = 100;
    let auto = quote_v1(&request, 123).unwrap();
    assert!(auto.max_pay_amount > auto.nov_amount);
    request.fee_max_pay_amount = auto.nov_amount;
    let error = quote_v1(&request, 123).unwrap_err().to_string();
    assert!(error.contains("fee.quote.max_pay_exceeded"));
    let mut store = NovNativeExecutionStoreV1::default();
    assert_eq!(
        quote_fee_policy_from_execution_request_v1(&request, &mut store, 123)
            .unwrap_err()
            .to_string(),
        error
    );
    assert_eq!(
        store.module_state.fee_quote_failure_counts["NOV:max_pay_exceeded"],
        1
    );
    assert!(store.module_state.account_asset_balances.is_empty());
}

#[test]
fn fresh_transfer_capability_does_not_open_legacy_or_unimplemented_assets() {
    let mut tx = transfer();
    assert!(require_execution_capability_v1(&tx, true).is_ok());
    assert!(require_execution_capability_v1(&tx, false).is_err());
    let NovTxKindV1::Transfer(transfer) = &mut tx.kind else {
        unreachable!()
    };
    transfer.from = vec![1; 32];
    assert!(require_execution_capability_v1(&tx, true).is_ok());
    let NovTxKindV1::Transfer(transfer) = &mut tx.kind else {
        unreachable!()
    };
    transfer.to = vec![2; 31];
    assert!(require_execution_capability_v1(&tx, true).is_err());
    let NovTxKindV1::Transfer(transfer) = &mut tx.kind else {
        unreachable!()
    };
    transfer.to = vec![2; 20];
    transfer.fee_policy.pay_asset = "USDT".into();
    assert!(require_execution_capability_v1(&tx, true).is_err());
}
