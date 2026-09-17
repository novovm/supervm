mod native_nonce_identity_tests {
    use super::*;

    #[test]
    fn native_nonce_identity_freezes_new_and_legacy_state_root_vectors() {
        super::native_state_root_v3_binds_semantic_head_and_protocol_config();
    }

    fn with_nonce_fixture<T>(test: impl FnOnce(&Path, &serde_json::Value) -> T) -> T {
        let _guard = common_candidate_plan_tests::PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        with_env_removed_v1(NOV_NATIVE_CHAIN_ID_ENV, || {
            with_env_override_v1(NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV, "json", || {
                with_test_native_execution_store_path_v1(|path| {
                    let params = serde_json::json!({ "native_execution_store_path": path });
                    test(&path, &params)
                })
            })
        })
    }

    // The public signing helper deliberately rewrites caller/subject metadata.
    // Sign the exact fixture intent instead, or alias tests silently test the
    // same normalized 20-byte address over and over.
    fn sign_exact_intent(tx: &mut NovNativeTxWireV1, seed: [u8; 32]) {
        tx.signature.clear();
        let unsigned_ir = nov_native_tx_to_adapter_tx_ir_v1(tx).expect("unsigned alias intent");
        tx.signature = novovm_adapter_novovm::signature_payload_with_seed_v1(&unsigned_ir, seed);
    }

    fn signed_alias(
        chain_id: u64,
        nonce: u64,
        seed: [u8; 32],
        caller_len: usize,
        spelling: usize,
    ) -> NovNativeTxWireV1 {
        let mut tx = build_signed_native_auth_test_tx_v1(
            chain_id,
            nonce,
            seed,
            "nonce-identity-fixture",
            19,
        );
        let public_key = tx.signature[..32].to_vec();
        let NovTxKindV1::Execute(execute) = &mut tx.kind else {
            unreachable!("execute fixture");
        };
        if caller_len == 32 {
            execute.caller = public_key;
        } else {
            assert_eq!(caller_len, 20);
        }
        let canonical = to_hex_prefixed_v1(&execute.caller);
        let bare = to_hex(&execute.caller);
        let fields = match spelling {
            0 => [
                Some(canonical.clone()),
                Some(canonical.clone()),
                Some(canonical),
            ],
            1 => {
                let value = format!("0X{}", bare.to_ascii_uppercase());
                [Some(value.clone()), Some(value.clone()), Some(value)]
            }
            2 => [Some(bare.clone()), Some(bare.clone()), Some(bare)],
            3 => {
                let value = format!(" \t{}\n", canonical.to_ascii_uppercase());
                [Some(value.clone()), Some(value.clone()), Some(value)]
            }
            4 => [None, None, None],
            5 => [
                Some(String::new()),
                Some(String::new()),
                Some(String::new()),
            ],
            6 => [Some(" \t".into()), Some(" \t".into()), Some(" \t".into())],
            7 => [Some(bare), None, None],
            8 => [None, None, Some(bare)],
            9 => [Some(canonical), None, Some(String::new())],
            _ => unreachable!("known account spelling"),
        };
        [
            execute.account_id,
            execute.fee_owner_account_id,
            execute.nonce_owner_account_id,
        ] = fields;
        let expected_kind = tx.kind.clone();
        sign_exact_intent(&mut tx, seed);
        assert_eq!(
            tx.kind, expected_kind,
            "signing must not normalize the fixture"
        );
        let encoded = novovm_protocol::encode_nov_native_tx_wire_v1(&tx).unwrap();
        let decoded = decode_nov_native_tx_wire_v1(&encoded).unwrap();
        assert_eq!(decoded, tx, "wire roundtrip must retain the signed alias");
        decoded
    }

    fn authenticated_reservation(
        tx: &NovNativeTxWireV1,
        params: &serde_json::Value,
    ) -> NovNativeDurableAuthReservationV1 {
        let ir = nov_native_tx_to_adapter_tx_ir_v1(tx).expect("signed intent IR");
        let hash = tx_hash_array_from_ir_v1(&ir);
        let (key, _) = verify_nov_native_auth_v1(params, tx, &ir, hash)
            .expect("valid direct signer must authenticate");
        let reservation = nov_native_durable_auth_reservation_v1(tx, &ir, hash)
            .expect("derive authenticated durable reservation");
        assert_eq!(reservation.runtime_key, key);
        reservation
    }

    #[test]
    fn native_nonce_identity_aliases_and_20_32_byte_callers_share_one_signer_bucket() {
        with_nonce_fixture(|_, params| {
            let chain_id = 98_917_201;
            let seed = [0x91; 32];
            let mut hashes = HashSet::new();
            let mut expected: Option<NovNativeDurableAuthReservationV1> = None;
            for caller_len in [20, 32] {
                for spelling in 0..10 {
                    let tx = signed_alias(chain_id, 0, seed, caller_len, spelling);
                    let reservation = authenticated_reservation(&tx, params);
                    assert!(hashes.insert(reservation.tx_hash.clone()));
                    if let Some(expected) = &expected {
                        assert_eq!(reservation.runtime_key, expected.runtime_key);
                        assert_eq!(reservation.identity_key, expected.identity_key);
                        assert_eq!(reservation.ledger_key, expected.ledger_key);
                        assert_ne!(reservation.reservation_id, expected.reservation_id);
                    } else {
                        expected = Some(reservation);
                    }
                }
            }
            assert_eq!(
                hashes.len(),
                20,
                "distinct signed intents retain distinct hashes"
            );
        });
    }

    #[test]
    fn native_nonce_identity_separates_signers_and_chains_but_not_native_kinds() {
        with_nonce_fixture(|_, params| {
            let chain_id = 98_917_202;
            let seed = [0x92; 32];
            let baseline_tx = signed_alias(chain_id, 0, seed, 20, 0);
            let baseline = authenticated_reservation(&baseline_tx, params);
            let other_signer =
                authenticated_reservation(&signed_alias(chain_id, 0, [0x93; 32], 20, 0), params);
            let other_chain =
                authenticated_reservation(&signed_alias(chain_id + 1, 0, seed, 20, 0), params);
            assert_ne!(baseline.runtime_key.1, other_signer.runtime_key.1);
            assert_ne!(baseline.identity_key, other_signer.identity_key);
            assert_eq!(baseline.runtime_key.1, other_chain.runtime_key.1);
            assert_ne!(baseline.runtime_key, other_chain.runtime_key);
            assert_ne!(baseline.identity_key, other_chain.identity_key);
            assert_ne!(baseline.ledger_key, other_chain.ledger_key);

            for caller_len in [20, 32] {
                let execute = signed_alias(chain_id, 0, seed, caller_len, 0);
                let NovTxKindV1::Execute(execute_kind) = &execute.kind else {
                    unreachable!();
                };
                let caller = execute_kind.caller.clone();
                for kind in [
                    NovTxKindV1::Transfer(novovm_protocol::NovTransferTxV1 {
                        from: caller.clone(),
                        to: vec![0x41; 20],
                        asset: "NOV".into(),
                        amount: 1,
                        nonce: 0,
                        fee_policy: execute_kind.fee_policy.clone(),
                    }),
                    NovTxKindV1::Governance(novovm_protocol::NovGovernanceTxV1 {
                        proposer: caller.clone(),
                        proposal_type: novovm_protocol::NovGovernanceProposalTypeV1::Parameter,
                        payload: br#"{"limit":1}"#.to_vec(),
                        nonce: 0,
                    }),
                ] {
                    let mut tx = NovNativeTxWireV1 {
                        chain_id,
                        kind,
                        signature: Vec::new(),
                    };
                    sign_exact_intent(&mut tx, seed);
                    let reservation = authenticated_reservation(&tx, params);
                    assert_eq!(reservation.runtime_key, baseline.runtime_key);
                    assert_eq!(reservation.ledger_key, baseline.ledger_key);
                    let raw = encode_native_auth_test_tx_v1(&tx);
                    let error = ingest_local_nov_raw_tx_payload_v1(params, &raw)
                        .expect_err("unsupported transaction kinds must remain disabled");
                    assert!(error.to_string().contains("only execute is enabled"));
                    assert!(get_network_runtime_native_pending_tx_payload_v1(
                        chain_id,
                        tx_hash_array_from_ir_v1(&nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap()),
                    )
                    .is_none());
                }
            }
            // Unsupported kinds did not reserve the shared nonce.
            ingest_local_nov_raw_tx_payload_v1(
                params,
                &encode_native_auth_test_tx_v1(&baseline_tx),
            )
            .expect("execute must still admit nonce zero");
            clear_native_auth_runtime_reservations_for_chain_v1(chain_id);
        });
    }

    #[test]
    fn native_nonce_identity_rejects_signed_delegation_and_signature_forgery() {
        with_nonce_fixture(|_, params| {
            let chain_id = 98_917_204;
            let seed = [0x94; 32];
            for caller_len in [20, 32] {
                let baseline = signed_alias(chain_id, 0, seed, caller_len, 0);
                for field in 0..3 {
                    let mut tx = baseline.clone();
                    let NovTxKindV1::Execute(execute) = &mut tx.kind else {
                        unreachable!()
                    };
                    let field_ref = match field {
                        0 => &mut execute.account_id,
                        1 => &mut execute.fee_owner_account_id,
                        _ => &mut execute.nonce_owner_account_id,
                    };
                    *field_ref = Some("unproven-delegated-owner".into());
                    sign_exact_intent(&mut tx, seed);
                    let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
                    let error =
                        verify_nov_native_auth_v1(params, &tx, &ir, tx_hash_array_from_ir_v1(&ir))
                            .expect_err("a real signature is not a delegation proof");
                    assert!(error.to_string().contains("must equal direct signer"));
                }
                for mutation in 0..4 {
                    let mut tx = baseline.clone();
                    match mutation {
                        0 => tx.signature[0] ^= 1,
                        1 => tx.signature[95] ^= 1,
                        2 => {
                            let NovTxKindV1::Execute(execute) = &mut tx.kind else {
                                unreachable!()
                            };
                            execute.caller[0] ^= 1;
                            // A valid signature with an unrelated caller is still invalid.
                            sign_exact_intent(&mut tx, seed);
                        }
                        _ => {
                            let NovTxKindV1::Execute(execute) = &mut tx.kind else {
                                unreachable!()
                            };
                            execute.nonce_owner_account_id = None;
                            // Even a semantically equivalent alias must be signed.
                        }
                    }
                    let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
                    let error =
                        verify_nov_native_auth_v1(params, &tx, &ir, tx_hash_array_from_ir_v1(&ir))
                            .expect_err("forged signer or unsigned metadata must fail");
                    assert!(error
                        .to_string()
                        .contains("signature or signer identity mismatch"));
                }
                let mut crossed = baseline.clone();
                let other_form =
                    signed_alias(chain_id, 0, seed, if caller_len == 20 { 32 } else { 20 }, 0);
                let NovTxKindV1::Execute(other) = other_form.kind else {
                    unreachable!()
                };
                let NovTxKindV1::Execute(execute) = &mut crossed.kind else {
                    unreachable!()
                };
                execute.nonce_owner_account_id = other.nonce_owner_account_id;
                sign_exact_intent(&mut crossed, seed);
                let ir = nov_native_tx_to_adapter_tx_ir_v1(&crossed).unwrap();
                let error =
                    verify_nov_native_auth_v1(params, &crossed, &ir, tx_hash_array_from_ir_v1(&ir))
                        .expect_err(
                            "nonce unification must not rewrite business subject authority",
                        );
                assert!(error.to_string().contains("must equal direct signer"));
            }
        });
    }

    #[test]
    fn native_nonce_identity_pending_alias_conflicts_and_consecutive_nonce_succeeds() {
        with_nonce_fixture(|path, params| {
            let chain_id = 98_917_205;
            let seed = [0x95; 32];
            clear_native_auth_runtime_reservations_for_chain_v1(chain_id);
            let first = signed_alias(chain_id, 0, seed, 20, 0);
            ingest_local_nov_raw_tx_payload_v1(params, &encode_native_auth_test_tx_v1(&first))
                .expect("first exact nonce admitted");
            for (caller_len, spelling) in (0..10)
                .map(|spelling| (32, spelling))
                .chain((1..10).map(|spelling| (20, spelling)))
            {
                let alias = signed_alias(chain_id, 0, seed, caller_len, spelling);
                let hash =
                    tx_hash_array_from_ir_v1(&nov_native_tx_to_adapter_tx_ir_v1(&alias).unwrap());
                let error = ingest_local_nov_raw_tx_payload_v1(
                    params,
                    &encode_native_auth_test_tx_v1(&alias),
                )
                .expect_err("an alias cannot reserve nonce zero a second time");
                assert!(error.to_string().contains("nonce conflict"));
                assert!(get_network_runtime_native_pending_tx_payload_v1(chain_id, hash).is_none());
            }
            let next = signed_alias(chain_id, 1, seed, 32, 2);
            let future_error =
                ingest_local_nov_raw_tx_payload_v1(params, &encode_native_auth_test_tx_v1(&next))
                    .expect_err(
                        "ingress does not queue future nonces before durable nonce advancement",
                    );
            assert!(future_error
                .to_string()
                .contains("durable nonce sequence mismatch"));
            // Advance only this test's authentication ledger. This is not a
            // claim that the pending fixture was executed or block-finalized.
            let first_reservation = authenticated_reservation(&first, params);
            let mut store = NovNativeExecutionStoreV1::default();
            commit_nov_native_durable_auth_reservation_v1(&mut store, &first_reservation).unwrap();
            save_nov_native_execution_store_v1(path, &store).unwrap();
            release_nov_native_auth_nonce_reservation_v1(&first_reservation).unwrap();
            ingest_local_nov_raw_tx_payload_v1(params, &encode_native_auth_test_tx_v1(&next))
                .expect("after durable advancement the signer can submit its next nonce through another representation");
            let registry = NOV_NATIVE_AUTH_NONCE_RESERVATIONS_V1
                .get()
                .unwrap()
                .lock()
                .unwrap();
            let keys = registry
                .keys()
                .filter(|key| key.0 == chain_id)
                .collect::<Vec<_>>();
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0].1, first_reservation.runtime_key.1);
            assert_eq!(keys[0].2, 1);
            drop(registry);
            clear_native_auth_runtime_reservations_for_chain_v1(chain_id);
        });
    }

    #[test]
    fn native_nonce_identity_durable_floor_survives_reopen_without_alias_reset() {
        with_nonce_fixture(|path, params| {
            let chain_id = 98_917_206;
            let seed = [0x96; 32];
            let first = signed_alias(chain_id, 0, seed, 20, 0);
            let first_reservation = authenticated_reservation(&first, params);
            let mut store = NovNativeExecutionStoreV1::default();
            assert_eq!(
                store.module_state.native_auth_nonce_identity_scheme,
                NATIVE_AUTH_NONCE_IDENTITY_SCHEME_V2
            );
            commit_nov_native_durable_auth_reservation_v1(&mut store, &first_reservation)
                .expect("commit authentication ledger fixture");
            save_nov_native_execution_store_v1(path, &store).expect("save authentication ledger");
            let persisted =
                serde_json::to_value(load_nov_native_execution_store_v1(path).unwrap()).unwrap();
            drop(store);
            clear_native_auth_runtime_reservations_for_chain_v1(chain_id);

            for (caller_len, spelling) in (0..10)
                .map(|spelling| (32, spelling))
                .chain((1..10).map(|spelling| (20, spelling)))
            {
                let alias = signed_alias(chain_id, 0, seed, caller_len, spelling);
                let reservation = authenticated_reservation(&alias, params);
                let error = verify_nov_native_durable_auth_nonce_v1(params, &reservation)
                    .expect_err("persisted nonce must not reset for an alias after reopen");
                assert!(error.to_string().contains("durable nonce conflict"));
            }
            assert_eq!(
                serde_json::to_value(load_nov_native_execution_store_v1(path).unwrap()).unwrap(),
                persisted,
                "rejected aliases must not mutate durable state",
            );
            let next = signed_alias(chain_id, 1, seed, 32, 4);
            let next_reservation = authenticated_reservation(&next, params);
            assert!(matches!(
                verify_nov_native_durable_auth_nonce_v1(params, &next_reservation).unwrap(),
                NovNativeDurableAuthNonceCheckV1::New {
                    expected_nonce: Some(1)
                },
            ));
            let mut reopened = load_nov_native_execution_store_v1(path).unwrap();
            commit_nov_native_durable_auth_reservation_v1(&mut reopened, &next_reservation)
                .unwrap();
            save_nov_native_execution_store_v1(path, &reopened).unwrap();
            let final_store = load_nov_native_execution_store_v1(path).unwrap();
            assert_eq!(final_store.module_state.native_auth_next_nonces.len(), 1);
            assert_eq!(
                final_store.module_state.native_auth_next_nonces[&first_reservation.identity_key],
                2
            );
            assert_eq!(
                final_store
                    .module_state
                    .native_auth_nonce_reservations
                    .len(),
                2
            );
        });
    }

    #[test]
    fn native_nonce_identity_rejects_exhausted_counter_before_reservation() {
        with_nonce_fixture(|_, params| {
            let chain_id = 98_917_207;
            for caller_len in [20, 32] {
                let tx = signed_alias(chain_id, u64::MAX, [0x97; 32], caller_len, 4);
                let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx).unwrap();
                assert!(
                    verify_nov_native_auth_v1(params, &tx, &ir, tx_hash_array_from_ir_v1(&ir))
                        .is_err()
                );
                assert!(ingest_local_nov_raw_tx_payload_v1(
                    params,
                    &encode_native_auth_test_tx_v1(&tx)
                )
                .is_err());
            }
            let valid = signed_alias(chain_id, 0, [0x97; 32], 20, 0);
            ingest_local_nov_raw_tx_payload_v1(params, &encode_native_auth_test_tx_v1(&valid))
                .expect("exhausted counter rejection must leave the signer nonce untouched");
            clear_native_auth_runtime_reservations_for_chain_v1(chain_id);
        });
    }

    #[test]
    fn native_nonce_identity_missing_json_maps_and_module_do_not_become_v2_genesis() {
        let source = NovNativeExecutionStoreV1::default();
        for field in ["native_auth_next_nonces", "native_auth_nonce_reservations"] {
            let mut incomplete = serde_json::to_value(&source).unwrap();
            incomplete["module_state"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                serde_json::from_value::<NovNativeExecutionStoreV1>(incomplete).is_err(),
                "V2 JSON with missing {field} must not manufacture an empty nonce map"
            );
        }
        let mut without_module = serde_json::to_value(&source).unwrap();
        without_module
            .as_object_mut()
            .unwrap()
            .remove("module_state");
        let legacy = serde_json::from_value::<NovNativeExecutionStoreV1>(without_module)
            .expect("legacy envelope can still be inspected read-only");
        assert!(legacy
            .module_state
            .native_auth_nonce_identity_scheme
            .is_empty());
        assert!(verify_native_nonce_identity_scheme_v2(&legacy).is_err());

        let mut unsupported = source;
        unsupported.module_state.native_auth_nonce_identity_scheme = "unknown-future-scheme".into();
        let decoded = serde_json::from_value::<NovNativeExecutionStoreV1>(
            serde_json::to_value(unsupported).unwrap(),
        )
        .unwrap();
        assert!(verify_native_nonce_identity_scheme_v2(&decoded).is_err());
    }

    #[test]
    fn native_nonce_identity_incomplete_v2_shard_rejects_without_partial_apply() {
        let mut source = NovNativeExecutionModuleStateV1::default();
        source.native_auth_next_nonces.insert("ab".repeat(32), 1);
        source
            .native_auth_nonce_reservations
            .insert("cd".repeat(32), "ef".repeat(32));
        let encoded = native_module_state_shard_value_v1(&source, "native_execution").unwrap();
        for field in ["native_auth_next_nonces", "native_auth_nonce_reservations"] {
            let mut incomplete: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
            incomplete.as_object_mut().unwrap().remove(field);
            let mut target = NovNativeExecutionModuleStateV1::default();
            target.native_auth_nonce_identity_scheme.clear();
            target.treasury_reserves.insert("NOV".into(), 31);
            target.native_auth_next_nonces.insert("01".repeat(32), 7);
            let before = target.clone();
            let error = native_apply_module_state_shard_v1(
                &mut target,
                "native_execution",
                &serde_json::to_vec(&incomplete).unwrap(),
            );
            assert!(error.is_err(), "V2 shard with missing {field} must reject");
            assert_eq!(
                target, before,
                "failed shard validation must not partly apply metadata"
            );
        }
    }

    #[test]
    fn native_nonce_identity_candidate_alias_batch_rejection_and_authority_parity() {
        use crate::tx_ingress::candidate_workspace as workspace;
        use common_candidate_plan_tests::{
            committed_block, genesis_plan, heads, raw_fixture, successor_plan, with_plan_runtime,
            PLAN_RUNTIME_TEST_LOCK,
        };

        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        with_env_removed_v1(NOV_NATIVE_CHAIN_ID_ENV, || {
            with_plan_runtime(|path, params| {
                let chain_id = 98_917_208;
                let seed = [0x98; 32];
                let parent = committed_block(
                    &run_nov_native_candidate_execution_plan_v1(
                        &genesis_plan(chain_id, vec![raw_fixture(chain_id, 981)]),
                        params,
                    )
                    .expect("create actual AOEM parent"),
                );
                let raw_alias = |nonce, caller_len, spelling| {
                    let mut tx = signed_alias(chain_id, nonce, seed, caller_len, spelling);
                    let NovTxKindV1::Execute(execute) = &mut tx.kind else {
                        unreachable!()
                    };
                    execute.args =
                        serde_json::to_vec(&serde_json::json!({ "asset": "NOV", "amount": 19 }))
                            .unwrap();
                    execute.fee_policy.pay_asset = "NOV".into();
                    execute.fee_policy.max_pay_amount = 1_000;
                    sign_exact_intent(&mut tx, seed);
                    encode_native_auth_test_tx_v1(&tx)
                };
                let first = raw_alias(0, 20, 2);
                let conflicting = raw_alias(0, 32, 4);
                let next = raw_alias(1, 32, 4);
                let invalid_plan = successor_plan(&parent, vec![first.clone(), conflicting]);
                let valid_plan = successor_plan(&parent, vec![first.clone(), next]);

                // A real pending reservation for the branch's first intent must
                // neither interfere with isolated execution nor be consumed by it.
                let pending_tx = decode_nov_native_tx_wire_v1(&first).unwrap();
                let pending_ir = nov_native_tx_to_adapter_tx_ir_v1(&pending_tx).unwrap();
                let pending_hash = tx_hash_array_from_ir_v1(&pending_ir);
                let (pending_key, pending_id) =
                    verify_nov_native_auth_v1(params, &pending_tx, &pending_ir, pending_hash)
                        .unwrap();
                reserve_nov_native_auth_nonce_v1(pending_key, pending_id, None).unwrap();
                observe_network_runtime_native_pending_tx_local_native_payload_v1(
                    chain_id,
                    pending_hash,
                    Some(&first),
                );
                let pending_before =
                    serde_json::to_value(novovm_network::get_network_runtime_native_pending_tx_v1(
                        chain_id,
                        pending_hash,
                    ))
                    .unwrap();
                let nonce_registry = || {
                    let registry = NOV_NATIVE_AUTH_NONCE_RESERVATIONS_V1
                        .get()
                        .unwrap()
                        .lock()
                        .unwrap();
                    registry
                        .iter()
                        .filter(|(key, _)| key.0 == chain_id)
                        .map(|(key, value)| (key.clone(), *value))
                        .collect::<BTreeMap<_, _>>()
                };
                let reservations_before = nonce_registry();
                let authority_before = heads(path, params, chain_id);
                let invalid = workspace::create_v1(&invalid_plan, params).unwrap();
                let invalid_error = workspace::execute_v1(chain_id, invalid.workspace_id, params)
                    .expect_err(
                        "same signer nonce cannot be consumed twice through different caller forms",
                    );
                assert!(format!("{invalid_error:#}").contains("duplicate nonce key"));
                assert!(
                    workspace::load_execution_v1(chain_id, invalid.workspace_id, params)
                        .unwrap()
                        .is_none()
                );
                assert_eq!(heads(path, params, chain_id), authority_before);

                let input = workspace::create_v1(&valid_plan, params).unwrap();
                let output = workspace::execute_v1(chain_id, input.workspace_id, params)
                    .expect("ordered aliases execute against one candidate-local signer nonce");
                assert!(output.transactions_authenticated);
                assert!(
                    output.aoem_called
                        && output.execution_completed
                        && output.candidate_state_persisted
                );
                assert!(!output.authority_state_published);
                assert!(
                    !output.chain_canonical
                        && !output.proof_sealed
                        && !output.safe
                        && !output.finalized
                );
                assert_eq!(heads(path, params, chain_id), authority_before);
                assert_eq!(nonce_registry(), reservations_before);
                assert_eq!(
                    get_network_runtime_native_pending_tx_payload_v1(chain_id, pending_hash),
                    Some(first)
                );
                assert_eq!(
                    serde_json::to_value(novovm_network::get_network_runtime_native_pending_tx_v1(
                        chain_id,
                        pending_hash
                    ))
                    .unwrap(),
                    pending_before,
                );
                let candidate_store = workspace::load_execution_snapshot_for_test_v1(
                    chain_id,
                    input.workspace_id,
                    params,
                )
                .unwrap();
                let reservation = authenticated_reservation(&pending_tx, params);
                assert_eq!(
                    candidate_store["module_state"]["native_auth_next_nonces"]
                        [&reservation.identity_key],
                    2
                );

                run_nov_native_candidate_execution_plan_v1(&valid_plan, params)
                    .expect("explicit authority execution of the exact same agreed plan");
                let authoritative =
                    load_validated_native_state_envelope_from_aoem_owner_v1(params, chain_id)
                        .unwrap()
                        .unwrap();
                assert_eq!(output.post_state_root, authoritative.state_root);
                assert_eq!(output.receipt_root, authoritative.receipt_root);
                assert_eq!(output.batch_result, authoritative.batch_result);
                assert_eq!(
                    candidate_store,
                    serde_json::to_value(authoritative.store).unwrap()
                );
                clear_native_auth_runtime_reservations_for_chain_v1(chain_id);
            })
        });
    }
}
