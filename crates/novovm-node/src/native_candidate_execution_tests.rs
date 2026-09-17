fn candidate_workspace_execution_raw(
    chain_id: u64,
    nonce: u64,
    seed: [u8; 32],
    amount: u64,
    method: &str,
) -> Vec<u8> {
    let mut tx = build_signed_native_auth_test_tx_v1(
        chain_id,
        nonce,
        seed,
        "candidate-execution-test",
        amount,
    );
    let NovTxKindV1::Execute(execute) = &mut tx.kind else {
        unreachable!();
    };
    execute.method = method.to_string();
    execute.args = serde_json::to_vec(&serde_json::json!({
        "asset": "NOV",
        "amount": amount,
    }))
    .unwrap();
    execute.fee_policy.pay_asset = "NOV".to_string();
    execute.fee_policy.max_pay_amount = 1_000;
    sign_nov_native_tx_with_seed_v1(&mut tx, seed).unwrap();
    encode_native_auth_test_tx_v1(&tx)
}

fn assert_candidate_workspace_execution_complete(info: &workspace::ExecutionInfoV1) {
    assert!(info.transactions_authenticated);
    assert!(info.aoem_called);
    assert!(info.execution_completed);
    assert!(info.candidate_state_persisted);
    assert!(!info.authority_state_published);
    assert!(!info.chain_canonical);
    assert!(!info.proof_sealed);
    assert!(!info.safe);
    assert!(!info.finalized);
    assert_eq!(info.post_state_root, info.batch_result.state_delta_root);
    assert_eq!(info.receipt_root, info.batch_result.receipt_root);
    assert!(!info.batch_result.batch_result_id.is_empty());
}

