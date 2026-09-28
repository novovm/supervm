#[test]
fn candidate_workspace_execution_registration_is_durable_unselected_and_fenced() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_917_307;
    with_plan_runtime(|path, params| {
        let parent =
            funded_candidate_parent(path, params, chain, &[[0xc1; 32]], raw_fixture(chain, 607));
        let plan = successor_plan(
            &parent,
            vec![candidate_workspace_execution_raw(
                chain,
                0,
                [0xc1; 32],
                31,
                "deposit_reserve",
            )],
        );
        let input = workspace::create_v1(&plan, params).unwrap();
        assert!(workspace::register_block_candidate_v1(chain, input.workspace_id, params).is_err());
        workspace::execute_v1(chain, input.workspace_id, params).unwrap();
        let artifact = workspace::load_block_artifact_v1(chain, input.workspace_id, params)
            .unwrap()
            .unwrap();
        let ledger_path = nov_native_block_ledger_rocksdb_path_v1(path);
        let ledger = NovNativeBlockLedgerV1::open(&ledger_path).unwrap();
        let observed = ledger
            .register_observed_unsealed_candidate(artifact.block().clone())
            .unwrap();
        assert!(!observed.local_aoem_readback_verified);
        let mut before =
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
        let record =
            workspace::register_block_candidate_v1(chain, input.workspace_id, params).unwrap();
        assert_eq!(record.candidate_source, "local_aoem_isolated_execution");
        assert!(record.local_aoem_readback_verified);
        assert!(!record.execution_selected_local && !record.chain_canonical && !record.finalized);
        assert_eq!(record.revision, observed.revision + 1);
        assert_eq!(
            record
                .isolated_execution_binding
                .as_ref()
                .unwrap()
                .output_digest,
            artifact.output_digest
        );
        assert_eq!(
            ledger
                .load_candidate_block(chain, record.block_hash)
                .unwrap()
                .unwrap(),
            *artifact.block()
        );
        assert!(ledger
            .load_seal_eligible_local_candidate_v1(chain, record.block_hash)
            .is_err());
        assert_eq!(
            workspace::register_block_candidate_v1(chain, input.workspace_id, params).unwrap(),
            record
        );
        assert!(ledger
            .prepare(NovNativeBlockCandidateInputV1 {
                context: plan.context,
                tx_hashes: plan.tx_hashes.clone(),
                raw_txs: plan.raw_txs.clone(),
                pre_state_root: plan.pre_state_root,
                aoem_parent: plan.aoem_parent.clone(),
            })
            .is_err());
        assert!(ledger.load_prepared(chain).unwrap().is_none());
        assert!(run_nov_native_candidate_execution_plan_v1(&plan, params).is_err(),
            "registered isolated plan must not advance the old authority path");
        let mut after =
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
        // Only the candidate graph changes. AOEM authority bytes, ledger head,
        // tx/receipt indexes, pending and nonce reservations remain identical.
        for value in [&mut before, &mut after] {
            value.as_object_mut().unwrap().remove("ledger_candidates");
            value.as_object_mut().unwrap().remove("ledger_children");
        }
        assert_eq!(after, before);
        drop(ledger);
        reset_native_aoem_semantic_ingress_session_v1();
        let ledger = NovNativeBlockLedgerV1::open(&ledger_path).unwrap();
        assert_eq!(
            ledger
                .load_candidate_record(chain, record.block_hash)
                .unwrap(),
            Some(record.clone())
        );
        assert_eq!(
            workspace::register_block_candidate_v1(chain, input.workspace_id, params).unwrap(),
            record
        );
        let ownership = ledger.load_aoem_ownership().unwrap().unwrap();
        let mut wrong = record.isolated_execution_binding.clone().unwrap();
        wrong.output_digest[0] ^= 1;
        assert!(ledger
            .register_isolated_candidate_v1(
                artifact.block().clone(),
                wrong,
                &ownership.namespace_digest,
                &ownership.protocol_config_commitment
            )
            .is_err());
        let aborted = ledger
            .abort_unselected_candidate_branch(chain, record.block_hash, "test cancellation")
            .unwrap();
        assert_eq!(aborted[0].lifecycle_status, "aborted_unsealed");
        assert!(workspace::register_block_candidate_v1(chain, input.workspace_id, params).is_err());
        workspace::abort_v1(chain, input.workspace_id, params).unwrap();
        assert!(workspace::register_block_candidate_v1(chain, input.workspace_id, params).is_err());
        let mut after_abort =
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
        after_abort
            .as_object_mut()
            .unwrap()
            .remove("ledger_candidates");
        after_abort
            .as_object_mut()
            .unwrap()
            .remove("ledger_children");
        assert_eq!(after_abort, before);
    });
}
