#[test]
fn isolated_candidate_pin_loss_corruption_and_abort_fail_closed() {
    // Synthetic ledger fixture tests storage rules, not AOEM execution.
    let mut test = TestLedgerV1::new("isolated-pins");
    let chain = 71_901;
    let namespace = "11".repeat(32);
    let protocol = "22".repeat(32);
    test.ledger()
        .bind_aoem_ownership(chain, &namespace, &protocol)
        .unwrap();
    let prepared = test
        .ledger()
        .prepare(candidate_input_v1(
            context_v1(chain, 1, [0; 32]),
            [1; 32],
            11,
            1,
        ))
        .unwrap();
    let parent = commit_bound_v1(test.ledger(), &prepared, commit_input_v1(31, 1, 1)).unwrap();
    let input = with_aoem_parent_v1(
        candidate_input_v1(
            context_v1(chain, 2, parent.header.block_hash),
            parent.header.post_state_root,
            12,
            1,
        ),
        &parent,
    );
    let block = build_observed_block_v1(input.clone(), commit_input_v1(32, 1, 2));
    let binding = NovNativeIsolatedExecutionBindingV1 {
        workspace_id: [3; 32],
        plan_commitment: [4; 32],
        output_digest: [5; 32],
    };
    assert!(test
        .ledger()
        .register_isolated_candidate_v1(block.clone(), binding.clone(), "wrong", &protocol)
        .is_err());
    let record = test
        .ledger()
        .register_isolated_candidate_v1(block.clone(), binding.clone(), &namespace, &protocol)
        .unwrap();
    assert!(test.ledger().prepare(input).is_err());
    test.reopen();
    assert_eq!(
        test.ledger().db.get(KEY_SCHEMA_V1).unwrap().unwrap(),
        ISOLATED_LEDGER_SCHEMA_V1.as_bytes()
    );
    assert_eq!(
        test.ledger()
            .load_candidate_record(chain, record.block_hash)
            .unwrap(),
        Some(record.clone())
    );
    let key = format!(
        "{}/isolated-execution-binding",
        candidate_record_key_v1(chain, &record.block_hash)
    );
    let bytes = test.ledger().db.get(key.as_bytes()).unwrap().unwrap();
    test.ledger()
        .db
        .put(KEY_SCHEMA_V1, NOV_NATIVE_BLOCK_LEDGER_SCHEMA_V1.as_bytes())
        .unwrap();
    assert!(test
        .ledger()
        .load_candidate_record(chain, record.block_hash)
        .is_err());
    test.ledger()
        .db
        .put(KEY_SCHEMA_V1, ISOLATED_LEDGER_SCHEMA_V1.as_bytes())
        .unwrap();
    test.ledger().db.delete(key.as_bytes()).unwrap();
    assert!(test
        .ledger()
        .load_candidate_record(chain, record.block_hash)
        .is_err());
    assert!(test
        .ledger()
        .register_isolated_candidate_v1(block.clone(), binding.clone(), &namespace, &protocol)
        .is_err());
    test.ledger().db.put(key.as_bytes(), &bytes).unwrap();
    let record_key = candidate_record_key_v1(chain, &record.block_hash);
    let original = test
        .ledger()
        .db
        .get(record_key.as_bytes())
        .unwrap()
        .unwrap();
    test.ledger().db.delete(record_key.as_bytes()).unwrap();
    assert!(test
        .ledger()
        .load_candidate_record(chain, record.block_hash)
        .is_err());
    test.ledger()
        .db
        .put(record_key.as_bytes(), original)
        .unwrap();
    let mut tampered = record.clone();
    tampered
        .isolated_execution_binding
        .as_mut()
        .unwrap()
        .output_digest[0] ^= 1;
    test.ledger()
        .db
        .put(
            record_key.as_bytes(),
            serde_json::to_vec(&tampered).unwrap(),
        )
        .unwrap();
    assert!(test
        .ledger()
        .load_candidate_record(chain, record.block_hash)
        .is_err());
    test.ledger()
        .db
        .put(record_key.as_bytes(), serde_json::to_vec(&record).unwrap())
        .unwrap();
    test.ledger()
        .abort_unselected_candidate_branch(chain, record.block_hash, "test abort")
        .unwrap();
    assert!(test
        .ledger()
        .register_isolated_candidate_v1(block, binding, &namespace, &protocol)
        .is_err());
    assert_eq!(
        test.ledger().load_head(chain).unwrap().unwrap().block_hash,
        parent.header.block_hash
    );
}
