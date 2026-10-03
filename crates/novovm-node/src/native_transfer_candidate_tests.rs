// Included beside the existing isolated-candidate fixtures. These tests use
// the real AOEM runtime. Record-profile tests also finalize an isolated test
// chain; the legacy cases below leave their candidates unpublished.
include!("native_record_candidate_tests.rs");
include!("native_transfer_process_parity_tests.rs");
fn transfer_candidate_raw(
    chain: u64,
    nonce: u64,
    seed: [u8; 32],
    recipient: [u8; 32],
    amount: u128,
) -> Vec<u8> {
    let mut tx = NovNativeTxWireV1 {
        chain_id: chain,
        kind: NovTxKindV1::Transfer(novovm_protocol::NovTransferTxV1 {
            from: Vec::new(),
            to: novovm_adapter_novovm::address_from_seed_v1(recipient),
            asset: "NOV".into(),
            amount,
            nonce,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "NOV".into(),
                max_pay_amount: 1_000,
                slippage_bps: 0,
            },
        }),
        signature: Vec::new(),
    };
    sign_nov_native_tx_with_seed_v1(&mut tx, seed).unwrap();
    encode_native_auth_test_tx_v1(&tx)
}

fn transfer_candidate_account(seed: [u8; 32]) -> String {
    to_hex_prefixed_v1(&novovm_adapter_novovm::address_from_seed_v1(seed))
}

fn transfer_candidate_reservation(raw: &[u8]) -> NovNativeDurableAuthReservationV1 {
    let tx = decode_nov_native_tx_wire_v1(raw).unwrap();
    let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
    nov_native_durable_auth_reservation_v1(&tx, &ir, tx_hash_array_from_ir_v1(&ir)).unwrap()
}

fn transfer_candidate_fee(raw: &[u8]) -> u128 {
    let tx = decode_nov_native_tx_wire_v1(raw).unwrap();
    let request = match &tx.kind {
        NovTxKindV1::Transfer(_) => native_transfer_dispatch::fee_request_v1(
            &tx,
            canonical_nov_native_tx_hash_from_payload_v1(raw).unwrap(),
        )
        .unwrap(),
        NovTxKindV1::Execute(_) => nov_native_tx_to_execution_request_v1(&tx).unwrap().unwrap(),
        _ => unreachable!("transfer candidate fixture only uses executable transactions"),
    };
    estimate_execution_fee_nov_v1(&request)
}

fn transfer_candidate_on_runtime_stack(test: fn()) {
    std::thread::Builder::new()
        .name("transfer-candidate-integration".into())
        .stack_size(crate::native_block_seal::service::FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1)
        .spawn(test)
        .unwrap()
        .join()
        .unwrap();
}

fn assert_transfer_candidate_compute_logs(receipt: &NovNativeExecutionReceiptV1, expected: usize) {
    let logs: Vec<_> = receipt
        .logs
        .iter()
        .filter(|log| log.event == "aoem.native_transfer.computed")
        .collect();
    assert_eq!(logs.len(), expected);
    for log in logs {
        assert_eq!(log.data["scheduler"], "aoem_generic_compute_v2");
        assert_eq!(log.data["phase"], "pre_global_fee_reduction");
        assert_eq!(log.data["authorizes_state_publication"], false);
    }
}

#[test]
fn candidate_workspace_transfer_mixed_execute_fee_nonce_and_recovery() {
    transfer_candidate_on_runtime_stack(exercise_transfer_candidate_mixed_execution);
}

#[test]
fn candidate_workspace_record_bundle_documents_recover_without_repair() {
    transfer_candidate_on_runtime_stack(|| {
        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        with_plan_runtime(|path, params| {
            funded_candidate_parent(
                path,
                params,
                98_919_725,
                &[[0xa1; 32]],
                raw_fixture(98_919_725, 1725),
            );
            workspace::exercise_record_profile_document_storage_for_test(98_919_725, params)
                .unwrap();
        });
    });
}

