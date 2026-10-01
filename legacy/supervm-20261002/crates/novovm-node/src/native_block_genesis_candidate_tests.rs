fn fresh_candidate_fixture() -> (
    TestLedgerV1,
    crate::tx_ingress::fresh_genesis::FreshGenesisConfigV1,
    NovNativeDurableBlockV1,
    NovNativeIsolatedExecutionBindingV1,
) {
    use crate::tx_ingress::fresh_genesis::{
        FreshGenesisConfigV1, GenesisValidatorV1, GENESIS_SCHEMA_V1,
    };
    let test = TestLedgerV1::new("fresh-candidate");
    let config = FreshGenesisConfigV1 {
        schema: GENESIS_SCHEMA_V1.into(),
        chain_id: 970041,
        timestamp_unix_ms: 1_900_000_000_000,
        protocol_config_commitment: [1; 32],
        allocations: vec![],
        total_initial_nov: "0".into(),
        validators: vec![GenesisValidatorV1 {
            public_key: ed25519_dalek::SigningKey::from_bytes(&[1; 32])
                .verifying_key()
                .to_bytes(),
            weight: 1,
        }],
    };
    let compiled = config.compile().unwrap();
    NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
        &test.path,
        &config,
        compiled.config_commitment(),
        [3; 32],
    )
    .unwrap();
    let block = build_observed_block_v1(
        candidate_input_v1(
            context_v1(config.chain_id, 1, [0; 32]),
            compiled.state_root(),
            4,
            1,
        ),
        commit_input_v1(4, 1, 1),
    );
    (
        test,
        config,
        block,
        NovNativeIsolatedExecutionBindingV1 {
            workspace_id: [4; 32],
            plan_commitment: [5; 32],
            output_digest: [6; 32],
        },
    )
}

#[test]
fn fresh_genesis_candidates_compete_without_selection_and_recover() {
    let (mut test, config, block, binding) = fresh_candidate_fixture();
    let pin = config.compile().unwrap().config_commitment();
    let register = |block, binding| {
        NovNativeBlockLedgerV1::register_fresh_genesis_candidate_v1(
            &test.path, pin, [3; 32], block, binding,
        )
    };
    assert!(NovNativeBlockLedgerV1::register_fresh_genesis_candidate_v1(
        &test.path,
        [9; 32],
        [3; 32],
        block.clone(),
        binding.clone(),
    )
    .is_err());
    let first = register(block.clone(), binding.clone()).unwrap();
    assert_eq!(register(block.clone(), binding.clone()).unwrap(), first);
    let mut other_binding = binding.clone();
    other_binding.output_digest = [9; 32];
    assert!(register(block.clone(), other_binding.clone()).is_err());
    let other = build_observed_block_v1(
        candidate_input_v1(
            context_v1(config.chain_id, 1, [0; 32]),
            block.header.pre_state_root,
            8,
            1,
        ),
        commit_input_v1(8, 1, 1),
    );
    let second = register(other, other_binding).unwrap();
    assert_ne!(first.block_hash, second.block_hash);
    assert!(!first.execution_selected_local && !second.execution_selected_local);
    assert!(test
        .ledger()
        .load_head_inner_v1(config.chain_id)
        .unwrap()
        .is_none());
    assert!(test
        .ledger()
        .load_aoem_ownership_inner_v1()
        .unwrap()
        .is_none());
    assert!(test
        .ledger()
        .load_by_height_inner_v1(config.chain_id, 1)
        .unwrap()
        .is_none());
    assert!(test.ledger().load_head(config.chain_id).is_err());
    assert!(NovNativeBlockLedgerV1::open(&test.path).is_err());
    test.ledger.take();
    assert!(
        NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&test.path, pin, [3; 32])
            .unwrap()
            .is_some()
    );
    assert_eq!(
        NovNativeBlockLedgerV1::register_fresh_genesis_candidate_v1(
            &test.path, pin, [3; 32], block, binding,
        )
        .unwrap(),
        first
    );
}

#[test]
fn fresh_genesis_candidates_reject_missing_or_downgraded_evidence_without_repair() {
    let (test, config, block, binding) = fresh_candidate_fixture();
    let pin = config.compile().unwrap().config_commitment();
    let register = || {
        NovNativeBlockLedgerV1::register_fresh_genesis_candidate_v1(
            &test.path,
            pin,
            [3; 32],
            block.clone(),
            binding.clone(),
        )
    };
    register().unwrap();
    let hash = block.header.block_hash;
    let chain = config.chain_id;
    let db = &test.ledger().db;
    for key in [
        isolated_candidate::pin_key(chain, &hash).into_bytes(),
        candidate_record_key_v1(chain, &hash).into_bytes(),
        candidate_artifact_key_v1(chain, &hash).into_bytes(),
        candidate_height_index_key_v1(chain, 1).into_bytes(),
        candidate_children_index_key_v1(chain, &[0; 32]).into_bytes(),
        KEY_CANDIDATE_GRAPH_SCHEMA_V1.to_vec(),
    ] {
        let original = db.get(&key).unwrap().unwrap();
        db.delete(&key).unwrap();
        assert!(register().is_err());
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&test.path, pin, [3; 32]).is_err()
        );
        assert!(db.get(&key).unwrap().is_none());
        db.put(&key, original).unwrap();
    }
    let schema = db.get(KEY_SCHEMA_V1).unwrap().unwrap();
    for old in [
        NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1,
        ISOLATED_LEDGER_SCHEMA_V1,
        "novovm-native-block-ledger/v1+genesis-manifest-reserved-v1",
    ] {
        db.put(KEY_SCHEMA_V1, old.as_bytes()).unwrap();
        assert!(register().is_err());
        assert!(NovNativeBlockLedgerV1::open(&test.path).is_err());
        assert_eq!(db.get(KEY_SCHEMA_V1).unwrap().unwrap(), old.as_bytes());
    }
    db.put(KEY_SCHEMA_V1, schema).unwrap();
    db.put(b"unexpected-selected-state", b"preserved").unwrap();
    assert!(register().is_err());
    assert_eq!(
        db.get(b"unexpected-selected-state").unwrap().unwrap(),
        b"preserved"
    );
}

#[test]
fn fresh_genesis_candidates_reject_wrong_parent_domain_before_writing() {
    let (test, config, block, binding) = fresh_candidate_fixture();
    let compiled = config.compile().unwrap();
    let pin = compiled.config_commitment();
    assert!(NovNativeBlockLedgerV1::register_fresh_genesis_candidate_v1(
        &test.path,
        pin,
        [9; 32],
        block,
        binding.clone(),
    )
    .is_err());
    for case in 0..4 {
        let mut context = context_v1(config.chain_id, 1, [0; 32]);
        let mut root = compiled.state_root();
        let mut version = 1;
        match case {
            0 => context.chain_id += 1,
            1 => context.timestamp_unix_ms = config.timestamp_unix_ms - 1,
            2 => root = [9; 32],
            _ => version = 2,
        }
        let invalid = build_observed_block_v1(
            candidate_input_v1(context, root, 4, 1),
            commit_input_v1(4, 1, version),
        );
        assert!(NovNativeBlockLedgerV1::register_fresh_genesis_candidate_v1(
            &test.path,
            pin,
            [3; 32],
            invalid,
            binding.clone(),
        )
        .is_err());
        assert!(test
            .ledger()
            .db
            .get(KEY_CANDIDATE_GRAPH_SCHEMA_V1)
            .unwrap()
            .is_none());
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&test.path, pin, [3; 32])
                .unwrap()
                .is_some()
        );
    }
}
