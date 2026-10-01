//! Typed record projection versus the independent complete physical encoder.
//! Arithmetic/fee/nonce/trace helpers below are real product functions, but no
//! AOEM execution, valid block seal, or throughput evidence is claimed here.
use super::*;
use crate::tx_ingress::native_store_records::{
    self as physical, NativeRecordAccessV1, RawPathChangeV1,
};
use crate::tx_ingress::native_transfer_state_access::TransferAccessV1;
use std::collections::{BTreeMap, BTreeSet};

type RawRecords = BTreeMap<Vec<String>, Vec<u8>>;

fn raw_records(store: &NovNativeExecutionStoreV1) -> RawRecords {
    physical::encode(store)
        .unwrap()
        .into_iter()
        .map(|(key, value)| {
            let (path, raw) = physical::unpack(&key, &value).unwrap();
            (
                path,
                if raw == physical::OBJECT {
                    b"{}".to_vec()
                } else {
                    raw.to_vec()
                },
            )
        })
        .collect()
}

struct Reader(RawRecords);
impl NativeRecordAccessV1 for Reader {
    fn read_path(&self, path: &[&str]) -> Result<Option<Vec<u8>>> {
        Ok(self
            .0
            .get(
                &path
                    .iter()
                    .map(|part| (*part).to_owned())
                    .collect::<Vec<_>>(),
            )
            .cloned())
    }
}

fn full_delta(
    before: &NovNativeExecutionStoreV1,
    after: &NovNativeExecutionStoreV1,
) -> Vec<RawPathChangeV1> {
    let before = raw_records(before);
    let after = raw_records(after);
    before
        .keys()
        .chain(after.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|path| {
            if before.get(path) == after.get(path) {
                return None;
            }
            Some(match after.get(path) {
                Some(value) => RawPathChangeV1::Put {
                    path: path.clone(),
                    value: value.clone(),
                },
                None => RawPathChangeV1::Delete { path: path.clone() },
            })
        })
        .collect()
}

fn merged(
    before: &NovNativeExecutionStoreV1,
    changes: &[RawPathChangeV1],
) -> NovNativeExecutionStoreV1 {
    let mut records = physical::encode(before).unwrap();
    for change in changes {
        match change {
            RawPathChangeV1::Put { path, value } => {
                records.insert(
                    physical::key(path).unwrap(),
                    physical::value(
                        path,
                        &physical::physical_path_value_v1(path, value).unwrap(),
                    )
                    .unwrap(),
                );
            }
            RawPathChangeV1::Delete { path } => {
                records.remove(&physical::key(path).unwrap());
            }
        }
    }
    physical::decode(records).unwrap()
}

struct Fixture {
    wire: NovNativeTxWireV1,
    request: NovExecutionRequestV1,
    subject: NovExecutionSubjectMetaV1,
    reservation: NovNativeDurableAuthReservationV1,
}