#[test]
fn candidate_workspace_record_documents_exceed_old_snapshot_limit_and_reopen() {
    transfer_candidate_on_runtime_stack(|| {
        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        with_plan_runtime(|path, params| {
            let chain = 98_919_710;
            funded_candidate_parent(path, params, chain, &[[0xa1; 32]], raw_fixture(chain, 1710));
            workspace::exercise_record_document_storage_for_test(chain, params).unwrap();
        });
    });
}

fn exercise_transfer_candidate_mixed_execution() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_919_701;
    let (a, b, c, d) = ([0xa1; 32], [0xa2; 32], [0xa3; 32], [0xa4; 32]);
    with_plan_runtime(|path, params| {
        let parent =
            funded_candidate_parent(path, params, chain, &[a, c], raw_fixture(chain, 1701));
        let parent_store = load_validated_native_state_envelope_from_aoem_owner_v1(params, chain)
            .unwrap()
            .unwrap()
            .store;
        let plan = successor_plan(
            &parent,
            vec![
                transfer_candidate_raw(chain, 0, a, b, 100),
                transfer_candidate_raw(chain, 0, c, d, 200), // independent of the first transfer
                transfer_candidate_raw(chain, 1, a, b, 50),  // must reread the preceding segment
                candidate_workspace_execution_raw(chain, 2, a, 25, "deposit_reserve"),
                transfer_candidate_raw(chain, 3, a, b, 10), // Execute is an ordered barrier
                transfer_candidate_raw(chain, 0, b, d, 1_000_000), // affordable fee, failed business
                transfer_candidate_raw(chain, 1, b, a, 20), // failed business consumed nonce zero
            ],
        );
        let fees: Vec<_> = plan
            .raw_txs
            .iter()
            .map(|raw| transfer_candidate_fee(raw))
            .collect();
        let ready = workspace::create_v1(&plan, params).unwrap();
        let authority =
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);

        // Stop after all candidate output bytes have been durably written,
        // before their completion marker. Recovery must not charge fees twice.
        let error =
            workspace::execute_with_checkpoint_v1(chain, ready.workspace_id, params, |stage| {
                if stage == workspace::ExecutionCheckpointV1::OutputWritten {
                    anyhow::bail!("transfer fixture: output written before completion");
                }
                Ok(())
            })
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("output written before completion"));
        assert!(
            workspace::load_execution_v1(chain, ready.workspace_id, params)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes),
            authority
        );

        reset_native_aoem_semantic_ingress_session_v1();
        let completed = std::cell::Cell::new(0);
        let result =
            workspace::execute_with_checkpoint_v1(chain, ready.workspace_id, params, |stage| {
                assert_eq!(
                    stage,
                    workspace::ExecutionCheckpointV1::Completed,
                    "fully written recovery must not recompute transfer outcomes"
                );
                completed.set(completed.get() + 1);
                Ok(())
            })
            .unwrap();
        assert_eq!(completed.get(), 1);
        assert_candidate_workspace_execution_complete(&result);
        assert_eq!(
            result.business_transition_computation_owner,
            "AOEM_transfer_compute_and_SUPERVM_host_ordered_settlement_or_execute"
        );
        assert_eq!(
            result
                .batch_result
                .per_tx_receipts
                .iter()
                .map(|receipt| receipt.status_ok)
                .collect::<Vec<_>>(),
            vec![true, true, true, true, true, false, true]
        );
        let snapshot =
            workspace::load_execution_snapshot_for_test_v1(chain, ready.workspace_id, params)
                .unwrap();
        let store: NovNativeExecutionStoreV1 = serde_json::from_value(snapshot.clone()).unwrap();
        for (index, (hash, fee)) in plan.tx_hashes.iter().zip(&fees).enumerate() {
            let receipt = &store.receipts[&to_hex(hash)];
            assert_eq!(receipt.settled_fee_nov, *fee, "fee at transaction {index}");
            assert_eq!(receipt.paid_amount, *fee);
            assert_eq!(receipt.paid_asset, "NOV");
            assert_eq!(receipt.status, index != 5);
            assert_transfer_candidate_compute_logs(receipt, usize::from(index != 3));
        }
        assert!(store.receipts[&to_hex(&plan.tx_hashes[5])]
            .failure_reason
            .as_deref()
            .unwrap()
            .starts_with("native.transfer.insufficient NOV"));
        let balance = |seed| {
            native_account_asset_balance_v1(&store, &transfer_candidate_account(seed), "NOV")
        };
        assert_eq!(
            balance(a),
            1_000 - 100 - 50 - 25 - 10 + 20 - fees[0] - fees[2] - fees[3] - fees[4]
        );
        assert_eq!(balance(b), 160 - 20 - fees[5] - fees[6]);
        assert_eq!(balance(c), 1_000 - 200 - fees[1]);
        assert_eq!(balance(d), 200);
        for (raw_index, nonce) in [(4, 4), (6, 2), (1, 1)] {
            let reservation = transfer_candidate_reservation(&plan.raw_txs[raw_index]);
            assert_eq!(
                store.module_state.native_auth_next_nonces[&reservation.identity_key],
                nonce
            );
        }

        // Fees are split PER TRANSACTION, including the failed business call;
        // rounding a summed batch fee would produce different bucket balances.
        let policy = resolve_treasury_settlement_policy_v1(&parent_store);
        let mut splits = (0u128, 0u128, 0u128);
        for (hash, fee) in plan.tx_hashes.iter().zip(&fees) {
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
                .find(|entry| entry.kind == "fee_settlement" && entry.tx_hash == tx_hash)
                .unwrap();
            assert_eq!(
                journal.reserve_bucket_delta_nov,
                i128::try_from(reserve).unwrap()
            );
            assert_eq!(
                journal.fee_bucket_delta_nov,
                i128::try_from(net_fee).unwrap()
            );
            assert_eq!(journal.risk_buffer_delta_nov, i128::try_from(risk).unwrap());
        }
        let fee_sum: u128 = fees.iter().sum();
        assert_eq!(splits.0 + splits.1 + splits.2, fee_sum);
        assert_eq!(
            store.module_state.treasury_reserve_bucket_nov
                - parent_store.module_state.treasury_reserve_bucket_nov,
            splits.0
        );
        assert_eq!(
            store.module_state.treasury_fee_bucket_nov
                - parent_store.module_state.treasury_fee_bucket_nov,
            splits.1
        );
        assert_eq!(
            store.module_state.treasury_risk_buffer_nov
                - parent_store.module_state.treasury_risk_buffer_nov,
            splits.2
        );
        assert_eq!(
            store.module_state.treasury_settled_nov_total
                - parent_store.module_state.treasury_settled_nov_total,
            fee_sum
        );
        assert_eq!(
            store.module_state.treasury_reserves["NOV"]
                - parent_store
                    .module_state
                    .treasury_reserves
                    .get("NOV")
                    .copied()
                    .unwrap_or(0),
            fee_sum + 25
        );
        assert_eq!(
            balance(a) + balance(b) + balance(c) + balance(d) + fee_sum + 25,
            2_000
        );
        let block = workspace::load_block_artifact_v1(chain, ready.workspace_id, params)
            .unwrap()
            .unwrap();
        assert_eq!(block.block().body.tx_hashes, plan.tx_hashes);
        assert!(!block.block().header.finalized && !block.block().header.proof_sealed);
        let replay =
            workspace::execute_with_checkpoint_v1(chain, ready.workspace_id, params, |_| {
                panic!("completed transfer candidate must be read back, not executed again")
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(replay).unwrap(),
            serde_json::to_value(result).unwrap()
        );
        assert_eq!(
            workspace::load_execution_snapshot_for_test_v1(chain, ready.workspace_id, params)
                .unwrap(),
            snapshot
        );
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes),
            authority
        );
    });
}

