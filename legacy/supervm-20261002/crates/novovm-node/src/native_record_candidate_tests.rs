// Included beside the existing real-AOEM candidate fixtures. All stores and
// validator keys below are isolated test fixtures, not a network acceptance run.

#[test]
fn candidate_workspace_record_profile_legacy_record_output_four_stage_recovery() {
    transfer_candidate_on_runtime_stack(|| {
        exercise_record_output_four_stage_recovery(RecordOutputRecoveryFixture::LegacyV1);
    });
}

#[test]
fn candidate_workspace_record_profile_previous_record_output_four_stage_recovery() {
    transfer_candidate_on_runtime_stack(|| {
        exercise_record_output_four_stage_recovery(RecordOutputRecoveryFixture::PreviousV2);
    });
}

#[test]
fn candidate_workspace_record_profile_delta_output_four_stage_recovery() {
    transfer_candidate_on_runtime_stack(|| {
        exercise_record_output_four_stage_recovery(RecordOutputRecoveryFixture::DeltaV3);
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordOutputRecoveryFixture {
    LegacyV1,
    PreviousV2,
    DeltaV3,
}

fn exercise_record_output_four_stage_recovery(format: RecordOutputRecoveryFixture) {
    use crate::tx_ingress::fresh_genesis::{
        publication::{publish_v1, verify_persisted_v1},
        FreshGenesisConfigV1, GenesisAllocationV1, GenesisValidatorV1, GENESIS_SCHEMA_RECORD_V2,
    };
    use workspace::ExecutionCheckpointV1 as Stage;
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    with_plan_runtime(|path, params| {
        let chain = match format {
            RecordOutputRecoveryFixture::LegacyV1 => 98_919_726,
            RecordOutputRecoveryFixture::PreviousV2 => 98_919_727,
            RecordOutputRecoveryFixture::DeltaV3 => 98_919_728,
        };
        let (payer, recipient) = ([0x79; 32], [0x7a; 32]);
        let config = FreshGenesisConfigV1 {
            schema: GENESIS_SCHEMA_RECORD_V2.into(),
            chain_id: chain,
            timestamp_unix_ms: 1_900_000_000_789,
            protocol_config_commitment: parse_fixed_hex_32_v1(
                &native_business_protocol_config_commitment_v1().unwrap(),
                "protocol",
            )
            .unwrap(),
            allocations: vec![GenesisAllocationV1 {
                account: novovm_adapter_novovm::address_from_seed_v1(payer)
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
        let namespace = parse_fixed_hex_32_v1(
            &native_aoem_owned_state_namespace_digest_v1(params, chain),
            "namespace",
        )
        .unwrap();
        let ledger = nov_native_block_ledger_rocksdb_path_v1(path);
        NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(&ledger, &config, pin, namespace)
            .unwrap();
        publish_v1(chain, pin, params).unwrap();
        let head_key = native_aoem_owned_state_head_key_v1(chain, &to_hex(&namespace));
        let authority = candidate_workspace_graph(params).get(&head_key).unwrap();
        let host_before = load_nov_native_execution_store_v1(path).unwrap();
        let genesis_before =
            serde_json::to_vec(&verify_persisted_v1(chain, pin, params).unwrap()).unwrap();
        let assert_authority_unchanged = || {
            assert_eq!(
                candidate_workspace_graph(params).get(&head_key).unwrap(),
                authority
            );
            assert_eq!(
                load_nov_native_execution_store_v1(path).unwrap(),
                host_before
            );
            assert_eq!(
                serde_json::to_vec(&verify_persisted_v1(chain, pin, params).unwrap()).unwrap(),
                genesis_before
            );
            assert!(
                NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(
                    &ledger, pin, namespace
                )
                .unwrap()
                .is_none()
            );
        };
        let plan_for = |amount| {
            make_plan(
                NovBlockExecutionContextV1 {
                    chain_id: chain,
                    block_height: 1,
                    parent_block_hash: [0; 32],
                    slot: 1,
                    timestamp_unix_ms: config.timestamp_unix_ms,
                },
                compiled.state_root(),
                None,
                vec![transfer_candidate_raw(chain, 0, payer, recipient, amount)],
            )
        };
        for (index, stage) in [
            Stage::OutputReserved,
            Stage::PartialOutput,
            Stage::OutputWritten,
            Stage::Completed,
        ]
        .into_iter()
        .enumerate()
        {
            let amount = index as u128 + 1;
            let plan = plan_for(amount);
            let ready = workspace::create_from_genesis_v1(&plan, pin, params).unwrap();
            let reserved_digest = match format {
                RecordOutputRecoveryFixture::LegacyV1 => Some(
                    workspace::seed_legacy_record_output_for_test_v1(
                        chain,
                        ready.workspace_id,
                        params,
                        stage,
                        false,
                    )
                    .unwrap(),
                ),
                RecordOutputRecoveryFixture::PreviousV2 => Some(
                    workspace::seed_previous_record_output_for_test_v1(
                        chain,
                        ready.workspace_id,
                        params,
                        stage,
                        false,
                    )
                    .unwrap(),
                ),
                RecordOutputRecoveryFixture::DeltaV3 => {
                    let stopped = std::cell::Cell::new(false);
                    let failure = workspace::execute_with_checkpoint_v1(
                        chain,
                        ready.workspace_id,
                        params,
                        |checkpoint| {
                            if checkpoint == stage {
                                stopped.set(true);
                                anyhow::bail!("delta output fixture crash checkpoint");
                            }
                            Ok(())
                        },
                    )
                    .unwrap_err();
                    assert!(stopped.get(), "{format:?} {stage:?}");
                    assert!(
                        format!("{failure:#}").contains("delta output fixture crash checkpoint"),
                        "{failure:#}"
                    );
                    None
                }
            };
            assert_eq!(
                workspace::load_execution_v1(chain, ready.workspace_id, params)
                    .unwrap()
                    .is_some(),
                stage == Stage::Completed
            );
            assert_authority_unchanged();
            // Every API opens a new workspace handle. Reset the execution
            // session too: recovery must be based on durable old bytes.
            reset_native_aoem_semantic_ingress_session_v1();
            let checkpoints = std::cell::RefCell::new(Vec::new());
            let recovered = workspace::execute_with_checkpoint_v1(
                chain,
                ready.workspace_id,
                params,
                |checkpoint| {
                    if matches!(stage, Stage::OutputWritten | Stage::Completed) {
                        assert_eq!(
                            checkpoint,
                            Stage::Completed,
                            "fully written {format:?} output must not re-execute"
                        );
                    }
                    checkpoints.borrow_mut().push(checkpoint);
                    Ok(())
                },
            )
            .unwrap();
            let expected = match stage {
                Stage::OutputWritten => vec![Stage::Completed],
                Stage::Completed => vec![],
                _ => vec![
                    Stage::OutputReserved,
                    Stage::PartialOutput,
                    Stage::OutputWritten,
                    Stage::Completed,
                ],
            };
            assert_eq!(*checkpoints.borrow(), expected);
            assert_candidate_workspace_execution_complete(&recovered);
            if let Some(digest) = reserved_digest {
                assert_eq!(
                    recovered.output_digest, digest,
                    "{format:?} reserved digest must not be upgraded to a new document"
                );
            }
            let digest = recovered.output_digest;
            assert_eq!(
                workspace::execute_v1(chain, ready.workspace_id, params)
                    .unwrap()
                    .output_digest,
                digest
            );
            let store = workspace::load_typed_execution_snapshot_for_test_v1(
                chain,
                ready.workspace_id,
                params,
            )
            .unwrap();
            assert_eq!(
                recovered.post_state_root,
                to_hex(
                    &native_record_commitment::consensus_state_root_v1(&store.module_state)
                        .unwrap()
                )
            );
            assert_eq!(
                recovered.receipt_root,
                to_hex(&native_record_commitment::cumulative_receipt_root_v1(&store).unwrap())
            );
            assert_eq!(
                workspace::load_execution_v1(chain, ready.workspace_id, params).unwrap(),
                Some(recovered.clone())
            );
            if format == RecordOutputRecoveryFixture::DeltaV3 {
                // The helper first validates the full input, then forbids
                // output materialization while checking the lazy view; its
                // explicit cold comparison is outside that read boundary.
                workspace::assert_delta_output_point_read_for_test_v1(
                    chain,
                    ready.workspace_id,
                    params,
                )
                .unwrap();
            }
            let fee = transfer_candidate_fee(&plan.raw_txs[0]);
            assert_eq!(
                native_account_asset_balance_v1(&store, &transfer_candidate_account(payer), "NOV"),
                1000 - amount - fee
            );
            assert_eq!(
                native_account_asset_balance_v1(
                    &store,
                    &transfer_candidate_account(recipient),
                    "NOV"
                ),
                amount
            );
            let reservation = transfer_candidate_reservation(&plan.raw_txs[0]);
            assert_eq!(
                store.module_state.native_auth_next_nonces[&reservation.identity_key],
                1
            );
            assert_eq!(store.module_state.aoem_semantic_ledger_sequence, 1);
            assert_eq!(store.receipts.len(), 1);
            let receipt = &store.receipts[&reservation.tx_hash];
            assert!(receipt.status);
            assert_eq!(receipt.settled_fee_nov, fee);
            assert_eq!(store.module_state.treasury_settled_nov_total, fee);
            assert_eq!(
                store.module_state.native_auth_nonce_reservations[&reservation.ledger_key],
                reservation.reservation_id
            );
            assert_transfer_candidate_compute_logs(receipt, 1);
            assert_authority_unchanged();
        }
        if format == RecordOutputRecoveryFixture::DeltaV3 {
            return;
        }
        let bad = plan_for(19);
        let ready = workspace::create_from_genesis_v1(&bad, pin, params).unwrap();
        let seed = match format {
            RecordOutputRecoveryFixture::LegacyV1 => {
                workspace::seed_legacy_record_output_for_test_v1
            }
            RecordOutputRecoveryFixture::PreviousV2 => {
                workspace::seed_previous_record_output_for_test_v1
            }
            RecordOutputRecoveryFixture::DeltaV3 => unreachable!(),
        };
        seed(
            chain,
            ready.workspace_id,
            params,
            Stage::OutputReserved,
            true,
        )
        .unwrap();
        reset_native_aoem_semantic_ingress_session_v1();
        let error =
            workspace::execute_with_checkpoint_v1(chain, ready.workspace_id, params, |_| {
                panic!("corrupt {format:?} reservation cannot publish or replace output")
            })
            .unwrap_err();
        assert!(format!("{error:#}").contains("reserved bytes"), "{error:#}");
        assert!(
            workspace::load_execution_v1(chain, ready.workspace_id, params)
                .unwrap()
                .is_none()
        );
        assert_authority_unchanged();
    });
}

#[test]
fn candidate_workspace_record_profile_fresh_transfers_recover_and_finalize() {
    transfer_candidate_on_runtime_stack(exercise_record_profile_fresh_transfers);
}

#[test]
fn candidate_workspace_record_profile_rejects_legacy_execute_outside_json_domain() {
    transfer_candidate_on_runtime_stack(|| {
        use crate::tx_ingress::fresh_genesis::{
            publication::{publish_v1, verify_persisted_v1},
            FreshGenesisConfigV1, GenesisAllocationV1, GenesisValidatorV1,
            GENESIS_SCHEMA_RECORD_V2,
        };
        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for mixed in [false, true] {
            with_plan_runtime(|path, params| {
                let chain = 98_919_723 + u64::from(mixed);
                let (a, b) = ([0x77; 32], [0x78; 32]);
                let amounts = if mixed {
                    [u128::from(u64::MAX); 2]
                } else {
                    [u128::MAX - 1_000, 1_000]
                };
                let config = FreshGenesisConfigV1 {
                    schema: GENESIS_SCHEMA_RECORD_V2.into(),
                    chain_id: chain,
                    timestamp_unix_ms: 1_900_000_000_456,
                    protocol_config_commitment: parse_fixed_hex_32_v1(
                        &native_business_protocol_config_commitment_v1().unwrap(),
                        "protocol",
                    )
                    .unwrap(),
                    allocations: [a, b]
                        .into_iter()
                        .zip(amounts)
                        .map(|(seed, amount)| GenesisAllocationV1 {
                            account: novovm_adapter_novovm::address_from_seed_v1(seed)
                                .try_into()
                                .unwrap(),
                            nov: amount.to_string(),
                        })
                        .collect(),
                    total_initial_nov: amounts.into_iter().sum::<u128>().to_string(),
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
                let namespace = parse_fixed_hex_32_v1(
                    &native_aoem_owned_state_namespace_digest_v1(params, chain),
                    "namespace",
                )
                .unwrap();
                NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
                    &nov_native_block_ledger_rocksdb_path_v1(path),
                    &config,
                    pin,
                    namespace,
                )
                .unwrap();
                publish_v1(chain, pin, params).unwrap();
                let before = verify_persisted_v1(chain, pin, params).unwrap();
                let mut raw_txs = Vec::new();
                if mixed {
                    // Both parent balances fit u64, but the Transfer barrier
                    // makes b's balance u64::MAX + 1 before the Execute.
                    raw_txs.push(transfer_candidate_raw(chain, 0, a, b, 1));
                }
                raw_txs.push(candidate_workspace_execution_raw(
                    chain,
                    0,
                    b,
                    1,
                    "deposit_reserve",
                ));
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
                    raw_txs,
                );
                let input = workspace::create_from_genesis_v1(&plan, pin, params).unwrap();
                let error = workspace::execute_with_checkpoint_v1(
                    chain,
                    input.workspace_id,
                    params,
                    |_| panic!("unsupported Execute must not publish candidate output"),
                )
                .unwrap_err();
                assert!(
                    error.to_string().contains("legacy Execute JSON domain"),
                    "{error:#}"
                );
                assert!(
                    workspace::load_execution_v1(chain, input.workspace_id, params)
                        .unwrap()
                        .is_none()
                );
                assert_eq!(
                    serde_json::to_vec(&verify_persisted_v1(chain, pin, params).unwrap()).unwrap(),
                    serde_json::to_vec(&before).unwrap()
                );
                assert!(!native_host_projection_has_state_v1(
                    &load_nov_native_execution_store_v1(path).unwrap()
                ));
            });
        }
    });
}

#[test]
fn candidate_workspace_record_profile_u128_supply_survives_real_aoem_and_reopen() {
    transfer_candidate_on_runtime_stack(|| {
        use crate::tx_ingress::fresh_genesis::{
            publication::{publish_v1, verify_persisted_v1},
            FreshGenesisConfigV1, GenesisAllocationV1, GenesisValidatorV1,
            GENESIS_SCHEMA_RECORD_V2,
        };
        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        with_plan_runtime(|path, params| {
            let chain = 98_919_722;
            let (a, b) = ([0x75; 32], [0x76; 32]);
            let config = FreshGenesisConfigV1 {
                schema: GENESIS_SCHEMA_RECORD_V2.into(),
                chain_id: chain,
                timestamp_unix_ms: 1_900_000_000_123,
                protocol_config_commitment: parse_fixed_hex_32_v1(
                    &native_business_protocol_config_commitment_v1().unwrap(),
                    "protocol",
                )
                .unwrap(),
                allocations: [(a, u128::MAX - 1_000), (b, 1_000)]
                    .into_iter()
                    .map(|(seed, amount)| GenesisAllocationV1 {
                        account: novovm_adapter_novovm::address_from_seed_v1(seed)
                            .try_into()
                            .unwrap(),
                        nov: amount.to_string(),
                    })
                    .collect(),
                total_initial_nov: u128::MAX.to_string(),
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
            let namespace = parse_fixed_hex_32_v1(
                &native_aoem_owned_state_namespace_digest_v1(params, chain),
                "namespace",
            )
            .unwrap();
            NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(
                &nov_native_block_ledger_rocksdb_path_v1(path),
                &config,
                pin,
                namespace,
            )
            .unwrap();
            assert!(
                publish_v1(chain, pin, params)
                    .unwrap()
                    .aoem_readback_verified
            );
            let raw = transfer_candidate_raw(chain, 0, a, b, 11);
            let fee = transfer_candidate_fee(&raw);
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
                vec![raw],
            );
            let input = workspace::create_from_genesis_v1(&plan, pin, params).unwrap();
            let result = workspace::execute_v1(chain, input.workspace_id, params).unwrap();
            assert_candidate_workspace_execution_complete(&result);
            assert!(result.batch_result.per_tx_receipts[0].status_ok);
            let store = workspace::load_typed_execution_snapshot_for_test_v1(
                chain,
                input.workspace_id,
                params,
            )
            .unwrap();
            assert_eq!(
                native_account_asset_balance_v1(&store, &transfer_candidate_account(a), "NOV"),
                u128::MAX - 1_000 - 11 - fee
            );
            assert_eq!(
                native_account_asset_balance_v1(&store, &transfer_candidate_account(b), "NOV"),
                1_011
            );
            assert_eq!(store.module_state.treasury_reserves["NOV"], fee);
            let block = workspace::load_block_artifact_v1(chain, input.workspace_id, params)
                .unwrap()
                .unwrap();
            assert_eq!(
                block.block().header.post_state_root,
                native_record_commitment::consensus_state_root_v1(&store.module_state).unwrap()
            );
            assert_eq!(
                block.block().header.cumulative_receipt_root,
                native_record_commitment::cumulative_receipt_root_v1(&store).unwrap()
            );
            reset_native_aoem_semantic_ingress_session_v1();
            assert_eq!(
                workspace::execute_with_checkpoint_v1(
                    chain,
                    input.workspace_id,
                    params,
                    |_| panic!("u128 record candidate must not reexecute")
                )
                .unwrap(),
                result
            );
            assert_eq!(
                workspace::load_typed_execution_snapshot_for_test_v1(
                    chain,
                    input.workspace_id,
                    params
                )
                .unwrap(),
                store
            );
            assert!(
                verify_persisted_v1(chain, pin, params)
                    .unwrap()
                    .aoem_readback_verified
            );
            assert!(!native_host_projection_has_state_v1(
                &load_nov_native_execution_store_v1(path).unwrap()
            ));
        });
    });
}

fn exercise_record_profile_fresh_transfers() {
    use crate::native_root_codecs::NativeRootCodecProfileV1 as Profile;
    use crate::tx_ingress::fresh_genesis::{
        publication::{publish_v1, verify_persisted_v1},
        FreshGenesisConfigV1, GenesisAllocationV1, GenesisValidatorV1, GENESIS_SCHEMA_RECORD_V2,
    };
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    with_plan_runtime(|path, params| {
        let chain = 98_919_721;
        let (a, b, c, d) = ([0x71; 32], [0x72; 32], [0x73; 32], [0x74; 32]);
        let namespace = parse_fixed_hex_32_v1(
            &native_aoem_owned_state_namespace_digest_v1(params, chain),
            "record test namespace",
        )
        .unwrap();
        let config = FreshGenesisConfigV1 {
            schema: GENESIS_SCHEMA_RECORD_V2.into(),
            chain_id: chain,
            timestamp_unix_ms: 1_900_000_000_000,
            protocol_config_commitment: parse_fixed_hex_32_v1(
                &native_business_protocol_config_commitment_v1().unwrap(),
                "protocol",
            )
            .unwrap(),
            allocations: [a, c]
                .into_iter()
                .map(|seed| GenesisAllocationV1 {
                    account: novovm_adapter_novovm::address_from_seed_v1(seed)
                        .try_into()
                        .unwrap(),
                    nov: "1000".into(),
                })
                .collect(),
            total_initial_nov: "2000".into(),
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
        let profile = Profile::RecordTreeV1;
        assert_eq!(compiled.root_codec_profile(), profile);
        assert_eq!(
            compiled.state_root(),
            native_record_commitment::consensus_state_root_v1(
                &compiled.initial_store().module_state
            )
            .unwrap()
        );
        let pin = compiled.config_commitment();
        let ledger = nov_native_block_ledger_rocksdb_path_v1(path);
        NovNativeBlockLedgerV1::reserve_fresh_genesis_config_v1(&ledger, &config, pin, namespace)
            .unwrap();
        let published = publish_v1(chain, pin, params).unwrap();
        assert!(published.aoem_genesis_state_persisted && published.aoem_readback_verified);
        assert_eq!(published.state_root, compiled.state_root());
        let head_key = native_aoem_owned_state_head_key_v1(chain, &to_hex(&namespace));
        let read_head = || {
            candidate_workspace_graph(params)
                .get(&head_key)
                .unwrap()
                .unwrap()
        };
        let genesis_head = read_head();
        let host_before = load_nov_native_execution_store_v1(path).unwrap();
        let assert_genesis_authority_unchanged = || {
            assert_eq!(read_head(), genesis_head);
            let verified = verify_persisted_v1(chain, pin, params).unwrap();
            assert!(verified.aoem_readback_verified && !verified.finalized);
            assert_eq!(verified.state_root, compiled.state_root());
            assert_eq!(
                load_nov_native_execution_store_v1(path).unwrap(),
                host_before
            );
            assert!(
                NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(
                    &ledger, pin, namespace
                )
                .unwrap()
                .is_none()
            );
        };
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
                transfer_candidate_raw(chain, 0, a, b, 100),
                transfer_candidate_raw(chain, 0, c, d, 200), // independent payer/accounts
                transfer_candidate_raw(chain, 1, a, b, 50),  // ordered conflict segment
                transfer_candidate_raw(chain, 0, b, d, 1_000_000), // fee paid, business fails
                transfer_candidate_raw(chain, 1, b, a, 20),  // failed business consumed nonce
            ],
        );
        let fees: Vec<_> = plan
            .raw_txs
            .iter()
            .map(|raw| transfer_candidate_fee(raw))
            .collect();
        let input = workspace::create_from_genesis_v1(&plan, pin, params).unwrap();
        let failure =
            workspace::execute_with_checkpoint_v1(chain, input.workspace_id, params, |point| {
                if point == workspace::ExecutionCheckpointV1::OutputWritten {
                    anyhow::bail!("record fixture output durable before completion");
                }
                Ok(())
            })
            .unwrap_err();
        assert!(failure
            .to_string()
            .contains("output durable before completion"));
        assert!(
            workspace::load_execution_v1(chain, input.workspace_id, params)
                .unwrap()
                .is_none()
        );
        assert_genesis_authority_unchanged();

        reset_native_aoem_semantic_ingress_session_v1();
        let completed = std::cell::Cell::new(0);
        let result =
            workspace::execute_with_checkpoint_v1(chain, input.workspace_id, params, |point| {
                assert_eq!(
                    point,
                    workspace::ExecutionCheckpointV1::Completed,
                    "OutputWritten recovery must not charge fees or recompute transactions"
                );
                completed.set(completed.get() + 1);
                Ok(())
            })
            .unwrap();
        assert_eq!(completed.get(), 1);
        assert_candidate_workspace_execution_complete(&result);
        assert_eq!(
            result
                .batch_result
                .per_tx_receipts
                .iter()
                .map(|receipt| receipt.status_ok)
                .collect::<Vec<_>>(),
            vec![true, true, true, false, true]
        );
        let snapshot =
            workspace::load_execution_snapshot_for_test_v1(chain, input.workspace_id, params)
                .unwrap();
        let store: NovNativeExecutionStoreV1 = serde_json::from_value(snapshot.clone()).unwrap();
        let balance = |seed| {
            native_account_asset_balance_v1(&store, &transfer_candidate_account(seed), "NOV")
        };
        assert_eq!(balance(a), 1_000 - 150 + 20 - fees[0] - fees[2]);
        assert_eq!(balance(b), 150 - 20 - fees[3] - fees[4]);
        assert_eq!(balance(c), 1_000 - 200 - fees[1]);
        assert_eq!(balance(d), 200);
        let fee_sum: u128 = fees.iter().sum();
        assert_eq!(
            balance(a) + balance(b) + balance(c) + balance(d) + fee_sum,
            2_000
        );
        assert_eq!(store.module_state.treasury_reserves["NOV"], fee_sum);
        assert_eq!(store.module_state.treasury_settled_nov_total, fee_sum);
        let policy = resolve_treasury_settlement_policy_v1(compiled.initial_store());
        let mut splits = (0u128, 0u128, 0u128);
        for (index, (hash, fee)) in plan.tx_hashes.iter().zip(&fees).enumerate() {
            let receipt = &store.receipts[&to_hex(hash)];
            assert_eq!(receipt.status, index != 3);
            assert_eq!(
                (
                    receipt.settled_fee_nov,
                    receipt.paid_amount,
                    receipt.paid_asset.as_str()
                ),
                (*fee, *fee, "NOV")
            );
            assert_transfer_candidate_compute_logs(receipt, 1);
            assert!(receipt
                .logs
                .iter()
                .any(|log| log.event == "aoem.native_asset.semantic_record_commit"));
            assert!(receipt.aoem_semantic_commit.is_some());
            let reserve = fee * u128::from(policy.reserve_share_bps) / 10_000;
            let net_fee = fee * u128::from(policy.fee_share_bps) / 10_000;
            let risk = fee - reserve - net_fee;
            splits.0 += reserve;
            splits.1 += net_fee;
            splits.2 += risk;
            let tx_hash = to_hex(hash);
            let journal = store
                .module_state
                .treasury_settlement_journal
                .iter()
                .find(|entry| entry.tx_hash == tx_hash)
                .unwrap();
            assert_eq!(
                (
                    journal.reserve_bucket_delta_nov,
                    journal.fee_bucket_delta_nov,
                    journal.risk_buffer_delta_nov
                ),
                (
                    i128::try_from(reserve).unwrap(),
                    i128::try_from(net_fee).unwrap(),
                    i128::try_from(risk).unwrap()
                )
            );
        }
        assert_eq!(
            (
                store.module_state.treasury_reserve_bucket_nov,
                store.module_state.treasury_fee_bucket_nov,
                store.module_state.treasury_risk_buffer_nov
            ),
            splits
        );
        assert_eq!(splits.0 + splits.1 + splits.2, fee_sum);
        assert!(store.receipts[&to_hex(&plan.tx_hashes[3])]
            .failure_reason
            .as_deref()
            .unwrap()
            .starts_with("native.transfer.insufficient NOV"));
        for (index, next_nonce) in [(2, 2), (1, 1), (4, 2)] {
            let reservation = transfer_candidate_reservation(&plan.raw_txs[index]);
            assert_eq!(
                store.module_state.native_auth_next_nonces[&reservation.identity_key],
                next_nonce
            );
        }
        assert_eq!(store.module_state.aoem_semantic_ledger_sequence, 5);
        assert_eq!(store.module_state.aoem_semantic_ledger_records.len(), 5);

        // Same business implementation and root codec, deliberately forced to
        // one AOEM task per call. This is a serial scheduling reference, not an
        // independent arithmetic oracle; the fee helper has separate parity tests.
        // Reuse ingress observations so local diagnostics cannot skew equality.
        let mut serial = compiled.initial_store().clone();
        serial.authority_chain_id = store.authority_chain_id;
        serial
            .authority_namespace_digest
            .clone_from(&store.authority_namespace_digest);
        for raw in &plan.raw_txs {
            let transaction = decode_nov_native_tx_wire_v1(raw).unwrap();
            let reservation = transfer_candidate_reservation(raw);
            let hash = canonical_nov_native_tx_hash_from_payload_v1(raw).unwrap();
            let request = native_transfer_dispatch::fee_request_v1(&transaction, hash).unwrap();
            let subject = fallback_execution_subject_meta_v1(&request);
            let ingress = store.receipts[&reservation.tx_hash]
                .aoem_semantic_ingress
                .clone()
                .unwrap();
            let item = native_transfer_dispatch::Item {
                transaction: &transaction,
                request: &request,
                subject: &subject,
                reservation: &reservation,
                ingress,
            };
            native_transfer_record_execution::execute_segment_v1(
                &mut serial,
                &[item],
                u128::from(config.timestamp_unix_ms),
            )
            .unwrap();
        }
        assert_eq!(serial, store, "parallel segments must equal serial scheduling, including failed receipts and nonce consumption");
        assert_eq!(
            serde_json::to_vec(&serial).unwrap(),
            serde_json::to_vec(&store).unwrap()
        );

        // Independent cold reconstruction from the complete persisted image.
        let state_root =
            native_record_commitment::consensus_state_root_v1(&store.module_state).unwrap();
        let receipt_root = native_record_commitment::cumulative_receipt_root_v1(&store).unwrap();
        assert_eq!(result.post_state_root, to_hex(&state_root));
        assert_eq!(result.receipt_root, to_hex(&receipt_root));
        assert_eq!(
            result.execution_evidence_commitment,
            native_aoem_execution_evidence_with_profile_v1(&result.batch_result, profile).unwrap()
        );
        assert_ne!(
            result.execution_evidence_commitment,
            native_aoem_execution_evidence_with_profile_v1(
                &result.batch_result,
                Profile::LegacyWireV1
            )
            .unwrap()
        );
        let artifact = workspace::load_block_artifact_v1(chain, input.workspace_id, params)
            .unwrap()
            .unwrap();
        let block = artifact.block();
        assert_eq!(
            artifact.fresh_genesis_identity(),
            Some(&compiled.identity())
        );
        assert_eq!(block.header.pre_state_root, compiled.state_root());
        assert_eq!(block.header.post_state_root, state_root);
        assert_eq!(block.header.cumulative_receipt_root, receipt_root);
        assert_eq!(
            block.header.post_state_root_codec,
            profile.state_root_codec()
        );
        assert_eq!(
            block.header.cumulative_receipt_root_codec,
            profile.receipt_root_codec()
        );
        assert_eq!(block.execution_evidence.post_state_root, state_root);
        assert_eq!(
            block.execution_evidence.cumulative_receipt_root,
            receipt_root
        );
        assert_eq!(
            to_hex(&block.execution_evidence.aoem_evidence_commitment),
            result.execution_evidence_commitment
        );
        assert_eq!(block.body.tx_hashes, plan.tx_hashes);
        assert!(!block.header.finalized && !block.header.proof_sealed && !block.header.safe);
        let replay =
            workspace::execute_with_checkpoint_v1(chain, input.workspace_id, params, |_| {
                panic!("complete record candidate must be recovered, not executed again")
            })
            .unwrap();
        assert_eq!(replay, result);
        assert_eq!(
            workspace::load_execution_snapshot_for_test_v1(chain, input.workspace_id, params)
                .unwrap(),
            snapshot
        );
        assert_genesis_authority_unchanged();

        // Only a finalized parent consumes these nonces for its successors. A
        // competing candidate based on the still-active genesis is not a replay.
        finalize_record_candidate_fixture(path, params, &config, &compiled, input.workspace_id);
        let parent =
            workspace::load_finalized_genesis_parent_v1(chain, input.workspace_id, pin, params)
                .unwrap();
        assert_eq!(parent.state(), &store);
        assert_eq!(parent.block(), block);
        workspace::exercise_finalized_record_queries_for_test_v1(
            chain,
            input.workspace_id,
            pin,
            params,
            &transfer_candidate_raw(chain, 0, a, b, 999),
            &[
                transfer_candidate_raw(chain, 2, a, b, 1),
                transfer_candidate_raw(chain, 0, d, a, 1),
            ],
        )
        .unwrap();
        let context = NovBlockExecutionContextV1 {
            chain_id: chain,
            block_height: 2,
            parent_block_hash: block.header.block_hash,
            slot: 2,
            timestamp_unix_ms: config.timestamp_unix_ms + 1,
        };
        assert!(
            parent
                .successor_plan(context, vec![plan.raw_txs[0].clone()], params)
                .is_err(),
            "finalized nonce zero cannot be reused"
        );
        assert!(
            parent
                .successor_plan(context, vec![plan.raw_txs[3].clone()], params)
                .is_err(),
            "failed business also consumed its authenticated nonce"
        );
        let next_plan = parent
            .successor_plan(
                context,
                vec![
                    transfer_candidate_raw(chain, 2, a, b, 100),
                    transfer_candidate_raw(chain, 1, c, d, 2),
                    transfer_candidate_raw(chain, 3, a, d, 3),
                    transfer_candidate_raw(chain, 2, b, d, 1_000_000),
                    transfer_candidate_raw(chain, 3, b, a, 4),
                ],
                params,
            )
            .unwrap();
        // The previous block left b with 150 - 20 - both settled fees.
        // Fund both following fees and the final successful amount; otherwise
        // the intended business-failure case would be a fee rejection instead.
        let funded_b =
            native_account_asset_balance_v1(parent.state(), &transfer_candidate_account(b), "NOV")
                + 100;
        let failed_business_fee = transfer_candidate_fee(&next_plan.raw_txs[3]);
        let final_success_fee = transfer_candidate_fee(&next_plan.raw_txs[4]);
        assert!(funded_b >= failed_business_fee + final_success_fee + 4);
        assert!(funded_b < failed_business_fee + 1_000_000);
        assert_eq!(
            next_plan.aoem_parent.as_ref().unwrap().state_root_codec,
            profile.state_root_codec()
        );
        assert_eq!(
            next_plan.aoem_parent.as_ref().unwrap().receipt_root_codec,
            profile.receipt_root_codec()
        );
        let final_head = read_head();
        let next_input = workspace::exercise_live_parent_admission_for_test_v1(
            &next_plan,
            input.workspace_id,
            pin,
            params,
            &genesis_head,
        )
        .unwrap();
        let next_result = workspace::exercise_light_first_compute_for_test_v1(
            chain,
            next_input.workspace_id,
            params,
        )
        .unwrap();
        workspace::assert_light_input_output_point_read_for_test_v1(
            chain,
            next_input.workspace_id,
            params,
        )
        .unwrap();
        workspace::exercise_light_input_recovery_for_test_v1(
            chain,
            next_input.workspace_id,
            params,
        )
        .unwrap();
        workspace::exercise_light_output_recovery_for_test_v1(
            chain,
            next_input.workspace_id,
            params,
        )
        .unwrap();
        assert_candidate_workspace_execution_complete(&next_result);
        assert_eq!(
            next_result
                .batch_result
                .per_tx_receipts
                .iter()
                .map(|receipt| receipt.status_ok)
                .collect::<Vec<_>>(),
            vec![true, true, true, false, true]
        );
        assert_eq!(next_result.batch_result.snapshot_metadata.state_version, 10);
        let next_store = workspace::load_typed_execution_snapshot_for_test_v1(
            chain,
            next_input.workspace_id,
            params,
        )
        .unwrap();
        let failed_receipt = &next_store.receipts[&to_hex(&next_plan.tx_hashes[3])];
        assert!(failed_receipt
            .failure_reason
            .as_deref()
            .unwrap()
            .starts_with("native.transfer.insufficient NOV"));
        assert_eq!(failed_receipt.settled_fee_nov, failed_business_fee);
        assert!(next_store.receipts[&to_hex(&next_plan.tx_hashes[4])]
            .failure_reason
            .is_none());
        // Preserve the earlier serial-reference standard for the actual NCW2
        // first-compute path too. Only scheduling changes, not the business
        // implementation or codec; use the observed ingress diagnostics.
        let mut next_serial = parent.state().clone();
        for raw in &next_plan.raw_txs {
            let transaction = decode_nov_native_tx_wire_v1(raw).unwrap();
            let reservation = transfer_candidate_reservation(raw);
            let hash = canonical_nov_native_tx_hash_from_payload_v1(raw).unwrap();
            let request = native_transfer_dispatch::fee_request_v1(&transaction, hash).unwrap();
            let subject = fallback_execution_subject_meta_v1(&request);
            let ingress = next_store.receipts[&reservation.tx_hash]
                .aoem_semantic_ingress
                .clone()
                .unwrap();
            native_transfer_record_execution::execute_segment_v1(
                &mut next_serial,
                &[native_transfer_dispatch::Item {
                    transaction: &transaction,
                    request: &request,
                    subject: &subject,
                    reservation: &reservation,
                    ingress,
                }],
                u128::from(context.timestamp_unix_ms),
            )
            .unwrap();
        }
        assert_eq!(next_store, next_serial);
        assert_eq!(
            serde_json::to_vec(&next_store).unwrap(),
            serde_json::to_vec(&next_serial).unwrap()
        );
        assert_eq!(next_store.receipts.len(), store.receipts.len() + 5);
        assert_eq!(
            next_result.post_state_root,
            to_hex(
                &native_record_commitment::consensus_state_root_v1(&next_serial.module_state)
                    .unwrap()
            )
        );
        assert_eq!(
            next_result.receipt_root,
            to_hex(&native_record_commitment::cumulative_receipt_root_v1(&next_serial).unwrap())
        );
        let next_fees: u128 = next_plan
            .raw_txs
            .iter()
            .map(|raw| transfer_candidate_fee(raw))
            .sum();
        assert_eq!(
            next_store.module_state.treasury_settled_nov_total,
            store.module_state.treasury_settled_nov_total + next_fees
        );
        assert_eq!(
            [a, b, c, d]
                .iter()
                .map(|seed| native_account_asset_balance_v1(
                    &next_store,
                    &transfer_candidate_account(*seed),
                    "NOV"
                ))
                .sum::<u128>()
                + next_store.module_state.treasury_settled_nov_total,
            2_000
        );
        for (index, next_nonce) in [(2, 4), (1, 2), (4, 4)] {
            let reservation = transfer_candidate_reservation(&next_plan.raw_txs[index]);
            assert_eq!(
                next_store.module_state.native_auth_next_nonces[&reservation.identity_key],
                next_nonce
            );
        }
        for (index, raw) in next_plan.raw_txs.iter().enumerate() {
            let reservation = transfer_candidate_reservation(raw);
            let receipt = &next_store.receipts[&reservation.tx_hash];
            assert_eq!(receipt.status, index != 3);
            assert_eq!(receipt.settled_fee_nov, transfer_candidate_fee(raw));
            assert_transfer_candidate_compute_logs(receipt, 1);
        }
        let next_block = workspace::load_block_artifact_v1(chain, next_input.workspace_id, params)
            .unwrap()
            .unwrap();
        assert_eq!(
            next_block.block().header.post_state_root_codec,
            profile.state_root_codec()
        );
        assert_eq!(
            next_block.block().header.cumulative_receipt_root_codec,
            profile.receipt_root_codec()
        );
        assert_eq!(
            read_head(),
            final_head,
            "an unfinalized successor must not publish authority"
        );
        assert_eq!(
            load_nov_native_execution_store_v1(path).unwrap(),
            host_before
        );
    });
}

fn finalize_record_candidate_fixture(
    path: &Path,
    params: &serde_json::Value,
    config: &crate::tx_ingress::fresh_genesis::FreshGenesisConfigV1,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    id: [u8; 32],
) {
    use crate::native_block_ledger::NovNativeFreshFinalityProofV1;
    use crate::native_block_seal::commit_v3::NovNativeSealDecisionCertificateV3;
    use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1;
    use crate::native_block_seal::{
        NovNativeBlockSealStoreV1, NovNativeSealLocalProposalRequestV1,
        NovNativeSealQuorumCertificateV1, NovNativeSealValidatorV1,
    };
    use crate::native_block_seal_overlay::{
        NovNativeSealEpochAuthorityV1, NovNativeSealValidatorTransportBindingV1,
    };
    let chain = config.chain_id;
    let pin = compiled.config_commitment();
    let set = compiled.validator_set();
    workspace::register_genesis_block_candidate_v1(chain, id, pin, params).unwrap();
    let block = workspace::load_block_artifact_v1(chain, id, params)
        .unwrap()
        .unwrap();
    let mut keys = (1..=4)
        .map(|seed| ed25519_dalek::SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    keys.sort_by_key(|key| {
        NovNativeSealValidatorV1::new(key.verifying_key().to_bytes(), 1)
            .unwrap()
            .validator_id
    });
    let seal_paths = (0..3)
        .map(|index| path.with_extension(format!("record-profile-seal-{index}")))
        .collect::<Vec<_>>();
    let stores = seal_paths
        .iter()
        .map(|path| NovNativeBlockSealStoreV1::open(path).unwrap())
        .collect::<Vec<_>>();
    let request = NovNativeSealLocalProposalRequestV1 {
        chain_id: chain,
        block_hash: block.block().header.block_hash,
        round: 0,
        justify_qc_hash: None,
    };
    let proposal =
        workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
            stores[0].sign_local_proposal(view, &request, set, &keys[0])
        })
        .unwrap();
    assert_eq!(
        proposal.subject.post_state_root_codec,
        compiled.root_codec_profile().state_root_codec()
    );
    assert_eq!(
        proposal.subject.cumulative_receipt_root_codec,
        compiled.root_codec_profile().receipt_root_codec()
    );
    let votes = (0..3)
        .map(|index| {
            workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
                stores[index].sign_local_vote(view, &proposal, set, &keys[index])
            })
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(NovNativeSealQuorumCertificateV1::from_votes(
        proposal.subject.clone(),
        set,
        votes[..2].to_vec()
    )
    .is_err());
    let qc =
        NovNativeSealQuorumCertificateV1::from_votes(proposal.subject.clone(), set, votes).unwrap();
    let decision_votes = (0..3)
        .map(|index| {
            workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
                stores[index].persist_local_verified_qc(view, &qc, set)?;
                stores[index].sign_local_decision_vote_v3(view, &qc, set, &keys[index])
            })
            .unwrap()
        })
        .collect();
    let decision = NovNativeSealDecisionCertificateV3::from_votes(qc, set, decision_votes).unwrap();
    workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
        stores[0].persist_local_verified_decision_certificate_v3(view, &decision, set)
    })
    .unwrap();
    drop(stores);
    let intent =
        workspace::prepare_genesis_promotion_v1(chain, id, pin, &seal_paths[0], params).unwrap();
    assert_eq!(intent.decision, decision);
    workspace::publish_genesis_promotion_v1(chain, id, pin, params).unwrap();
    workspace::complete_genesis_promotion_v1(chain, id, pin, params).unwrap();
    let authority = NovNativeSealEpochAuthorityV1::derive_operator_pinned_fresh_genesis_epoch(
        config,
        pin,
        set.validators
            .iter()
            .map(|validator| NovNativeSealValidatorTransportBindingV1 {
                validator_id: validator.validator_id,
                transport_peer_id: novovm_network::peer_id_from_ed25519_public_key_v1(
                    &validator.public_key,
                ),
            })
            .collect(),
    )
    .unwrap();
    let proof = NovNativeFreshFinalityProofV1 {
        authority,
        witness: NovNativeSealRoundMessageV1::DecisionCertificateV3 {
            proposal: Box::new(proposal),
            decision: Box::new(decision),
            certificate: None,
        },
    };
    proof
        .validate_archived_block(config, block.block())
        .unwrap();
    let finalized =
        workspace::finalize_genesis_promotion_v1(chain, id, pin, &proof, params).unwrap();
    assert!(finalized.finalized && finalized.ledger_publication_completed);
}

