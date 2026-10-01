#[derive(Clone, Copy)]
enum SuccessorBundleStage {
    Prepared,
    Published,
    Finalized,
}

// Reuse the real third-block record fixture, with its finalized second-block
// archive and complete decision witness. No alternative genesis or fake QC.
fn exercise_successor_publication_bundle(
    ledger: &Path,
    pin: [u8; 32],
    namespace: [u8; 32],
    parent: [u8; 32],
    artifact: &workspace::IsolatedBlockArtifactV1,
    stage: SuccessorBundleStage,
    params: &serde_json::Value,
) {
    use crate::native_block_ledger::{
        NovNativeBlockLedgerV1 as Ledger, NovNativeIsolatedExecutionBindingV1 as Binding,
    };
    let binding = Binding {
        workspace_id: artifact.workspace_id,
        plan_commitment: artifact.plan_commitment,
        output_digest: artifact.output_digest,
    };
    let read = || {
        Ledger::load_verified_successor_publication_v1(ledger, pin, namespace, parent, &binding)
    };
    let before = successor_bundle_ledger_snapshot(ledger);
    let (bundle, count) = Ledger::count_fresh_ledger_verifications_for_test_v1(read);
    let bundle = bundle.unwrap();
    assert_eq!(count, 1, "one direct bundle must fully verify the ledger once");
    // These are the five original Verify lookups. Count only this direct
    // ledger read, not the coordinator's independent NCW2 source validation.
    let ((commitment, archive, published, repeated_published, finality), count) =
        Ledger::count_fresh_ledger_verifications_for_test_v1(|| {
            (
                Ledger::verify_fresh_successor_promotion_target_v1(
                    ledger, pin, namespace, parent, &binding,
                )
                .unwrap(),
                Ledger::load_fresh_finalized_archive_v1(
                    ledger,
                    pin,
                    namespace,
                    artifact.block().header.height - 1,
                )
                .unwrap(),
                Ledger::load_fresh_successor_published_block_v1(ledger, pin, namespace).unwrap(),
                Ledger::load_fresh_successor_published_block_v1(ledger, pin, namespace).unwrap(),
                Ledger::load_fresh_successor_finality_v1(ledger, pin, namespace).unwrap(),
            )
        });
    assert_eq!(count, 5, "reference getters must retain complete verification");
    assert_eq!(bundle.commitment, commitment);
    assert_eq!(bundle.parent_archive.block, archive.block);
    assert_eq!(bundle.parent_archive.proof, archive.proof);
    assert_eq!(bundle.parent_archive.execution, archive.execution);
    assert_eq!(bundle.parent_archive.commitment, archive.commitment);
    assert_eq!(
        serde_json::to_value(&bundle.parent_archive.config).unwrap(),
        serde_json::to_value(&archive.config).unwrap()
    );
    assert_eq!(bundle.parent_archive.execution.workspace_id, parent);
    assert_eq!(bundle.published_block, published);
    assert_eq!(bundle.published_block, repeated_published);
    assert_eq!(bundle.finality, finality);
    assert_eq!(
        bundle.published_block.is_some(),
        !matches!(stage, SuccessorBundleStage::Prepared)
    );
    assert_eq!(bundle.finality.is_some(), matches!(stage, SuccessorBundleStage::Finalized));
    assert_eq!(successor_bundle_ledger_snapshot(ledger), before);

    workspace::exercise_publication_readback_for_test_v1(
        artifact.block().header.chain_id,
        artifact.workspace_id,
        params,
        &bundle.parent_archive,
    )
    .unwrap();
    let verify = || {
        workspace::without_materialization_for_test(|| {
            workspace::verify_successor_authority_v1(
                artifact.block().header.chain_id,
                parent,
                artifact.workspace_id,
                pin,
                params,
            )
        })
    };
    let ((verified, count), artifact_count) =
        workspace::IsolatedBlockArtifactV1::count_validations_for_test_v1(
            artifact.workspace_id,
            || Ledger::count_fresh_ledger_verifications_for_test_v1(verify),
        );
    assert_eq!(artifact_count, 1, "read-only Verify must fully validate the child once");
    assert_eq!(count, 1, "NCW2/V3 Verify reuses the same-call verified direct parent");
    if matches!(stage, SuccessorBundleStage::Prepared) {
        // Merely preparing a valid intent cannot turn the old authority head
        // into this child, even after its artifact has been fully validated.
        let error = verified.unwrap_err();
        assert!(format!("{error:#}").contains(
            "successor publication requires the exact live parent or completed target"
        ));
    } else {
        let header = &artifact.block().header;
        // The reference getter values above supply every prior report field;
        // artifact equivalence is checked independently by the readback helper.
        assert_eq!(verified.unwrap(), workspace::FreshSuccessorPublicationV1 {
            chain_id: header.chain_id,
            block_hash: header.block_hash,
            workspace_id: artifact.workspace_id,
            intent_commitment: commitment,
            state_root: header.post_state_root,
            receipt_root: header.cumulative_receipt_root,
            state_version: header.state_version,
            aoem_authority_published: true,
            aoem_readback_verified: true,
            ledger_publication_completed: published.is_some(),
            finalized: finality.is_some(),
        });
        workspace::exercise_publication_verify_corruption_for_test_v1(
            header.chain_id, parent, artifact.workspace_id, pin, params,
        ).unwrap();
    }
    assert_eq!(successor_bundle_ledger_snapshot(ledger), before);

    if matches!(stage, SuccessorBundleStage::Prepared) {
        let mut wrong_parent = parent;
        wrong_parent[0] ^= 1;
        assert!(Ledger::load_verified_successor_publication_v1(
            ledger, pin, namespace, wrong_parent, &binding,
        ).is_err());
        for field in 0..3 {
            let mut wrong = binding.clone();
            match field {
                0 => wrong.workspace_id[0] ^= 1,
                1 => wrong.plan_commitment[0] ^= 1,
                _ => wrong.output_digest[0] ^= 1,
            }
            assert!(Ledger::load_verified_successor_publication_v1(
                ledger, pin, namespace, parent, &wrong,
            ).is_err());
        }
        let mut wrong_domain = pin;
        wrong_domain[0] ^= 1;
        assert!(Ledger::load_verified_successor_publication_v1(
            ledger, wrong_domain, namespace, parent, &binding,
        ).is_err());
        wrong_domain = namespace;
        wrong_domain[0] ^= 1;
        assert!(Ledger::load_verified_successor_publication_v1(
            ledger, pin, wrong_domain, parent, &binding,
        ).is_err());
        assert_eq!(successor_bundle_ledger_snapshot(ledger), before);

        // A valid JSON archive with a changed execution digest still has to
        // match its actual registered block and signed decision.
        let archive_key = format!(
            "native_block_ledger/v1/successor/finalized/{:016x}",
            artifact.block().header.height - 1
        ).into_bytes();
        let original = before.iter().find(|(key, _)| key == &archive_key).unwrap();
        let mut changed: serde_json::Value = serde_json::from_slice(&original.1).unwrap();
        let digest_byte = changed["execution"]["output_digest"][0].as_u64().unwrap();
        changed["execution"]["output_digest"][0] = serde_json::json!(digest_byte ^ 1);
        successor_bundle_ledger_fault(
            ledger, &archive_key, Some(serde_json::to_vec(&changed).unwrap()), &read,
        );
        successor_bundle_ledger_fault(ledger, &archive_key, None, &read);
        successor_bundle_ledger_fault(
            ledger, b"native_block_ledger/v1/successor/promotion-pin", Some(vec![0; 32]), &read,
        );
        successor_bundle_ledger_fault(
            ledger, b"native_block_ledger/v1/successor/promotion", None, &read,
        );
    }
    if matches!(stage, SuccessorBundleStage::Finalized) {
        successor_bundle_ledger_fault(
            ledger, b"native_block_ledger/v1/successor/finalized-intent", None, &verify,
        );
        successor_bundle_ledger_fault(
            ledger, b"bundle-test-unexpected-ledger-state", Some(vec![1]), &verify,
        );
        let archive_key = format!(
            "native_block_ledger/v1/successor/finalized/{:016x}",
            artifact.block().header.height - 1
        ).into_bytes();
        successor_bundle_ledger_fault(ledger, &archive_key, None, &verify);
        assert!(verify().unwrap().finalized, "restored public Verify must succeed again");
    }
    assert!(read().is_ok(), "restored fixture must remain fully readable");
    assert_eq!(successor_bundle_ledger_snapshot(ledger), before);
}