impl Fixture {
    fn new(
        nonce: u64,
        amount: u128,
        max_fee: u128,
        self_transfer: bool,
        long_account: bool,
    ) -> Self {
        let seed = [0x51; 32];
        let from = if long_account {
            ed25519_dalek::SigningKey::from_bytes(&seed)
                .verifying_key()
                .to_bytes()
                .to_vec()
        } else {
            novovm_adapter_novovm::address_from_seed_v1(seed)
        };
        let mut wire = NovNativeTxWireV1 {
            chain_id: 81742,
            kind: NovTxKindV1::Transfer(novovm_protocol::NovTransferTxV1 {
                from: from.clone(),
                to: if self_transfer { from } else { vec![2; 32] },
                asset: "NOV".into(),
                amount,
                nonce,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "NOV".into(),
                    max_pay_amount: max_fee,
                    slippage_bps: 0,
                },
            }),
            signature: Vec::new(),
        };
        // Sign the exact 20/32-byte intent; the convenience signer rewrites it.
        let unsigned = nov_native_tx_to_adapter_tx_ir_v1(&wire).unwrap();
        wire.signature = novovm_adapter_novovm::signature_payload_with_seed_v1(&unsigned, seed);
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&wire).unwrap();
        let hash = tx_hash_array_from_ir_v1(&ir);
        let reservation = nov_native_durable_auth_reservation_v1(&wire, &ir, hash).unwrap();
        let request = native_transfer_dispatch::fee_request_v1(&wire, hash).unwrap();
        let subject = fallback_execution_subject_meta_v1(&request);
        Self {
            wire,
            request,
            subject,
            reservation,
        }
    }

    fn store(&self) -> NovNativeExecutionStoreV1 {
        let mut store = NovNativeExecutionStoreV1 {
            authority_chain_id: Some(self.wire.chain_id),
            ..Default::default()
        };
        store.module_state.account_asset_balances.insert(
            self.subject.account_id.clone(),
            BTreeMap::from([("NOV".into(), 10000), ("USDT".into(), u128::MAX)]),
        );
        store.module_state.account_asset_balances.insert(
            "unrelated".into(),
            BTreeMap::from([("NOV".into(), u128::MAX)]),
        );
        store
            .module_state
            .treasury_reserves
            .insert("USDT".into(), u128::MAX);
        store
            .module_state
            .fee_quote_failure_counts
            .insert("USDT:max_pay_exceeded".into(), 7);
        store.module_state.clearing_daily_window_day = 9;
        store.module_state.clearing_daily_nov_used = 123;
        store
    }

    fn business(
        &self,
        store: &mut NovNativeExecutionStoreV1,
        now: u128,
        expired: bool,
    ) -> (NovSettledFeeV1, NovNativeExecutionReceiptV1) {
        let NovTxKindV1::Transfer(transfer) = &self.wire.kind else {
            unreachable!()
        };
        let recipient = to_hex_prefixed_v1(&transfer.to);
        let snapshot = crate::native_transfer_delta::TransferSnapshot {
            payer_balance: native_account_asset_balance_v1(store, &self.subject.account_id, "NOV"),
            recipient_balance: native_account_asset_balance_v1(store, &recipient, "NOV"),
            next_nonce: store
                .module_state
                .native_auth_next_nonces
                .get(&self.reservation.identity_key)
                .copied()
                .unwrap_or(0),
        };
        let fee = (|| {
            let mut quote = quote_fee_policy_from_execution_request_v1(&self.request, store, now)?;
            if expired {
                quote.expires_at_unix_ms = now - 1;
            }
            settle_fee_quote_into_treasury_v1(
                store,
                &quote,
                &self.reservation.tx_hash,
                &self.subject,
                now,
            )
        })();
        let (fee, receipt) = match fee {
            Err(error) => {
                let fee = unresolved_settled_fee_v1(&self.request);
                let receipt = build_failed_native_receipt_v1(
                    &self.request,
                    &fee,
                    &self.subject,
                    "fee".into(),
                    "settlement".into(),
                    error.to_string(),
                );
                (fee, receipt)
            }
            Ok(fee) => {
                let outcome = crate::native_transfer_delta::compute_outcome_v1(
                    &crate::native_transfer_delta::TransferIntent {
                        tx_hash: self.request.tx_hash,
                        from: transfer.from.as_slice().try_into().unwrap(),
                        to: transfer.to.as_slice().try_into().unwrap(),
                        nonce_identity: self.reservation.identity_key.clone(),
                        nonce: transfer.nonce,
                        amount: transfer.amount,
                        approved_fee: fee.nov_amount,
                        fee_cap: fee.nov_amount,
                    },
                    &snapshot,
                    None,
                )
                .unwrap();
                store
                    .module_state
                    .account_asset_balances
                    .entry(self.subject.account_id.clone())
                    .or_default()
                    .insert("NOV".into(), outcome.delta().payer.after);
                if recipient != self.subject.account_id && outcome.is_success() {
                    store
                        .module_state
                        .account_asset_balances
                        .entry(recipient)
                        .or_default()
                        .insert("NOV".into(), outcome.delta().recipient.after);
                }
                let receipt = match outcome.failure() {
                    Some(crate::native_transfer_delta::TransferExecutionFailureV1::Business(
                        error,
                    )) => build_failed_native_receipt_v1(
                        &self.request,
                        &fee,
                        &self.subject,
                        "native_asset".into(),
                        "transfer".into(),
                        format!("native.transfer.{error}"),
                    ),
                    None => build_success_native_receipt_v1(
                        &self.request,
                        &fee,
                        &self.subject,
                        "native_asset",
                        "transfer",
                        Vec::new(),
                    ),
                    _ => panic!("successful fee settlement cannot create a local fee rejection"),
                };
                (fee, receipt)
            }
        };
        commit_nov_native_durable_auth_reservation_v1(store, &self.reservation).unwrap();
        (fee, receipt)
    }

    fn finalized(
        &self,
        store: &mut NovNativeExecutionStoreV1,
        fee: &NovSettledFeeV1,
        mut receipt: NovNativeExecutionReceiptV1,
        now: u128,
    ) -> u64 {
        let sequence = store
            .module_state
            .aoem_semantic_ledger_sequence
            .checked_add(1)
            .unwrap();
        // Explicit projection-only metadata, not a fabricated AOEM/consensus proof.
        receipt.aoem_semantic_ingress = Some(NovAoemSemanticIngressMetaV1 {
            execution_kernel: "projection-test-only".into(),
            semantic_entry: "projection-only".into(),
            semantic_ledger_sequence: sequence,
            semantic_ledger_prev_seal: store.module_state.aoem_semantic_ledger_head.clone(),
            semantic_ledger_commit_seal: format!("projection-only-{sequence}"),
            ..Default::default()
        });
        receipt.aoem_semantic_commit = build_native_receipt_aoem_semantic_commit_v1(&receipt);
        store.module_state.aoem_semantic_ledger_sequence = sequence;
        store.module_state.aoem_semantic_ledger_head = receipt
            .aoem_semantic_ingress
            .as_ref()
            .unwrap()
            .semantic_ledger_commit_seal
            .clone();
        store
            .receipts
            .insert(receipt.tx_hash.clone(), receipt.clone());
        let trace =
            build_execution_trace_v1(&self.request, fee, &receipt, &self.subject, store, now);
        persist_execution_trace_v1(store, trace);
        store.last_updated_unix_ms = now;
        let mirror = build_native_aoem_semantic_ledger_mirror_record_v1(&receipt, now).unwrap();
        assert!(store
            .module_state
            .aoem_semantic_ledger_records
            .insert(sequence, mirror)
            .is_none());
        sequence
    }
}

