mod common_candidate_plan_tests {
    use super::*;
    use crate::native_candidate_plan::NovNativeCandidateExecutionPlanV1;
    use std::sync::Mutex;

    // The existing AOEM fixture helpers scope process environment and reset the
    // shared session. Keep this group serialized even with the default runner.
    pub(super) static PLAN_RUNTIME_TEST_LOCK: Mutex<()> = Mutex::new(());

    pub(super) fn raw_fixture(chain_id: u64, identity: u64) -> Vec<u8> {
        let raw = build_test_native_execute_raw_hex_with_chain_v1(
            chain_id,
            identity,
            &format!("acct-common-plan-{identity}"),
            identity + 10,
        );
        decode_eth_send_raw_hex_payload_v1(&raw, "raw_tx").expect("decode signed plan fixture")
    }

    fn protocol_commitment() -> [u8; 32] {
        parse_fixed_hex_32_v1(
            &native_business_protocol_config_commitment_v1().expect("protocol commitment"),
            "test protocol commitment",
        )
        .expect("decode protocol commitment")
    }

    pub(super) fn make_plan(
        context: NovBlockExecutionContextV1,
        pre_state_root: [u8; 32],
        aoem_parent: Option<NovNativePreparedAoemParentV1>,
        raw_txs: Vec<Vec<u8>>,
    ) -> NovNativeCandidateExecutionPlanV1 {
        let tx_hashes = raw_txs
            .iter()
            .map(|raw| canonical_nov_native_tx_hash_from_payload_v1(raw).expect("canonical hash"))
            .collect();
        NovNativeCandidateExecutionPlanV1::new(
            context,
            protocol_commitment(),
            pre_state_root,
            aoem_parent,
            tx_hashes,
            raw_txs,
        )
        .expect("build self-consistent local execution plan")
    }

    pub(super) fn genesis_plan(chain_id: u64, raw_txs: Vec<Vec<u8>>) -> NovNativeCandidateExecutionPlanV1 {
        let mut genesis = NovNativeExecutionStoreV1::default();
        bind_native_business_protocol_config_v1(&mut genesis).expect("bind genesis protocol");
        make_plan(
            NovBlockExecutionContextV1 {
                chain_id,
                block_height: 1,
                parent_block_hash: [0; 32],
                slot: 7,
                // Deliberately independent of the machine's wall clock.
                timestamp_unix_ms: 1_700_000_000_123,
            },
            parse_fixed_hex_32_v1(
                &native_semantic_ledger_state_digest_v1(&genesis.module_state),
                "genesis pre-state root",
            )
            .expect("decode genesis root"),
            None,
            raw_txs,
        )
    }

    pub(super) fn successor_plan(
        parent: &NovNativeDurableBlockV1,
        raw_txs: Vec<Vec<u8>>,
    ) -> NovNativeCandidateExecutionPlanV1 {
        let head = &parent.header;
        make_plan(
            NovBlockExecutionContextV1 {
                chain_id: head.chain_id,
                block_height: head.height + 1,
                parent_block_hash: head.block_hash,
                slot: head.slot + 1,
                timestamp_unix_ms: head.timestamp_unix_ms + 250,
            },
            head.post_state_root,
            Some(NovNativePreparedAoemParentV1 {
                batch_id: head.aoem_batch_id.clone(),
                batch_result_id: head.aoem_batch_result_id.clone(),
                state_root: head.post_state_root,
                state_root_codec: head.post_state_root_codec.clone(),
                cumulative_receipt_root: head.cumulative_receipt_root,
                receipt_root_codec: head.cumulative_receipt_root_codec.clone(),
                state_version: head.state_version,
            }),
            raw_txs,
        )
    }

    pub(super) fn with_plan_runtime<T>(test: impl FnOnce(&Path, &serde_json::Value) -> T) -> T {
        with_test_native_execution_store_path_v1(|path| {
            with_test_native_aoem_persist_runtime_v1(path.as_path(), |aoem_path| {
                let params = serde_json::json!({
                    "native_execution_store_path": path,
                    "aoem_state_namespace": aoem_path.to_string_lossy(),
                    "aoem_owned_gate_config": {
                        "production_candidate": true,
                        "semantic_graph_v3_required": true,
                        "shadow": true,
                        "compare": true,
                        "source": "common_candidate_plan_test"
                    }
                });
                test(path.as_path(), &params)
            })
        })
    }

