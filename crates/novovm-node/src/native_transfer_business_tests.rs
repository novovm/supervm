use super::*;

fn fixture(max_fee: u128) -> OwnedItem {
    let mut transaction = NovNativeTxWireV1 {
        chain_id: 81_742,
        kind: NovTxKindV1::Transfer(novovm_protocol::NovTransferTxV1 {
            from: vec![1; 20],
            to: vec![2; 32],
            asset: "NOV".into(),
            amount: 10,
            nonce: 0,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "NOV".into(),
                max_pay_amount: max_fee,
                slippage_bps: 0,
            },
        }),
        signature: Vec::new(),
    };
    sign_nov_native_tx_with_seed_v1(&mut transaction, [0x76; 32]).unwrap();
    let ir = nov_native_tx_to_adapter_tx_ir_v1(&transaction).unwrap();
    let tx_hash = tx_hash_array_from_ir_v1(&ir);
    let request = fee_request_v1(&transaction, tx_hash).unwrap();
    let subject = fallback_execution_subject_meta_v1(&request);
    let reservation = nov_native_durable_auth_reservation_v1(&transaction, &ir, tx_hash).unwrap();
    OwnedItem::capture(&Item {
        transaction: &transaction,
        request: &request,
        subject: &subject,
        reservation: &reservation,
        ingress: NovAoemSemanticIngressMetaV1::default(),
    })
    .unwrap()
}

fn accounts(item: &OwnedItem) -> (String, String) {
    let NovTxKindV1::Transfer(tx) = &item.transaction.kind else {
        unreachable!()
    };
    (to_hex_prefixed_v1(&tx.from), to_hex_prefixed_v1(&tx.to))
}

fn funded(item: &OwnedItem) -> NovNativeExecutionStoreV1 {
    let mut store = NovNativeExecutionStoreV1::default();
    let (payer, recipient) = accounts(item);
    store.module_state.account_asset_balances.insert(
        payer,
        BTreeMap::from([("NOV".into(), 1_000), ("USDT".into(), 77)]),
    );
    store
        .module_state
        .account_asset_balances
        .insert(recipient, BTreeMap::from([("ETH".into(), 91)]));
    // An explicit state split prevents environment fallback from determining
    // the settlement switch or shares in these pure business tests.
    store.module_state.treasury_reserve_share_bps = 5_000;
    store.module_state.treasury_fee_share_bps = 3_000;
    store.module_state.treasury_risk_buffer_share_bps = 2_000;
    store.module_state.treasury_min_reserve_bucket_nov = 1;
    store.module_state.treasury_min_fee_bucket_nov = 1;
    store.module_state.treasury_min_risk_buffer_nov = 1;
    store
}

fn reduce_one(
    source: &NovNativeExecutionStoreV1,
    item: &OwnedItem,
    policy: NovTreasurySettlementPolicyV1,
    now_ms: u128,
) -> (Step, TransferIntent) {
    let quote = quote_v1(&item.request, now_ms).map_err(|error| error.to_string());
    let NovTxKindV1::Transfer(tx) = &item.transaction.kind else {
        unreachable!()
    };
    let intent = TransferIntent {
        tx_hash: item.request.tx_hash,
        from: Account::try_from(tx.from.as_slice()).unwrap(),
        to: Account::try_from(tx.to.as_slice()).unwrap(),
        nonce_identity: item.reservation.identity_key.clone(),
        nonce: tx.nonce,
        amount: tx.amount,
        approved_fee: estimate_execution_fee_nov_v1(&item.request),
        fee_cap: quote
            .as_ref()
            .map(|value| value.max_pay_amount)
            .unwrap_or(0),
    };
    let outcome = crate::native_transfer_delta::compute_outcome_v1(
        &intent,
        &snapshot_v1(source, &intent),
        quote.as_ref().err().map(String::as_str),
    )
    .unwrap();
    let captured = capture_store(source, std::slice::from_ref(item)).unwrap();
    let (mut batch, _) = reduce(
        captured,
        vec![item.clone()],
        vec![intent.clone()],
        vec![quote],
        policy,
        vec![outcome],
        vec![0],
        1,
        now_ms,
    )
    .unwrap();
    assert_eq!(batch.steps.len(), 1);
    assert_eq!(batch.recomputed, 0);
    (batch.steps.remove(0), intent)
}

fn history_receipt(item: &OwnedItem) -> NovNativeExecutionReceiptV1 {
    build_failed_native_receipt_v1(
        &item.request,
        &unresolved_settled_fee_v1(&item.request),
        &item.subject,
        "test".into(),
        "history".into(),
        "history sentinel".into(),
    )
}

