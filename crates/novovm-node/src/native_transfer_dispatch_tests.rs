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

struct FinalizerFixture {
    transaction: NovNativeTxWireV1,
    request: NovExecutionRequestV1,
    subject: NovExecutionSubjectMetaV1,
    reservation: NovNativeDurableAuthReservationV1,
}

impl FinalizerFixture {
    fn new(nonce: u64, amount: u128, max_fee: u128) -> Self {
        let mut transaction = transfer();
        let NovTxKindV1::Transfer(transfer) = &mut transaction.kind else {
            unreachable!();
        };
        transfer.nonce = nonce;
        transfer.amount = amount;
        transfer.fee_policy.max_pay_amount = max_fee;
        sign_nov_native_tx_with_seed_v1(&mut transaction, [0xb3; 32]).unwrap();
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&transaction).unwrap();
        let tx_hash = tx_hash_array_from_ir_v1(&ir);
        let request = fee_request_v1(&transaction, tx_hash).unwrap();
        let subject = fallback_execution_subject_meta_v1(&request);
        let reservation =
            nov_native_durable_auth_reservation_v1(&transaction, &ir, tx_hash).unwrap();
        Self {
            transaction,
            request,
            subject,
            reservation,
        }
    }

    fn item(&self) -> Item<'_> {
        Item {
            transaction: &self.transaction,
            request: &self.request,
            subject: &self.subject,
            reservation: &self.reservation,
            ingress: NovAoemSemanticIngressMetaV1 {
                execution_kernel: "AOEM".into(),
                semantic_entry: "test.transfer.finalizer".into(),
                plan_id: 17,
                wire_digest: "bound-test-wire".into(),
                ..Default::default()
            },
        }
    }

    fn funded_store(&self, balance: u128) -> NovNativeExecutionStoreV1 {
        let mut store = NovNativeExecutionStoreV1::default();
        store.module_state.account_asset_balances.insert(
            self.subject.account_id.clone(),
            BTreeMap::from([("NOV".into(), balance), ("USDT".into(), 91)]),
        );
        store
    }
}

#[test]
fn legacy_finalizer_preserves_direct_receipt_and_store_bytes() {
    let fixture = FinalizerFixture::new(0, 10, 0);
    let mut store = fixture.funded_store(1_000);
    let before = store.module_state.clone();
    let mut mirrors = Vec::new();
    let mut finalizer = LegacyFinalizer {
        before: None,
        mirrors: &mut mirrors,
    };
    finalizer.begin(&store).unwrap();
    assert!(finalizer.begin(&store).is_err());
    let fee = settle_fee_policy_from_execution_request_v1(
        &fixture.request,
        &fixture.subject,
        &mut store,
        123,
    )
    .unwrap();
    let receipt = build_failed_native_receipt_v1(
        &fixture.request,
        &fee,
        &fixture.subject,
        "native_asset".into(),
        "transfer".into(),
        "test business failure".into(),
    );
    let mut expected = store.clone();
    let mut expected_mirrors = Vec::new();
    finalize_native_execution_receipt_v1(
        &mut expected,
        &fixture.request,
        &fee,
        &fixture.subject,
        Some(&fixture.reservation),
        Some(fixture.item().ingress),
        &before,
        Path::new(""),
        Some(&mut expected_mirrors),
        123,
        receipt.clone(),
    )
    .unwrap();
    finalizer
        .finish(
            &mut store,
            &fixture.request,
            &fee,
            &fixture.subject,
            &fixture.reservation,
            fixture.item().ingress,
            123,
            receipt.clone(),
        )
        .unwrap();
    assert!(finalizer
        .finish(
            &mut store,
            &fixture.request,
            &fee,
            &fixture.subject,
            &fixture.reservation,
            fixture.item().ingress,
            123,
            receipt,
        )
        .is_err());
    assert_eq!(
        serde_json::to_vec(&store).unwrap(),
        serde_json::to_vec(&expected).unwrap()
    );
    assert_eq!(
        serde_json::to_vec(&mirrors).unwrap(),
        serde_json::to_vec(&expected_mirrors).unwrap()
    );
}

#[derive(Default)]
struct RecordingFinalizer {
    events: Vec<&'static str>,
    next_nonce_at_begin: Vec<u64>,
    nonce_identity: String,
    receipt_count_before: usize,
    abort_begin: bool,
    abort_finish: bool,
}

impl TransferReceiptFinalizerV1 for RecordingFinalizer {
    fn begin(&mut self, store: &NovNativeExecutionStoreV1) -> Result<()> {
        assert_eq!(self.events.len() % 2, 0, "previous item must finish first");
        self.events.push("begin");
        self.receipt_count_before = store.receipts.len();
        self.next_nonce_at_begin.push(
            store
                .module_state
                .native_auth_next_nonces
                .get(&self.nonce_identity)
                .copied()
                .unwrap_or(0),
        );
        if self.abort_begin {
            bail!("test finalizer begin failure");
        }
        Ok(())
    }

