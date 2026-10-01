//! Incremental preparation shares the existing real-AOEM document fixture.
use super::*;
use native_store_records::RawPathChangeV1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestDocument<S> {
    z_exact_integer: u128,
    store: S,
    a_schema: String,
}

fn inline() -> TestDocument<()> {
    TestDocument {
        z_exact_integer: u128::MAX,
        store: (),
        a_schema: "prepared-delta-test/v1".into(),
    }
}

fn put<T: Serialize>(path: &[&str], value: &T) -> Result<RawPathChangeV1> {
    Ok(RawPathChangeV1::Put {
        path: path.iter().map(|part| (*part).into()).collect(),
        value: native_store_records::typed_raw_v1(value)?,
    })
}

fn stage(
    workspace: &WorkspaceStore,
    parent: &StoreRef,
    changes: Vec<RawPathChangeV1>,
) -> Result<RecordTreeUpdatesV1> {
    let (physical_root, state_root, receipt_root, records, bytes) = parent.rooted_parts()?.unwrap();
    let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
    let mut physical = RecordOverlayV1::new(&reader, physical_root);
    let mut state = RecordOverlayV1::new(&reader, state_root);
    let mut receipts = RecordOverlayV1::new(&reader, receipt_root);
    let (records, blob_bytes) =
        native_store_records::apply_raw_changes_to_overlay_v1(&mut physical, &changes)?
            .checked_apply(records, bytes)?;
    native_record_commitment::apply_consensus_changes_v1(&mut state, &changes)?;
    for change in &changes {
        match change {
            RawPathChangeV1::Put { path, value }
                if path.first().map(String::as_str) == Some("receipts") =>
            {
                let receipt: NovNativeExecutionReceiptV1 = serde_json::from_slice(value)?;
                receipts.stage(&[native_record_commitment::receipt_change_v1(&receipt)?])?;
            }
            RawPathChangeV1::Delete { path }
                if path.first().map(String::as_str) == Some("receipts") =>
            {
                receipts.stage(&[RecordChange::Delete {
                    key: parse_fixed_hex_32_v1(&path[1], "test receipt")?.to_vec(),
                }])?;
            }
            _ => {}
        }
    }
    Ok(RecordTreeUpdatesV1 {
        physical: physical.finish(),
        state: state.finish(),
        receipts: receipts.finish(),
        records,
        blob_bytes,
        changes: Some(changes),
    })
}

