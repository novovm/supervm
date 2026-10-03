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
            &fixture.transaction,
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
            &fixture.transaction,
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
        _transaction: &NovNativeTxWireV1,
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

fn component_fixture(seed: u8, recipient: u8, nonce: u64, amount: u128) -> FinalizerFixture {
    let mut transaction = transfer();
    let NovTxKindV1::Transfer(transfer) = &mut transaction.kind else {
        unreachable!()
    };
    transfer.to = novovm_adapter_novovm::address_from_seed_v1([recipient; 32]);
    transfer.nonce = nonce;
    transfer.amount = amount;
    sign_nov_native_tx_with_seed_v1(&mut transaction, [seed; 32]).unwrap();
    let ir = nov_native_tx_to_adapter_tx_ir_v1(&transaction).unwrap();
    let tx_hash = tx_hash_array_from_ir_v1(&ir);
    let request = fee_request_v1(&transaction, tx_hash).unwrap();
    let subject = fallback_execution_subject_meta_v1(&request);
    let reservation = nov_native_durable_auth_reservation_v1(&transaction, &ir, tx_hash).unwrap();
    FinalizerFixture {
        transaction,
        request,
        subject,
        reservation,
    }
}

#[test]
#[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
fn real_aoem_typed_record_effects_match_fullencode_batch_and_each_serial_prefix() {
    use super::super::native_transfer_record_execution as record;
    for case in [
        "success",
        "business",
        "quote",
        "balance",
        "paused",
        "overflow",
        "recipient_overflow",
        "self",
        "zero",
        "day_window",
        "capacity_samepayer",
        "capacity_shared_credit",
    ] {
        let first = if case == "quote" {
            FinalizerFixture::new(0, 10, 1)
        } else {
            component_fixture(
                81,
                if case == "self" { 81 } else { 90 },
                0,
                match case {
                    "business" => u128::MAX,
                    "zero" => 0,
                    "capacity_samepayer" | "capacity_shared_credit" => 1_000_000_000_000,
                    _ => 10,
                },
            )
        };
        let second = if case == "quote" {
            FinalizerFixture::new(1, 1, 0)
        } else {
            component_fixture(81, 90, 1, 1)
        };
        let mut fixtures = [first, component_fixture(82, 90, 0, 3), second];
        if case == "capacity_samepayer" {
            fixtures.swap(1, 2);
        }
        let mut initial = NovNativeExecutionStoreV1::default();
        for fixture in &fixtures {
            let balance = if case == "balance" {
                0
            } else if case.starts_with("capacity_")
                && fixture.subject.account_id == fixtures[0].subject.account_id
            {
                1_000_000_001_000
            } else {
                1_000
            };
            initial
                .module_state
                .account_asset_balances
                .entry(fixture.subject.account_id.clone())
                .or_insert_with(|| {
                    BTreeMap::from([("NOV".into(), balance), ("USDT".into(), u128::MAX)])
                });
        }
        match case {
            "paused" => initial.module_state.treasury_settlement_paused = true,
            "overflow" => initial.module_state.treasury_settlements = u64::MAX - 1,
            "recipient_overflow" => {
                initial.module_state.account_asset_balances.insert(
                    to_hex_prefixed_v1(&novovm_adapter_novovm::address_from_seed_v1([90; 32])),
                    BTreeMap::from([("NOV".into(), u128::MAX)]),
                );
            }
            "day_window" => {
                initial.module_state.clearing_daily_window_day = 0;
                initial.module_state.clearing_daily_nov_used = 900;
            }
            "capacity_samepayer" | "capacity_shared_credit" => {
                let available = estimate_execution_fee_nov_v1(&fixtures[1].request);
                assert!(estimate_execution_fee_nov_v1(&fixtures[0].request) > available);
                // Real policy capacity rejects the larger fee but permits the
                // next smaller one. Only the record codec supports this u128
                // boundary; do not push it through the old JSON state codec.
                initial.module_state.treasury_settled_nov_total = u128::MAX - available;
            }
            _ => {}
        }
        let now = 86_400_123;
        let items = fixtures
            .iter()
            .map(FinalizerFixture::item)
            .collect::<Vec<_>>();
        let mut actual = initial.clone();
        let mut old_batch = initial.clone();
        take_component_observation_for_test_v1();
        record::execute_segment_v1(&mut actual, &items, now).unwrap();
        let observation = take_component_observation_for_test_v1().unwrap();
        assert_eq!(observation.graphs, 1, "{case}: only one actual AOEM graph");
        assert_eq!(observation.transactions, fixtures.len());
        record::execute_segment_fullencode_oracle_for_test_v1(&mut old_batch, &items, now).unwrap();
        assert_eq!(
            actual, old_batch,
            "{case}: complete batch, receipts and mirrors"
        );
        let mut typed_prefix = initial.clone();
        let mut old_prefix = initial;
        for (index, item) in items.iter().enumerate() {
            record::execute_segment_v1(&mut typed_prefix, std::slice::from_ref(item), now).unwrap();
            record::execute_segment_fullencode_oracle_for_test_v1(
                &mut old_prefix,
                std::slice::from_ref(item),
                now,
            )
            .unwrap();
            assert_eq!(typed_prefix, old_prefix, "{case}: full prefix {index}");
        }
        assert_eq!(actual, old_prefix, "{case}: batch versus old serial prefix");
        if case.starts_with("capacity_") {
            assert!(observation.recomputed_transactions > 0);
            let NovTxKindV1::Transfer(accepted) = &fixtures[1].transaction.kind else {
                unreachable!();
            };
            assert_eq!(
                native_account_asset_balance_v1(&actual, &to_hex_prefixed_v1(&accepted.to), "NOV"),
                accepted.amount,
                "rejected speculative trillion-unit credit must not survive reduction"
            );
            let fee = estimate_execution_fee_nov_v1(&fixtures[1].request);
            for (index, fixture) in fixtures.iter().enumerate() {
                let receipt = &actual.receipts[&fixture.reservation.tx_hash];
                assert_eq!(receipt.status, index == 1);
                assert_eq!(receipt.settled_fee_nov, if index == 1 { fee } else { 0 });
                assert_eq!(receipt.paid_amount, receipt.settled_fee_nov);
                if index != 1 {
                    assert!(receipt
                        .failure_reason
                        .as_deref()
                        .unwrap()
                        .starts_with("fee.settlement.amount_overflow"));
                }
            }
            let first_identity = &fixtures[0].reservation.identity_key;
            let other_identity = &fixtures
                .iter()
                .find(|fixture| fixture.reservation.identity_key != *first_identity)
                .unwrap()
                .reservation
                .identity_key;
            assert_eq!(
                actual.module_state.native_auth_next_nonces[first_identity],
                2
            );
            assert_eq!(
                actual.module_state.native_auth_next_nonces[other_identity],
                1
            );
            assert_eq!(
                actual.module_state.native_auth_nonce_reservations.len(),
                fixtures.len()
            );
            assert_eq!(actual.module_state.treasury_settlements, 1);
            assert_eq!(actual.module_state.treasury_settled_nov_total, u128::MAX);
            let remaining: u128 = actual
                .module_state
                .account_asset_balances
                .values()
                .map(|assets| assets.get("NOV").copied().unwrap_or(0))
                .sum();
            assert_eq!(
                remaining + fee,
                1_000_000_002_000,
                "the only charged fee must equal the total account debit"
            );
        }
        eprintln!("typed transfer record effects fullencode parity case={case} transactions=3");
    }
}

