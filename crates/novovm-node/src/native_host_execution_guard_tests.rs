mod restoration_host_guard {
    use super::*;

    // The historical test suite has several unrelated environment locks. Run
    // each guard check alone in a child test process instead of changing the
    // parent runner's environment and racing existing compatibility fixtures.
    fn isolated_case(name: &str, permit: Option<&str>) -> bool {
        let full_name = format!("tx_ingress::tests::restoration_host_guard::{name}");
        isolated_named_case(&full_name, permit, true)
    }

    pub(super) fn isolated_named_case(
        full_name: &str,
        permit: Option<&str>,
        clear_runtime_environment: bool,
    ) -> bool {
        const CHILD: &str = "NOVOVM_TEST_RESTORATION_HOST_GUARD_CHILD";
        if std::env::var(CHILD).as_deref() == Ok(full_name) {
            return true;
        }
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", full_name, "--nocapture"]);
        command.env_remove(NOV_NATIVE_ALLOW_LEGACY_HOST_EXECUTION_ENV);
        if clear_runtime_environment {
            for (key, _) in std::env::vars_os() {
                let token = key.to_string_lossy();
                if token.starts_with("NOVOVM_") || token.starts_with("AOEM_") {
                    command.env_remove(key);
                }
            }
            command
                .env(NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV, "false")
                .env(NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV, "json");
        }
        command.env(CHILD, full_name);
        if let Some(permit) = permit {
            command.env(NOV_NATIVE_ALLOW_LEGACY_HOST_EXECUTION_ENV, permit);
        }
        let output = command.output().expect("spawn isolated guard test");
        assert!(
            output.status.success(),
            "guard child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "child must execute the exact test, not pass with zero selected tests"
        );
        false
    }

    fn request() -> NovExecutionRequestV1 {
        NovExecutionRequestV1 {
            tx_hash: [0x81; 32],
            chain_id: 91_801,
            caller: vec![1; 20],
            target: NovExecutionRequestTargetV1::NativeModule("treasury".into()),
            method: "guard_test".into(),
            args: b"{}".to_vec(),
            fee_pay_asset: "NOV".into(),
            fee_max_pay_amount: 0,
            fee_slippage_bps: 0,
            gas_like_limit: None,
            nonce: 0,
        }
    }

    fn assert_guard(error: anyhow::Error) {
        assert!(
            error
                .to_string()
                .starts_with("legacy Host execution is disabled:"),
            "wrong rejection: {error:#}"
        );
    }

    fn assert_no_state_files(path: &Path) {
        assert!(!path.exists(), "guard created Host state");
        assert!(
            !nov_native_execution_store_lock_path_v1(path).exists(),
            "guard acquired the Host state lock"
        );
        assert!(!nov_native_execution_store_rocksdb_path_v1(path).exists());
        assert!(!nov_native_block_ledger_rocksdb_path_v1(path).exists());
    }

    #[test]
    fn restoration_host_guard_default_stops_dispatch_and_mutator_before_io() {
        if !isolated_case(
            "restoration_host_guard_default_stops_dispatch_and_mutator_before_io",
            None,
        ) {
            return;
        }
        with_test_native_execution_store_path_v1(|path| {
            assert_guard(
                dispatch_and_persist_nov_execution_request_with_store_path_v1(&path, &request())
                    .unwrap_err(),
            );
            let mut called = false;
            let result = mutate_nov_native_execution_store_with_aoem_semantic_commit_v1(
                &path,
                "guard-test",
                "test-intent",
                "test-account",
                "test-mutation",
                1,
                |_| {
                    called = true;
                    Ok(())
                },
            );
            assert_guard(result.unwrap_err());
            assert!(!called, "Host business closure must not run");
            assert_no_state_files(&path);
        });
    }

    #[test]
    fn restoration_host_guard_ownership_labels_and_rpc_tokens_cannot_bypass() {
        if !isolated_case(
            "restoration_host_guard_ownership_labels_and_rpc_tokens_cannot_bypass",
            None,
        ) {
            return;
        }
        with_test_native_execution_store_path_v1(|path| {
            let params = serde_json::json!({
                "chain_id": 91_803,
                "native_execution_store_path": path,
                "raw_txs": ["0x00"],
                "allow_legacy_host_execution": true,
                "NOVOVM_ALLOW_LEGACY_HOST_EXECUTION": "1",
                "aoem_owned_gate_config": {
                    "production_candidate": true,
                    "semantic_graph_v3_required": true,
                    "shadow": true,
                    "compare": true
                }
            });
            // Include the trusted internal entry. A public-RPC-only check would
            // otherwise leave the same Host loop active behind the node tick.
            assert_guard(run_nov_send_raw_transaction_batch_from_params_v1(&params).unwrap_err());
            assert_guard(
                run_nov_send_raw_transaction_batch_internal_v1(&params, true).unwrap_err(),
            );
            assert_guard(
                run_nov_execute_pending_native_tx_batch_from_params_v1(&params).unwrap_err(),
            );
            assert_guard(run_nov_native_execution_tick_from_params_v1(&params).unwrap_err());
            assert_no_state_files(&path);
        });
    }

    #[test]
    fn restoration_host_guard_immediate_rejected_but_pending_and_queries_remain() {
        if !isolated_case(
            "restoration_host_guard_immediate_rejected_but_pending_and_queries_remain",
            None,
        ) {
            return;
        }
        let chain_id = 91_805;
        with_test_native_execution_store_path_v1(|path| {
            let raw = build_test_native_execute_raw_hex_with_chain_v1(
                chain_id,
                0,
                "guard-signed-intent",
                7,
            );
            let mut params = serde_json::json!({
                "chain_id": chain_id,
                "native_execution_store_path": path,
                "raw_tx": raw,
                "pipeline_only": false
            });
            let before = snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(
                chain_id, 32, None,
            )
            .len();
            assert_guard(run_nov_send_raw_transaction_internal_v1(&params, true).unwrap_err());
            assert_eq!(
                snapshot_network_runtime_native_active_pending_txs_for_repair_window_v1(
                    chain_id, 32, None,
                )
                .len(),
                before,
                "denied immediate execution must not admit or reserve the transaction"
            );
            assert_no_state_files(&path);
            assert_eq!(
                get_nov_native_account_asset_balance_with_store_path_v1(&path, "reader", "NOV")
                    .expect("read-only balance remains available"),
                0
            );
            assert!(
                get_nov_native_execution_receipt_by_hash_with_store_path_v1(&path, "00")
                    .expect("read-only receipt remains available")
                    .is_none()
            );
            assert_no_state_files(&path);
            params["pipeline_only"] = serde_json::json!(true);
            let accepted = run_nov_send_raw_transaction_internal_v1(&params, true)
                .expect("authenticated pending-only ingress must remain available");
            assert_eq!(accepted["accepted"], true);
            assert_eq!(accepted["pipeline_only"], true);
            assert_eq!(accepted["immediate_execution"], false);
            assert!(accepted["native_receipt"].is_null());
        });
    }

    #[test]
    fn restoration_host_guard_invalid_operator_token_fails_closed() {
        if !isolated_case(
            "restoration_host_guard_invalid_operator_token_fails_closed",
            Some("yes-please"),
        ) {
            return;
        }
        assert_guard(require_legacy_host_execution_comparison_v1("invalid-token").unwrap_err());
    }

    #[test]
    fn restoration_host_guard_explicit_comparison_can_still_persist() {
        if !isolated_case(
            "restoration_host_guard_explicit_comparison_can_still_persist",
            Some("1"),
        ) {
            return;
        }
        with_test_native_execution_store_path_v1(|path| {
            let mut called = false;
            mutate_nov_native_execution_store_with_aoem_semantic_commit_v1(
                &path,
                "guard-test",
                "test-intent",
                "test-account",
                "test-mutation",
                1,
                |store| {
                    called = true;
                    store.module_state.treasury_reserve_bucket_nov = 9;
                    Ok(())
                },
            )
            .expect("explicit compatibility comparison remains executable");
            assert!(called);
            assert!(path.exists());
            assert_eq!(
                load_nov_native_execution_store_v1(&path)
                    .unwrap()
                    .module_state
                    .treasury_reserve_bucket_nov,
                9
            );
        });
    }
}