#[test]
fn business_capture_is_bounded_to_declared_accounts_assets_nonces_and_receipts() {
    let item = fixture(0);
    let mut source = funded(&item);
    let (payer, recipient) = accounts(&item);
    for index in 0..300 {
        let key = format!("unrelated-{index}");
        source.module_state.account_asset_balances.insert(
            key.clone(),
            BTreeMap::from([("NOV".into(), 99), ("USDT".into(), 88)]),
        );
        source
            .module_state
            .native_auth_next_nonces
            .insert(key.clone(), 23);
        source
            .module_state
            .native_auth_nonce_reservations
            .insert(key.clone(), "other-reservation".into());
        source.receipts.insert(key, history_receipt(&item));
    }
    source
        .module_state
        .native_auth_next_nonces
        .insert(item.reservation.identity_key.clone(), 0);
    source.module_state.native_auth_nonce_reservations.insert(
        item.reservation.ledger_key.clone(),
        item.reservation.reservation_id.clone(),
    );
    source
        .receipts
        .insert(item.reservation.tx_hash.clone(), history_receipt(&item));
    source.module_state.treasury_reserves =
        BTreeMap::from([("NOV".into(), 43), ("USDT".into(), 89)]);
    source.module_state.treasury_settled_by_asset =
        BTreeMap::from([("NOV".into(), 43), ("USDT".into(), 89)]);
    source.module_state.treasury_settlement_failure_counts =
        BTreeMap::from([("policy_fallback".into(), 7), ("foreign-only".into(), 19)]);
    source
        .module_state
        .fee_oracle_rates_ppm
        .insert("USDT".into(), 123);
    source.module_state.execution_trace_order = vec!["history-trace".into()];
    source.module_state.aoem_semantic_ledger_sequence = 987;
    source.module_state.aoem_semantic_ledger_head = "history-head".into();
    source.last_updated_unix_ms = 456;
    let before = source.clone();
    let captured = capture_store(&source, std::slice::from_ref(&item)).unwrap();
    assert_eq!(source, before);
    assert_eq!(captured.module_state.account_asset_balances.len(), 2);
    assert_eq!(
        captured.module_state.account_asset_balances[&payer],
        BTreeMap::from([("NOV".into(), 1_000)])
    );
    assert!(captured.module_state.account_asset_balances[&recipient].is_empty());
    assert_eq!(captured.module_state.native_auth_next_nonces.len(), 1);
    assert_eq!(
        captured.module_state.native_auth_nonce_reservations.len(),
        1
    );
    assert_eq!(captured.receipts.len(), 1);
    assert!(captured.receipts.contains_key(&item.reservation.tx_hash));
    assert_eq!(
        captured.module_state.treasury_reserves,
        BTreeMap::from([("NOV".into(), 43)])
    );
    assert_eq!(
        captured.module_state.treasury_settled_by_asset,
        BTreeMap::from([("NOV".into(), 43)])
    );
    assert_eq!(
        captured.module_state.treasury_settlement_failure_counts,
        BTreeMap::from([("policy_fallback".into(), 7)])
    );
    assert!(captured.module_state.fee_oracle_rates_ppm.is_empty());
    assert!(captured.module_state.execution_trace_order.is_empty());
    assert!(captured.module_state.execution_traces_by_tx.is_empty());
    assert!(captured
        .module_state
        .aoem_semantic_ledger_records
        .is_empty());
    assert_eq!(captured.module_state.aoem_semantic_ledger_sequence, 0);
    assert!(captured.module_state.aoem_semantic_ledger_head.is_empty());
    assert_eq!(captured.last_updated_unix_ms, 0);
}

#[test]
fn business_capture_distinguishes_absent_account_absent_nov_and_present_zero() {
    let item = fixture(0);
    let (_, recipient) = accounts(&item);
    for expected in [None, Some(None), Some(Some(0))] {
        let mut source = funded(&item);
        source
            .module_state
            .account_asset_balances
            .remove(&recipient);
        if let Some(nov) = expected {
            let assets = source
                .module_state
                .account_asset_balances
                .entry(recipient.clone())
                .or_default();
            assets.insert("USDT".into(), 5);
            if let Some(value) = nov {
                assets.insert("NOV".into(), value);
            }
        }
        let captured = capture_store(&source, std::slice::from_ref(&item)).unwrap();
        assert_eq!(balance(&captured, &recipient), expected);
    }
}

