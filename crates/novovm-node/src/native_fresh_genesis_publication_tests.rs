#[test]
fn candidate_workspace_execution_fresh_genesis_real_aoem_publication_and_retry() {
    use crate::tx_ingress::fresh_genesis::{
        publication::{publish_v1, verify_persisted_v1},
        FreshGenesisConfigV1, GenesisAllocationV1, GenesisValidatorV1, GENESIS_SCHEMA_V1,
    };
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    with_plan_runtime(|path, params| {
        let chain = 98_917_411;
        let namespace = parse_fixed_hex_32_v1(
            &native_aoem_owned_state_namespace_digest_v1(params, chain),
            "namespace",
        )
        .unwrap();
        let config = FreshGenesisConfigV1 {
            schema: GENESIS_SCHEMA_V1.into(),
            chain_id: chain,
            timestamp_unix_ms: 1900000000000,
            protocol_config_commitment: parse_fixed_hex_32_v1(
                &native_business_protocol_config_commitment_v1().unwrap(),
                "protocol",
            )
            .unwrap(),
            allocations: vec![GenesisAllocationV1 {
                account: novovm_adapter_novovm::address_from_seed_v1([0xc3; 32])
                    .try_into()
                    .unwrap(),
                nov: "1000".into(),
            }],
            total_initial_nov: "1000".into(),
            validators: (1..=4)
                .map(|seed| GenesisValidatorV1 {
                    public_key: ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                        .verifying_key()
                        .to_bytes(),
                    weight: 1,
                })
                .collect(),
        };
        let compiled = config.compile().unwrap();
        let pin = compiled.config_commitment();
        let db_path = native_aoem_owned_state_db_path_v1(params);
        assert!(!db_path.exists());
        assert!(publish_v1(chain, pin, params).is_err()); // no manifest, no AOEM DB
        assert!(!db_path.exists());
        let ledger = nov_native_block_ledger_rocksdb_path_v1(path);
        NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(&ledger, &config, pin, namespace)
            .unwrap();
        assert!(verify_persisted_v1(chain, pin, params).is_err());
        assert!(!db_path.exists());
        fs::create_dir(&db_path).unwrap();
        fs::write(db_path.join("occupied-test-data"), b"preserve").unwrap();
        assert!(publish_v1(chain, pin, params).is_err());
        assert_eq!(
            fs::read(db_path.join("occupied-test-data")).unwrap(),
            b"preserve"
        );
        fs::remove_file(db_path.join("occupied-test-data")).unwrap();
        fs::remove_dir(&db_path).unwrap(); // only this test-created, now-empty directory
        let first = publish_v1(chain, pin, params).unwrap();
        assert!(first.aoem_genesis_state_persisted && first.aoem_readback_verified);
        assert_eq!(first.state_root, compiled.state_root());
        assert!(!first.finalized && !first.chain_canonical);
        let second = publish_v1(chain, pin, params).unwrap();
        let verified = verify_persisted_v1(chain, pin, params).unwrap();
        assert_eq!(verified.state_root, compiled.state_root());
        assert!(verified.aoem_readback_verified && !verified.finalized);
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap()
        );
        assert!(!native_host_projection_has_state_v1(
            &load_nov_native_execution_store_v1(path).unwrap()
        ));
        assert!(NovNativeBlockLedgerV1::open(&ledger).is_err());
        assert!(load_validated_native_state_envelope_from_aoem_owner_v1(params, chain).is_err());
        assert!(publish_v1(chain, [9; 32], params).is_err());

        // Real AOEM persistence fault states, not a mocked storage adapter.
        // A completed head with missing data is corruption and must not heal.
        let open_graph = || {
            novovm_exec::AoemSemanticGraphStoreV1::open(
                &native_aoem_owned_runtime_config_v1().unwrap(),
                &db_path,
                &novovm_exec::AoemStorageProviderConfigV1::default(),
            )
            .unwrap()
        };
        let graph = open_graph();
        let head_key = native_aoem_owned_state_head_key_v1(chain, &to_hex(&namespace));
        let head = graph.get(&head_key).unwrap().unwrap();
        assert_eq!(head.len(), 152);
        assert_eq!(&head[..4], b"NVG1");
        let mut chunk_key = b"NVM1GENESIS".to_vec();
        chunk_key.extend_from_slice(&head[108..140]);
        chunk_key.extend_from_slice(&0u32.to_be_bytes());
        let chunk = graph.get(&chunk_key).unwrap().unwrap();
        drop(graph);
        use novovm_exec::{
            AoemAtomicGraphRequestV1, AoemAtomicGraphStepV1, AoemAtomicGraphWriteV1 as Write,
        };
        let change = |id, write, completion| {
            open_graph()
                .commit(AoemAtomicGraphRequestV1 {
                    graph_id: id,
                    steps: vec![AoemAtomicGraphStepV1 {
                        task_kind: 1,
                        task_payload: vec![1],
                        writes: vec![write],
                        event: None,
                    }],
                    completion_write: completion,
                })
                .unwrap();
        };
        change(
            911,
            Write::Delete {
                key: chunk_key.clone(),
            },
            Write::Put {
                key: head_key.clone(),
                value: head.clone(),
            },
        );
        assert!(publish_v1(chain, pin, params)
            .err()
            .unwrap()
            .to_string()
            .contains("image readback incomplete"));
        assert!(open_graph().get(&chunk_key).unwrap().is_none());
        change(
            912,
            Write::Put {
                key: chunk_key.clone(),
                value: chunk,
            },
            Write::Put {
                key: head_key.clone(),
                value: head.clone(),
            },
        );
        // No completed head + exact partial image: replay the pinned publication.
        change(
            913,
            Write::Delete { key: chunk_key },
            Write::Delete {
                key: head_key.clone(),
            },
        );
        assert!(verify_persisted_v1(chain, pin, params)
            .err()
            .unwrap()
            .to_string()
            .contains("completion head is absent"));
        assert!(open_graph().get(&head_key).unwrap().is_none());
        assert!(
            publish_v1(chain, pin, params)
                .unwrap()
                .aoem_readback_verified
        );
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), head);
        let mut claim_path = db_path.as_os_str().to_os_string();
        claim_path.push(".fresh-genesis-claim-v1");
        let claim_path = PathBuf::from(claim_path);
        let claim = fs::read(&claim_path).unwrap();
        fs::write(&claim_path, b"wrong owner").unwrap();
        assert!(publish_v1(chain, pin, params).is_err());
        fs::write(&claim_path, claim).unwrap();
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), head);

        // First signed transaction executes from real genesis, never a fake
        // transaction-parent envelope. Authority stays at the genesis image.
        let plan = make_plan(
            NovBlockExecutionContextV1 {
                chain_id: chain,
                block_height: 1,
                parent_block_hash: [0; 32],
                slot: 1,
                timestamp_unix_ms: config.timestamp_unix_ms,
            },
            compiled.state_root(),
            None,
            vec![candidate_workspace_execution_raw(
                chain,
                0,
                [0xc3; 32],
                10,
                "deposit_reserve",
            )],
        );
        assert!(workspace::create_from_genesis_v1(&plan, [9; 32], params).is_err());
        let mut bad_head = head.clone();
        bad_head[140..148].copy_from_slice(&u64::MAX.to_be_bytes());
        change(
            914,
            Write::Put {
                key: head_key.clone(),
                value: bad_head.clone(),
            },
            Write::Put {
                key: head_key.clone(),
                value: bad_head,
            },
        );
        assert!(workspace::create_from_genesis_v1(&plan, pin, params).is_err());
        change(
            915,
            Write::Put {
                key: head_key.clone(),
                value: head.clone(),
            },
            Write::Put {
                key: head_key.clone(),
                value: head.clone(),
            },
        );
        let wrong_root = make_plan(plan.context, [9; 32], None, plan.raw_txs.clone());
        assert!(workspace::create_from_genesis_v1(&wrong_root, pin, params).is_err());
        let mut too_early = plan.context;
        too_early.timestamp_unix_ms -= 1;
        let too_early = make_plan(too_early, compiled.state_root(), None, plan.raw_txs.clone());
        assert!(workspace::create_from_genesis_v1(&too_early, pin, params).is_err());
        let input = workspace::create_from_genesis_v1(&plan, pin, params).unwrap();
        assert_eq!(input.parent_block_hash, [0; 32]);
        let result = workspace::execute_v1(chain, input.workspace_id, params).unwrap();
        assert_candidate_workspace_execution_complete(&result);
        assert_eq!(result.batch_result.per_tx_receipts.len(), 1);
        assert!(result.batch_result.per_tx_receipts[0].status_ok);
        assert_eq!(result.batch_result.snapshot_metadata.state_version, 1);
        let block = workspace::load_block_artifact_v1(chain, input.workspace_id, params)
            .unwrap()
            .unwrap();
        assert_eq!(block.block().header.height, 1);
        assert_eq!(block.block().header.pre_state_root, compiled.state_root());
        assert!(block.block().header.aoem_parent.is_none());
        assert!(!block.block().header.finalized);
        assert_eq!(block.fresh_genesis_identity(), Some(&compiled.identity()));
        assert_ne!(
            compiled.identity().anchor(),
            block.block().header.block_hash
        );
        // Competing first proposals must share one chain identity. Never derive
        // it from either proposal's block hash or candidate-local state output.
        let mut competing_context = plan.context;
        competing_context.slot += 1;
        let competing_plan = make_plan(
            competing_context,
            compiled.state_root(),
            None,
            plan.raw_txs.clone(),
        );
        let competing = workspace::create_from_genesis_v1(&competing_plan, pin, params).unwrap();
        let competing_result =
            workspace::execute_v1(chain, competing.workspace_id, params).unwrap();
        assert_candidate_workspace_execution_complete(&competing_result);
        assert!(competing_result.batch_result.per_tx_receipts[0].status_ok);
        let competing_block =
            workspace::load_block_artifact_v1(chain, competing.workspace_id, params)
                .unwrap()
                .unwrap();
        assert_ne!(
            competing_block.block().header.block_hash,
            block.block().header.block_hash
        );
        assert_eq!(
            competing_block.fresh_genesis_identity(),
            block.fresh_genesis_identity()
        );
        assert!(!competing_block.block().header.finalized);
        assert!(
            workspace::register_block_candidate_v1(chain, competing.workspace_id, params).is_err()
        );
        assert!(workspace::register_block_candidate_v1(chain, input.workspace_id, params).is_err());
        change(
            919,
            Write::Delete {
                key: head_key.clone(),
            },
            Write::Delete {
                key: head_key.clone(),
            },
        );
        assert!(workspace::register_genesis_block_candidate_v1(
            chain,
            input.workspace_id,
            pin,
            params
        )
        .is_err());
        assert!(open_graph().get(&head_key).unwrap().is_none());
        change(
            920,
            Write::Put {
                key: head_key.clone(),
                value: head.clone(),
            },
            Write::Put {
                key: head_key.clone(),
                value: head.clone(),
            },
        );
        assert!(workspace::register_genesis_block_candidate_v1(
            chain,
            input.workspace_id,
            [9; 32],
            params
        )
        .is_err());
        let registered =
            workspace::register_genesis_block_candidate_v1(chain, input.workspace_id, pin, params)
                .unwrap();
        let other_registered = workspace::register_genesis_block_candidate_v1(
            chain,
            competing.workspace_id,
            pin,
            params,
        )
        .unwrap();
        for record in [&registered, &other_registered] {
            assert_eq!(record.candidate_source, "local_aoem_isolated_execution");
            assert!(!record.execution_selected_local);
            assert!(!record.chain_canonical && !record.finalized && !record.proof_sealed);
        }
        assert_eq!(
            workspace::register_genesis_block_candidate_v1(chain, input.workspace_id, pin, params)
                .unwrap(),
            registered
        );
        assert!(NovNativeBlockLedgerV1::open(&ledger).is_err());
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_config_v1(&ledger, pin, namespace)
                .unwrap()
                .is_some()
        );
        assert!(
            verify_persisted_v1(chain, pin, params)
                .unwrap()
                .aoem_readback_verified
        );
        assert_eq!(
            workspace::create_from_genesis_v1(&plan, pin, params)
                .unwrap()
                .workspace_id,
            input.workspace_id
        );
        assert!(workspace::create_from_genesis_v1(&plan, [9; 32], params).is_err());
        assert!(workspace::create_v1(&plan, params).is_err());
        let restored = workspace::execute_v1(chain, input.workspace_id, params).unwrap();
        assert_eq!(restored.output_digest, result.output_digest);
        assert_eq!(
            workspace::load_block_artifact_v1(chain, input.workspace_id, params)
                .unwrap()
                .unwrap()
                .fresh_genesis_identity(),
            Some(&compiled.identity())
        );
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), head);
        assert!(!native_host_projection_has_state_v1(
            &load_nov_native_execution_store_v1(path).unwrap()
        ));
        exercise_fresh_genesis_signing(
            path,
            params,
            &compiled,
            input.workspace_id,
            competing.workspace_id,
        );
        workspace::abort_v1(chain, competing.workspace_id, params).unwrap();
        assert!(workspace::with_verified_genesis_block_candidate_v1(
            chain,
            competing.workspace_id,
            pin,
            params,
            |_| Ok(())
        )
        .is_err());
        assert!(workspace::register_genesis_block_candidate_v1(
            chain,
            competing.workspace_id,
            pin,
            params
        )
        .is_err());
        assert_eq!(
            workspace::register_genesis_block_candidate_v1(chain, input.workspace_id, pin, params)
                .unwrap(),
            registered
        );
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), head);
    });
}

include!("native_fresh_genesis_signing_tests.rs");
