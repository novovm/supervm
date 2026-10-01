// Included beside the existing real-AOEM candidate fixtures. All stores and
// validator keys below are isolated test fixtures, not a network acceptance run.

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
                vec![transfer_candidate_raw(chain, 2, a, d, 1)],
                params,
            )
            .unwrap();
        assert_eq!(
            next_plan.aoem_parent.as_ref().unwrap().state_root_codec,
            profile.state_root_codec()
        );
        assert_eq!(
            next_plan.aoem_parent.as_ref().unwrap().receipt_root_codec,
            profile.receipt_root_codec()
        );
        let final_head = read_head();
        let next_input = workspace::create_from_finalized_genesis_v1(
            &next_plan,
            input.workspace_id,
            pin,
            params,
        )
        .unwrap();
        let next_result = workspace::execute_v1(chain, next_input.workspace_id, params).unwrap();
        assert_candidate_workspace_execution_complete(&next_result);
        assert!(next_result.batch_result.per_tx_receipts[0].status_ok);
        assert_eq!(next_result.batch_result.snapshot_metadata.state_version, 6);
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