#[test]
fn candidate_workspace_execution_competing_results_match_authority_and_survive_parent_gc() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let chain_id = 98_917_301;
    with_plan_runtime(|path, params| {
        let parent = committed_block(
            &run_nov_native_candidate_execution_plan_v1(
                &genesis_plan(chain_id, vec![raw_fixture(chain_id, 601)]),
                params,
            )
            .unwrap(),
        );
        let left_plan = successor_plan(
            &parent,
            vec![
                candidate_workspace_execution_raw(chain_id, 0, [0xc1; 32], 31, "deposit_reserve"),
                candidate_workspace_execution_raw(chain_id, 1, [0xc1; 32], 32, "unknown_method"),
            ],
        );
        // Both branches spend the same identity/nonce from the same snapshot.
        // Reserving against the process-wide pending registry would break this.
        let right_plan = successor_plan(
            &parent,
            vec![candidate_workspace_execution_raw(
                chain_id,
                0,
                [0xc1; 32],
                71,
                "deposit_reserve",
            )],
        );
        let late_plan = successor_plan(
            &parent,
            vec![candidate_workspace_execution_raw(
                chain_id,
                0,
                [0xc2; 32],
                91,
                "deposit_reserve",
            )],
        );
        let left = workspace::create_v1(&left_plan, params).unwrap();
        let right = workspace::create_v1(&right_plan, params).unwrap();
        let late = workspace::create_v1(&late_plan, params).unwrap();
        let pending_tx = decode_nov_native_tx_wire_v1(&left_plan.raw_txs[0]).unwrap();
        let pending_ir = nov_native_tx_to_adapter_tx_ir_v1(&pending_tx).unwrap();
        let pending_hash = tx_hash_array_from_ir_v1(&pending_ir);
        let (key, reservation_id) =
            verify_nov_native_auth_v1(params, &pending_tx, &pending_ir, pending_hash).unwrap();
        reserve_nov_native_auth_nonce_v1(key, reservation_id, None).unwrap();
        observe_network_runtime_native_pending_tx_local_native_payload_v1(
            chain_id,
            pending_hash,
            Some(&left_plan.raw_txs[0]),
        );
        let watched = left_plan
            .tx_hashes
            .iter()
            .chain(&right_plan.tx_hashes)
            .chain(&late_plan.tx_hashes)
            .copied()
            .collect::<Vec<_>>();
        let before = candidate_workspace_authority_fingerprint(path, params, chain_id, &watched);
        assert!(!before["runtime_nonce_reservations"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(
            workspace::load_execution_v1(chain_id, left.workspace_id, params)
                .unwrap()
                .is_none()
        );
        let left_result = workspace::execute_v1(chain_id, left.workspace_id, params).unwrap();
        let right_result = workspace::execute_v1(chain_id, right.workspace_id, params).unwrap();
        for (input, result) in [(&left, &left_result), (&right, &right_result)] {
            assert_candidate_workspace_execution_complete(result);
            assert_eq!(result.workspace_id, input.workspace_id);
            assert_eq!(result.plan_commitment, input.plan_commitment);
        }
        assert_ne!(left_result.post_state_root, right_result.post_state_root);
        assert!(left_result.batch_result.per_tx_receipts[0].status_ok);
        assert!(!left_result.batch_result.per_tx_receipts[1].status_ok);
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain_id, &watched),
            before
        );
        let snapshot =
            workspace::load_execution_snapshot_for_test_v1(chain_id, left.workspace_id, params)
                .unwrap();
        let counters_before = native_aoem_semantic_ingress_runtime_reuse_counters_v1();
        let replay =
            workspace::execute_with_checkpoint_v1(chain_id, left.workspace_id, params, |_| {
                panic!("completed replay must not execute or write any output checkpoint")
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(&replay).unwrap(),
            serde_json::to_value(&left_result).unwrap()
        );
        assert_eq!(
            serde_json::to_value(
                workspace::load_execution_v1(chain_id, left.workspace_id, params)
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
            serde_json::to_value(&left_result).unwrap()
        );
        assert_eq!(
            native_aoem_semantic_ingress_runtime_reuse_counters_v1(),
            counters_before
        );
        let namespace = native_aoem_owned_state_namespace_digest_v1(params, chain_id);
        let graph = candidate_workspace_graph(params);
        let old_head: NovAoemOwnedNativeStateHeadV1 = serde_json::from_slice(
            &graph
                .get(&native_aoem_owned_state_head_key_v1(chain_id, &namespace))
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        drop(graph);
        let committed = committed_block(
            &run_nov_native_candidate_execution_plan_v1(&left_plan, params).unwrap(),
        );
        let authoritative =
            load_validated_native_state_envelope_from_aoem_owner_v1(params, chain_id)
                .unwrap()
                .unwrap();
        assert_eq!(left_result.post_state_root, authoritative.state_root);
        assert_eq!(left_result.receipt_root, authoritative.receipt_root);
        assert_eq!(
            snapshot,
            serde_json::to_value(&authoritative.store).unwrap()
        );
        assert_eq!(
            native_aoem_execution_evidence_commitment_v1(&left_result.batch_result).unwrap(),
            native_aoem_execution_evidence_commitment_v1(&authoritative.batch_result).unwrap()
        );
        assert_eq!(left_result.batch_result, authoritative.batch_result);
        assert_eq!(
            committed.header.post_state_root,
            parse_fixed_hex_32_v1(&left_result.post_state_root, "candidate result root").unwrap()
        );
        let graph = candidate_workspace_graph(params);
        for index in 0..old_head.chunk_count {
            let key = native_aoem_owned_state_chunk_key_v1(
                chain_id,
                &namespace,
                &old_head.batch_result_id,
                index,
            )
            .unwrap();
            assert!(
                graph.get(&key).unwrap().is_none(),
                "authority parent chunk must really be pruned"
            );
        }
        drop(graph);
        reset_native_aoem_semantic_ingress_session_v1();
        let after_advance =
            candidate_workspace_authority_fingerprint(path, params, chain_id, &watched);
        assert!(
            workspace::load_execution_v1(chain_id, late.workspace_id, params)
                .unwrap()
                .is_none()
        );
        // This is a first execution after GC, not merely loading an old result.
        let late_result = workspace::execute_v1(chain_id, late.workspace_id, params).unwrap();
        assert_candidate_workspace_execution_complete(&late_result);
        assert_ne!(late_result.post_state_root, left_result.post_state_root);
        assert_eq!(
            serde_json::to_value(
                workspace::load_execution_v1(chain_id, right.workspace_id, params)
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
            serde_json::to_value(&right_result).unwrap()
        );
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain_id, &watched),
            after_advance
        );
        workspace::corrupt_execution_output_for_test_v1(chain_id, late.workspace_id, params)
            .unwrap();
        assert!(workspace::load_execution_v1(chain_id, late.workspace_id, params).is_err());
        assert!(workspace::execute_v1(chain_id, late.workspace_id, params).is_err());
        assert_eq!(
            workspace::abort_v1(chain_id, late.workspace_id, params)
                .unwrap()
                .status,
            workspace::WorkspaceStatusV1::Aborted
        );
        let aborted = workspace::abort_v1(chain_id, right.workspace_id, params).unwrap();
        assert_eq!(aborted.status, workspace::WorkspaceStatusV1::Aborted);
        assert!(workspace::execute_v1(chain_id, right.workspace_id, params).is_err());
        assert!(workspace::load_execution_v1(chain_id, right.workspace_id, params).is_err());
        assert!(workspace::load_execution_snapshot_for_test_v1(
            chain_id,
            right.workspace_id,
            params
        )
        .is_err());
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain_id, &watched),
            after_advance
        );
        clear_native_auth_runtime_reservations_for_chain_v1(chain_id);
    });
}