#[derive(Clone, Default)]
struct RootedTransferCountingReader {
    nodes: BTreeMap<[u8; 32], Vec<u8>>,
    blobs: BTreeMap<[u8; 32], Vec<u8>>,
    measure: std::cell::Cell<bool>,
    node_reads: std::cell::Cell<usize>,
    chunk_reads: std::cell::Cell<usize>,
    record_keys: std::cell::RefCell<std::collections::BTreeSet<Vec<u8>>>,
}

impl RootedTransferCountingReader {
    fn absorb(&mut self, update: &crate::native_state_records::StagedRecordUpdate) {
        for (hash, bytes) in update.nodes() {
            if let Some(previous) = self.nodes.insert(*hash, bytes.clone()) {
                assert_eq!(previous, *bytes, "immutable test node changed");
            }
        }
        for (hash, bytes) in update.blobs() {
            if let Some(previous) = self.blobs.insert(*hash, bytes.clone()) {
                assert_eq!(previous, *bytes, "immutable test blob changed");
            }
        }
    }

    fn begin_measurement(&self) {
        self.node_reads.set(0);
        self.chunk_reads.set(0);
        self.record_keys.borrow_mut().clear();
        self.measure.set(true);
    }

    fn end_measurement(&self) -> (usize, usize, std::collections::BTreeSet<Vec<u8>>) {
        self.measure.set(false);
        (
            self.node_reads.get(),
            self.chunk_reads.get(),
            self.record_keys.borrow().clone(),
        )
    }
}