#[test]
fn candidate_workspace_transfer_auth_rejections_and_fresh_only_admission() {
    transfer_candidate_on_runtime_stack(exercise_transfer_candidate_auth_rejections);
}

fn exercise_transfer_candidate_auth_rejections() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_919_702;
    let (a, b) = ([0xb1; 32], [0xb2; 32]);
    with_plan_runtime(|path, params| {
        let parent = funded_candidate_parent(path, params, chain, &[a], raw_fixture(chain, 1702));
        let valid = transfer_candidate_raw(chain, 0, a, b, 10);
        let admitted =
            fresh_pool::PendingTransaction::authenticate(valid.clone(), chain, params).unwrap();
        assert_eq!(admitted.raw, valid);
        assert_eq!(admitted.nonce, 0);
        assert!(
            ingest_local_nov_raw_tx_payload_v1(params, &valid).is_err(),
            "legacy ingress must not admit a Transfer without its candidate executor"
        );
        assert!(get_network_runtime_native_pending_tx_payload_v1(chain, admitted.hash).is_none());

        let mut invalid_signature =
            decode_nov_native_tx_wire_v1(&transfer_candidate_raw(chain, 0, b, a, 12)).unwrap();
        invalid_signature.signature[40] ^= 1;
        let invalid_signature = encode_native_auth_test_tx_v1(&invalid_signature);
        assert!(fresh_pool::PendingTransaction::authenticate(
            invalid_signature.clone(),
            chain,
            params
        )
        .is_err());
        let cases = [
            (
                "invalid final signature",
                vec![valid.clone(), invalid_signature],
            ),
            (
                "nonce gap",
                vec![transfer_candidate_raw(chain, 1, a, b, 10)],
            ),
            (
                "duplicate signer nonce",
                vec![valid.clone(), transfer_candidate_raw(chain, 0, a, b, 11)],
            ),
            (
                "wrong chain",
                vec![transfer_candidate_raw(chain + 1, 0, a, b, 10)],
            ),
        ];
        for (label, raw_txs) in cases {
            let plan = successor_plan(&parent, raw_txs);
            let ready = workspace::create_v1(&plan, params).unwrap();
            let authority =
                candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
            let counters = native_aoem_semantic_ingress_runtime_reuse_counters_v1();
            let error =
                workspace::execute_with_checkpoint_v1(chain, ready.workspace_id, params, |_| {
                    panic!("{label}: authentication must precede AOEM output checkpoints")
                })
                .unwrap_err();
            assert!(!error.to_string().is_empty(), "{label}");
            assert!(
                workspace::load_execution_v1(chain, ready.workspace_id, params)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                native_aoem_semantic_ingress_runtime_reuse_counters_v1(),
                counters,
                "{label}"
            );
            assert_eq!(
                candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes),
                authority,
                "{label}"
            );
        }
        let plan = successor_plan(&parent, vec![valid]);
        let ready = workspace::create_v1(&plan, params).unwrap();
        let result = workspace::execute_v1(chain, ready.workspace_id, params).unwrap();
        assert_candidate_workspace_execution_complete(&result);
        assert!(
            result.batch_result.per_tx_receipts[0].status_ok,
            "rejected candidates and legacy admission must not reserve the valid nonce"
        );
        clear_native_auth_runtime_reservations_for_chain_v1(chain);
    });
}