#[test]
fn candidate_workspace_execution_authenticates_the_entire_batch_before_any_output() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let chain_id = 98_917_302;
    with_plan_runtime(|path, params| {
        let genesis = genesis_plan(chain_id, vec![raw_fixture(chain_id, 701)]);
        let parent =
            committed_block(&run_nov_native_candidate_execution_plan_v1(&genesis, params).unwrap());
        let valid =
            candidate_workspace_execution_raw(chain_id, 0, [0xd1; 32], 41, "deposit_reserve");
        let mut invalid_signature = decode_nov_native_tx_wire_v1(&valid).unwrap();
        invalid_signature.signature[40] ^= 1;
        let mut wrong_subject = decode_nov_native_tx_wire_v1(&valid).unwrap();
        let NovTxKindV1::Execute(execute) = &mut wrong_subject.kind else {
            unreachable!()
        };
        execute.fee_owner_account_id = Some(format!("0x{}", "ab".repeat(32)));
        sign_nov_native_tx_with_seed_v1(&mut wrong_subject, [0xd1; 32]).unwrap();
        let same_nonce_other_intent =
            candidate_workspace_execution_raw(chain_id, 0, [0xd1; 32], 42, "deposit_reserve");
        let mut last_bad_signature = decode_nov_native_tx_wire_v1(
            &candidate_workspace_execution_raw(chain_id, 0, [0xd2; 32], 43, "deposit_reserve"),
        )
        .unwrap();
        last_bad_signature.signature[80] ^= 1;
        let cases = [
            (
                "signature",
                vec![encode_native_auth_test_tx_v1(&invalid_signature)],
            ),
            (
                "subject",
                vec![encode_native_auth_test_tx_v1(&wrong_subject)],
            ),
            (
                "chain",
                vec![candidate_workspace_execution_raw(
                    chain_id + 1,
                    0,
                    [0xd3; 32],
                    44,
                    "deposit_reserve",
                )],
            ),
            (
                "nonce gap",
                vec![candidate_workspace_execution_raw(
                    chain_id,
                    1,
                    [0xd4; 32],
                    45,
                    "deposit_reserve",
                )],
            ),
            ("same nonce", vec![valid.clone(), same_nonce_other_intent]),
            (
                "last signature",
                vec![
                    valid.clone(),
                    encode_native_auth_test_tx_v1(&last_bad_signature),
                ],
            ),
            ("already committed", genesis.raw_txs.clone()),
        ];
        for (label, raws) in cases {
            let plan = successor_plan(&parent, raws);
            let ready = workspace::create_v1(&plan, params).unwrap();
            let before =
                candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes);
            let counters_before = native_aoem_semantic_ingress_runtime_reuse_counters_v1();
            let error =
                workspace::execute_with_checkpoint_v1(chain_id, ready.workspace_id, params, |_| {
                    panic!("{label}: invalid input must not reserve or write an output checkpoint")
                })
                .expect_err(label);
            assert!(!error.to_string().is_empty());
            assert!(
                workspace::load_execution_v1(chain_id, ready.workspace_id, params)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                native_aoem_semantic_ingress_runtime_reuse_counters_v1(),
                counters_before,
                "{label}: authentication must precede AOEM precommit"
            );
            assert_eq!(
                candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes),
                before,
                "{label}: authority and pending must not change"
            );
            let aborted = workspace::abort_v1(chain_id, ready.workspace_id, params).unwrap();
            assert_eq!(aborted.status, workspace::WorkspaceStatusV1::Aborted);
            assert!(workspace::execute_v1(chain_id, ready.workspace_id, params).is_err());
        }
        let plan = successor_plan(&parent, vec![valid]);
        let ready = workspace::create_v1(&plan, params).unwrap();
        let complete = workspace::execute_v1(chain_id, ready.workspace_id, params).unwrap();
        assert_candidate_workspace_execution_complete(&complete);
        assert!(
            complete.batch_result.per_tx_receipts[0].status_ok,
            "earlier rejected batches must not consume this candidate nonce"
        );
        assert!(workspace::load_execution_v1(chain_id, [0xff; 32], params)
            .unwrap()
            .is_none());
        assert!(workspace::execute_v1(chain_id, [0xff; 32], params).is_err());
    });
}