pub(super) fn exercise(
    workspace: &WorkspaceStore,
    parent: &StoreRef,
    parent_store: &NovNativeExecutionStoreV1,
) -> Result<Vec<u8>> {
    let receipt: NovNativeExecutionReceiptV1 = serde_json::from_value(serde_json::json!({
        "tx_hash":to_hex(&[0x8c;32]),"status":true,"target":"native","module":"native_asset",
        "method":"transfer","settled_fee_nov":0,"paid_asset":"NOV","paid_amount":0,
        "logs":[],"failure_reason":null,"fee_contract":"unified"
    }))?;
    let changes = vec![
        put(
            &[
                "module_state",
                "account_asset_balances",
                "record-profile-account",
                "NOV",
            ],
            &(u128::MAX - 5),
        )?,
        put(
            &[
                "module_state",
                "native_auth_next_nonces",
                "prepared-delta-account",
            ],
            &1u64,
        )?,
        put(&["receipts", &receipt.tx_hash], &receipt)?,
    ];
    let mut after = parent_store.clone();
    after
        .module_state
        .account_asset_balances
        .get_mut("record-profile-account")
        .unwrap()
        .insert("NOV".into(), u128::MAX - 5);
    after
        .module_state
        .native_auth_next_nonces
        .insert("prepared-delta-account".into(), 1);
    after
        .receipts
        .insert(receipt.tx_hash.clone(), receipt.clone());
    let updates = stage(workspace, parent, changes.clone())?;
    let fast = without_materialization_for_test(|| {
        prepare_delta(workspace, &inline(), &["store"], parent, &updates)
    })?;
    let full = TestDocument {
        z_exact_integer: u128::MAX,
        store: after,
        a_schema: inline().a_schema,
    };
    let cold = prepare_record_profile(
        workspace,
        &full,
        &["store"],
        &full.store,
        Some((parent, parent_store)),
        Some(stage(workspace, parent, changes.clone())?),
    )?;
    assert!(is_delta_document(&fast.bytes)?);
    assert_eq!(
        fast.bytes, cold.bytes,
        "new pure-delta writer preserves exact previous V3 bytes"
    );
    assert_eq!(fast.state().rooted_parts()?, cold.state.rooted_parts()?);
    without_materialization_for_test(|| persist_delta(workspace, &fast, &updates))?;
    without_materialization_for_test(|| persist_delta(workspace, &fast, &updates))?;
    assert!(without_materialization_for_test(|| persist(workspace, &cold)).is_err());
    let (restored, _): (TestDocument<NovNativeExecutionStoreV1>, _) =
        decode(workspace, &fast.bytes, &["store"])?;
    assert_eq!(restored.store, full.store);
    verify_after_reopen(workspace, &fast.bytes, parent)?;

    for fault in 0..8 {
        let mut invalid = stage(workspace, parent, changes.clone())?;
        match fault {
            0 => invalid.records += 1,
            1 => invalid.blob_bytes += 1,
            2 => invalid.changes = None,
            3 => {
                invalid.changes.as_mut().unwrap().pop();
            }
            4 => invalid.changes.as_mut().unwrap().reverse(),
            5 => {
                invalid.changes.as_mut().unwrap()[0] = changes[1].clone();
            }
            6 => {
                invalid.changes.as_mut().unwrap().insert(
                    1,
                    put(
                        &[
                            "module_state",
                            "account_asset_balances",
                            "record-profile-account",
                            "USDT",
                        ],
                        &99u128,
                    )?,
                );
            }
            _ => {
                let reader = AoemStateReaderV1::new(&workspace.graph, workspace.scope);
                invalid.state =
                    RecordOverlayV1::new(&reader, parent.rooted_parts()?.unwrap().1).finish();
            }
        }
        let error = prepare_delta(workspace, &inline(), &["store"], parent, &invalid)
            .err()
            .context("invalid delta accepted")?;
        assert!(
            error.downcast_ref::<DeltaOutputTooLarge>().is_none(),
            "invalid state must not masquerade as capacity"
        );
        assert!(persist_delta(workspace, &fast, &invalid).is_err());
    }
    let mut wrong_parent = parent.clone();
    wrong_parent.bundle.as_mut().unwrap().state.root[0] ^= 1;
    assert!(prepare_delta(workspace, &inline(), &["store"], &wrong_parent, &updates).is_err());
    let non_null = TestDocument {
        z_exact_integer: 1u128,
        store: 1u8,
        a_schema: "not-null".into(),
    };
    assert!(prepare_delta(workspace, &non_null, &["store"], parent, &updates).is_err());

    let mut modified_receipt = receipt.clone();
    modified_receipt.paid_amount = 1;
    for change in [
        put(&["receipts", &receipt.tx_hash], &modified_receipt)?,
        RawPathChangeV1::Delete {
            path: vec!["receipts".into(), receipt.tx_hash.clone()],
        },
    ] {
        let invalid = stage(workspace, fast.state(), vec![change])?;
        assert!(prepare_delta(workspace, &inline(), &["store"], fast.state(), &invalid).is_err());
    }
    let oversized = TestDocument {
        z_exact_integer: u128::MAX,
        store: (),
        a_schema: "x".repeat(MAX_PAYLOAD_BYTES_V1),
    };
    let error = prepare_delta(workspace, &oversized, &["store"], parent, &updates)
        .err()
        .context("oversized document accepted")?;
    assert!(matches!(
        error.downcast_ref::<DeltaOutputTooLarge>(),
        Some(DeltaOutputTooLarge::Document)
    ));
    // A capacity result did not consume updates or mutate a reservation: retry
    // with the original metadata has exactly the previous bytes and roots.
    assert_eq!(
        prepare_delta(workspace, &inline(), &["store"], parent, &updates)?.bytes,
        fast.bytes
    );
    let mut changed_bytes = prepare_delta(workspace, &inline(), &["store"], parent, &updates)?;
    changed_bytes.bytes.push(b' ');
    assert!(persist_delta(workspace, &changed_bytes, &updates).is_err());
    Ok(fast.bytes)
}

pub(super) fn verify_after_reopen(
    workspace: &WorkspaceStore,
    bytes: &[u8],
    parent: &StoreRef,
) -> Result<()> {
    without_materialization_for_test(|| {
        let decoded = decode_delta::<TestDocument<()>>(workspace, bytes, &["store"], parent)?
            .context("delta fixture not V3")?;
        assert_eq!(decoded.inline.z_exact_integer, u128::MAX);
        assert_eq!(decoded.inline.a_schema, "prepared-delta-test/v1");
        assert_eq!(decoded.changes.len(), 3);
        Ok(())
    })
}

#[test]
fn pure_delta_replay_budget_is_not_just_document_length() {
    // Physically meaningful string-valued reservation paths. Their combined
    // values exceed the replay budget although the path-only witness is tiny.
    let raw = format!("\"{}\"", "x".repeat(8 * 1024 * 1024 - 1024));
    let changes = (0..9)
        .map(|index| RawPathChangeV1::Put {
            path: vec![
                "module_state".into(),
                "native_auth_nonce_reservations".into(),
                format!("{index:04}"),
            ],
            value: raw.as_bytes().to_vec(),
        })
        .collect::<Vec<_>>();
    assert!(delta::Witness::from_changes(&changes).is_ok());
    assert!(!delta::writer_budget_allows(&changes).unwrap());
}