fn successor_bundle_ledger_snapshot(path: &Path) -> Vec<(Vec<u8>, Vec<u8>)> {
    let db = rocksdb::DB::open_default(path).unwrap();
    db.iterator(rocksdb::IteratorMode::Start)
        .map(|entry| {
            let (key, value) = entry.unwrap();
            (key.to_vec(), value.to_vec())
        })
        .collect()
}

fn successor_bundle_ledger_fault<T>(
    path: &Path,
    key: &[u8],
    replacement: Option<Vec<u8>>,
    read: &impl Fn() -> Result<T>,
) {
    let before = successor_bundle_ledger_snapshot(path);
    let db = rocksdb::DB::open_default(path).unwrap();
    let original = db.get(key).unwrap();
    if let Some(bytes) = &replacement {
        db.put(key, bytes).unwrap();
    } else {
        assert!(original.is_some(), "missing-key fault requires existing evidence");
        db.delete(key).unwrap();
    }
    drop(db);
    let faulted = successor_bundle_ledger_snapshot(path);
    let rejection = read().err();
    let after = successor_bundle_ledger_snapshot(path);
    let db = rocksdb::DB::open_default(path).unwrap();
    if let Some(bytes) = original {
        db.put(key, bytes).unwrap(); // Explicit fixture restoration, never recovery repair.
    } else {
        db.delete(key).unwrap();
    }
    drop(db);
    let rejection = rejection.unwrap_or_else(|| {
        panic!("corrupted ledger evidence was accepted: {}", to_hex(key))
    });
    assert!(
        !format!("{rejection:#}").contains("unexpected full candidate store materialization"),
        "corrupt evidence must be rejected, not sent to a forbidden cold fallback"
    );
    assert_eq!(after, faulted, "rejected bundle must not repair or mutate evidence");
    assert_eq!(successor_bundle_ledger_snapshot(path), before);
}