#[test]
fn component_prestate_validation_checks_both_balances_and_nonce_without_weakening_binding() {
    use crate::native_transfer_delta::compute_outcome_v1;
    let intent = TransferIntent {
        tx_hash: [1; 32],
        from: [1; 20].into(),
        to: [2; 32].into(),
        nonce_identity: "exact-signer-domain".into(),
        nonce: 0,
        amount: 5,
        approved_fee: 2,
        fee_cap: 2,
    };
    let snapshot = TransferSnapshot {
        payer_balance: 100,
        recipient_balance: 10,
        next_nonce: 0,
    };
    let outcome = compute_outcome_v1(&intent, &snapshot, None).unwrap();
    assert!(outcome_binding_v1(&outcome, &intent).is_ok());
    assert!(outcome_matches_snapshot_v1(&outcome, snapshot));
    for altered in [
        TransferSnapshot {
            payer_balance: 101,
            ..snapshot
        },
        TransferSnapshot {
            recipient_balance: 11,
            ..snapshot
        },
        TransferSnapshot {
            next_nonce: 1,
            ..snapshot
        },
    ] {
        assert!(!outcome_matches_snapshot_v1(&outcome, altered));
    }
    let mut other = intent.clone();
    other.from = [1; 32].into();
    assert!(outcome_binding_v1(&outcome, &other).is_err());
    other = intent;
    other.tx_hash = [2; 32];
    assert!(outcome_binding_v1(&outcome, &other).is_err());
}