    fn finish(
        &mut self,
        store: &mut NovNativeExecutionStoreV1,
        request: &NovExecutionRequestV1,
        settled_fee: &NovSettledFeeV1,
        subject: &NovExecutionSubjectMetaV1,
        reservation: &NovNativeDurableAuthReservationV1,
        ingress: NovAoemSemanticIngressMetaV1,
        now_ms: u128,
        mut receipt: NovNativeExecutionReceiptV1,
    ) -> Result<()> {
        assert_eq!(self.events.len() % 2, 1);
        self.events.push("finish");
        assert_eq!(store.receipts.len(), self.receipt_count_before);
        assert_eq!(receipt.tx_hash, reservation.tx_hash);
        assert_eq!(to_hex(&request.tx_hash), receipt.tx_hash);
        assert_eq!(receipt.account_id, subject.account_id);
        assert_eq!(receipt.settled_fee_nov, settled_fee.nov_amount);
        assert!(receipt
            .logs
            .iter()
            .any(|log| log.event == "aoem.native_transfer.computed"));
        // The generic dispatcher must not invoke the legacy finalizer first.
        assert_eq!(store.module_state.aoem_semantic_ledger_sequence, 0);
        assert!(store.module_state.execution_traces_by_tx.is_empty());
        assert!(receipt.aoem_semantic_ingress.is_none());
        if self.abort_finish {
            bail!("test finalizer finish failure");
        }
        commit_nov_native_durable_auth_reservation_v1(store, reservation)?;
        receipt.aoem_semantic_ingress = Some(ingress);
        store.receipts.insert(receipt.tx_hash.clone(), receipt);
        store.last_updated_unix_ms = now_ms;
        Ok(())
    }
}

#[test]
#[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
fn real_aoem_custom_finalizer_receives_success_and_every_failure_in_order() {
    for case in [
        "success", "business", "quote", "balance", "paused", "overflow",
    ] {
        let first = FinalizerFixture::new(
            0,
            if case == "business" { u128::MAX } else { 10 },
            if case == "quote" { 1 } else { 0 },
        );
        let second = FinalizerFixture::new(1, 1, 0);
        let mut store = first.funded_store(if case == "balance" { 0 } else { 1_000 });
        if case == "paused" {
            store.module_state.treasury_settlement_paused = true;
        }
        if case == "overflow" {
            store.module_state.treasury_settlements = u64::MAX;
        }
        let mut finalizer = RecordingFinalizer {
            nonce_identity: first.reservation.identity_key.clone(),
            ..Default::default()
        };
        let peak = execute_with_finalizer_v1(
            &mut store,
            &[first.item(), second.item()],
            123,
            &mut finalizer,
        )
        .unwrap();
        assert!(peak >= 1, "actual AOEM callbacks must execute");
        assert_eq!(finalizer.events, ["begin", "finish", "begin", "finish"]);
        assert_eq!(finalizer.next_nonce_at_begin, [0, 1]);
        let receipt = &store.receipts[&first.reservation.tx_hash];
        assert_eq!(receipt.status, case == "success", "{case}");
        let charged = matches!(case, "success" | "business");
        assert_eq!(receipt.settled_fee_nov > 0, charged, "{case}");
        match case {
            "business" => assert!(receipt
                .failure_reason
                .as_ref()
                .unwrap()
                .starts_with("native.transfer.")),
            "quote" => assert!(receipt
                .failure_reason
                .as_ref()
                .unwrap()
                .starts_with("fee.quote.")),
            "balance" | "paused" | "overflow" => {
                assert!(receipt.failure_reason.as_ref().unwrap().starts_with("fee."))
            }
            _ => {}
        }
        assert_eq!(
            store.module_state.native_auth_next_nonces[&first.reservation.identity_key],
            2
        );
        assert_eq!(store.module_state.native_auth_nonce_reservations.len(), 2);
        assert_eq!(
            store.module_state.account_asset_balances[&first.subject.account_id]["USDT"],
            91
        );
        assert_eq!(store.receipts.len(), 2);
        eprintln!("custom transfer finalizer case={case} callback_peak={peak}");
    }
}

#[test]
#[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
fn real_aoem_custom_finalizer_error_stops_before_next_transaction() {
    let first = FinalizerFixture::new(0, 10, 0);
    let second = FinalizerFixture::new(1, 10, 0);
    for abort_begin in [true, false] {
        let mut store = first.funded_store(1_000);
        let original = store.clone();
        let mut finalizer = RecordingFinalizer {
            nonce_identity: first.reservation.identity_key.clone(),
            abort_begin,
            abort_finish: !abort_begin,
            ..Default::default()
        };
        let error = execute_with_finalizer_v1(
            &mut store,
            &[first.item(), second.item()],
            123,
            &mut finalizer,
        )
        .unwrap_err();
        if abort_begin {
            assert!(error.to_string().contains("begin failure"));
            assert_eq!(finalizer.events, ["begin"]);
            assert_eq!(
                store, original,
                "begin error must precede every fee mutation"
            );
        } else {
            assert!(error.to_string().contains("finish failure"));
            assert_eq!(finalizer.events, ["begin", "finish"]);
            assert!(
                native_account_asset_balance_v1(&store, &first.subject.account_id, "NOV") < 1_000
            );
        }
        assert!(store.receipts.is_empty());
        assert!(store.module_state.native_auth_next_nonces.is_empty());
        assert!(store.module_state.native_auth_nonce_reservations.is_empty());
    }
}