impl crate::native_state_tree::StateNodeReader for RootedTransferCountingReader {
    fn read_node(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        if self.measure.get() {
            self.node_reads.set(self.node_reads.get() + 1);
        }
        Ok(self.nodes.get(hash).cloned())
    }
}

impl crate::native_state_records::StateRecordReader for RootedTransferCountingReader {
    fn read_record_chunk(&self, hash: [u8; 32], index: u32) -> Result<Option<Vec<u8>>> {
        if self.measure.get() {
            self.chunk_reads.set(self.chunk_reads.get() + 1);
        }
        let Some(blob) = self.blobs.get(&hash) else {
            return Ok(None);
        };
        if self.measure.get() {
            self.record_keys
                .borrow_mut()
                .insert(rooted_transfer_test_blob_parts(blob).0.to_vec());
        }
        let start = usize::try_from(index)?
            .checked_mul(crate::native_state_records::RECORD_CHUNK_BYTES_V1)
            .context("test record chunk offset overflow")?;
        Ok(blob.get(start..).map(|tail| {
            tail[..tail
                .len()
                .min(crate::native_state_records::RECORD_CHUNK_BYTES_V1)]
                .to_vec()
        }))
    }
}

// Inspect only our own canonical fixture blobs to attribute measured reads.
// Production parsing/hash validation remains in read_record, never this helper.
fn rooted_transfer_test_blob_parts(blob: &[u8]) -> (&[u8], &[u8]) {
    assert_eq!(&blob[..4], b"NRB1");
    let key_end = 10 + usize::from(u16::from_be_bytes(blob[4..6].try_into().unwrap()));
    (&blob[10..key_end], &blob[key_end..])
}