fn exercise(
    initial: NovNativeExecutionStoreV1,
    fixtures: &[Fixture],
    expire: bool,
) -> (NovNativeExecutionStoreV1, Vec<RawPathChangeV1>) {
    let batch: Vec<_> = fixtures
        .iter()
        .map(|fixture| (&fixture.wire, &fixture.reservation))
        .collect();
    let mut sparse = TransferAccessV1::for_batch(&batch)
        .unwrap()
        .load(&Reader(raw_records(&initial)))
        .unwrap();
    let mut effects = TransferRecordEffectsV1::new(sparse.captured_records_v1());
    let mut full = initial.clone();
    for (index, fixture) in fixtures.iter().enumerate() {
        let now = 10 * NOV_MILLIS_PER_DAY_V1 + index as u128 + 123;
        let before = full.clone();
        let (fee, receipt) = fixture.business(&mut full, now, expire);
        let actual = fixture.business(sparse.working_store_mut(), now, expire);
        assert_eq!(
            serde_json::to_vec(&actual).unwrap(),
            serde_json::to_vec(&(fee.clone(), receipt.clone())).unwrap()
        );
        let changes = effects
            .business(sparse.working_store(), &fixture.wire, &fixture.reservation)
            .unwrap();
        assert_eq!(changes, full_delta(&before, &full), "business item {index}");
        assert!(changes.iter().all(|change| !matches!(change, RawPathChangeV1::Put { path, .. } | RawPathChangeV1::Delete { path } if path.first().is_some_and(|part| part == "receipts" || part == "last_updated_unix_ms"))));
        let before_final = full.clone();
        let old_order = full.module_state.execution_trace_order.clone();
        let sequence = fixture.finalized(&mut full, &fee, receipt.clone(), now);
        fixture.finalized(sparse.working_store_mut(), &fee, receipt, now);
        // The current key may already be in the window: then the pre-persist
        // oldest candidate survives. Projection must inspect actual presence.
        let eviction_candidate =
            (old_order.len() >= NOV_EXECUTION_TRACE_MAX_ENTRIES_V1).then(|| old_order[0].as_str());
        let changes = effects
            .finalized(
                sparse.working_store(),
                &fixture.reservation.tx_hash,
                sequence,
                eviction_candidate,
            )
            .unwrap();
        assert_eq!(
            changes,
            full_delta(&before_final, &full),
            "finalized item {index}"
        );
    }
    let net = effects.net_changes();
    assert_eq!(net, full_delta(&initial, &full));
    assert_eq!(net, sparse.changed_records().unwrap());
    sparse.validate_changes_v1(&net).unwrap();
    assert_eq!(merged(&initial, &net), full);
    (full, net)
}