#[test]
fn candidate_workspace_execution_completed_checkpoints_resume_and_abort_fail_closed() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let chain_id = 98_917_303;
    with_plan_runtime(|path, params| {
        let parent = committed_block(
            &run_nov_native_candidate_execution_plan_v1(
                &genesis_plan(chain_id, vec![raw_fixture(chain_id, 801)]),
                params,
            )
            .unwrap(),
        );
        for (index, checkpoint) in [
            workspace::ExecutionCheckpointV1::OutputReserved,
            workspace::ExecutionCheckpointV1::PartialOutput,
            workspace::ExecutionCheckpointV1::OutputWritten,
            workspace::ExecutionCheckpointV1::Completed,
        ]
        .into_iter()
        .enumerate()
        {
            let plan = successor_plan(
                &parent,
                vec![candidate_workspace_execution_raw(
                    chain_id,
                    0,
                    [0xe1 + index as u8; 32],
                    51 + index as u64,
                    "deposit_reserve",
                )],
            );
            let ready = workspace::create_v1(&plan, params).unwrap();
            let before =
                candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes);
            let error = workspace::execute_with_checkpoint_v1(
                chain_id,
                ready.workspace_id,
                params,
                |point| {
                    if point == checkpoint {
                        anyhow::bail!("injected completed output checkpoint {point:?}");
                    }
                    Ok(())
                },
            )
            .expect_err("interrupt only after an acknowledged graph commit");
            assert!(error
                .to_string()
                .contains("injected completed output checkpoint"));
            reset_native_aoem_semantic_ingress_session_v1();
            let recovered =
                workspace::load_execution_v1(chain_id, ready.workspace_id, params).unwrap();
            assert_eq!(
                recovered.is_some(),
                checkpoint == workspace::ExecutionCheckpointV1::Completed
            );
            let resumed = if checkpoint == workspace::ExecutionCheckpointV1::Completed {
                workspace::execute_with_checkpoint_v1(chain_id, ready.workspace_id, params, |_| {
                    panic!("a lost Completed reply must be a read-only replay")
                })
                .unwrap()
            } else {
                workspace::execute_v1(chain_id, ready.workspace_id, params)
                    .expect("finish exact incomplete output")
            };
            assert_candidate_workspace_execution_complete(&resumed);
            assert_eq!(resumed.workspace_id, ready.workspace_id);
            assert_eq!(resumed.plan_commitment, plan.plan_commitment);
            assert_eq!(
                candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes),
                before
            );
        }
        let plan = successor_plan(
            &parent,
            vec![candidate_workspace_execution_raw(
                chain_id,
                0,
                [0xef; 32],
                66,
                "deposit_reserve",
            )],
        );
        let ready = workspace::create_v1(&plan, params).unwrap();
        let before =
            candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes);
        workspace::execute_with_checkpoint_v1(chain_id, ready.workspace_id, params, |point| {
            if point == workspace::ExecutionCheckpointV1::PartialOutput {
                anyhow::bail!("stop partial output before abort");
            }
            Ok(())
        })
        .expect_err("leave recoverable but uncompleted candidate output");
        workspace::abort_v1(chain_id, ready.workspace_id, params).unwrap();
        reset_native_aoem_semantic_ingress_session_v1();
        assert!(workspace::execute_v1(chain_id, ready.workspace_id, params).is_err());
        assert!(workspace::load_execution_v1(chain_id, ready.workspace_id, params).is_err());
        assert!(workspace::load_execution_snapshot_for_test_v1(
            chain_id,
            ready.workspace_id,
            params
        )
        .is_err());
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes),
            before
        );
    });
}

