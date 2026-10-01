//! Tests run inside the existing real-AOEM three-root document fixture.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceMetadata {
    schema: String,
    store: (),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceMetadata {
    schema: String,
    amount: u128,
    store: (),
}

fn write_fixture_key(workspace: &WorkspaceStore, key: &[u8], value: Option<Vec<u8>>) -> Result<()> {
    let digest = sha256_bytes_v1(&[
        b"reference-metadata-test-write-v1\0",
        key,
        &serde_json::to_vec(&value)?,
    ]);
    let write = match value {
        Some(value) => AoemAtomicGraphWriteV1::Put {
            key: key.to_vec(),
            value,
        },
        None => AoemAtomicGraphWriteV1::Delete { key: key.to_vec() },
    };
    workspace.graph.commit(AoemAtomicGraphRequestV1 {
        graph_id: u64::from_be_bytes(digest[..8].try_into()?).max(1),
        steps: vec![AoemAtomicGraphStepV1 {
            task_kind: 0,
            task_payload: vec![],
            writes: vec![write.clone()],
            event: None,
        }],
        completion_write: write,
    })?;
    Ok(())
}

pub(super) fn exercise_metadata_reference_storage(
    workspace: &WorkspaceStore,
    source_v2: &[u8],
    source_v3: &[u8],
    source_v1: &[u8],
    inherited: &StagedRecordUpdate,
) -> Result<Vec<u8>> {
    let source = without_materialization_for_test(|| {
        decode_published_output_metadata::<SourceMetadata>(workspace, source_v2, &["store"])
    })?;
    assert_eq!(source.inline.schema, "root-bundle-test/v1");
    let () = source.inline.store;
    let source_delta = without_materialization_for_test(|| {
        decode_published_output_metadata::<SourceMetadata>(workspace, source_v3, &["store"])
    })?;
    assert_eq!(
        source.state.rooted_parts()?,
        source_delta.state.rooted_parts()?
    );
    let metadata = ReferenceMetadata {
        schema: "reference-metadata-test/v1".into(),
        amount: u128::MAX,
        store: (),
    };
    let prepared = without_materialization_for_test(|| {
        prepare_reference(workspace, &metadata, &["store"], &source.state)
    })?;
    assert_eq!(
        prepared.state().rooted_parts()?,
        source.state.rooted_parts()?
    );
    assert!(prepared.updates.iter().all(|update| {
        update.parent_root() == update.root()
            && update.nodes().is_empty()
            && update.blobs().is_empty()
    }));
    assert!(decode_metadata::<ReferenceMetadata>(workspace, &prepared.bytes, &["store"]).is_err());
    without_materialization_for_test(|| persist_reference(workspace, &prepared))?;
    verify_metadata_reference_after_reopen(workspace, &prepared.bytes)?;
    // Another source version with the same current roots/stats produces exactly
    // the same input reference. The source bytes themselves are bound by NCW2.
    let from_delta = prepare_reference(workspace, &metadata, &["store"], &source_delta.state)?;
    assert_eq!(prepared.bytes, from_delta.bytes);
    without_materialization_for_test(|| persist_reference(workspace, &from_delta))?;
    assert!(without_materialization_for_test(|| read_store(workspace, prepared.state())).is_err());
    let store = read_store(workspace, prepared.state())?;
    assert_eq!(
        store.module_state.account_asset_balances["record-profile-account"]["NOV"],
        u128::MAX - 7
    );
    assert_eq!(
        store.module_state.account_asset_balances["record-profile-account"]["USDT"],
        99
    );
    for legacy_or_output in [source_v1, source_v2, source_v3] {
        assert!(
            decode_metadata::<SourceMetadata>(workspace, legacy_or_output, &["store"]).is_err()
        );
    }
    assert!(
        decode_published_output_metadata::<SourceMetadata>(workspace, source_v1, &["store"])
            .is_err()
    );
    assert!(decode_metadata::<ReferenceMetadata>(
        workspace,
        &serde_json::to_vec(&metadata)?,
        &["store"]
    )
    .is_err());
    assert!(decode_metadata::<ReferenceMetadata>(workspace, &prepared.bytes, &["other"]).is_err());
    let legacy: Document = serde_json::from_slice(source_v1)?;
    assert!(prepare_reference(workspace, &metadata, &["store"], &legacy.state).is_err());
    let non_null = RawValue::from_string("{\"store\":{}}".into())?;
    assert!(prepare_reference(workspace, &non_null, &["store"], &source.state).is_err());
    let oversized = ReferenceMetadata {
        schema: "x".repeat(MAX_PAYLOAD_BYTES_V1),
        amount: u128::MAX,
        store: (),
    };
    assert!(prepare_reference(workspace, &oversized, &["store"], &source.state).is_err());

    for fault in 0..9 {
        let mut bad: Document = serde_json::from_slice(&prepared.bytes)?;
        match fault {
            0 => bad.state.bundle = None,
            1 => bad.schema = DOCUMENT_SCHEMA.into(),
            2 => bad.state.bundle.as_mut().unwrap().state.root[0] ^= 1,
            3 => bad.state.bundle.as_mut().unwrap().physical.parent_root[0] ^= 1,
            4 => bad.state.bundle.as_mut().unwrap().receipt_codec = "wrong".into(),
            5 => bad.state.records += 1,
            6 => bad.state.blob_bytes += 1,
            7 => {
                bad.inline = RawValue::from_string(
                    "{\"schema\":\"tampered\",\"amount\":1,\"store\":null}".into(),
                )?;
            }
            _ => {
                bad.inline = RawValue::from_string(
                    "{\"schema\":\"reference-metadata-test/v1\",\"amount\":1,\"store\":{}}".into(),
                )?;
            }
        }
        assert!(decode_metadata::<ReferenceMetadata>(
            workspace,
            &serde_json::to_vec(&bad)?,
            &["store"]
        )
        .is_err());
    }
    let mut invalid_v3: Document = serde_json::from_slice(source_v3)?;
    invalid_v3.witness = None;
    assert!(decode_published_output_metadata::<SourceMetadata>(
        workspace,
        &serde_json::to_vec(&invalid_v3)?,
        &["store"]
    )
    .is_err());

    let commitment = record_document_commitment(&prepared.bytes);
    for role in [b"physical".as_slice(), b"state", b"receipts"] {
        let id = prepared_role_id(commitment, role);
        let marker = [b"NST1".as_slice(), &workspace.scope, b"c", &id].concat();
        let saved = workspace
            .graph
            .get(&marker)?
            .context("test role marker missing")?;
        write_fixture_key(workspace, &marker, None)?;
        assert!(
            decode_metadata::<ReferenceMetadata>(workspace, &prepared.bytes, &["store"]).is_err()
        );
        assert!(
            workspace.graph.get(&marker)?.is_none(),
            "reader must not repair missing completion"
        );
        write_fixture_key(workspace, &marker, Some(saved))?;
    }
    // A root node is mandatory even when all completion markers are present.
    let root_key = [
        b"NST1".as_slice(),
        &workspace.scope,
        b"n",
        &prepared.state.root,
    ]
    .concat();
    let root_bytes = workspace
        .graph
        .get(&root_key)?
        .context("test root node missing")?;
    write_fixture_key(workspace, &root_key, None)?;
    assert!(decode_metadata::<ReferenceMetadata>(workspace, &prepared.bytes, &["store"]).is_err());
    write_fixture_key(workspace, &root_key, Some(root_bytes))?;

    // Metadata is explicitly not whole-history data availability. Deleting an
    // untouched inherited blob leaves the metadata valid, but a required point
    // read and the explicit cold boundary must fail, never fabricate absence.
    let path = [
        "module_state",
        "account_asset_balances",
        "record-profile-account",
        "USDT",
    ]
    .map(str::to_owned);
    let key = native_store_records::key(&path)?;
    let blob_hash = inherited
        .blobs()
        .iter()
        .find_map(|(hash, bytes)| {
            let len = usize::from(u16::from_be_bytes(bytes[4..6].try_into().ok()?));
            (bytes.get(10..10 + len) == Some(key.as_slice())).then_some(*hash)
        })
        .context("test inherited record blob missing")?;
    let blob_key = [
        b"NST1".as_slice(),
        &workspace.scope,
        b"b",
        &blob_hash,
        &0u32.to_be_bytes(),
    ]
    .concat();
    let blob = workspace
        .graph
        .get(&blob_key)?
        .context("test blob chunk missing")?;
    write_fixture_key(workspace, &blob_key, None)?;
    verify_metadata_reference_after_reopen(workspace, &prepared.bytes)?;
    assert!(crate::native_state_records::read_record(
        &AoemStateReaderV1::new(&workspace.graph, workspace.scope),
        prepared.state.root,
        &key
    )
    .is_err());
    assert!(read_store(workspace, prepared.state()).is_err());
    write_fixture_key(workspace, &blob_key, Some(blob))?;
    assert_eq!(read_store(workspace, prepared.state())?, store);
    Ok(prepared.bytes)
}

pub(super) fn verify_metadata_reference_after_reopen(
    workspace: &WorkspaceStore,
    bytes: &[u8],
) -> Result<()> {
    without_materialization_for_test(|| {
        let metadata = decode_metadata::<ReferenceMetadata>(workspace, bytes, &["store"])?;
        assert_eq!(metadata.inline.schema, "reference-metadata-test/v1");
        assert_eq!(metadata.inline.amount, u128::MAX);
        assert!(metadata.state.rooted_parts()?.is_some());
        Ok(())
    })
}