#[test]
fn typed_effects_actual_fee_helpers_match_full_encoding_for_success_and_five_rejections() {
    for case in [
        "success",
        "cap",
        "insufficient",
        "paused",
        "overflow",
        "expired",
    ] {
        let fixture = Fixture::new(0, 17, if case == "cap" { 1 } else { 0 }, false, false);
        let mut initial = fixture.store();
        match case {
            "insufficient" => {
                initial
                    .module_state
                    .account_asset_balances
                    .remove(&fixture.subject.account_id);
            }
            "paused" => initial.module_state.treasury_settlement_paused = true,
            "overflow" => initial.module_state.treasury_settled_nov_total = u128::MAX,
            _ => {}
        }
        let hash = fixture.reservation.tx_hash.clone();
        let identity = fixture.reservation.identity_key.clone();
        let (full, _) = exercise(initial, &[fixture], case == "expired");
        assert_eq!(full.receipts[&hash].status, case == "success", "{case}");
        assert_eq!(full.module_state.native_auth_next_nonces[&identity], 1);
        assert_eq!(
            full.receipts[&hash].settled_fee_nov > 0,
            case == "success",
            "{case}"
        );
    }
}

#[test]
fn typed_effects_business_failure_and_zero_missing_recipient_keep_exact_structural_records() {
    for amount in [0, 17, u128::MAX] {
        let fixture = Fixture::new(0, amount, 0, false, false);
        let initial = fixture.store();
        let hash = fixture.reservation.tx_hash.clone();
        let recipient = to_hex_prefixed_v1(&[2; 32]);
        assert!(!initial
            .module_state
            .account_asset_balances
            .contains_key(&recipient));
        let (full, changes) = exercise(initial, &[fixture], false);
        assert!(full.receipts[&hash].settled_fee_nov > 0);
        let created = amount != u128::MAX;
        assert_eq!(full.receipts[&hash].status, created);
        assert_eq!(
            full.module_state
                .account_asset_balances
                .contains_key(&recipient),
            created
        );
        assert_eq!(changes.iter().filter(|change| matches!(change, RawPathChangeV1::Put { path, value } if path == &vec!["module_state".to_string(), "account_asset_balances".to_string(), recipient.clone()] && value == b"{}")).count(), usize::from(created));
        if created {
            assert_eq!(
                full.module_state.account_asset_balances[&recipient]["NOV"],
                amount
            );
        }
    }
}