#[test]
fn business_step_preserves_unrelated_map_keys_and_defers_nonce_to_original_finalizer() {
    let item = fixture(0);
    let mut source = funded(&item);
    let (payer, recipient) = accounts(&item);
    source
        .module_state
        .treasury_reserves
        .insert("USDT".into(), 19);
    source
        .module_state
        .treasury_settled_by_asset
        .insert("USDT".into(), 29);
    source
        .module_state
        .treasury_settlement_failure_counts
        .insert("foreign-only".into(), 17);
    source
        .module_state
        .native_auth_next_nonces
        .insert("other-identity".into(), 34);
    source
        .module_state
        .native_auth_nonce_reservations
        .insert("other-ledger-key".into(), "other-reservation".into());
    source
        .receipts
        .insert("other-receipt".into(), history_receipt(&item));
    source.module_state.aoem_semantic_ledger_sequence = 78;
    source.module_state.aoem_semantic_ledger_head = "unchanged-head".into();
    source.last_updated_unix_ms = 990;
    let before = source.clone();
    let policy = resolve_treasury_settlement_policy_v1(&source);
    let (step, intent) = reduce_one(&source, &item, policy, 123);
    assert_eq!(source, before, "business input reduction is detached");
    assert!(step.receipt.status);
    assert_eq!(step.nonce_after, 1);
    step.apply(&mut source, &intent).unwrap();
    assert_eq!(
        source.module_state.account_asset_balances[&payer]["NOV"],
        1_000 - 10 - step.fee.nov_amount
    );
    assert_eq!(
        source.module_state.account_asset_balances[&recipient]["NOV"],
        10
    );
    assert_eq!(
        source.module_state.account_asset_balances[&payer]["USDT"],
        77
    );
    assert_eq!(
        source.module_state.account_asset_balances[&recipient]["ETH"],
        91
    );
    assert_eq!(
        source.module_state.treasury_reserves["NOV"],
        step.fee.nov_amount
    );
    assert_eq!(source.module_state.treasury_reserves["USDT"], 19);
    assert_eq!(source.module_state.treasury_settled_by_asset["USDT"], 29);
    assert_eq!(
        source.module_state.treasury_settlement_failure_counts["foreign-only"],
        17
    );
    assert_eq!(
        source.module_state.native_auth_next_nonces,
        before.module_state.native_auth_next_nonces
    );
    assert_eq!(
        source.module_state.native_auth_nonce_reservations,
        before.module_state.native_auth_nonce_reservations
    );
    assert_eq!(source.receipts, before.receipts);
    assert_eq!(source.module_state.aoem_semantic_ledger_sequence, 78);
    assert_eq!(
        source.module_state.aoem_semantic_ledger_head,
        "unchanged-head"
    );
    assert_eq!(source.last_updated_unix_ms, 990);
    commit_nov_native_durable_auth_reservation_v1(&mut source, &item.reservation).unwrap();
    assert_eq!(
        source.module_state.native_auth_next_nonces[&item.reservation.identity_key],
        1
    );
}

#[test]
fn business_step_rejects_stale_balance_or_nonce_before_applying_fee_changes() {
    let item = fixture(0);
    let source = funded(&item);
    let policy = resolve_treasury_settlement_policy_v1(&source);
    let (step, intent) = reduce_one(&source, &item, policy, 123);
    for change_nonce in [false, true] {
        let mut stale = source.clone();
        if change_nonce {
            stale
                .module_state
                .native_auth_next_nonces
                .insert(intent.nonce_identity.clone(), 1);
        } else {
            stale
                .module_state
                .account_asset_balances
                .get_mut(&intent.from.to_hex_prefixed())
                .unwrap()
                .insert("NOV".into(), 999);
        }
        let before = stale.clone();
        let error = step.apply(&mut stale, &intent).unwrap_err().to_string();
        assert!(error.contains("ordered candidate prefix"), "{error}");
        assert_eq!(stale, before);
    }
}

#[test]
fn business_step_rejects_stale_fee_or_journal_before_any_patch_mutation() {
    let item = fixture(0);
    let source = funded(&item);
    let policy = resolve_treasury_settlement_policy_v1(&source);
    let (step, intent) = reduce_one(&source, &item, policy, 123);
    let mut published = source.clone();
    step.apply(&mut published, &intent).unwrap();
    let entry = published.module_state.treasury_settlement_journal[0].clone();
    for changed_field in 0..4 {
        let mut stale = source.clone();
        match changed_field {
            0 => stale.module_state.treasury_settled_nov_total = 5,
            1 => {
                stale.module_state.treasury_reserves.insert("NOV".into(), 0);
            }
            2 => stale.module_state.treasury_settlement_journal_next_seq = 8,
            _ => stale
                .module_state
                .treasury_settlement_journal
                .push(entry.clone()),
        }
        let before = stale.clone();
        let error = step.apply(&mut stale, &intent).unwrap_err().to_string();
        assert!(error.contains("ordered candidate prefix"), "{error}");
        assert_eq!(stale, before, "no earlier fee/account patch may leak");
    }
}