#[test]
fn candidate_workspace_transfer_global_fee_pause_keeps_balances_and_advances_nonces() {
    transfer_candidate_on_runtime_stack(exercise_transfer_candidate_global_fee_pause);
}

fn exercise_transfer_candidate_global_fee_pause() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_919_703;
    let (a, b) = ([0xc5; 32], [0xc6; 32]);
    // The policy is pinned before the parent is built; no mid-candidate config change.
    with_env_override_v1(NOV_NATIVE_TREASURY_SETTLEMENT_PAUSED_ENV, "true", || {
        with_plan_runtime(|path, params| {
            let parent =
                funded_candidate_parent(path, params, chain, &[a], raw_fixture(chain, 1703));
            let parent_store =
                load_validated_native_state_envelope_from_aoem_owner_v1(params, chain)
                    .unwrap()
                    .unwrap()
                    .store;
            assert!(resolve_treasury_settlement_policy_v1(&parent_store).settlement_paused);
            let plan = successor_plan(
                &parent,
                vec![
                    transfer_candidate_raw(chain, 0, a, b, 100),
                    transfer_candidate_raw(chain, 0, b, a, 30),
                    transfer_candidate_raw(chain, 1, a, b, 999),
                ],
            );
            let ready = workspace::create_v1(&plan, params).unwrap();
            let authority =
                candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
            native_transfer_dispatch::take_component_observation_for_test_v1();
            let result = workspace::execute_v1(chain, ready.workspace_id, params).unwrap();
            let observation = native_transfer_dispatch::take_component_observation_for_test_v1()
                .expect("paused candidate must execute a real Transfer graph");
            assert_eq!(observation.transactions, plan.raw_txs.len());
            assert_eq!(observation.graphs, 1);
            assert!(observation.recomputed_transactions > 0);
            assert_candidate_workspace_execution_complete(&result);
            assert!(result
                .batch_result
                .per_tx_receipts
                .iter()
                .all(|receipt| !receipt.status_ok));
            let store: NovNativeExecutionStoreV1 = serde_json::from_value(
                workspace::load_execution_snapshot_for_test_v1(chain, ready.workspace_id, params)
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                store.module_state.account_asset_balances,
                parent_store.module_state.account_asset_balances,
                "global fee refusal must discard both speculative fee and transfer writes"
            );
            assert_eq!(
                store.module_state.treasury_reserves,
                parent_store.module_state.treasury_reserves
            );
            assert_eq!(
                store.module_state.treasury_settled_nov_total,
                parent_store.module_state.treasury_settled_nov_total
            );
            assert_eq!(
                store.module_state.treasury_reserve_bucket_nov,
                parent_store.module_state.treasury_reserve_bucket_nov
            );
            assert_eq!(
                store.module_state.treasury_fee_bucket_nov,
                parent_store.module_state.treasury_fee_bucket_nov
            );
            assert_eq!(
                store.module_state.treasury_risk_buffer_nov,
                parent_store.module_state.treasury_risk_buffer_nov
            );
            for hash in &plan.tx_hashes {
                let receipt = &store.receipts[&to_hex(hash)];
                assert_transfer_candidate_compute_logs(receipt, 1);
                assert_eq!((receipt.settled_fee_nov, receipt.paid_amount), (0, 0));
                assert_eq!((&*receipt.module, &*receipt.method), ("fee", "settlement"));
                assert!(receipt
                    .failure_reason
                    .as_deref()
                    .unwrap()
                    .starts_with("fee.settlement.settlement_paused"));
            }
            for (raw_index, nonce) in [(2, 2), (1, 1)] {
                let reservation = transfer_candidate_reservation(&plan.raw_txs[raw_index]);
                assert_eq!(
                    store.module_state.native_auth_next_nonces[&reservation.identity_key],
                    nonce
                );
            }
            assert_eq!(
                candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes),
                authority
            );
        });
    });
}

