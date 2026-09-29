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
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(&ledger, pin, namespace)
                .unwrap()
                .is_none()
        );
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
            vec![
                candidate_workspace_execution_raw(chain, 0, [0xc3; 32], 10, "deposit_reserve"),
                candidate_workspace_execution_raw(chain, 1, [0xc3; 32], 10, "deposit_reserve"),
            ],
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
        assert_eq!(result.batch_result.per_tx_receipts.len(), 2);
        assert!(result
            .batch_result
            .per_tx_receipts
            .iter()
            .all(|receipt| receipt.status_ok));
        assert_eq!(result.batch_result.snapshot_metadata.state_version, 2);
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
        exercise_fresh_genesis_service(path, params, &compiled, competing.workspace_id);
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
        let seal_path = path.with_extension("genesis-seal-0");
        assert!(
            workspace::publish_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .is_err()
        );
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), head);
        let intent = workspace::prepare_genesis_promotion_v1(
            chain,
            input.workspace_id,
            pin,
            &seal_path,
            params,
        )
        .unwrap();
        let replay = workspace::prepare_genesis_promotion_v1(
            chain,
            input.workspace_id,
            pin,
            &seal_path,
            params,
        )
        .unwrap();
        assert_eq!(intent, replay);
        assert_eq!(intent.commitment().unwrap(), replay.commitment().unwrap());
        assert!(workspace::abort_v1(chain, input.workspace_id, params).is_err());
        assert!(workspace::with_verified_genesis_block_candidate_v1(
            chain,
            input.workspace_id,
            pin,
            params,
            |_| Ok(())
        )
        .is_err());
        assert!(workspace::register_genesis_block_candidate_v1(
            chain,
            input.workspace_id,
            pin,
            params
        )
        .is_err());
        assert!(NovNativeBlockLedgerV1::open(&ledger).is_err());
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), head);
        assert!(
            verify_persisted_v1(chain, pin, params)
                .unwrap()
                .aoem_readback_verified
        );
        // A missing/corrupt independent journal pin must never be repaired.
        let pin_key = b"native_block_ledger/v1/genesis/promotion-pin";
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        let saved = db.get(pin_key).unwrap().unwrap();
        db.put(pin_key, [0u8; 32]).unwrap();
        drop(db);
        assert!(workspace::prepare_genesis_promotion_v1(
            chain,
            input.workspace_id,
            pin,
            &seal_path,
            params
        )
        .is_err());
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        assert_eq!(db.get(pin_key).unwrap().unwrap(), vec![0u8; 32]);
        db.put(pin_key, saved).unwrap();
        drop(db);
        assert_eq!(
            workspace::prepare_genesis_promotion_v1(
                chain,
                input.workspace_id,
                pin,
                &seal_path,
                params
            )
            .unwrap(),
            intent
        );
        use workspace::PromotionCheckpointV1 as Point;
        assert!(
            workspace::verify_genesis_promotion_v1(chain, input.workspace_id, pin, params).is_err()
        );
        assert!(workspace::publish_with_checkpoint_v1(
            chain,
            input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::BeforePublication {
                    anyhow::bail!("injected before publication");
                }
                Ok(())
            }
        )
        .is_err());
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), head);
        assert!(workspace::publish_with_checkpoint_v1(
            chain,
            input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::AfterPublication {
                    anyhow::bail!("injected lost success response");
                }
                Ok(())
            }
        )
        .is_err());
        let promoted_head = open_graph().get(&head_key).unwrap().unwrap();
        assert_eq!(&promoted_head[..4], b"NVP1");
        let published =
            workspace::publish_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .unwrap();
        assert!(published.aoem_authority_published && published.aoem_readback_verified);
        assert!(!published.ledger_publication_completed && !published.finalized);
        assert_eq!(published.state_root, block.block().header.post_state_root);
        assert_eq!(published.intent_commitment, intent.commitment().unwrap());
        assert_eq!(
            workspace::verify_genesis_promotion_v1(chain, input.workspace_id, pin, params).unwrap(),
            published
        );
        assert_eq!(
            workspace::publish_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .unwrap(),
            published
        );
        assert_eq!(
            workspace::load_execution_v1(chain, input.workspace_id, params)
                .unwrap()
                .unwrap()
                .output_digest,
            result.output_digest
        );
        assert!(workspace::abort_v1(chain, input.workspace_id, params).is_err());
        assert!(load_validated_native_state_envelope_from_aoem_owner_v1(params, chain).is_err());
        assert!(verify_persisted_v1(chain, pin, params).is_err());
        assert!(workspace::complete_with_checkpoint_v1(
            chain,
            input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::BeforeLedgerCommit {
                    anyhow::bail!("before ledger publication");
                }
                Ok(())
            }
        )
        .is_err());
        assert!(
            !workspace::verify_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .unwrap()
                .ledger_publication_completed
        );
        assert!(workspace::complete_with_checkpoint_v1(
            chain,
            input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::AfterLedgerCommit {
                    anyhow::bail!("lost ledger success response");
                }
                Ok(())
            }
        )
        .is_err());
        let completed =
            workspace::complete_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .unwrap();
        assert!(completed.ledger_publication_completed);
        assert!(!completed.finalized);
        assert!(workspace::load_finalized_genesis_parent_v1(
            chain,
            input.workspace_id,
            pin,
            params
        )
        .is_err());
        assert_eq!(
            completed,
            workspace::resume_genesis_promotion_v1(
                chain,
                input.workspace_id,
                pin,
                &seal_path,
                &ledger,
                params
            )
            .unwrap()
        );
        let wrong_ledger = path.with_extension("unrelated-ledger");
        assert!(workspace::resume_genesis_promotion_v1(
            chain,
            input.workspace_id,
            pin,
            &seal_path,
            &wrong_ledger,
            params
        )
        .is_err());
        assert!(!wrong_ledger.exists());
        assert_eq!(
            NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(&ledger, pin, namespace)
                .unwrap()
                .as_ref(),
            Some(block.block())
        );
        assert_eq!(
            completed,
            workspace::verify_genesis_promotion_v1(chain, input.workspace_id, pin, params).unwrap()
        );
        assert_eq!(
            completed,
            workspace::complete_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .unwrap()
        );
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), promoted_head);
        assert!(workspace::abort_v1(chain, input.workspace_id, params).is_err());
        assert!(NovNativeBlockLedgerV1::open(&ledger).is_err());
        // Committed indexes are authoritative: retries must not rebuild a lost one.
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        let (index_key, index_value) = db
            .iterator(rocksdb::IteratorMode::Start)
            .map(|entry| entry.unwrap())
            .find(|(key, _)| String::from_utf8_lossy(key).contains("/tx/"))
            .expect("published transaction index");
        db.delete(&index_key).unwrap();
        drop(db);
        assert!(
            workspace::complete_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .is_err()
        );
        assert!(
            workspace::verify_genesis_promotion_v1(chain, input.workspace_id, pin, params).is_err()
        );
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(&ledger, pin, namespace)
                .is_err()
        );
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        assert!(db.get(&index_key).unwrap().is_none());
        db.put(&index_key, &index_value).unwrap(); // Explicit fixture restoration only.
        drop(db);
        assert_eq!(
            completed,
            workspace::complete_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .unwrap()
        );
        assert!(workspace::register_genesis_block_candidate_v1(
            chain,
            input.workspace_id,
            pin,
            params
        )
        .is_err());
        use crate::native_block_ledger::NovNativeFreshFinalityProofV1;
        use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
        use crate::native_block_seal_overlay::{
            NovNativeSealEpochAuthorityV1, NovNativeSealValidatorTransportBindingV1,
        };
        let authority = NovNativeSealEpochAuthorityV1::derive_operator_pinned_fresh_genesis_epoch(
            &config,
            pin,
            compiled
                .validator_set()
                .validators
                .iter()
                .map(|v| NovNativeSealValidatorTransportBindingV1 {
                    validator_id: v.validator_id,
                    transport_peer_id: novovm_network::peer_id_from_ed25519_public_key_v1(
                        &v.public_key,
                    ),
                })
                .collect(),
        )
        .unwrap();
        let store = crate::native_block_seal::NovNativeBlockSealStoreV1::open_existing_read_only(
            &seal_path,
        )
        .unwrap()
        .unwrap();
        let proposal = store
            .load_proposal(intent.decision.prepare.proposal_hash)
            .unwrap()
            .unwrap();
        let proof = NovNativeFreshFinalityProofV1 {
            authority,
            witness: Message::DecisionCertificateV3 {
                proposal: Box::new(proposal),
                decision: Box::new(intent.decision.clone()),
                certificate: None,
            },
        };
        drop(store);
        assert!(
            NovNativeBlockLedgerV1::load_fresh_genesis_finality_v1(&ledger, pin, namespace)
                .unwrap()
                .is_none()
        );
        let mut invalid = proof.clone();
        if let Message::DecisionCertificateV3 { decision, .. } = &mut invalid.witness {
            decision.votes.truncate(2);
        }
        assert!(workspace::finalize_genesis_promotion_v1(
            chain,
            input.workspace_id,
            pin,
            &invalid,
            params
        )
        .is_err());
        let mut invalid = proof.clone();
        invalid.authority.genesis_block_hash[0] ^= 1;
        assert!(workspace::finalize_genesis_promotion_v1(
            chain,
            input.workspace_id,
            pin,
            &invalid,
            params
        )
        .is_err());
        assert!(
            !workspace::verify_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .unwrap()
                .finalized
        );
        let finalized = workspace::finalize_genesis_promotion_v1(
            chain,
            input.workspace_id,
            pin,
            &proof,
            params,
        )
        .unwrap();
        assert!(finalized.finalized && finalized.ledger_publication_completed);
        proof
            .validate_archived_block(&config, block.block())
            .unwrap();
        let mut changed_parent = block.block().clone();
        changed_parent.header.post_state_root = [0xa5; 32];
        assert!(proof
            .validate_archived_block(&config, &changed_parent)
            .is_err());
        let mut changed_genesis = config.clone();
        changed_genesis.timestamp_unix_ms += 1;
        assert!(proof
            .validate_archived_block(&changed_genesis, block.block())
            .is_err());
        let parent =
            workspace::load_finalized_genesis_parent_v1(chain, input.workspace_id, pin, params)
                .unwrap();
        assert_eq!(parent.block(), block.block());
        assert_eq!(parent.workspace_id(), input.workspace_id);
        assert_eq!(parent.output_digest(), result.output_digest);
        assert_eq!(parent.batch_result(), &result.batch_result);
        assert_eq!(parent.finality_proof(), &proof);
        assert_eq!(
            serde_json::to_value(parent.genesis_config()).unwrap(),
            serde_json::to_value(&config).unwrap()
        );
        assert_eq!(
            serde_json::to_value(parent.state()).unwrap(),
            workspace::load_execution_snapshot_for_test_v1(chain, input.workspace_id, params)
                .unwrap()
        );
        let next_context = NovBlockExecutionContextV1 {
            chain_id: chain,
            block_height: 2,
            parent_block_hash: block.block().header.block_hash,
            slot: 2,
            timestamp_unix_ms: config.timestamp_unix_ms + 1,
        };
        let next_raw =
            candidate_workspace_execution_raw(chain, 2, [0xc3; 32], 10, "deposit_reserve");
        let next_plan = parent
            .successor_plan(next_context, vec![next_raw.clone()], params)
            .unwrap();
        assert_eq!(
            next_plan.pre_state_root,
            block.block().header.post_state_root
        );
        assert_eq!(next_plan.aoem_parent.as_ref().unwrap().state_version, 2);
        assert_eq!(
            next_plan.aoem_parent.as_ref().unwrap().batch_result_id,
            result.batch_result.batch_result_id
        );
        assert_eq!(
            next_plan,
            parent
                .successor_plan(next_context, vec![next_raw.clone()], params)
                .unwrap()
        );
        assert!(parent
            .successor_plan(next_context, plan.raw_txs.clone(), params)
            .is_err()); // nonce 0 already used
        assert!(parent
            .successor_plan(
                next_context,
                vec![candidate_workspace_execution_raw(
                    chain,
                    3,
                    [0xc3; 32],
                    10,
                    "deposit_reserve"
                )],
                params
            )
            .is_err());
        let mut wrong_context = next_context;
        wrong_context.parent_block_hash = [9; 32];
        assert!(parent
            .successor_plan(wrong_context, vec![next_raw.clone()], params)
            .is_err());
        wrong_context = next_context;
        wrong_context.block_height = 3;
        assert!(parent
            .successor_plan(wrong_context, vec![next_raw], params)
            .is_err());
        assert!(workspace::create_from_finalized_genesis_v1(
            &next_plan,
            input.workspace_id,
            [9; 32],
            params
        )
        .is_err());
        let next_input = workspace::create_from_finalized_genesis_v1(
            &next_plan,
            input.workspace_id,
            pin,
            params,
        )
        .unwrap();
        assert_eq!(
            next_input,
            workspace::create_from_finalized_genesis_v1(
                &next_plan,
                input.workspace_id,
                pin,
                params
            )
            .unwrap()
        );
        assert!(workspace::create_v1(&next_plan, params).is_err());
        let next_result = workspace::execute_v1(chain, next_input.workspace_id, params).unwrap();
        assert!(
            next_result.aoem_called
                && next_result.execution_completed
                && next_result.candidate_state_persisted
        );
        assert!(!next_result.authority_state_published && !next_result.finalized);
        assert_eq!(next_result.batch_result.snapshot_metadata.state_version, 3);
        assert!(next_result.batch_result.per_tx_receipts[0].status_ok);
        assert_eq!(
            Some(next_result.clone()),
            workspace::load_execution_v1(chain, next_input.workspace_id, params).unwrap()
        );
        assert_eq!(
            next_result,
            workspace::execute_v1(chain, next_input.workspace_id, params).unwrap()
        );
        let next_state: NovNativeExecutionStoreV1 = serde_json::from_value(
            workspace::load_execution_snapshot_for_test_v1(chain, next_input.workspace_id, params)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(next_state.receipts.len(), parent.state().receipts.len() + 1);
        for (hash, receipt) in &parent.state().receipts {
            assert_eq!(next_state.receipts.get(hash), Some(receipt));
        }
        assert_eq!(
            next_state
                .module_state
                .treasury_reserves
                .values()
                .copied()
                .sum::<u128>(),
            30 + next_state
                .receipts
                .values()
                .map(|r| r.settled_fee_nov)
                .sum::<u128>()
        );
        assert_eq!(
            parent
                .state()
                .module_state
                .treasury_reserves
                .values()
                .copied()
                .sum::<u128>(),
            20 + parent
                .state()
                .receipts
                .values()
                .map(|r| r.settled_fee_nov)
                .sum::<u128>()
        );
        let next_block = workspace::load_block_artifact_v1(chain, next_input.workspace_id, params)
            .unwrap()
            .unwrap();
        assert_eq!(next_block.block().header.height, 2);
        assert_eq!(
            next_block.block().header.parent_block_hash,
            block.block().header.block_hash
        );
        assert!(next_block.fresh_genesis_identity().is_some());
        let next_subject = parent.successor_seal_subject(&next_block, 0).unwrap();
        next_subject.validate(compiled.validator_set()).unwrap();
        assert_eq!(
            next_subject.proof_version,
            crate::native_block_seal::NOV_NATIVE_BLOCK_SEAL_FRESH_SUCCESSOR_PROOF_V1
        );
        assert_eq!(
            next_subject.justify_qc_hash,
            proof
                .validated_decision_target(&config, block.block())
                .unwrap()
        );
        assert_ne!(
            next_subject.justify_qc_hash,
            intent.decision.certificate_hash
        );
        assert_ne!(
            next_subject.justify_qc_hash,
            intent.decision.prepare.qc_hash
        );
        assert_eq!(
            next_subject,
            parent.successor_seal_subject(&next_block, 0).unwrap()
        );
        let next_round = parent.successor_seal_subject(&next_block, 1).unwrap();
        assert_eq!(next_round.justify_qc_hash, next_subject.justify_qc_hash);
        assert!(parent.successor_seal_subject(&block, 0).is_err());
        // Same epoch identity, distinct height-dependent proof domain. This
        // metadata check is not a live execution/signing capability.
        proof
            .authority
            .validate_subject_domain_v1(&next_subject)
            .unwrap();
        let mut incomplete_parent = proof.clone();
        if let Message::DecisionCertificateV3 { decision, .. } = &mut incomplete_parent.witness {
            decision.votes.truncate(2);
        }
        assert!(incomplete_parent
            .validated_decision_target(&config, block.block())
            .is_err());
        assert!(
            workspace::register_block_candidate_v1(chain, next_input.workspace_id, params).is_err()
        );
        assert!(workspace::register_genesis_block_candidate_v1(
            chain,
            next_input.workspace_id,
            pin,
            params
        )
        .is_err());
        assert!(workspace::register_finalized_successor_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            [9; 32],
            params
        )
        .is_err());
        assert!(workspace::register_finalized_successor_v1(
            chain,
            input.workspace_id,
            input.workspace_id,
            pin,
            params
        )
        .is_err());
        assert!(workspace::with_verified_finalized_successor_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
            |_| -> Result<()> { panic!("unregistered successor reached signer") }
        )
        .is_err());
        let second_record = workspace::register_finalized_successor_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
        )
        .unwrap();
        assert_eq!(
            second_record.block_hash,
            next_block.block().header.block_hash
        );
        assert!(second_record.local_aoem_readback_verified);
        assert!(
            !second_record.execution_selected_local
                && !second_record.chain_canonical
                && !second_record.proof_sealed
                && !second_record.finalized
        );
        assert_eq!(
            second_record,
            workspace::register_finalized_successor_v1(
                chain,
                input.workspace_id,
                next_input.workspace_id,
                pin,
                params
            )
            .unwrap()
        );
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        let (second_key, second_value) = db
            .iterator(rocksdb::IteratorMode::Start)
            .map(|entry| entry.unwrap())
            .find(|(_, value)| {
                serde_json::from_slice::<
                        crate::native_block_ledger::NovNativeBlockCandidateRecordV1,
                    >(value)
                    .is_ok_and(|record| record.block_hash == second_record.block_hash)
            })
            .expect("second candidate durable record");
        db.delete(&second_key).unwrap();
        drop(db);
        assert!(workspace::with_verified_finalized_successor_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
            |_| -> Result<()> { panic!("missing successor record reached signer") }
        )
        .is_err());
        assert!(workspace::register_finalized_successor_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params
        )
        .is_err());
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        assert!(db.get(&second_key).unwrap().is_none());
        db.put(&second_key, &second_value).unwrap(); // Explicit fixture restoration only.
        drop(db);
        let competing_next_plan = parent
            .successor_plan(
                next_context,
                vec![candidate_workspace_execution_raw(
                    chain,
                    2,
                    [0xc3; 32],
                    11,
                    "deposit_reserve",
                )],
                params,
            )
            .unwrap();
        let competing_next = workspace::create_from_finalized_genesis_v1(
            &competing_next_plan,
            input.workspace_id,
            pin,
            params,
        )
        .unwrap();
        workspace::execute_v1(chain, competing_next.workspace_id, params).unwrap();
        let competing_next_record = workspace::register_finalized_successor_v1(
            chain,
            input.workspace_id,
            competing_next.workspace_id,
            pin,
            params,
        )
        .unwrap();
        assert_ne!(competing_next_record.block_hash, second_record.block_hash);
        assert!(
            !competing_next_record.execution_selected_local && !competing_next_record.finalized
        );
        assert_eq!(
            second_record,
            workspace::register_finalized_successor_v1(
                chain,
                input.workspace_id,
                next_input.workspace_id,
                pin,
                params
            )
            .unwrap()
        );
        exercise_fresh_successor_signing(
            path,
            params,
            &compiled,
            input.workspace_id,
            next_input.workspace_id,
            competing_next.workspace_id,
        );
        assert!(NovNativeBlockLedgerV1::open(&ledger).is_err());
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), promoted_head);
        assert_eq!(
            serde_json::to_value(parent.state()).unwrap(),
            workspace::load_execution_snapshot_for_test_v1(chain, input.workspace_id, params)
                .unwrap()
        );
        assert_eq!(
            finalized,
            workspace::verify_genesis_promotion_v1(chain, input.workspace_id, pin, params).unwrap()
        );
        assert_eq!(
            finalized,
            workspace::finalize_genesis_promotion_v1(
                chain,
                input.workspace_id,
                pin,
                &proof,
                params
            )
            .unwrap()
        );
        assert_eq!(
            NovNativeBlockLedgerV1::load_fresh_genesis_finality_v1(&ledger, pin, namespace)
                .unwrap(),
            Some(proof.clone())
        );
        assert_eq!(
            NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(&ledger, pin, namespace)
                .unwrap()
                .as_ref(),
            Some(block.block())
        );
        assert!(!block.block().header.finalized); // Original signed artifact is immutable.
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        let finality_pin_key = b"native_block_ledger/v1/genesis/finality-pin";
        let original_pin = db.get(finality_pin_key).unwrap().unwrap();
        db.delete(finality_pin_key).unwrap();
        drop(db);
        assert!(
            workspace::verify_genesis_promotion_v1(chain, input.workspace_id, pin, params).is_err()
        );
        assert!(workspace::finalize_genesis_promotion_v1(
            chain,
            input.workspace_id,
            pin,
            &proof,
            params
        )
        .is_err());
        assert!(workspace::load_finalized_genesis_parent_v1(
            chain,
            input.workspace_id,
            pin,
            params
        )
        .is_err());
        assert!(workspace::with_verified_finalized_successor_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
            |_| -> Result<()> { panic!("missing parent finality reached signer") }
        )
        .is_err());
        assert!(workspace::register_finalized_successor_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params
        )
        .is_err());
        assert!(workspace::create_from_finalized_genesis_v1(
            &next_plan,
            input.workspace_id,
            pin,
            params
        )
        .is_err()); // ready replay still requires live finality
        assert_eq!(
            Some(next_result.clone()),
            workspace::load_execution_v1(chain, next_input.workspace_id, params).unwrap()
        ); // archived isolated result is not live signing authority
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        assert!(db.get(finality_pin_key).unwrap().is_none());
        db.put(finality_pin_key, original_pin).unwrap();
        drop(db);
        let mut corrupt = promoted_head.clone();
        corrupt[0] ^= 1;
        change(
            930,
            Write::Put {
                key: head_key.clone(),
                value: corrupt.clone(),
            },
            Write::Put {
                key: head_key.clone(),
                value: corrupt.clone(),
            },
        );
        assert!(
            workspace::publish_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .is_err()
        );
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), corrupt);
        change(
            931,
            Write::Put {
                key: head_key.clone(),
                value: promoted_head.clone(),
            },
            Write::Put {
                key: head_key.clone(),
                value: promoted_head.clone(),
            },
        );
        // The successor intent is durable, but authority is still the first block.
        let publish_successor = || {
            workspace::publish_successor_authority_v1(
                chain,
                input.workspace_id,
                next_input.workspace_id,
                pin,
                params,
            )
        };
        let verify_successor = || {
            workspace::verify_successor_authority_v1(
                chain,
                input.workspace_id,
                next_input.workspace_id,
                pin,
                params,
            )
        };
        let finalize_successor = || {
            workspace::finalize_successor_v1(
                chain,
                input.workspace_id,
                next_input.workspace_id,
                pin,
                params,
            )
        };
        assert!(finalize_successor().is_err());
        assert!(
            NovNativeBlockLedgerV1::load_fresh_successor_finality_v1(&ledger, pin, namespace)
                .unwrap()
                .is_none()
        );
        assert!(verify_successor().is_err());
        assert!(workspace::publish_successor_authority_v1(
            chain,
            input.workspace_id,
            competing_next.workspace_id,
            pin,
            params
        )
        .is_err());
        assert!(workspace::publish_successor_with_checkpoint_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::BeforePublication {
                    anyhow::bail!("before successor publication");
                }
                Ok(())
            }
        )
        .is_err());
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), promoted_head);
        assert!(workspace::publish_successor_with_checkpoint_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::AfterPublication {
                    anyhow::bail!("lost successor success response");
                }
                Ok(())
            }
        )
        .is_err());
        let successor_head = open_graph().get(&head_key).unwrap().unwrap();
        assert_eq!(&successor_head[..4], b"NVP2");
        assert_ne!(successor_head, promoted_head);
        let published_successor = publish_successor().unwrap();
        assert_eq!(published_successor, verify_successor().unwrap());
        assert_eq!(published_successor, publish_successor().unwrap());
        assert!(
            published_successor.aoem_authority_published
                && published_successor.aoem_readback_verified
        );
        assert!(
            !published_successor.ledger_publication_completed && !published_successor.finalized
        );
        assert_eq!(published_successor.state_version, 3);
        assert_eq!(
            published_successor.state_root,
            next_block.block().header.post_state_root
        );
        assert!(
            workspace::publish_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .is_err()
        );
        assert_eq!(
            open_graph().get(&head_key).unwrap().unwrap(),
            successor_head
        );
        assert!(workspace::load_finalized_genesis_parent_v1(
            chain,
            input.workspace_id,
            pin,
            params
        )
        .is_err());
        assert_eq!(
            NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(&ledger, pin, namespace)
                .unwrap()
                .as_ref(),
            Some(block.block())
        );
        let evidence_key =
            workspace::publication_evidence_key_for_test_v1(chain, next_input.workspace_id, params)
                .unwrap();
        change(
            940,
            Write::Delete {
                key: evidence_key.clone(),
            },
            Write::Delete {
                key: evidence_key.clone(),
            },
        );
        assert!(publish_successor().is_err());
        assert!(verify_successor().is_err());
        assert!(open_graph().get(&evidence_key).unwrap().is_none());
        assert_eq!(
            open_graph().get(&head_key).unwrap().unwrap(),
            successor_head
        );
        change(
            941,
            Write::Put {
                key: evidence_key.clone(),
                value: successor_head.clone(),
            },
            Write::Put {
                key: evidence_key.clone(),
                value: successor_head.clone(),
            },
        ); // Explicit fixture restoration.
        assert_eq!(verify_successor().unwrap(), published_successor);
        let complete_successor = || {
            workspace::complete_successor_ledger_v1(
                chain,
                input.workspace_id,
                next_input.workspace_id,
                pin,
                params,
            )
        };
        assert!(finalize_successor().is_err()); // AOEM alone is not a complete ledger.
        assert!(workspace::complete_successor_with_checkpoint_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::BeforeLedgerCommit {
                    anyhow::bail!("before successor ledger commit");
                }
                Ok(())
            }
        )
        .is_err());
        assert!(
            NovNativeBlockLedgerV1::load_fresh_successor_published_block_v1(
                &ledger, pin, namespace
            )
            .unwrap()
            .is_none()
        );
        assert!(workspace::complete_successor_with_checkpoint_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::AfterLedgerCommit {
                    anyhow::bail!("lost ledger success response");
                }
                Ok(())
            }
        )
        .is_err());
        let completed_successor = complete_successor().unwrap();
        assert!(completed_successor.ledger_publication_completed && !completed_successor.finalized);
        assert_eq!(complete_successor().unwrap(), completed_successor);
        assert_eq!(verify_successor().unwrap(), completed_successor);
        assert_eq!(
            NovNativeBlockLedgerV1::load_fresh_successor_published_block_v1(
                &ledger, pin, namespace
            )
            .unwrap()
            .as_ref(),
            Some(next_block.block())
        );
        assert_eq!(
            NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(&ledger, pin, namespace)
                .unwrap()
                .as_ref(),
            Some(block.block())
        );
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        let indexes = db
            .iterator(rocksdb::IteratorMode::Start)
            .map(|entry| entry.unwrap())
            .filter(|(key, _)| String::from_utf8_lossy(key).contains("/tx/"))
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 3);
        let head_bytes = db
            .iterator(rocksdb::IteratorMode::Start)
            .map(|entry| entry.unwrap())
            .find(|(key, _)| key.ends_with(b"execution_head"))
            .unwrap()
            .1;
        let head: serde_json::Value = serde_json::from_slice(&head_bytes).unwrap();
        assert_eq!(head["height"], 2);
        assert_eq!(head["block_count"], 2);
        assert_eq!(head["cumulative_tx_count"], 3);
        assert_eq!(head["state_version"], 3);
        assert_eq!(head["finalized"], false);
        let receipts = db
            .iterator(rocksdb::IteratorMode::Start)
            .map(|entry| entry.unwrap())
            .filter(|(key, _)| String::from_utf8_lossy(key).contains("/receipt/"))
            .collect::<Vec<_>>();
        assert_eq!(receipts.len(), 3);
        drop(db);
        // Both ancestor and successor query indexes are required after commit.
        for (key, value) in indexes.into_iter().chain(receipts) {
            let db = rocksdb::DB::open_default(&ledger).unwrap();
            db.delete(&key).unwrap();
            drop(db);
            assert!(complete_successor().is_err());
            assert!(verify_successor().is_err());
            let db = rocksdb::DB::open_default(&ledger).unwrap();
            assert!(db.get(&key).unwrap().is_none());
            db.put(&key, &value).unwrap(); // Explicit fixture restoration only.
        }
        assert_eq!(complete_successor().unwrap(), completed_successor);
        change(
            942,
            Write::Put {
                key: head_key.clone(),
                value: promoted_head.clone(),
            },
            Write::Put {
                key: head_key.clone(),
                value: promoted_head.clone(),
            },
        );
        assert!(complete_successor().is_err());
        assert!(publish_successor().is_err());
        assert_eq!(open_graph().get(&head_key).unwrap().unwrap(), promoted_head);
        change(
            943,
            Write::Put {
                key: head_key.clone(),
                value: successor_head.clone(),
            },
            Write::Put {
                key: head_key.clone(),
                value: successor_head.clone(),
            },
        );
        assert_eq!(verify_successor().unwrap(), completed_successor);
        assert!(workspace::load_finalized_genesis_parent_v1(
            chain,
            next_input.workspace_id,
            pin,
            params,
        )
        .is_err()); // Published indexes alone must not authorize a next parent.
        assert!(workspace::finalize_successor_v1(
            chain,
            input.workspace_id,
            competing_next.workspace_id,
            pin,
            params
        )
        .is_err());
        assert!(workspace::finalize_successor_with_checkpoint_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::BeforeFinalityCommit {
                    anyhow::bail!("before finality commit");
                }
                Ok(())
            }
        )
        .is_err());
        assert!(
            NovNativeBlockLedgerV1::load_fresh_successor_finality_v1(&ledger, pin, namespace)
                .unwrap()
                .is_none()
        );
        assert!(workspace::finalize_successor_with_checkpoint_v1(
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            params,
            |point| {
                if point == Point::AfterFinalityCommit {
                    anyhow::bail!("lost finality success response");
                }
                Ok(())
            }
        )
        .is_err());
        let finalized_successor = finalize_successor().unwrap();
        assert!(finalized_successor.finalized && finalized_successor.ledger_publication_completed);
        assert_eq!(finalize_successor().unwrap(), finalized_successor);
        assert_eq!(verify_successor().unwrap(), finalized_successor);
        assert_eq!(complete_successor().unwrap(), finalized_successor);
        let archived =
            NovNativeBlockLedgerV1::load_fresh_successor_finality_v1(&ledger, pin, namespace)
                .unwrap()
                .unwrap();
        let Message::DecisionCertificateV3 { decision, .. } = &archived.witness else {
            panic!("full decision required");
        };
        decision.verify(compiled.validator_set()).unwrap();
        assert_eq!(
            decision.prepare.subject.block_hash,
            next_block.block().header.block_hash
        );
        assert!(
            !NovNativeBlockLedgerV1::load_fresh_successor_published_block_v1(
                &ledger, pin, namespace
            )
            .unwrap()
            .unwrap()
            .header
            .finalized
        );
        let finality_key = b"native_block_ledger/v1/successor/finalized-intent";
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        let original = db.get(finality_key).unwrap().unwrap();
        db.delete(finality_key).unwrap();
        drop(db);
        assert!(finalize_successor().is_err());
        assert!(verify_successor().is_err());
        assert!(complete_successor().is_err());
        assert!(
            NovNativeBlockLedgerV1::load_fresh_successor_finality_v1(&ledger, pin, namespace)
                .is_err()
        );
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        assert!(db.get(finality_key).unwrap().is_none());
        db.put(finality_key, [0; 32]).unwrap();
        drop(db);
        assert!(finalize_successor().is_err());
        let db = rocksdb::DB::open_default(&ledger).unwrap();
        assert_eq!(db.get(finality_key).unwrap().unwrap(), [0; 32]);
        db.put(finality_key, original).unwrap(); // Explicit fixture restoration.
        drop(db);
        assert_eq!(verify_successor().unwrap(), finalized_successor);
        exercise_fresh_successor_relay(
            path,
            params,
            chain,
            input.workspace_id,
            next_input.workspace_id,
            pin,
            &archived,
        );
        let latest_parent = workspace::load_finalized_genesis_parent_v1(
            chain,
            next_input.workspace_id,
            pin,
            params,
        )
        .unwrap();
        assert_eq!(latest_parent.block(), next_block.block());
        assert_eq!(latest_parent.workspace_id(), next_input.workspace_id);
        assert_eq!(latest_parent.finality_proof(), &archived);
        assert_eq!(
            serde_json::to_value(latest_parent.state()).unwrap(),
            serde_json::to_value(&next_state).unwrap()
        );
        assert!(archived
            .validate_archived_block(&config, next_block.block())
            .is_err());
        archived
            .validate_archived_certificate(&config, next_block.block())
            .unwrap();
        let third_context = NovBlockExecutionContextV1 {
            chain_id: chain,
            block_height: 3,
            parent_block_hash: next_block.block().header.block_hash,
            slot: 3,
            timestamp_unix_ms: config.timestamp_unix_ms + 2,
        };
        assert!(latest_parent
            .successor_plan(third_context, next_plan.raw_txs.clone(), params)
            .is_err());
        let third_plan = latest_parent
            .successor_plan(
                third_context,
                vec![candidate_workspace_execution_raw(
                    chain,
                    3,
                    [0xc3; 32],
                    10,
                    "deposit_reserve",
                )],
                params,
            )
            .unwrap();
        assert!(workspace::create_from_finalized_genesis_v1(
            &third_plan,
            input.workspace_id,
            pin,
            params
        )
        .is_err());
        let third = workspace::create_from_finalized_genesis_v1(
            &third_plan,
            next_input.workspace_id,
            pin,
            params,
        )
        .unwrap();
        assert_eq!(
            third,
            workspace::create_from_finalized_genesis_v1(
                &third_plan,
                next_input.workspace_id,
                pin,
                params
            )
            .unwrap()
        );
        let third_result = workspace::execute_v1(chain, third.workspace_id, params).unwrap();
        assert!(
            third_result.aoem_called
                && third_result.execution_completed
                && third_result.candidate_state_persisted
        );
        assert!(!third_result.authority_state_published && !third_result.finalized);
        assert_eq!(third_result.batch_result.snapshot_metadata.state_version, 4);
        assert!(third_result.batch_result.per_tx_receipts[0].status_ok);
        assert_eq!(
            third_result,
            workspace::execute_v1(chain, third.workspace_id, params).unwrap()
        );
        let third_block = workspace::load_block_artifact_v1(chain, third.workspace_id, params)
            .unwrap()
            .unwrap();
        assert_eq!(third_block.block().header.height, 3);
        let third_subject = latest_parent
            .successor_seal_subject(&third_block, 0)
            .unwrap();
        assert_eq!(
            third_subject.justify_qc_hash,
            archived
                .validated_decision_target(&config, next_block.block())
                .unwrap()
        );
        assert_eq!(
            open_graph().get(&head_key).unwrap().unwrap(),
            successor_head
        );
        assert_eq!(verify_successor().unwrap(), finalized_successor);
        exercise_fresh_sequence(
            path,
            params,
            &compiled,
            next_input.workspace_id,
            third.workspace_id,
            &plan,
        );
        let successor_head = open_graph().get(&head_key).unwrap().unwrap();
        assert!(workspace::load_v1(chain, input.workspace_id, params).unwrap().is_none());
        assert!(workspace::corrupt_execution_output_for_test_v1(chain, input.workspace_id, params).is_err());
        assert!(workspace::load_finalized_genesis_parent_v1(
            chain,
            next_input.workspace_id,
            pin,
            params
        )
        .is_err());
        assert!(finalize_successor().is_err());
        assert!(verify_successor().is_err());
        assert!(publish_successor().is_err());
        assert!(
            workspace::verify_genesis_promotion_v1(chain, input.workspace_id, pin, params).is_err()
        );
        assert!(
            workspace::publish_genesis_promotion_v1(chain, input.workspace_id, pin, params)
                .is_err()
        );
        assert_eq!(
            open_graph().get(&head_key).unwrap().unwrap(),
            successor_head
        );
    });
}

include!("native_fresh_genesis_signing_tests.rs");
include!("native_fresh_successor_signing_tests.rs");
include!("native_fresh_genesis_service_tests.rs");
include!("native_fresh_successor_relay_tests.rs");
include!("native_fresh_sequence_tests.rs");
include!("native_fresh_independent_tests.rs");