    pub(super) fn committed_block(out: &serde_json::Value) -> NovNativeDurableBlockV1 {
        let block: NovNativeDurableBlockV1 = serde_json::from_value(
            out.get("durable_block_candidate_committed")
                .expect("durable committed candidate output")
                .clone(),
        )
        .expect("decode complete durable candidate");
        assert!(block.header.aoem_readback_verified);
        assert!(block.header.canonical_local);
        assert!(!block.header.proof_sealed);
        assert!(!block.header.safe);
        assert!(!block.header.finalized);
        block
    }

    pub(super) fn heads(path: &Path, params: &serde_json::Value, chain_id: u64) -> serde_json::Value {
        let envelope = load_validated_native_state_envelope_from_aoem_owner_v1(params, chain_id)
            .expect("read AOEM authority head");
        let ledger = NovNativeBlockLedgerV1::open(&nov_native_block_ledger_rocksdb_path_v1(path))
            .expect("read durable candidate ledger");
        serde_json::json!({
            "aoem_head": envelope,
            "ledger_head": ledger.load_head(chain_id).expect("ledger head"),
            "prepared": ledger.load_prepared(chain_id).expect("prepared slot"),
            "host_projection": load_nov_native_execution_store_v1(path).expect("Host projection"),
        })
    }

    fn assert_rejected_without_head_change(
        plan: &NovNativeCandidateExecutionPlanV1,
        path: &Path,
        params: &serde_json::Value,
    ) {
        let before = heads(path, params, plan.context.chain_id);
        assert!(
            run_nov_native_candidate_execution_plan_v1(plan, params).is_err(),
            "invalid or conflicting plan must fail",
        );
        assert_eq!(heads(path, params, plan.context.chain_id), before);
    }

    #[test]
    fn same_two_height_plan_has_identical_durable_blocks_across_storage_roots() {
        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let chain_id = 98_916_101;
        let first_plan = genesis_plan(
            chain_id,
            vec![raw_fixture(chain_id, 1), raw_fixture(chain_id, 2)],
        );
        let execute_chain = |node_index: u64| {
            with_plan_runtime(|path, params| {
                // Perturb arrival order and add a transaction outside the plan.
                // The explicit Host API must not drain this process-wide pool.
                let mut arrivals = first_plan.raw_txs.clone();
                if node_index == 0 {
                    arrivals.reverse();
                }
                let unrelated = raw_fixture(chain_id, 900 + node_index);
                let unrelated_hash = canonical_nov_native_tx_hash_from_payload_v1(&unrelated)
                    .expect("unrelated hash");
                arrivals.push(unrelated);
                for raw in &arrivals {
                    let hash = canonical_nov_native_tx_hash_from_payload_v1(raw).unwrap();
                    observe_network_runtime_native_pending_tx_local_native_payload_v1(
                        chain_id,
                        hash,
                        Some(raw),
                    );
                }
                let unrelated_before =
                    serde_json::to_value(novovm_network::get_network_runtime_native_pending_tx_v1(
                        chain_id,
                        unrelated_hash,
                    ))
                    .unwrap();
                let first = committed_block(
                    &run_nov_native_candidate_execution_plan_v1(&first_plan, params)
                        .expect("execute agreed genesis plan through real AOEM"),
                );
                assert_eq!(first.body.tx_hashes, first_plan.tx_hashes);
                assert_eq!(
                    first.header.timestamp_unix_ms,
                    first_plan.context.timestamp_unix_ms
                );
                let second_plan = successor_plan(&first, vec![raw_fixture(chain_id, 3)]);
                let second = committed_block(
                    &run_nov_native_candidate_execution_plan_v1(&second_plan, params)
                        .expect("execute agreed successor plan through real AOEM"),
                );
                assert_eq!(second.header.parent_block_hash, first.header.block_hash);
                assert_eq!(
                    second.header.timestamp_unix_ms,
                    second_plan.context.timestamp_unix_ms
                );
                assert_eq!(
                    serde_json::to_value(novovm_network::get_network_runtime_native_pending_tx_v1(
                        chain_id,
                        unrelated_hash,
                    ))
                    .unwrap(),
                    unrelated_before,
                    "extra pending transaction must remain untouched",
                );
                assert!(get_nov_native_execution_receipt_by_hash_with_store_path_v1(
                    path,
                    &to_hex_prefixed_v1(&unrelated_hash),
                )
                .expect("query unrelated receipt")
                .is_none());

                let before_replay = heads(path, params, chain_id);
                let replay = run_nov_native_candidate_execution_plan_v1(&second_plan, params)
                    .expect("same committed plan is an idempotent replay");
                assert_eq!(replay["batch_replay"], true);
                assert_eq!(replay["aoem_reexecution"], false);
                assert_eq!(committed_block(&replay), second);
                assert_eq!(heads(path, params, chain_id), before_replay);

                let third_plan = successor_plan(&second, vec![raw_fixture(chain_id, 4)]);
                let ledger =
                    NovNativeBlockLedgerV1::open(&nov_native_block_ledger_rocksdb_path_v1(path))
                        .expect("open ledger to stage the next candidate without executing it");
                let prepared_third = ledger
                    .prepare(NovNativeBlockCandidateInputV1 {
                        context: third_plan.context,
                        tx_hashes: third_plan.tx_hashes.clone(),
                        raw_txs: third_plan.raw_txs.clone(),
                        pre_state_root: third_plan.pre_state_root,
                        aoem_parent: third_plan.aoem_parent.clone(),
                    })
                    .expect("persist unresolved third-height candidate");
                drop(ledger);
                let before_old_replay = heads(path, params, chain_id);
                let old_replay = run_nov_native_candidate_execution_plan_v1(&first_plan, params)
                    .expect("old committed plan replays without touching a newer prepared slot");
                assert_eq!(old_replay["batch_replay"], true);
                assert_eq!(old_replay["aoem_reexecution"], false);
                assert_eq!(committed_block(&old_replay), first);
                assert_eq!(heads(path, params, chain_id), before_old_replay);
                assert_eq!(
                    before_old_replay["prepared"],
                    serde_json::to_value(prepared_third).unwrap()
                );

                let competing_third = successor_plan(&second, vec![raw_fixture(chain_id, 5)]);
                assert_rejected_without_head_change(&competing_third, path, params);
                (first, second)
            })
        };
        let first_directory = execute_chain(0);
        let second_directory = execute_chain(1);
        assert_eq!(first_directory, second_directory,
            "all block, state, receipt, body and AOEM evidence commitments must be storage-independent");
    }