fn transfer_candidate_capacity_parent(
    path: &Path,
    params: &serde_json::Value,
    chain: u64,
    seed: [u8; 32],
) -> NovNativeDurableBlockV1 {
    // Test-only exact import, before any AOEM authority exists. Saturating the
    // settlement counter exercises the same checked-capacity gate without
    // inventing balances beyond the JSON fixture's integer representation.
    assert!(
        load_validated_native_state_envelope_from_aoem_owner_v1(params, chain)
            .unwrap()
            .is_none()
    );
    assert!(!native_host_projection_has_state_v1(
        &load_nov_native_execution_store_v1(path).unwrap()
    ));
    let mut allocation = NovNativeExecutionStoreV1::default();
    credit_native_account_asset_balance_v1(
        &mut allocation,
        &transfer_candidate_account(seed),
        "NOV",
        1_000,
    );
    allocation.module_state.treasury_settlements = u64::MAX;
    let namespace = native_aoem_owned_state_namespace_digest_v1(params, chain);
    let anchor =
        native_host_projection_bootstrap_anchor_commitment_v1(&allocation, chain, &namespace)
            .unwrap();
    save_nov_native_execution_store_v1(path, &allocation).unwrap();
    let mut expected = allocation.clone();
    bind_native_business_protocol_config_v1(&mut expected).unwrap();
    let base = genesis_plan(chain, vec![raw_fixture(chain, 1704)]);
    let plan = make_plan(
        base.context,
        parse_fixed_hex_32_v1(
            &native_semantic_ledger_state_digest_v1(&expected.module_state),
            "capacity fixture pre-state",
        )
        .unwrap(),
        None,
        base.raw_txs,
    );
    with_env_override_v1(
        "NOVOVM_ALLOW_AOEM_STATE_BOOTSTRAP_FROM_HOST",
        "true",
        || {
            with_env_override_v1(
                NOV_NATIVE_AOEM_STATE_BOOTSTRAP_HOST_ANCHOR_ENV,
                &anchor,
                || {
                    verify_native_aoem_state_bootstrap_from_host_authorization_v1(
                        &allocation,
                        chain,
                        &namespace,
                    )
                    .unwrap();
                    committed_block(
                        &run_nov_native_candidate_execution_plan_v1(&plan, params).unwrap(),
                    )
                },
            )
        },
    )
}