#[test]
fn typed_effects_self_transfer_and_twenty_thirty_two_byte_signer_aliases_preserve_nonce_order() {
    let fixtures = [
        Fixture::new(0, 0, 0, true, false),
        Fixture::new(1, 17, 0, true, true),
        Fixture::new(2, 0, 0, false, false),
    ];
    assert_eq!(
        fixtures[0].reservation.identity_key,
        fixtures[1].reservation.identity_key
    );
    assert_ne!(
        fixtures[0].subject.account_id,
        fixtures[1].subject.account_id
    );
    let mut initial = fixtures[0].store();
    initial.module_state.account_asset_balances.insert(
        fixtures[1].subject.account_id.clone(),
        BTreeMap::from([("NOV".into(), 1000)]),
    );
    let identity = fixtures[0].reservation.identity_key.clone();
    let (full, _) = exercise(initial, &fixtures, false);
    assert_eq!(full.module_state.native_auth_next_nonces[&identity], 3);
    assert!(full.receipts.values().all(|receipt| receipt.status));
}

#[test]
fn typed_effects_full_trace_and_journal_windows_match_actual_helper_eviction() {
    let fixture = Fixture::new(0, 17, 0, false, false);
    let mut initial = fixture.store();
    let fee = settle_fee_policy_from_execution_request_v1(
        &fixture.request,
        &fixture.subject,
        &mut initial,
        123,
    )
    .unwrap();
    let entry = initial.module_state.treasury_settlement_journal[0].clone();
    let mut receipt = build_success_native_receipt_v1(
        &fixture.request,
        &fee,
        &fixture.subject,
        "native_asset",
        "transfer",
        Vec::new(),
    );
    initial.module_state.treasury_settlement_journal.clear();
    initial.module_state.treasury_settlement_journal_next_seq = 0;
    for index in 0..NOV_EXECUTION_TRACE_MAX_ENTRIES_V1 {
        receipt.tx_hash = format!("{index:064x}");
        let trace = build_execution_trace_v1(
            &fixture.request,
            &fee,
            &receipt,
            &fixture.subject,
            &initial,
            index as u128,
        );
        persist_execution_trace_v1(&mut initial, trace);
        let mut entry = entry.clone();
        entry.tx_hash = receipt.tx_hash.clone();
        entry.source_amount = u128::MAX - index as u128;
        append_treasury_settlement_journal_v1(&mut initial, entry);
    }
    let oldest = initial.module_state.execution_trace_order[0].clone();
    let (full, changes) = exercise(initial, &[fixture], false);
    assert_eq!(
        full.module_state.execution_trace_order.len(),
        NOV_EXECUTION_TRACE_MAX_ENTRIES_V1
    );
    assert_eq!(
        full.module_state.treasury_settlement_journal.len(),
        NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1
    );
    assert_eq!(full.module_state.treasury_settlement_journal[0].seq, 2);
    assert_eq!(full.module_state.treasury_settlement_journal_next_seq, 513);
    assert!(!full
        .module_state
        .execution_traces_by_tx
        .contains_key(&oldest));
    assert!(changes.iter().any(
        |change| matches!(change, RawPathChangeV1::Delete { path } if path.last() == Some(&oldest))
    ));
}