#[test]
fn candidate_workspace_record_profile_rooted_transfer_point_reads_ignore_unrelated_history() {
    transfer_candidate_on_runtime_stack(|| {
        use crate::native_state_records::{stage_record_update, RecordChange};
        use crate::native_state_tree::empty_root;
        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        with_plan_runtime(|_path, params| {
            let chain = 98_919_731;
            let (payer_seed, recipient_seed) = ([0x7a; 32], [0x7b; 32]);
            let raw = transfer_candidate_raw(chain, 0, payer_seed, recipient_seed, 17);
            let transaction = decode_nov_native_tx_wire_v1(&raw).unwrap();
            let ir = nov_native_tx_to_adapter_tx_ir_v1(&transaction).unwrap();
            let tx_hash = tx_hash_array_from_ir_v1(&ir);
            // Real signature verification precedes every execution fixture.
            verify_nov_native_auth_v1(params, &transaction, &ir, tx_hash).unwrap();
            let reservation = transfer_candidate_reservation(&raw);
            let request = native_transfer_dispatch::fee_request_v1(&transaction, tx_hash).unwrap();
            let subject = fallback_execution_subject_meta_v1(&request);
            let ingress = NovAoemSemanticIngressMetaV1 {
                execution_kernel: "AOEM".into(),
                semantic_entry: "test.rooted_transfer.compute".into(),
                plan_id: 731,
                wire_digest: to_hex(&sha256_bytes_v1(&[&raw])),
                // This component test submits real generic compute tasks, not
                // the candidate's separate raw-wire precommit. Do not claim it.
                ..Default::default()
            };
            let item = || native_transfer_dispatch::Item {
                transaction: &transaction,
                request: &request,
                subject: &subject,
                reservation: &reservation,
                ingress: ingress.clone(),
            };
            let mut measurements = Vec::new();
            for history in [0usize, 1024] {
                let mut original = NovNativeExecutionStoreV1 {
                    authority_chain_id: Some(chain),
                    authority_namespace_digest: native_aoem_owned_state_namespace_digest_v1(
                        params, chain,
                    ),
                    ..Default::default()
                };
                bind_native_business_protocol_config_v1(&mut original).unwrap();
                original.module_state.account_asset_balances.insert(
                    subject.account_id.clone(),
                    BTreeMap::from([("NOV".into(), 10_000), ("USDT".into(), 91)]),
                );
                original.module_state.account_asset_balances.insert(
                    transfer_candidate_account(recipient_seed),
                    BTreeMap::from([("NOV".into(), 20)]),
                );
                let mut historical_keys = std::collections::BTreeSet::new();
                for index in 0..history {
                    let account = format!("unrelated-history-{index}");
                    original.module_state.account_asset_balances.insert(
                        account.clone(),
                        BTreeMap::from([("NOV".into(), u128::MAX - index as u128)]),
                    );
                    historical_keys.insert(
                        native_store_records::key(&[
                            "module_state".into(),
                            "account_asset_balances".into(),
                            account.clone(),
                        ])
                        .unwrap(),
                    );
                    historical_keys.insert(
                        native_store_records::key(&[
                            "module_state".into(),
                            "account_asset_balances".into(),
                            account,
                            "NOV".into(),
                        ])
                        .unwrap(),
                    );
                }
                // Deliberately seed unrelated record history, not a claimed
                // independently finalized chain. The executor consumes trusted
                // tree roots; full candidate/authority gates are tested above.
                for index in 0..usize::from(history != 0) * 100 {
                    let mut receipt = build_failed_native_receipt_v1(
                        &request,
                        &unresolved_settled_fee_v1(&request),
                        &subject,
                        "fee".into(),
                        "quote".into(),
                        "old fixture receipt".into(),
                    );
                    receipt.tx_hash = format!("{:064x}", index + 1);
                    historical_keys.insert(
                        native_store_records::key(&["receipts".into(), receipt.tx_hash.clone()])
                            .unwrap(),
                    );
                    historical_keys.insert(
                        parse_fixed_hex_32_v1(&receipt.tx_hash, "old receipt")
                            .unwrap()
                            .to_vec(),
                    );
                    original.receipts.insert(receipt.tx_hash.clone(), receipt);
                }
                let physical_records = native_store_records::encode(&original).unwrap();
                let parent_records = physical_records.len();
                let parent_bytes = physical_records
                    .iter()
                    .map(|(key, value)| 10 + key.len() + value.len())
                    .sum::<usize>();
                let physical = stage_record_update(
                    &RootedTransferCountingReader::default(),
                    empty_root(),
                    &physical_records
                        .into_iter()
                        .map(|(key, value)| RecordChange::Put { key, value })
                        .collect::<Vec<_>>(),
                )
                .unwrap();
                let state = native_record_commitment::stage_consensus_import_v1(
                    &RootedTransferCountingReader::default(),
                    &original.module_state,
                )
                .unwrap();
                let receipts = native_record_commitment::stage_receipt_import_v1(
                    &RootedTransferCountingReader::default(),
                    &original,
                )
                .unwrap();
                let mut reader = RootedTransferCountingReader::default();
                for update in [&physical, &state, &receipts] {
                    reader.absorb(update);
                }

                // Imports are complete before counters start. No scan/import or
                // cold materialization is part of the measured execution scope.
                reader.begin_measurement();
                let update = native_transfer_record_execution::execute_rooted_segment_v1(
                    &reader,
                    physical.root(),
                    state.root(),
                    receipts.root(),
                    &[item()],
                    123,
                )
                .unwrap();
                let measured = reader.end_measurement();
                assert_eq!(
                    update.peak_inflight, 1,
                    "one real AOEM callback must execute"
                );
                assert!(measured.0 > 0 && measured.1 > 0);
                assert!(
                    measured.2.is_disjoint(&historical_keys),
                    "rooted computation read an unrelated historical blob"
                );
                eprintln!("rooted transfer history_accounts={history} historical_receipts={} node_reads={} chunk_reads={} unique_record_keys={} peak_inflight={}", usize::from(history != 0) * 100, measured.0, measured.1, measured.2.len(), update.peak_inflight);
                measurements.push(measured);

                let (records, bytes) = update
                    .stats
                    .checked_apply(parent_records, parent_bytes)
                    .unwrap();
                let actual = native_transfer_record_execution::materialize_update_v1(
                    &reader,
                    &update.physical,
                    records,
                    bytes,
                )
                .unwrap();
                let mut serial = original.clone();
                native_transfer_record_execution::execute_segment_v1(&mut serial, &[item()], 123)
                    .unwrap();
                assert_eq!(actual, serial, "rooted and cold scheduling must preserve complete state, receipts, fees and nonce");
                assert_eq!(
                    serde_json::to_vec(&actual).unwrap(),
                    serde_json::to_vec(&serial).unwrap()
                );
                assert_eq!(
                    native_record_commitment::consensus_state_root_v1(&actual.module_state)
                        .unwrap(),
                    update.state.root()
                );
                assert_eq!(
                    native_record_commitment::cumulative_receipt_root_v1(&actual).unwrap(),
                    update.receipts.root()
                );
                let fee = transfer_candidate_fee(&raw);
                assert_eq!(
                    native_account_asset_balance_v1(&actual, &subject.account_id, "NOV"),
                    10_000 - 17 - fee
                );
                assert_eq!(
                    native_account_asset_balance_v1(
                        &actual,
                        &transfer_candidate_account(recipient_seed),
                        "NOV"
                    ),
                    37
                );
                assert_eq!(
                    actual.module_state.native_auth_next_nonces[&reservation.identity_key],
                    1
                );
                assert_eq!(actual.receipts[&reservation.tx_hash].settled_fee_nov, fee);
                assert_eq!(actual.module_state.treasury_reserves["NOV"], fee);

                if history == 0 {
                    let mut wrong = original.clone();
                    wrong
                        .module_state
                        .account_asset_balances
                        .get_mut(&subject.account_id)
                        .unwrap()
                        .insert("NOV".into(), 9_999);
                    let wrong_state = native_record_commitment::stage_consensus_import_v1(
                        &RootedTransferCountingReader::default(),
                        &wrong.module_state,
                    )
                    .unwrap();
                    reader.absorb(&wrong_state);
                    let error = native_transfer_record_execution::execute_rooted_segment_v1(
                        &reader,
                        physical.root(),
                        wrong_state.root(),
                        receipts.root(),
                        &[item()],
                        123,
                    )
                    .err()
                    .expect("different state root must be rejected");
                    assert!(
                        format!("{error:#}").contains("physical/state read mismatch"),
                        "{error:#}"
                    );

                    let mut wrong = original.clone();
                    wrong.receipts.insert(
                        reservation.tx_hash.clone(),
                        actual.receipts[&reservation.tx_hash].clone(),
                    );
                    let wrong_receipts = native_record_commitment::stage_receipt_import_v1(
                        &RootedTransferCountingReader::default(),
                        &wrong,
                    )
                    .unwrap();
                    reader.absorb(&wrong_receipts);
                    let error = native_transfer_record_execution::execute_rooted_segment_v1(
                        &reader,
                        physical.root(),
                        state.root(),
                        wrong_receipts.root(),
                        &[item()],
                        123,
                    )
                    .err()
                    .expect("different receipt root must be rejected");
                    assert!(
                        format!("{error:#}").contains("physical/receipt read mismatch"),
                        "{error:#}"
                    );

                    let payer_key = native_store_records::key(&[
                        "module_state".into(),
                        "account_asset_balances".into(),
                        subject.account_id.clone(),
                        "NOV".into(),
                    ])
                    .unwrap();
                    let blob_hash = physical
                        .blobs()
                        .iter()
                        .find_map(|(hash, blob)| {
                            let (key, value) = rooted_transfer_test_blob_parts(blob);
                            (key == payer_key && value.starts_with(b"NSV1")).then_some(*hash)
                        })
                        .expect("payer's physical NOV record blob");
                    let mut missing = reader.clone();
                    missing.blobs.remove(&blob_hash);
                    let error = native_transfer_record_execution::execute_rooted_segment_v1(
                        &missing,
                        physical.root(),
                        state.root(),
                        receipts.root(),
                        &[item()],
                        123,
                    )
                    .err()
                    .expect("missing touched blob cannot synthesize a zero balance");
                    assert!(
                        format!("{error:#}").contains("state record chunk missing"),
                        "{error:#}"
                    );
                }
            }
            let small = &measurements[0];
            let large = &measurements[1];
            assert_eq!(
                large.1, small.1,
                "adding unrelated history must not add record-blob reads"
            );
            assert_eq!(
                large.2, small.2,
                "record access set must depend on the transfer, not history"
            );
            // Patricia traversal grows with key-path depth, not a full-tree
            // scan. The exact blob-set assertion above is the stronger guard.
            assert!(
                large.0 <= small.0 * 4 + 128,
                "unexpected node-read growth: small={} large={}",
                small.0,
                large.0
            );
        });
    });
}