#[test]
#[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
fn real_aoem_components_preserve_serial_receipts_and_repair_global_fee_rejections() {
    for case in ["success", "business", "paused", "overflow"] {
        // The late B->C bridge must join both A/B and C/D chains. E/F remains
        // independent although its transactions are interleaved with them.
        let fixtures = [
            component_fixture(71, 72, 0, 100),
            component_fixture(71, 72, 1, 10),
            component_fixture(75, 76, 0, 40),
            component_fixture(73, 74, 0, 50),
            component_fixture(72, 73, 0, if case == "business" { 1_000_000 } else { 30 }),
            component_fixture(73, 74, 1, 15),
            component_fixture(75, 76, 1, 5),
        ];
        let mut initial = NovNativeExecutionStoreV1::default();
        for index in [0, 2, 3] {
            initial.module_state.account_asset_balances.insert(
                fixtures[index].subject.account_id.clone(),
                BTreeMap::from([("NOV".into(), 1_000)]),
            );
        }
        if case == "paused" {
            initial.module_state.treasury_settlement_paused = true;
        }
        if case == "overflow" {
            initial.module_state.treasury_settlements = u64::MAX - 1;
        }
        let mut actual = initial.clone();
        let mut actual_mirrors = Vec::new();
        take_component_observation_for_test_v1();
        execute_v1(
            &mut actual,
            &fixtures
                .iter()
                .map(FinalizerFixture::item)
                .collect::<Vec<_>>(),
            123,
            &mut actual_mirrors,
        )
        .unwrap();
        let observation = take_component_observation_for_test_v1().unwrap();
        assert_eq!(observation.transactions, fixtures.len());
        assert_eq!(observation.components, 2);
        assert!(observation.peak_inflight >= 1);
        assert!(observation.recomputed_transactions <= fixtures.len());
        assert_eq!(
            observation.graphs, 1,
            "fee repair must stay in the admitted graph"
        );
        if matches!(case, "success" | "business") {
            assert_eq!(observation.recomputed_transactions, 0);
            assert_eq!(observation.graphs, 1);
        } else {
            assert!(
                observation.recomputed_transactions > 0,
                "must exercise genuine global-fee correction"
            );
        }
        let mut expected = initial;
        let mut expected_mirrors = Vec::new();
        for fixture in &fixtures {
            execute_v1(&mut expected, &[fixture.item()], 123, &mut expected_mirrors).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&actual).unwrap(),
            serde_json::to_vec(&expected).unwrap(),
            "{case}: every balance, fee, nonce, receipt, root and compute digest"
        );
        assert_eq!(
            serde_json::to_vec(&actual_mirrors).unwrap(),
            serde_json::to_vec(&expected_mirrors).unwrap(),
            "{case}: ordered semantic mirror bytes"
        );
        assert_eq!(actual.receipts.len(), fixtures.len());
        if case == "success" {
            assert!(actual.receipts.values().all(|receipt| receipt.status));
        }
        if case == "business" {
            let failure = &actual.receipts[&fixtures[4].reservation.tx_hash];
            assert!(!failure.status);
            assert!(failure.settled_fee_nov > 0);
            assert_eq!(
                actual.module_state.native_auth_next_nonces[&fixtures[4].reservation.identity_key],
                1
            );
        }
        eprintln!(
            "native transfer component serial parity case={case} observation={observation:?}"
        );
    }
}

#[test]
#[ignore = "requires the packaged AOEM runtime; run explicitly for integration evidence"]
fn real_aoem_checked_credit_preserves_serial_store_and_global_fee_repairs() {
    for case in ["success", "business", "paused", "overflow", "zero"] {
        let fixtures = [
            component_fixture(81, 90, 0, if case == "zero" { 0 } else { 30 }),
            component_fixture(82, 90, 0, if case == "business" { 1_000_000 } else { 40 }),
            component_fixture(83, 90, 0, 5),
            component_fixture(81, 90, 1, 7),
            component_fixture(82, 90, 1, 9),
        ];
        let mut initial = NovNativeExecutionStoreV1::default();
        for fixture in &fixtures[..3] {
            initial.module_state.account_asset_balances.insert(
                fixture.subject.account_id.clone(),
                BTreeMap::from([("NOV".into(), 1_000)]),
            );
        }
        if case == "paused" {
            initial.module_state.treasury_settlement_paused = true;
        }
        if case == "overflow" {
            initial.module_state.treasury_settlements = u64::MAX;
        }
        let mut actual = initial.clone();
        let mut mirrors = Vec::new();
        take_component_observation_for_test_v1();
        execute_v1(
            &mut actual,
            &fixtures
                .iter()
                .map(FinalizerFixture::item)
                .collect::<Vec<_>>(),
            123,
            &mut mirrors,
        )
        .unwrap();
        let observation = take_component_observation_for_test_v1().unwrap();
        assert_eq!(observation.components, 3);
        assert_eq!(
            observation.graphs, 1,
            "shared-credit repair must stay in the admitted graph"
        );
        if matches!(case, "paused" | "overflow") {
            // The first rejected credit also invalidates predictions in OTHER
            // components, not only the rejected payer's later nonce.
            assert!(observation.recomputed_transactions >= 2);
        } else {
            assert_eq!(observation.recomputed_transactions, 0);
        }
        let mut expected = initial;
        let mut expected_mirrors = Vec::new();
        for fixture in &fixtures {
            execute_v1(&mut expected, &[fixture.item()], 123, &mut expected_mirrors).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&actual).unwrap(),
            serde_json::to_vec(&expected).unwrap(),
            "{case}: full Store, receipts, nonce, fee allocation, roots and compute digests"
        );
        assert_eq!(
            serde_json::to_vec(&mirrors).unwrap(),
            serde_json::to_vec(&expected_mirrors).unwrap(),
            "{case}: exact original-order semantic mirrors"
        );
        eprintln!("checked credit full-store parity case={case} observation={observation:?}");
    }
}
