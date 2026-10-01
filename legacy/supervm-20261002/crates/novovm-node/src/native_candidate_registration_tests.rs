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
        workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params, |view| {
            assert_eq!(view.load_seal_eligible_local_candidate_v1(chain, record.block_hash)?.0, record);
            assert!(view.load_seal_eligible_local_candidate_v1(chain, [0x73; 32]).is_err());
            assert!(view.abort_unselected_candidate_branch(chain, record.block_hash, "forbidden").is_err());
            Ok(())
        }).unwrap();
        assert!(ledger.load_seal_eligible_local_candidate_v1(chain, record.block_hash).is_err());
        use crate::native_block_seal::{NovNativeBlockSealStoreV1, NovNativeSealValidatorV1,
            NovNativeSealValidatorSetV1, NovNativeSealLocalProposalRequestV1};
        let key = ed25519_dalek::SigningKey::from_bytes(&[0xd4; 32]);
        let set = NovNativeSealValidatorSetV1::new(chain, 1, 1, vec![
            NovNativeSealValidatorV1::new(key.verifying_key().to_bytes(), 1).unwrap()
        ]).unwrap();
        let seal = NovNativeBlockSealStoreV1::open(&path.with_extension("isolated-seal-test")).unwrap();
        let parent_request = NovNativeSealLocalProposalRequestV1 {
            chain_id: chain, block_hash: record.parent_block_hash, round: 0, justify_qc_hash: None,
        };
        let parent_proposal = seal.sign_local_proposal(&ledger, &parent_request, &set, &key).unwrap();
        let parent_vote = seal.sign_local_vote(&ledger, &parent_proposal, &set, &key).unwrap();
        let parent_qc = crate::native_block_seal::NovNativeSealQuorumCertificateV1::from_votes(
            parent_proposal.subject, &set, vec![parent_vote]).unwrap();
        seal.persist_local_verified_qc(&ledger, &parent_qc, &set).unwrap();
        let request = NovNativeSealLocalProposalRequestV1 {
            chain_id: chain, block_hash: record.block_hash, round: 0, justify_qc_hash: Some(parent_qc.qc_hash),
        };
        let mut no_justify = request.clone();
        no_justify.justify_qc_hash = None;
        assert!(workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params,
            |view| seal.sign_local_proposal(view, &no_justify, &set, &key)).is_err());
        assert!(seal.sign_local_proposal(&ledger, &request, &set, &key).is_err());
        let proposal = workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params,
            |view| seal.sign_local_proposal(view, &request, &set, &key)).unwrap();
        let vote = workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params,
            |view| seal.sign_local_vote(view, &proposal, &set, &key)).unwrap();
        assert!(seal.sign_local_vote(&ledger, &proposal, &set, &key).is_err());
        assert_eq!(workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params,
            |view| seal.sign_local_vote(view, &proposal, &set, &key)).unwrap(), vote);
        let candidate_qc = crate::native_block_seal::NovNativeSealQuorumCertificateV1::from_votes(
            proposal.subject.clone(), &set, vec![vote]).unwrap();
        workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params,
            |view| seal.persist_local_verified_qc(view, &candidate_qc, &set)).unwrap();
        let decision = workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params,
            |view| seal.sign_local_decision_vote_v3(view, &candidate_qc, &set, &key)).unwrap();
        assert!(seal.sign_local_decision_vote_v3(&ledger, &candidate_qc, &set, &key).is_err());
        assert_eq!(workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params,
            |view| seal.sign_local_decision_vote_v3(view, &candidate_qc, &set, &key)).unwrap(), decision);
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
        drop(seal);
        let seal = NovNativeBlockSealStoreV1::open(&path.with_extension("isolated-seal-test")).unwrap();
        assert_eq!(workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params,
            |view| seal.sign_local_decision_vote_v3(view, &candidate_qc, &set, &key)).unwrap(), decision);
        assert!(seal.sign_local_decision_vote_v3(&ledger, &candidate_qc, &set, &key).is_err());
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
        assert!(workspace::with_verified_block_candidate_v1::<()>(chain, input.workspace_id, params, |_| {
            panic!("aborted candidate must not enter signing callback")
        }).is_err());
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

#[test]
fn candidate_workspace_execution_registration_live_scope_rejects_stale_parent_and_aborted_workspace() {
    let _guard = PLAN_RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for abort_workspace in [false, true] {
        let chain = 98_917_308 + u64::from(abort_workspace);
        with_plan_runtime(|path, params| {
            let parent = funded_candidate_parent(path, params, chain, &[[0xc2; 32]], raw_fixture(chain, 608));
            let plan = successor_plan(&parent, vec![candidate_workspace_execution_raw(
                chain, 0, [0xc2; 32], 31, "deposit_reserve")]);
            let input = workspace::create_v1(&plan, params).unwrap();
            workspace::execute_v1(chain, input.workspace_id, params).unwrap();
            let record = workspace::register_block_candidate_v1(chain, input.workspace_id, params).unwrap();
            workspace::with_verified_block_candidate_v1(chain, input.workspace_id, params, |_| Ok(())).unwrap();
            if abort_workspace {
                workspace::abort_v1(chain, input.workspace_id, params).unwrap();
            } else {
                let competing = successor_plan(&parent, vec![candidate_workspace_execution_raw(
                    chain, 0, [0xc2; 32], 32, "deposit_reserve")]);
                run_nov_native_candidate_execution_plan_v1(&competing, params).unwrap();
            }
            let before = candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
            assert!(workspace::with_verified_block_candidate_v1::<()>(chain, input.workspace_id, params,
                |_| panic!("invalid live evidence must never reach signing")).is_err());
            let ledger = NovNativeBlockLedgerV1::open(&nov_native_block_ledger_rocksdb_path_v1(path)).unwrap();
            assert_eq!(ledger.load_candidate_record(chain, record.block_hash).unwrap(), Some(record));
            assert_eq!(candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes), before);
        });
    }
}