    #[test]
    fn wrong_context_parent_prestate_protocol_and_body_leave_authority_unchanged() {
        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let chain_id = 98_916_102;
        with_plan_runtime(|path, params| {
            let first_plan = genesis_plan(chain_id, vec![raw_fixture(chain_id, 11)]);
            let first = committed_block(
                &run_nov_native_candidate_execution_plan_v1(&first_plan, params)
                    .expect("execute parent"),
            );
            let next = successor_plan(&first, vec![raw_fixture(chain_id, 12)]);

            let mut wrong_context = next.context;
            wrong_context.parent_block_hash[0] ^= 1;
            let wrong_parent_hash = make_plan(
                wrong_context,
                next.pre_state_root,
                next.aoem_parent.clone(),
                next.raw_txs.clone(),
            );
            assert_rejected_without_head_change(&wrong_parent_hash, path, params);

            let mut wrong_context = next.context;
            wrong_context.block_height += 1;
            let skipped_height = make_plan(
                wrong_context,
                next.pre_state_root,
                next.aoem_parent.clone(),
                next.raw_txs.clone(),
            );
            assert_rejected_without_head_change(&skipped_height, path, params);

            let mut regressed_context = next.context;
            regressed_context.slot = first.header.slot - 1;
            let regressed_slot = make_plan(
                regressed_context,
                next.pre_state_root,
                next.aoem_parent.clone(),
                next.raw_txs.clone(),
            );
            assert_rejected_without_head_change(&regressed_slot, path, params);

            let mut regressed_context = next.context;
            regressed_context.timestamp_unix_ms = first.header.timestamp_unix_ms - 1;
            let regressed_timestamp = make_plan(
                regressed_context,
                next.pre_state_root,
                next.aoem_parent.clone(),
                next.raw_txs.clone(),
            );
            assert_rejected_without_head_change(&regressed_timestamp, path, params);

            let mut wrong_root = next.pre_state_root;
            wrong_root[0] ^= 1;
            let mut wrong_parent = next.aoem_parent.clone().unwrap();
            wrong_parent.state_root = wrong_root;
            let wrong_prestate = make_plan(
                next.context,
                wrong_root,
                Some(wrong_parent),
                next.raw_txs.clone(),
            );
            assert_rejected_without_head_change(&wrong_prestate, path, params);

            let mut wrong_parent = next.aoem_parent.clone().unwrap();
            wrong_parent.batch_id.push_str("-conflict");
            let wrong_aoem_parent = make_plan(
                next.context,
                next.pre_state_root,
                Some(wrong_parent),
                next.raw_txs.clone(),
            );
            assert_rejected_without_head_change(&wrong_aoem_parent, path, params);

            let mut wrong_protocol = next.protocol_config_commitment;
            wrong_protocol[0] ^= 1;
            let wrong_protocol = NovNativeCandidateExecutionPlanV1::new(
                next.context,
                wrong_protocol,
                next.pre_state_root,
                next.aoem_parent.clone(),
                next.tx_hashes.clone(),
                next.raw_txs.clone(),
            )
            .expect("self-consistent plan with a foreign protocol commitment");
            assert_rejected_without_head_change(&wrong_protocol, path, params);

            let mut changed_body = next.clone();
            changed_body.raw_txs[0].push(0xff);
            assert_rejected_without_head_change(&changed_body, path, params);
            let mut changed_hash = next.clone();
            changed_hash.tx_hashes[0][0] ^= 1;
            assert_rejected_without_head_change(&changed_hash, path, params);

            // A correct plan commitment does not make an announced transaction
            // hash authoritative. The Host must independently rebuild it.
            let wrong_canonical_hash = NovNativeCandidateExecutionPlanV1::new(
                next.context,
                next.protocol_config_commitment,
                next.pre_state_root,
                next.aoem_parent.clone(),
                changed_hash.tx_hashes,
                next.raw_txs.clone(),
            )
            .expect("recompute plan commitment around a false transaction hash");
            wrong_canonical_hash
                .validate()
                .expect("plan-level structure and commitment are valid");
            let before_wrong_hash = heads(path, params, chain_id);
            let error = run_nov_native_candidate_execution_plan_v1(&wrong_canonical_hash, params)
                .expect_err("Host canonical hash reconstruction must reject an incorrect hash");
            assert!(error
                .to_string()
                .contains("authenticated ordered transactions"));
            assert_eq!(heads(path, params, chain_id), before_wrong_hash);

            let mut replay_context = first_plan.context;
            replay_context.timestamp_unix_ms += 1;
            let conflicting_replay = make_plan(
                replay_context,
                first_plan.pre_state_root,
                None,
                first_plan.raw_txs.clone(),
            );
            assert_rejected_without_head_change(&conflicting_replay, path, params);

            let before_input_rejections = heads(path, params, chain_id);
            let mut missing_ownership = params.clone();
            missing_ownership
                .as_object_mut()
                .unwrap()
                .remove("aoem_owned_gate_config");
            let error = run_nov_native_candidate_execution_plan_v1(&next, &missing_ownership)
                .expect_err("typed execution requires explicit AOEM ownership");
            assert!(error
                .to_string()
                .contains("explicit AOEM production ownership"));
            assert_eq!(heads(path, params, chain_id), before_input_rejections);
            for key in ["raw_txs", "block_execution_context"] {
                let mut overridden = params.clone();
                overridden[key] = serde_json::json!({"unexpected": "request_override"});
                let error = run_nov_native_candidate_execution_plan_v1(&next, &overridden)
                    .expect_err("node configuration cannot replace typed plan inputs");
                assert!(error.to_string().contains("cannot be overridden"));
                assert_eq!(heads(path, params, chain_id), before_input_rejections);
            }

            // Even an exact durable transaction replay must not expose the
            // Host context boundary through the public JSON/RPC batch API.
            let mut public_replay = params.clone();
            public_replay["raw_txs"] = serde_json::json!(first_plan
                .raw_txs
                .iter()
                .map(|raw| to_hex_prefixed_v1(raw))
                .collect::<Vec<_>>());
            public_replay["block_execution_context"] = serde_json::json!({
                "chain_id": first_plan.context.chain_id,
                "block_height": first_plan.context.block_height,
                "parent_block_hash": to_hex_prefixed_v1(&first_plan.context.parent_block_hash),
                "slot": first_plan.context.slot,
                "timestamp_unix_ms": first_plan.context.timestamp_unix_ms,
            });
            let error = run_nov_send_raw_transaction_batch_from_params_v1(&public_replay)
                .expect_err("public replay cannot supply Host-owned execution context");
            assert!(error.to_string().contains("Host-owned"));
            assert_eq!(heads(path, params, chain_id), before_input_rejections);

            let second = committed_block(
                &run_nov_native_candidate_execution_plan_v1(&next, params)
                    .expect("valid next plan still succeeds after all rejections"),
            );
            assert_eq!(second.header.height, 2);
        });
    }