const CANDIDATE_EXECUTION_PROCESS_PARAMS_ENV: &str =
    "NOVOVM_TEST_CANDIDATE_EXECUTION_PROCESS_PARAMS";
const CANDIDATE_EXECUTION_PROCESS_MODE_ENV: &str = "NOVOVM_TEST_CANDIDATE_EXECUTION_PROCESS_MODE";
const CANDIDATE_EXECUTION_PROCESS_MANIFEST_ENV: &str =
    "NOVOVM_TEST_CANDIDATE_EXECUTION_PROCESS_MANIFEST";

fn run_candidate_workspace_execution_process(
    params: &serde_json::Value,
    mode: &str,
    manifest: &Path,
    log_path: &Path,
    governance_events_path: &Path,
) -> u32 {
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    let log = fs::File::create(log_path).expect("create isolated execution worker log");
    let stderr = log.try_clone().unwrap();
    let mut child = ChildGuard(std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tx_ingress::tests::candidate_workspace_tests::candidate_workspace_execution_process_worker",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env(CANDIDATE_EXECUTION_PROCESS_PARAMS_ENV, serde_json::to_string(params).unwrap())
        .env(CANDIDATE_EXECUTION_PROCESS_MODE_ENV, mode)
        .env(CANDIDATE_EXECUTION_PROCESS_MANIFEST_ENV, manifest)
        // The parent never mutates this process-global event setting. Each
        // isolated child gets a private, writable sentinel file instead.
        .env("NOVOVM_GOVERNANCE_EVENTS_PATH", governance_events_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(stderr))
        .spawn().expect("spawn a distinct execution-result recovery process"));
    let pid = child.0.id();
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("poll execution worker") {
            break status;
        }
        if started.elapsed() >= std::time::Duration::from_secs(90) {
            let _ = child.0.kill();
            let _ = child.0.wait();
            let bytes = fs::read(log_path).unwrap_or_default();
            panic!(
                "candidate execution child {mode} exceeded 90 seconds: {}",
                String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(65_536)..])
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    if !status.success() {
        let bytes = fs::read(log_path).unwrap_or_default();
        panic!(
            "candidate execution child {mode} failed ({status}): {}",
            String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(65_536)..])
        );
    }
    pid
}

