#[test]
fn fresh_runtime_revision_reuses_only_unchanged_fully_verified_database() {
    let (fixture, config, block, binding) = fresh_candidate_fixture();
    let pin = config.compile().unwrap().config_commitment();
    let namespace = [3; 32];
    NovNativeBlockLedgerV1::register_fresh_genesis_candidate_v1(
        &fixture.path,
        pin,
        namespace,
        block.clone(),
        binding,
    )
    .unwrap();
    let load =
        || NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&fixture.path, pin, namespace);
    let session = NovNativeBlockLedgerV1::retain_existing_fresh_session_v1(&fixture.path).unwrap();
    let nested =
        NovNativeBlockLedgerV1::retain_existing_fresh_session_v1(&fixture.path.join(".")).unwrap();
    let (_, count) = NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
        assert_eq!(
            serde_json::to_vec(&load().unwrap().unwrap()).unwrap(),
            serde_json::to_vec(&config).unwrap()
        );
        assert!(load().unwrap().is_some());
        assert!(load().unwrap().is_some());
    });
    assert_eq!(
        count, 1,
        "one physical unchanged revision is fully validated once"
    );
    for (wrong_pin, wrong_namespace) in [(pin, [4; 32]), ([9; 32], namespace)] {
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(
                &fixture.path,
                wrong_pin,
                wrong_namespace,
            )
            .is_err(),
            "cache is not permission for another genesis or namespace"
        );
    }

    // Same-byte writes still advance the revision. Never use only a head hash
    // or a schema/key shortlist as a proxy for changes to historical data.
    let key = candidate_artifact_key_v1(config.chain_id, &block.header.block_hash);
    let original = fixture.ledger().db.get(&key).unwrap().unwrap();
    fixture.ledger().db.put(&key, &original).unwrap();
    let (_, count) = NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
        load().unwrap();
        load().unwrap();
    });
    assert_eq!(count, 1);

    for replacement in [None, Some(b"corrupt candidate".to_vec())] {
        if let Some(bytes) = &replacement {
            fixture.ledger().db.put(&key, bytes).unwrap();
        } else {
            fixture.ledger().db.delete(&key).unwrap();
        }
        assert!(load().is_err());
        assert!(
            load().is_err(),
            "failed revisions cannot be cached as valid"
        );
        assert_eq!(fixture.ledger().db.get(&key).unwrap(), replacement);
        fixture.ledger().db.put(&key, &original).unwrap();
        load().unwrap();
    }
    fixture
        .ledger()
        .db
        .put(b"unexpected-revision-key", b"invalid")
        .unwrap();
    assert!(
        load().is_err(),
        "unknown writes must invalidate even if the head is unchanged"
    );
    fixture
        .ledger()
        .db
        .delete(b"unexpected-revision-key")
        .unwrap();
    load().unwrap();

    let (_, count) = NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
        NovNativeBlockLedgerV1::audit_fresh_genesis_config_v1(&fixture.path, pin, namespace)
            .unwrap();
        NovNativeBlockLedgerV1::audit_fresh_genesis_config_v1(&fixture.path, pin, namespace)
            .unwrap();
    });
    assert_eq!(
        count, 2,
        "explicit full audits bypass and invalidate the reusable revision"
    );
    load().unwrap();
    drop(session);
    let (_, count) = NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
        load().unwrap();
    });
    assert_eq!(
        count, 0,
        "nested runtime lease retains the same content revision"
    );
    drop(nested);
    let (_, count) = NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
        load().unwrap();
        load().unwrap();
    });
    assert_eq!(
        count, 2,
        "last lease clears the revision even if a DB handle survives"
    );

    let _fresh_session =
        NovNativeBlockLedgerV1::retain_existing_fresh_session_v1(&fixture.path).unwrap();
    let (_, count) = NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
        load().unwrap();
    });
    assert_eq!(count, 1, "a new runtime starts with full verification");
}

#[test]
fn fresh_runtime_revision_cannot_cross_physical_databases() {
    let (first, config, _, _) = fresh_candidate_fixture();
    let (second, _, _, _) = fresh_candidate_fixture();
    let pin = config.compile().unwrap().config_commitment();
    let _first = NovNativeBlockLedgerV1::retain_existing_fresh_session_v1(&first.path).unwrap();
    let _second = NovNativeBlockLedgerV1::retain_existing_fresh_session_v1(&second.path).unwrap();
    let (_, count) = NovNativeBlockLedgerV1::count_fresh_ledger_verifications_for_test_v1(|| {
        NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&first.path, pin, [3; 32]).unwrap();
        NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&second.path, pin, [3; 32]).unwrap();
    });
    assert_eq!(
        count, 2,
        "even identical genesis and sequence need per-owner validation"
    );
    second.ledger().db.put(b"unexpected", b"bad").unwrap();
    assert!(
        NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&second.path, pin, [3; 32]).is_err()
    );
    assert!(
        NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&first.path, pin, [3; 32]).is_ok()
    );
}