#[test]
fn candidate_workspace_transfer_global_capacity_refusal_discards_speculative_credit() {
    transfer_candidate_on_runtime_stack(exercise_transfer_candidate_global_capacity_refusal);
}

fn exercise_transfer_candidate_global_capacity_refusal() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_919_704;
    let (a, b) = ([0xd5; 32], [0xd6; 32]);
    with_plan_runtime(|path, params| {
        let parent = transfer_candidate_capacity_parent(path, params, chain, a);
        let parent_store = load_validated_native_state_envelope_from_aoem_owner_v1(params, chain)
            .unwrap()
            .unwrap()
            .store;
        assert_eq!(parent_store.module_state.treasury_settlements, u64::MAX);
        assert!(!resolve_treasury_settlement_policy_v1(&parent_store).settlement_paused);
        let plan = successor_plan(
            &parent,
            vec![
                transfer_candidate_raw(chain, 0, a, b, 100),
                transfer_candidate_raw(chain, 0, b, a, 30),
                transfer_candidate_raw(chain, 1, a, b, 999),
            ],
        );
        let ready = workspace::create_v1(&plan, params).unwrap();
        let authority =
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
        native_transfer_dispatch::take_component_observation_for_test_v1();
        let result = workspace::execute_v1(chain, ready.workspace_id, params).unwrap();
        let observation = native_transfer_dispatch::take_component_observation_for_test_v1()
            .expect("capacity-refused candidate must execute a real Transfer graph");
        assert_eq!(observation.transactions, plan.raw_txs.len());
        assert_eq!(observation.graphs, 1);
        assert!(observation.recomputed_transactions > 0);
        assert_candidate_workspace_execution_complete(&result);
        assert!(result
            .batch_result
            .per_tx_receipts
            .iter()
            .all(|receipt| !receipt.status_ok));
        let snapshot =
            workspace::load_execution_snapshot_for_test_v1(chain, ready.workspace_id, params)
                .unwrap();
        let store: NovNativeExecutionStoreV1 = serde_json::from_value(snapshot.clone()).unwrap();
        assert_eq!(
            store.module_state.account_asset_balances,
            parent_store.module_state.account_asset_balances
        );
        assert_eq!(store.module_state.treasury_settlements, u64::MAX);
        assert_eq!(
            store.module_state.treasury_reserves,
            parent_store.module_state.treasury_reserves
        );
        assert_eq!(
            store.module_state.treasury_settled_nov_total,
            parent_store.module_state.treasury_settled_nov_total
        );
        assert_eq!(
            store.module_state.treasury_reserve_bucket_nov,
            parent_store.module_state.treasury_reserve_bucket_nov
        );
        assert_eq!(
            store.module_state.treasury_fee_bucket_nov,
            parent_store.module_state.treasury_fee_bucket_nov
        );
        assert_eq!(
            store.module_state.treasury_risk_buffer_nov,
            parent_store.module_state.treasury_risk_buffer_nov
        );
        for (index, hash) in plan.tx_hashes.iter().enumerate() {
            let receipt = &store.receipts[&to_hex(hash)];
            assert_transfer_candidate_compute_logs(receipt, 1);
            assert_eq!((receipt.settled_fee_nov, receipt.paid_amount), (0, 0));
            assert_eq!((&*receipt.module, &*receipt.method), ("fee", "settlement"));
            let reason = receipt.failure_reason.as_deref().unwrap();
            if index == 1 {
                // B's preceding incoming transfer was only speculative and
                // globally refused. Its dependent payment sees zero, not 100.
                assert!(
                    reason.starts_with("fee.clearing.insufficient_user_balance"),
                    "{reason}"
                );
                assert!(reason.contains("available=0"), "{reason}");
            } else {
                assert!(
                    reason.starts_with("fee.settlement.amount_overflow"),
                    "{reason}"
                );
            }
        }
        for (raw_index, nonce) in [(2, 2), (1, 1)] {
            let reservation = transfer_candidate_reservation(&plan.raw_txs[raw_index]);
            assert_eq!(
                store.module_state.native_auth_next_nonces[&reservation.identity_key],
                nonce
            );
        }
        reset_native_aoem_semantic_ingress_session_v1();
        let reopened = workspace::load_execution_v1(chain, ready.workspace_id, params)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(reopened).unwrap(),
            serde_json::to_value(result).unwrap()
        );
        assert_eq!(
            workspace::load_execution_snapshot_for_test_v1(chain, ready.workspace_id, params)
                .unwrap(),
            snapshot
        );
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes),
            authority
        );
    });
}