#[test]
fn candidate_workspace_execution_results_and_abort_survive_three_real_processes() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    // Only fixture paths/environment are prepared in this process. All AOEM
    // and ledger handles belong to children that exit before the next opens.
    with_plan_runtime(|path, params| {
        struct Files(Vec<PathBuf>);
        impl Drop for Files {
            fn drop(&mut self) {
                for file in &self.0 {
                    let _ = fs::remove_file(file);
                }
            }
        }
        let manifest = path.with_extension("candidate-execution-process.json");
        let events = path.with_extension("candidate-execution-governance-events.jsonl");
        let mut files = Files(vec![manifest.clone(), events.clone()]);
        for mode in ["execute", "recover_abort", "recover_aborted"] {
            let log = path.with_extension(format!("candidate-execution-process-{mode}.log"));
            files.0.push(log.clone());
            let pid =
                run_candidate_workspace_execution_process(params, mode, &manifest, &log, &events);
            assert_ne!(pid, std::process::id());
            let evidence: serde_json::Value =
                serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
            // A typo in the exact test filter must not pass by running 0 tests.
            assert_eq!(evidence["completed_phase"], mode);
            assert_eq!(evidence["completed_pid"].as_u64(), Some(u64::from(pid)));
        }
    });
}

#[test]
#[ignore = "invoked only by the bounded candidate execution multi-process parent"]
fn candidate_workspace_execution_process_worker() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let params: serde_json::Value = serde_json::from_str(
        &std::env::var(CANDIDATE_EXECUTION_PROCESS_PARAMS_ENV).expect("parent fixture params"),
    )
    .unwrap();
    let mode = std::env::var(CANDIDATE_EXECUTION_PROCESS_MODE_ENV).unwrap();
    let manifest_path =
        PathBuf::from(std::env::var_os(CANDIDATE_EXECUTION_PROCESS_MANIFEST_ENV).unwrap());
    let event_path = PathBuf::from(std::env::var_os("NOVOVM_GOVERNANCE_EVENTS_PATH").unwrap());
    let path = resolve_native_execution_store_path_from_params_v1(&params).unwrap();
    let chain_id = 98_917_304;
    let mut manifest = if mode == "execute" {
        fs::write(
            &event_path,
            b"{\"schema\":\"candidate-execution-test-sentinel\"}\n",
        )
        .unwrap();
        let governance_before = fs::read(&event_path).unwrap();
        let parent = committed_block(
            &run_nov_native_candidate_execution_plan_v1(
                &genesis_plan(chain_id, vec![raw_fixture(chain_id, 901)]),
                &params,
            )
            .unwrap(),
        );
        let mut explicit_privacy = decode_nov_native_tx_wire_v1(
            &candidate_workspace_execution_raw(chain_id, 1, [0xf1; 32], 82, "deposit_reserve"),
        )
        .unwrap();
        let NovTxKindV1::Execute(execute) = &mut explicit_privacy.kind else {
            unreachable!()
        };
        execute.execution_policy = NovExecutionPolicyV1::PrivacyRequired;
        execute.privacy_mode = NovPrivacyModeV1::Public;
        sign_nov_native_tx_with_seed_v1(&mut explicit_privacy, [0xf1; 32]).unwrap();
        let mut implicit_privacy = decode_nov_native_tx_wire_v1(
            &candidate_workspace_execution_raw(chain_id, 2, [0xf1; 32], 83, "deposit_reserve"),
        )
        .unwrap();
        let NovTxKindV1::Execute(execute) = &mut implicit_privacy.kind else {
            unreachable!()
        };
        execute.fee_policy.pay_asset = "NUSD".to_string();
        assert_eq!(execute.execution_policy, NovExecutionPolicyV1::Standard);
        sign_nov_native_tx_with_seed_v1(&mut implicit_privacy, [0xf1; 32]).unwrap();
        let plan = successor_plan(
            &parent,
            vec![
                candidate_workspace_execution_raw(chain_id, 0, [0xf1; 32], 81, "deposit_reserve"),
                encode_native_auth_test_tx_v1(&explicit_privacy),
                encode_native_auth_test_tx_v1(&implicit_privacy),
            ],
        );
        let authority =
            candidate_workspace_persistent_fingerprint(&path, &params, chain_id, &plan.tx_hashes);
        let ready = workspace::create_v1(&plan, &params).unwrap();
        let result = workspace::execute_v1(chain_id, ready.workspace_id, &params).unwrap();
        assert_candidate_workspace_execution_complete(&result);
        assert!(result.batch_result.per_tx_receipts[0].status_ok);
        for receipt in &result.batch_result.per_tx_receipts[1..] {
            assert!(!receipt.status_ok, "both explicit and fee-asset-induced privacy demands must be rejected as ordinary execution receipts");
            assert!(receipt
                .error_class
                .as_deref()
                .is_some_and(|error| error.contains(ERR_PRIVACY_REQUIRED_BUT_PATH_NOT_AVAILABLE)));
        }
        assert_eq!(
            fs::read(&event_path).unwrap(),
            governance_before,
            "isolated policy evaluation must not append shared governance events"
        );
        assert_eq!(
            candidate_workspace_persistent_fingerprint(&path, &params, chain_id, &plan.tx_hashes),
            authority
        );
        let snapshot =
            workspace::load_execution_snapshot_for_test_v1(chain_id, ready.workspace_id, &params)
                .unwrap();
        serde_json::json!({ "plan": plan, "ready": ready, "result": result, "snapshot": snapshot, "authority": authority, "governance_before": governance_before })
    } else {
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let plan: NovNativeCandidateExecutionPlanV1 =
            serde_json::from_value(manifest["plan"].clone()).unwrap();
        let id: [u8; 32] =
            serde_json::from_value(manifest["ready"]["workspace_id"].clone()).unwrap();
        assert_eq!(
            candidate_workspace_persistent_fingerprint(&path, &params, chain_id, &plan.tx_hashes),
            manifest["authority"]
        );
        let counters_before = native_aoem_semantic_ingress_runtime_reuse_counters_v1();
        match mode.as_str() {
            "recover_abort" => {
                assert_eq!(manifest["completed_phase"], "execute");
                let loaded = workspace::load_execution_v1(chain_id, id, &params)
                    .unwrap()
                    .unwrap();
                assert_candidate_workspace_execution_complete(&loaded);
                assert_eq!(serde_json::to_value(&loaded).unwrap(), manifest["result"]);
                assert_eq!(
                    workspace::load_execution_snapshot_for_test_v1(chain_id, id, &params).unwrap(),
                    manifest["snapshot"]
                );
                let replay = workspace::execute_with_checkpoint_v1(chain_id, id, &params, |_| {
                    panic!("result after process exit must load without reexecuting or rewriting checkpoints")
                }).unwrap();
                assert_eq!(serde_json::to_value(replay).unwrap(), manifest["result"]);
                assert_eq!(
                    workspace::abort_v1(chain_id, id, &params).unwrap().status,
                    workspace::WorkspaceStatusV1::Aborted
                );
                assert!(workspace::load_execution_v1(chain_id, id, &params).is_err());
            }
            "recover_aborted" => {
                assert_eq!(manifest["completed_phase"], "recover_abort");
                assert_eq!(
                    workspace::load_v1(chain_id, id, &params)
                        .unwrap()
                        .unwrap()
                        .status,
                    workspace::WorkspaceStatusV1::Aborted
                );
                assert!(workspace::execute_v1(chain_id, id, &params).is_err());
                assert!(workspace::load_execution_v1(chain_id, id, &params).is_err());
                assert!(
                    workspace::load_execution_snapshot_for_test_v1(chain_id, id, &params).is_err()
                );
                assert!(workspace::create_v1(&plan, &params).is_err());
            }
            _ => panic!("unknown candidate execution recovery mode: {mode}"),
        }
        assert_eq!(
            native_aoem_semantic_ingress_runtime_reuse_counters_v1(),
            counters_before,
            "recovery and aborted reads must not resubmit precommit"
        );
        assert_eq!(
            serde_json::to_value(fs::read(&event_path).unwrap()).unwrap(),
            manifest["governance_before"]
        );
        assert_eq!(
            candidate_workspace_persistent_fingerprint(&path, &params, chain_id, &plan.tx_hashes),
            manifest["authority"]
        );
        manifest
    };
    manifest["completed_phase"] = serde_json::json!(mode);
    manifest["completed_pid"] = serde_json::json!(std::process::id());
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    reset_native_aoem_semantic_ingress_session_v1();
    // These are clean process exits. They do not simulate OS/power-loss crashes.
}