#[test]
fn typed_effects_u128_account_values_and_day_rollover_remain_lossless() {
    let fixture = Fixture::new(0, 1, 0, false, false);
    let mut initial = fixture.store();
    initial
        .module_state
        .account_asset_balances
        .get_mut(&fixture.subject.account_id)
        .unwrap()
        .insert("NOV".into(), u128::MAX);
    let account = fixture.subject.account_id.clone();
    let (full, changes) = exercise(initial, &[fixture], false);
    assert_eq!(full.module_state.clearing_daily_window_day, 10);
    assert!(full.module_state.account_asset_balances[&account]["NOV"] > u64::MAX as u128);
    let token = changes
        .iter()
        .find_map(|change| match change {
            RawPathChangeV1::Put { path, value }
                if path
                    == &vec![
                        "module_state".into(),
                        "account_asset_balances".into(),
                        account.clone(),
                        "NOV".into(),
                    ] =>
            {
                Some(value)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<u128>(token).unwrap(),
        full.module_state.account_asset_balances[&account]["NOV"]
    );
}

#[test]
fn typed_effects_full_trace_window_existing_current_hash_reorders_without_eviction() {
    let fixture = Fixture::new(0, 17, 0, false, false);
    let mut initial = fixture.store();
    let fee = unresolved_settled_fee_v1(&fixture.request);
    let mut receipt = build_failed_native_receipt_v1(
        &fixture.request,
        &fee,
        &fixture.subject,
        "fee".into(),
        "quote".into(),
        "earlier diagnostic attempt".into(),
    );
    for index in 0..NOV_EXECUTION_TRACE_MAX_ENTRIES_V1 {
        receipt.tx_hash = if index == 200 {
            fixture.reservation.tx_hash.clone()
        } else {
            format!("{index:064x}")
        };
        let trace = build_execution_trace_v1(
            &fixture.request,
            &fee,
            &receipt,
            &fixture.subject,
            &initial,
            index as u128,
        );
        persist_execution_trace_v1(&mut initial, trace);
    }
    assert!(!initial.receipts.contains_key(&fixture.reservation.tx_hash));
    let old_order = initial.module_state.execution_trace_order.clone();
    let oldest = old_order[0].clone();
    let oldest_trace = initial.module_state.execution_traces_by_tx[&oldest].clone();
    let current = fixture.reservation.tx_hash.clone();
    let (full, changes) = exercise(initial, &[fixture], false);
    let expected_order: Vec<_> = old_order
        .into_iter()
        .filter(|hash| hash != &current)
        .chain(std::iter::once(current.clone()))
        .collect();
    assert_eq!(full.module_state.execution_trace_order, expected_order);
    assert_eq!(
        full.module_state.execution_trace_order.len(),
        NOV_EXECUTION_TRACE_MAX_ENTRIES_V1
    );
    assert_eq!(
        full.module_state.execution_traces_by_tx.len(),
        NOV_EXECUTION_TRACE_MAX_ENTRIES_V1
    );
    assert_eq!(
        full.module_state.execution_traces_by_tx[&oldest],
        oldest_trace
    );
    assert!(!changes.iter().any(|change| matches!(change, RawPathChangeV1::Delete { path } if path.get(1).is_some_and(|field| field == "execution_traces_by_tx"))));
}

#[test]
fn typed_effects_missing_declared_path_fails_and_net_oracle_detects_omissions_and_extras() {
    let fixture = Fixture::new(0, 17, 0, false, false);
    let initial = fixture.store();
    let mut sparse = TransferAccessV1::for_batch(&[(&fixture.wire, &fixture.reservation)])
        .unwrap()
        .load(&Reader(raw_records(&initial)))
        .unwrap();
    let mut captured = sparse.captured_records_v1();
    let nonce_path = vec![
        "module_state".into(),
        "native_auth_next_nonces".into(),
        fixture.reservation.identity_key.clone(),
    ];
    assert!(captured.remove(&nonce_path).is_some());
    let mut incomplete = TransferRecordEffectsV1::new(captured);
    fixture.business(sparse.working_store_mut(), 123, false);
    assert!(incomplete
        .business(sparse.working_store(), &fixture.wire, &fixture.reservation)
        .is_err());
    assert!(
        incomplete.net_changes().is_empty(),
        "unknown projected path must leave the journal unchanged"
    );

    let (full, net) = exercise(initial.clone(), &[fixture], false);
    let oracle = full_delta(&initial, &full);
    let mut omitted = net.clone();
    omitted.retain(
        |change| !matches!(change, RawPathChangeV1::Put { path, .. } if path == &nonce_path),
    );
    assert_ne!(
        omitted, oracle,
        "a legal-looking subset is not a complete patch"
    );
    let mut extra = net;
    extra.push(RawPathChangeV1::Put {
        path: vec!["module_state".into(), "treasury_policy_version".into()],
        value: b"99".to_vec(),
    });
    assert_ne!(
        extra, oracle,
        "an additional policy mutation must not match the net oracle"
    );
}