#[test]
fn candidate_workspace_transfer_public_key_accounts_keep_balances_and_share_signer_nonce() {
    transfer_candidate_on_runtime_stack(exercise_transfer_candidate_public_key_accounts);
}

fn exercise_transfer_candidate_public_key_accounts() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_919_705;
    let (a, b) = ([0xe5; 32], [0xe6; 32]);
    let a20 = novovm_adapter_novovm::address_from_seed_v1(a);
    let a32 = ed25519_dalek::SigningKey::from_bytes(&a)
        .verifying_key()
        .to_bytes()
        .to_vec();
    let b32 = ed25519_dalek::SigningKey::from_bytes(&b)
        .verifying_key()
        .to_bytes()
        .to_vec();
    let exact_raw = |nonce, from: Vec<u8>, to: Vec<u8>, amount| {
        let mut tx =
            decode_nov_native_tx_wire_v1(&transfer_candidate_raw(chain, nonce, a, b, amount))
                .unwrap();
        let NovTxKindV1::Transfer(transfer) = &mut tx.kind else {
            unreachable!()
        };
        transfer.from = from;
        transfer.to = to;
        let expected_kind = tx.kind.clone();
        // The convenience signer rewrites `from` to the 20-byte address. Sign
        // the exact complete wire intent so this genuinely exercises 32 bytes.
        tx.signature.clear();
        let unsigned = nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
        tx.signature = novovm_adapter_novovm::signature_payload_with_seed_v1(&unsigned, a);
        let raw = encode_native_auth_test_tx_v1(&tx);
        assert_eq!(
            decode_nov_native_tx_wire_v1(&raw).unwrap().kind,
            expected_kind
        );
        raw
    };
    with_plan_runtime(|path, params| {
        let parent = funded_candidate_parent(path, params, chain, &[a], raw_fixture(chain, 1705));
        let plan = successor_plan(
            &parent,
            vec![
                exact_raw(0, a20.clone(), a32.clone(), 300),
                exact_raw(1, a32.clone(), b32.clone(), 50),
                exact_raw(2, a32.clone(), a32.clone(), 100), // self transfer only loses its fee
                exact_raw(3, a20.clone(), b32.clone(), 20), // returns to 20-byte form, same nonce owner
            ],
        );
        let reservations: Vec<_> = plan
            .raw_txs
            .iter()
            .map(|raw| transfer_candidate_reservation(raw))
            .collect();
        assert!(reservations
            .iter()
            .all(|entry| entry.identity_key == reservations[0].identity_key));
        let fees: Vec<_> = plan
            .raw_txs
            .iter()
            .map(|raw| transfer_candidate_fee(raw))
            .collect();
        for raw in &plan.raw_txs {
            assert!(
                fresh_pool::PendingTransaction::authenticate(raw.clone(), chain, params).is_ok()
            );
        }
        let ready = workspace::create_v1(&plan, params).unwrap();
        let authority =
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
        let result = workspace::execute_v1(chain, ready.workspace_id, params).unwrap();
        assert_candidate_workspace_execution_complete(&result);
        assert!(result
            .batch_result
            .per_tx_receipts
            .iter()
            .all(|receipt| receipt.status_ok));
        let snapshot =
            workspace::load_execution_snapshot_for_test_v1(chain, ready.workspace_id, params)
                .unwrap();
        let store: NovNativeExecutionStoreV1 = serde_json::from_value(snapshot.clone()).unwrap();
        let a20_key = to_hex_prefixed_v1(&a20);
        let a32_key = to_hex_prefixed_v1(&a32);
        let b32_key = to_hex_prefixed_v1(&b32);
        assert_ne!(a20_key, a32_key);
        let balance20 = native_account_asset_balance_v1(&store, &a20_key, "NOV");
        let balance32 = native_account_asset_balance_v1(&store, &a32_key, "NOV");
        let recipient = native_account_asset_balance_v1(&store, &b32_key, "NOV");
        assert_eq!(balance20, 1_000 - 300 - 20 - fees[0] - fees[3]);
        assert_eq!(balance32, 300 - 50 - fees[1] - fees[2]);
        assert_eq!(recipient, 70);
        assert!(
            !store
                .module_state
                .account_asset_balances
                .contains_key(&transfer_candidate_account(b)),
            "32-byte recipient must not be silently normalized to its 20-byte address"
        );
        assert_eq!(
            balance20 + balance32 + recipient + fees.iter().sum::<u128>(),
            1_000
        );
        assert_eq!(
            store.module_state.native_auth_next_nonces[&reservations[0].identity_key],
            4
        );
        for (index, hash) in plan.tx_hashes.iter().enumerate() {
            let receipt = &store.receipts[&to_hex(hash)];
            let expected_owner = if matches!(index, 1 | 2) {
                &a32_key
            } else {
                &a20_key
            };
            assert_eq!(&receipt.account_id, expected_owner);
            assert_eq!(&receipt.fee_owner_account_id, expected_owner);
            assert_eq!(receipt.settled_fee_nov, fees[index]);
            assert_transfer_candidate_compute_logs(receipt, 1);
        }
        reset_native_aoem_semantic_ingress_session_v1();
        let replay =
            workspace::execute_with_checkpoint_v1(chain, ready.workspace_id, params, |_| {
                panic!("completed 32-byte account candidate must recover without execution")
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(replay).unwrap(),
            serde_json::to_value(result).unwrap()
        );
        assert_eq!(
            workspace::load_execution_snapshot_for_test_v1(chain, ready.workspace_id, params)
                .unwrap(),
            snapshot
        );
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes),
            authority
        );
    });
}
