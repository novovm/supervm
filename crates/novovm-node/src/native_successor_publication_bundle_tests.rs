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
            ledger, b"native_block_ledger/v1/successor/finalized-intent", None, &read,
        );
        successor_bundle_ledger_fault(
            ledger, b"bundle-test-unexpected-ledger-state", Some(vec![1]), &read,
        );
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

fn successor_bundle_ledger_fault(
    path: &Path,
    key: &[u8],
    replacement: Option<Vec<u8>>,
    read: &impl Fn() -> Result<crate::native_block_ledger::VerifiedSuccessorPublicationV1>,
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
    let rejected = read().is_err();
    let after = successor_bundle_ledger_snapshot(path);
    let db = rocksdb::DB::open_default(path).unwrap();
    if let Some(bytes) = original {
        db.put(key, bytes).unwrap(); // Explicit fixture restoration, never recovery repair.
    } else {
        db.delete(key).unwrap();
    }
    drop(db);
    assert!(rejected, "corrupted ledger evidence was accepted: {}", to_hex(key));
    assert_eq!(after, faulted, "rejected bundle must not repair or mutate evidence");
    assert_eq!(successor_bundle_ledger_snapshot(path), before);
}
