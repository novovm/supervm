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
                account: [7; 20],
                nov: "123".into(),
            }],
            total_initial_nov: "123".into(),
            validators: vec![GenesisValidatorV1 {
                public_key: ed25519_dalek::SigningKey::from_bytes(&[1; 32])
                    .verifying_key()
                    .to_bytes(),
                weight: 1,
            }],
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
    });
}