    #[test]
    fn invalid_signature_and_skipped_nonce_never_advance_aoem_or_ledger() {
        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let chain_id = 98_916_103;
        with_plan_runtime(|path, params| {
            let mut invalid_auth = build_signed_native_auth_test_tx_v1(
                chain_id,
                0,
                [0x53; 32],
                "acct-plan-invalid-auth",
                9,
            );
            invalid_auth.signature[40] ^= 1;
            let invalid_auth_plan =
                genesis_plan(chain_id, vec![encode_native_auth_test_tx_v1(&invalid_auth)]);
            assert_rejected_without_head_change(&invalid_auth_plan, path, params);

            let invalid_nonce = build_signed_native_auth_test_tx_v1(
                chain_id,
                1,
                [0x54; 32],
                "acct-plan-skipped-nonce",
                9,
            );
            let invalid_nonce_plan = genesis_plan(
                chain_id,
                vec![encode_native_auth_test_tx_v1(&invalid_nonce)],
            );
            assert_rejected_without_head_change(&invalid_nonce_plan, path, params);
            let after = heads(path, params, chain_id);
            assert!(after["aoem_head"].is_null());
            assert!(after["ledger_head"].is_null());
            assert!(after["prepared"].is_null());
        });
    }

    #[test]
    fn prepared_plan_recovers_after_aoem_commit_without_reexecution() {
        let _guard = PLAN_RUNTIME_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let chain_id = 98_916_104;
        with_plan_runtime(|path, params| {
            let plan = genesis_plan(chain_id, vec![raw_fixture(chain_id, 21)]);
            let failure = with_env_override_v1(
                NOVOVM_TEST_FAIL_AFTER_AOEM_BEFORE_BLOCK_COMMIT_ENV,
                "true",
                || run_nov_native_candidate_execution_plan_v1(&plan, params),
            )
            .expect_err("inject stop after AOEM persistence before candidate commit");
            assert!(failure.to_string().contains("test fault after AOEM"));
            let interrupted = heads(path, params, chain_id);
            assert!(!interrupted["aoem_head"].is_null());
            assert!(interrupted["ledger_head"].is_null());
            assert!(!interrupted["prepared"].is_null());
            reset_native_aoem_semantic_ingress_session_v1();
            clear_native_auth_runtime_reservations_for_chain_v1(chain_id);

            let restored = run_nov_native_candidate_execution_plan_v1(&plan, params)
                .expect("same durable prepared plan recovers the committed AOEM result");
            assert_eq!(restored["batch_replay"], true);
            assert_eq!(restored["aoem_reexecution"], false);
            let block = committed_block(&restored);
            assert_eq!(block.body.tx_hashes, plan.tx_hashes);
            assert_eq!(
                block.header.timestamp_unix_ms,
                plan.context.timestamp_unix_ms
            );
            let recovered = heads(path, params, chain_id);
            assert_eq!(recovered["aoem_head"], interrupted["aoem_head"]);
            assert!(!recovered["ledger_head"].is_null());
            assert!(recovered["prepared"].is_null());
            let replay = run_nov_native_candidate_execution_plan_v1(&plan, params)
                .expect("recovered candidate replays idempotently");
            assert_eq!(committed_block(&replay), block);
            assert_eq!(heads(path, params, chain_id), recovered);
        });
    }
}

mod candidate_workspace_tests {
    use super::*;
    use super::common_candidate_plan_tests::{
        committed_block, genesis_plan, heads, make_plan, raw_fixture, successor_plan,
        with_plan_runtime, PLAN_RUNTIME_TEST_LOCK,
    };
    use crate::native_candidate_plan::NovNativeCandidateExecutionPlanV1;
    use std::sync::Mutex;

    include!("native_candidate_workspace_tests.rs");
    include!("native_candidate_execution_tests.rs");
}