#[test]
fn business_fee_rejection_keeps_daily_window_and_fallback_diagnostics_without_payment() {
    let item = fixture(0);
    let mut source = funded(&item);
    source.module_state.clearing_daily_window_day = 0;
    source.module_state.clearing_daily_nov_used = 37;
    let mut policy = resolve_treasury_settlement_policy_v1(&source);
    policy.source = "default_fallback_invalid_env".into();
    policy.settlement_paused = true;
    let before_balances = source.module_state.account_asset_balances.clone();
    let (step, intent) = reduce_one(&source, &item, policy, 86_400_007);
    assert!(!step.receipt.status);
    assert_eq!(step.fee.nov_amount, 0);
    assert_eq!(step.nonce_after, 1);
    step.apply(&mut source, &intent).unwrap();
    assert_eq!(source.module_state.account_asset_balances, before_balances);
    assert!(source.module_state.treasury_reserves.is_empty());
    assert_eq!(source.module_state.treasury_settlements, 0);
    assert_eq!(source.module_state.clearing_daily_window_day, 1);
    assert_eq!(source.module_state.clearing_daily_nov_used, 0);
    assert_eq!(
        source.module_state.treasury_settlement_failure_counts["policy_fallback"],
        1
    );
    assert_eq!(
        source.module_state.treasury_settlement_failure_counts["settlement_paused"],
        1
    );
    assert!(source.module_state.last_fee_quote.is_some());
    assert!(source.module_state.last_fee_quote_failure.is_none());
    assert!(source.module_state.native_auth_next_nonces.is_empty());
}

#[test]
fn business_quote_rejection_preserves_previous_quote_and_does_not_run_settlement() {
    let item = fixture(1);
    let mut source = funded(&item);
    let earlier = fixture(0);
    source.module_state.last_fee_quote = Some(quote_v1(&earlier.request, 7).unwrap());
    source.module_state.clearing_daily_nov_used = 39;
    let prior_quote = source.module_state.last_fee_quote.clone();
    let mut policy = resolve_treasury_settlement_policy_v1(&source);
    policy.source = "default_fallback_invalid_env".into();
    policy.settlement_paused = true;
    let before_balances = source.module_state.account_asset_balances.clone();
    let (step, intent) = reduce_one(&source, &item, policy, 86_400_007);
    assert!(!step.receipt.status);
    assert_eq!(step.nonce_after, 1);
    step.apply(&mut source, &intent).unwrap();
    assert_eq!(source.module_state.last_fee_quote, prior_quote);
    assert!(source
        .module_state
        .last_fee_quote_failure
        .as_deref()
        .unwrap()
        .contains("max_pay_exceeded"));
    assert_eq!(
        source.module_state.fee_quote_failure_counts["NOV:max_pay_exceeded"],
        1
    );
    assert!(source
        .module_state
        .treasury_settlement_failure_counts
        .is_empty());
    assert_eq!(source.module_state.clearing_daily_window_day, 0);
    assert_eq!(source.module_state.clearing_daily_nov_used, 39);
    assert_eq!(source.module_state.account_asset_balances, before_balances);
    assert!(source.module_state.treasury_reserves.is_empty());
}

#[test]
fn business_capture_rejects_an_oversized_fee_journal_without_mutating_source() {
    let item = fixture(0);
    let mut source = funded(&item);
    let policy = resolve_treasury_settlement_policy_v1(&source);
    let (step, intent) = reduce_one(&source, &item, policy, 123);
    step.apply(&mut source, &intent).unwrap();
    let entry = source.module_state.treasury_settlement_journal[0].clone();
    source.module_state.treasury_settlement_journal =
        vec![entry.clone(); NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1];
    assert!(capture_store(&source, std::slice::from_ref(&item)).is_ok());
    source.module_state.treasury_settlement_journal.push(entry);
    let before = source.clone();
    let error = capture_store(&source, std::slice::from_ref(&item))
        .unwrap_err()
        .to_string();
    assert!(error.contains("journal exceeds fixed bound"), "{error}");
    assert_eq!(source, before);
}
