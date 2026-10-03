use crate::tx_ingress::candidate_workspace as workspace;

fn candidate_workspace_graph(params: &serde_json::Value) -> novovm_exec::AoemSemanticGraphStoreV1 {
    novovm_exec::AoemSemanticGraphStoreV1::open(
        &native_aoem_owned_runtime_config_v1().expect("workspace test runtime"),
        &native_aoem_owned_state_db_path_v1(params),
        &novovm_exec::AoemStorageProviderConfigV1::default(),
    )
    .expect("reopen persisted AOEM graph")
}

// Compare authority bytes and query-visible facts, not telemetry counters. The
// workspace is deliberately allowed to add its own AOEM keyspace, never these.
fn candidate_workspace_authority_fingerprint(
    path: &Path,
    params: &serde_json::Value,
    chain_id: u64,
    watched_txs: &[[u8; 32]],
) -> serde_json::Value {
    let authority_heads = heads(path, params, chain_id);
    let namespace = native_aoem_owned_state_namespace_digest_v1(params, chain_id);
    let graph = candidate_workspace_graph(params);
    let head_bytes = graph
        .get(&native_aoem_owned_state_head_key_v1(chain_id, &namespace))
        .expect("read raw authority head")
        .expect("executed parent has authority head");
    let head: NovAoemOwnedNativeStateHeadV1 =
        serde_json::from_slice(&head_bytes).expect("decode authority head");
    let chunks: Vec<_> = (0..head.chunk_count)
        .map(|index| {
            let key = native_aoem_owned_state_chunk_key_v1(
                chain_id,
                &namespace,
                &head.batch_result_id,
                index,
            )
            .unwrap();
            let bytes = graph.get(&key).unwrap().expect("authority chunk exists");
            (key, bytes)
        })
        .collect();
    let bootstrap = graph
        .get(&native_block_ledger_bootstrap_marker_key_v1(
            chain_id, &namespace,
        ))
        .unwrap();
    drop(graph);
    let ledger = NovNativeBlockLedgerV1::open_existing_read_only(
        &nov_native_block_ledger_rocksdb_path_v1(path),
    )
    .unwrap()
    .expect("existing authority ledger");
    let blocks = ledger.load_blocks_from_height(chain_id, 1, 100).unwrap();
    let mut candidates = Vec::new();
    let mut children = Vec::new();
    for height in 1..=blocks.len() as u64 + 1 {
        candidates.push(
            ledger
                .load_candidate_records_by_height(chain_id, height)
                .unwrap(),
        );
    }
    let mut all_txs = watched_txs.to_vec();
    for block in &blocks {
        all_txs.extend_from_slice(&block.body.tx_hashes);
        assert_eq!(
            ledger
                .load_by_hash(chain_id, block.header.block_hash)
                .unwrap(),
            Some(block.clone()),
        );
        children.push(
            ledger
                .load_candidate_children(chain_id, block.header.block_hash)
                .unwrap(),
        );
    }
    all_txs.sort_unstable();
    all_txs.dedup();
    let transaction_facts: Vec<_> = all_txs.iter().map(|hash| {
        serde_json::json!({
            "hash": hash,
            "tx_location": ledger.load_tx_location(chain_id, *hash).unwrap(),
            "receipt_location": ledger.load_receipt_location(chain_id, *hash).unwrap(),
            "pending": novovm_network::get_network_runtime_native_pending_tx_v1(chain_id, *hash),
            "pending_payload": novovm_network::get_network_runtime_native_pending_tx_payload_v1(chain_id, *hash),
            "receipt": get_nov_native_execution_receipt_by_hash_with_store_path_v1(
                path, &to_hex_prefixed_v1(hash)).unwrap(),
        })
    }).collect();
    let mut reservations: Vec<_> = NOV_NATIVE_AUTH_NONCE_RESERVATIONS_V1
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("nonce registry")
        .iter()
        .filter(|((stored_chain, _, _), _)| *stored_chain == chain_id)
        .map(|(key, value)| (key.clone(), *value))
        .collect();
    reservations.sort_unstable();
    serde_json::json!({
        "authority_heads": authority_heads,
        "aoem_raw_head": head_bytes,
        "aoem_raw_chunks": chunks,
        "aoem_bootstrap_marker": bootstrap,
        "ledger_blocks": blocks,
        "ledger_ownership": ledger.load_aoem_ownership().unwrap(),
        "ledger_candidates": candidates,
        "ledger_children": children,
        "transaction_facts": transaction_facts,
        "runtime_nonce_reservations": reservations,
    })
}

fn assert_candidate_workspace_input_only(info: &workspace::WorkspaceInfoV1) {
    assert_eq!(
        info.input_snapshot_verified,
        info.status == workspace::WorkspaceStatusV1::Ready
    );
    assert!(!info.transactions_authenticated);
    assert!(!info.execution_completed);
    assert!(!info.chain_canonical);
    assert!(!info.proof_sealed);
    assert!(!info.safe);
    assert!(!info.finalized);
}

#[test]
fn candidate_workspace_competing_inputs_are_isolated_and_survive_authority_snapshot_gc() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain_id = 98_917_201;
    with_plan_runtime(|path, params| {
        let parent = committed_block(
            &run_nov_native_candidate_execution_plan_v1(
                &genesis_plan(chain_id, vec![raw_fixture(chain_id, 101)]),
                params,
            )
            .expect("execute actual AOEM parent"),
        );
        let left_plan = successor_plan(&parent, vec![raw_fixture(chain_id, 102)]);
        let right_plan = successor_plan(&parent, vec![raw_fixture(chain_id, 103)]);
        let incomplete_plan = successor_plan(&parent, vec![raw_fixture(chain_id, 104)]);
        let pending_tx = build_signed_native_auth_test_tx_v1(
            chain_id,
            0,
            [0xa5; 32],
            "workspace-unrelated-pending",
            29,
        );
        let pending_raw = encode_native_auth_test_tx_v1(&pending_tx);
        let pending_ir = nov_native_tx_to_adapter_tx_ir_v1(&pending_tx).unwrap();
        let pending_hash = tx_hash_array_from_ir_v1(&pending_ir);
        let (reservation, reservation_id) =
            verify_nov_native_auth_v1(params, &pending_tx, &pending_ir, pending_hash).unwrap();
        reserve_nov_native_auth_nonce_v1(reservation, reservation_id, None).unwrap();
        observe_network_runtime_native_pending_tx_local_native_payload_v1(
            chain_id,
            pending_hash,
            Some(&pending_raw),
        );
        let watched = [
            left_plan.tx_hashes[0],
            right_plan.tx_hashes[0],
            incomplete_plan.tx_hashes[0],
            pending_hash,
        ];
        let before = candidate_workspace_authority_fingerprint(path, params, chain_id, &watched);
        assert!(!before["runtime_nonce_reservations"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(
            novovm_network::get_network_runtime_native_pending_tx_v1(chain_id, pending_hash)
                .is_some()
        );

        let left = workspace::create_v1(&left_plan, params).expect("stage left input");
        let right = workspace::create_v1(&right_plan, params).expect("stage competing right input");
        assert_ne!(left.workspace_id, right.workspace_id);
        assert_ne!(left.slot, right.slot);
        assert_eq!(left.parent_block_hash, right.parent_block_hash);
        assert_eq!(left.parent_snapshot_digest, right.parent_snapshot_digest);
        assert_ne!(left.payload_digest, right.payload_digest);
        assert_eq!(left.status, workspace::WorkspaceStatusV1::Ready);
        assert_candidate_workspace_input_only(&left);
        assert_candidate_workspace_input_only(&right);
        assert_eq!(workspace::create_v1(&left_plan, params).unwrap(), left);
        assert_eq!(
            workspace::list_v1(chain_id, params).unwrap(),
            vec![left.clone(), right.clone()]
        );
        assert_eq!(
            workspace::load_v1(chain_id, [0xff; 32], params).unwrap(),
            None
        );
        assert!(workspace::abort_v1(chain_id, [0xff; 32], params).is_err());

        workspace::create_with_checkpoint_v1(&incomplete_plan, params, |point| {
            if point == workspace::CheckpointV1::PartialPayload {
                anyhow::bail!("test interrupted input copy");
            }
            Ok(())
        })
        .expect_err("leave an incomplete workspace before authority advances");
        let incomplete = workspace::list_v1(chain_id, params).unwrap().pop().unwrap();
        assert_eq!(incomplete.status, workspace::WorkspaceStatusV1::Staging);
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain_id, &watched),
            before
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
        let next = committed_block(
            &run_nov_native_candidate_execution_plan_v1(&left_plan, params)
                .expect("only the explicit authority executor advances height"),
        );
        assert_eq!(next.header.height, 2);
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
                "old authority chunk {index} must actually be pruned"
            );
        }
        drop(graph);
        // All workspace API calls reopen their graph. Also drop the Host's
        // thread-local runtime; this is process-local reopen, not a process crash.
        reset_native_aoem_semantic_ingress_session_v1();
        let after_advance =
            candidate_workspace_authority_fingerprint(path, params, chain_id, &watched);
        assert_eq!(
            workspace::load_v1(chain_id, left.workspace_id, params).unwrap(),
            Some(left.clone())
        );
        assert_eq!(
            workspace::load_v1(chain_id, right.workspace_id, params).unwrap(),
            Some(right.clone())
        );
        assert_eq!(workspace::create_v1(&right_plan, params).unwrap(), right);
        assert_eq!(workspace::create_v1(&left_plan, params).unwrap(), left);
        assert!(
            workspace::create_v1(&incomplete_plan, params).is_err(),
            "incomplete copy cannot use a different current parent"
        );
        let aborted = workspace::abort_v1(chain_id, right.workspace_id, params).unwrap();
        assert_eq!(aborted.status, workspace::WorkspaceStatusV1::Aborted);
        assert_candidate_workspace_input_only(&aborted);
        assert_eq!(
            workspace::abort_v1(chain_id, right.workspace_id, params).unwrap(),
            aborted
        );
        reset_native_aoem_semantic_ingress_session_v1();
        assert_eq!(
            workspace::load_v1(chain_id, right.workspace_id, params).unwrap(),
            Some(aborted)
        );
        assert!(
            workspace::create_v1(&right_plan, params).is_err(),
            "tombstone must prevent revival after reopen"
        );
        let aborted_partial =
            workspace::abort_v1(chain_id, incomplete.workspace_id, params).unwrap();
        assert_eq!(
            aborted_partial.status,
            workspace::WorkspaceStatusV1::Aborted
        );
        workspace::corrupt_first_chunk_for_test_v1(chain_id, left.workspace_id, params)
            .expect("damage only the separate candidate input copy");
        assert!(
            workspace::load_v1(chain_id, left.workspace_id, params).is_err(),
            "Ready never hides corrupted input bytes"
        );
        assert!(
            workspace::create_v1(&left_plan, params).is_err(),
            "Ready replay must validate stored bytes"
        );
        let discarded = workspace::abort_v1(chain_id, left.workspace_id, params)
            .expect("a corrupted ready workspace can be tombstoned without trusting its payload");
        assert_eq!(discarded.status, workspace::WorkspaceStatusV1::Aborted);
        assert_eq!(
            workspace::load_v1(chain_id, left.workspace_id, params).unwrap(),
            Some(discarded)
        );
        assert!(
            workspace::create_v1(&left_plan, params).is_err(),
            "discarding corruption cannot revive an input plan"
        );
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain_id, &watched),
            after_advance
        );
        clear_native_auth_runtime_reservations_for_chain_v1(chain_id);
    });
}

#[test]
fn candidate_workspace_completed_checkpoints_recover_without_authority_mutation() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain_id = 98_917_202;
    with_plan_runtime(|path, params| {
        let parent = committed_block(
            &run_nov_native_candidate_execution_plan_v1(
                &genesis_plan(chain_id, vec![raw_fixture(chain_id, 201)]),
                params,
            )
            .unwrap(),
        );
        for (index, failure_point) in [
            workspace::CheckpointV1::Reserved,
            workspace::CheckpointV1::PartialPayload,
            workspace::CheckpointV1::PayloadWritten,
            workspace::CheckpointV1::Ready,
        ]
        .into_iter()
        .enumerate()
        {
            let plan = successor_plan(&parent, vec![raw_fixture(chain_id, 202 + index as u64)]);
            let before =
                candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes);
            let error = workspace::create_with_checkpoint_v1(&plan, params, |point| {
                if point == failure_point {
                    anyhow::bail!("injected completed checkpoint {point:?}");
                }
                Ok(())
            })
            .expect_err("inject interruption only after an acknowledged graph commit");
            assert!(error.to_string().contains("injected completed checkpoint"));
            reset_native_aoem_semantic_ingress_session_v1();
            let entries = workspace::list_v1(chain_id, params).unwrap();
            assert_eq!(entries.len(), index + 1);
            let interrupted = entries.last().unwrap();
            let expected_status = if failure_point == workspace::CheckpointV1::Ready {
                workspace::WorkspaceStatusV1::Ready
            } else {
                workspace::WorkspaceStatusV1::Staging
            };
            assert_eq!(interrupted.status, expected_status);
            assert_candidate_workspace_input_only(interrupted);
            assert_eq!(
                workspace::load_v1(chain_id, interrupted.workspace_id, params)
                    .unwrap()
                    .as_ref(),
                Some(interrupted)
            );
            let resumed = if failure_point == workspace::CheckpointV1::Ready {
                workspace::create_with_checkpoint_v1(&plan, params, |_| {
                    panic!("already-ready replay must not rewrite any graph checkpoint")
                })
                .expect("a lost Ready reply is an exact read-only replay")
            } else {
                workspace::create_v1(&plan, params).expect("finish exact interrupted copy")
            };
            assert_eq!(resumed.status, workspace::WorkspaceStatusV1::Ready);
            assert_eq!(resumed.workspace_id, interrupted.workspace_id);
            assert_eq!(resumed.slot, interrupted.slot);
            assert_eq!(resumed.payload_digest, interrupted.payload_digest);
            assert_candidate_workspace_input_only(&resumed);
            assert_eq!(
                workspace::list_v1(chain_id, params).unwrap().len(),
                index + 1
            );
            assert_eq!(
                candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes),
                before
            );
        }
    });
}

#[test]
fn candidate_workspace_rejects_wrong_parent_context_protocol_and_canonical_hash() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain_id = 98_917_203;
    with_plan_runtime(|path, params| {
        let genesis = genesis_plan(chain_id, vec![raw_fixture(chain_id, 301)]);
        assert!(
            workspace::create_v1(&genesis, params).is_err(),
            "workspace cannot bootstrap authority"
        );
        let parent =
            committed_block(&run_nov_native_candidate_execution_plan_v1(&genesis, params).unwrap());
        let plan = successor_plan(&parent, vec![raw_fixture(chain_id, 302)]);
        let before =
            candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes);
        let mut invalid_plans = vec![genesis];
        for change in 0..4 {
            let mut context = plan.context;
            match change {
                0 => context.parent_block_hash[0] ^= 1,
                1 => context.block_height += 1,
                2 => context.slot = parent.header.slot,
                _ => context.timestamp_unix_ms = parent.header.timestamp_unix_ms - 1,
            }
            invalid_plans.push(make_plan(
                context,
                plan.pre_state_root,
                plan.aoem_parent.clone(),
                plan.raw_txs.clone(),
            ));
        }
        let mut wrong_parent = plan.aoem_parent.clone().unwrap();
        wrong_parent.batch_id.push_str("-wrong");
        invalid_plans.push(make_plan(
            plan.context,
            plan.pre_state_root,
            Some(wrong_parent),
            plan.raw_txs.clone(),
        ));
        let mut wrong_parent = plan.aoem_parent.clone().unwrap();
        wrong_parent.state_root[0] ^= 1;
        invalid_plans.push(make_plan(
            plan.context,
            wrong_parent.state_root,
            Some(wrong_parent),
            plan.raw_txs.clone(),
        ));
        let mut protocol = plan.protocol_config_commitment;
        protocol[0] ^= 1;
        invalid_plans.push(
            NovNativeCandidateExecutionPlanV1::new(
                plan.context,
                protocol,
                plan.pre_state_root,
                plan.aoem_parent.clone(),
                plan.tx_hashes.clone(),
                plan.raw_txs.clone(),
            )
            .unwrap(),
        );
        let mut hashes = plan.tx_hashes.clone();
        hashes[0][0] ^= 1;
        invalid_plans.push(
            NovNativeCandidateExecutionPlanV1::new(
                plan.context,
                plan.protocol_config_commitment,
                plan.pre_state_root,
                plan.aoem_parent.clone(),
                hashes,
                plan.raw_txs.clone(),
            )
            .expect("a plan commitment alone does not prove its canonical tx hash"),
        );
        let mut changed_body = plan.clone();
        changed_body.raw_txs[0].push(0xff);
        invalid_plans.push(changed_body);
        for invalid in invalid_plans {
            assert!(workspace::create_v1(&invalid, params).is_err());
            assert!(
                workspace::list_v1(chain_id, params).unwrap().is_empty(),
                "invalid inputs must not reserve slots"
            );
            assert_eq!(
                candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes),
                before
            );
        }
        let mut no_ownership = params.clone();
        no_ownership
            .as_object_mut()
            .unwrap()
            .remove("aoem_owned_gate_config");
        assert!(workspace::create_v1(&plan, &no_ownership).is_err());
        assert!(workspace::list_v1(chain_id, params).unwrap().is_empty());
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes),
            before
        );
        assert_candidate_workspace_input_only(&workspace::create_v1(&plan, params).unwrap());
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain_id, &plan.tx_hashes),
            before
        );
    });
}

#[test]
fn candidate_workspace_capacity_counts_staging_and_aborted_without_slot_reuse() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain_id = 98_917_204;
    with_plan_runtime(|path, params| {
        let parent = committed_block(
            &run_nov_native_candidate_execution_plan_v1(
                &genesis_plan(chain_id, vec![raw_fixture(chain_id, 401)]),
                params,
            )
            .unwrap(),
        );
        let base = successor_plan(&parent, vec![raw_fixture(chain_id, 402)]);
        let before =
            candidate_workspace_authority_fingerprint(path, params, chain_id, &base.tx_hashes);
        let plans: Vec<_> = (0..=workspace::MAX_WORKSPACES_V1)
            .map(|index| {
                let mut context = base.context;
                context.slot += index as u64;
                make_plan(
                    context,
                    base.pre_state_root,
                    base.aoem_parent.clone(),
                    base.raw_txs.clone(),
                )
            })
            .collect();
        for plan in &plans[..workspace::MAX_WORKSPACES_V1] {
            workspace::create_with_checkpoint_v1(plan, params, |point| {
                assert_eq!(point, workspace::CheckpointV1::Reserved);
                anyhow::bail!("leave capacity slot durably staging")
            })
            .expect_err("reserve without completing input copy");
        }
        let staged = workspace::list_v1(chain_id, params).unwrap();
        assert_eq!(staged.len(), 32);
        for (slot, info) in staged.iter().enumerate() {
            assert_eq!(info.slot, slot);
            assert_eq!(info.status, workspace::WorkspaceStatusV1::Staging);
            assert_candidate_workspace_input_only(info);
        }
        let aborted = workspace::abort_v1(chain_id, staged[0].workspace_id, params).unwrap();
        assert_eq!(aborted.status, workspace::WorkspaceStatusV1::Aborted);
        let ready = workspace::create_v1(&plans[1], params).unwrap();
        assert_eq!(ready.status, workspace::WorkspaceStatusV1::Ready);
        assert_eq!(ready.slot, 1);
        reset_native_aoem_semantic_ingress_session_v1();
        let full = workspace::list_v1(chain_id, params).unwrap();
        assert_eq!(full.len(), 32);
        assert_eq!(full[0], aborted);
        assert_eq!(full[1], ready);
        assert!(
            workspace::create_v1(&plans[0], params).is_err(),
            "aborted slot cannot revive"
        );
        let error = workspace::create_v1(&plans[32], params)
            .expect_err("all lifecycle states consume capacity");
        assert!(error.to_string().contains("slot capacity exhausted"));
        assert_eq!(workspace::list_v1(chain_id, params).unwrap(), full);
        assert_eq!(
            workspace::load_v1(chain_id, staged[31].workspace_id, params).unwrap(),
            Some(staged[31].clone())
        );
        assert_eq!(
            candidate_workspace_authority_fingerprint(path, params, chain_id, &base.tx_hashes),
            before
        );
    });
}

const CANDIDATE_WORKSPACE_PROCESS_PARAMS_ENV: &str =
    "NOVOVM_TEST_CANDIDATE_WORKSPACE_PROCESS_PARAMS";
const CANDIDATE_WORKSPACE_PROCESS_MODE_ENV: &str = "NOVOVM_TEST_CANDIDATE_WORKSPACE_PROCESS_MODE";
const CANDIDATE_WORKSPACE_PROCESS_MANIFEST_ENV: &str =
    "NOVOVM_TEST_CANDIDATE_WORKSPACE_PROCESS_MANIFEST";

fn candidate_workspace_persistent_fingerprint(
    path: &Path,
    params: &serde_json::Value,
    chain_id: u64,
    watched_txs: &[[u8; 32]],
) -> serde_json::Value {
    let mut fingerprint =
        candidate_workspace_authority_fingerprint(path, params, chain_id, watched_txs);
    fingerprint
        .as_object_mut()
        .unwrap()
        .remove("runtime_nonce_reservations");
    for transaction in fingerprint["transaction_facts"].as_array_mut().unwrap() {
        let transaction = transaction.as_object_mut().unwrap();
        transaction.remove("pending");
        transaction.remove("pending_payload");
    }
    fingerprint
}

fn run_candidate_workspace_process(
    params: &serde_json::Value,
    mode: &str,
    manifest: &Path,
    log_path: &Path,
) -> u32 {
    // A failed assertion or poll error must not leave a worker holding RocksDB
    // locks. Output goes to a file, so no full stdout/stderr pipe can deadlock it.
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    let log = fs::File::create(log_path).expect("create isolated child process log");
    let stderr = log.try_clone().expect("clone child log handle");
    let mut child = ChildGuard(
        std::process::Command::new(
            std::env::current_exe().expect("locate this Rust test executable"),
        )
        .args([
            "--exact",
            "tx_ingress::tests::candidate_workspace_tests::candidate_workspace_process_worker",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env(
            CANDIDATE_WORKSPACE_PROCESS_PARAMS_ENV,
            serde_json::to_string(params).unwrap(),
        )
        .env(CANDIDATE_WORKSPACE_PROCESS_MODE_ENV, mode)
        .env(CANDIDATE_WORKSPACE_PROCESS_MANIFEST_ENV, manifest)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(stderr))
        .spawn()
        .expect("spawn a separate candidate workspace test process"),
    );
    let pid = child.0.id();
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child
            .0
            .try_wait()
            .expect("poll candidate workspace test process")
        {
            break status;
        }
        if started.elapsed() >= std::time::Duration::from_secs(90) {
            let _ = child.0.kill();
            let _ = child.0.wait();
            let bytes = fs::read(log_path).unwrap_or_default();
            panic!(
                "candidate workspace child {mode} timed out after 90 seconds: {}",
                String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(65_536)..])
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    if !status.success() {
        let bytes = fs::read(log_path).unwrap_or_default();
        panic!(
            "candidate workspace child {mode} failed ({status}): {}",
            String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(65_536)..])
        );
    }
    pid
}

#[test]
fn candidate_workspace_ready_and_abort_survive_three_real_process_lifetimes() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // The parent only configures paths and environment. It never opens AOEM or
    // the ledger, which would retain OS locks across the worker process boundary.
    with_plan_runtime(|path, params| {
        struct ManifestFiles(Vec<PathBuf>);
        impl Drop for ManifestFiles {
            fn drop(&mut self) {
                for path in &self.0 {
                    let _ = fs::remove_file(path);
                }
            }
        }
        let manifest = path.with_extension("candidate-workspace-process.json");
        let mut files = ManifestFiles(vec![manifest.clone()]);
        for mode in ["create", "reopen_abort", "reopen_aborted"] {
            let log = path.with_extension(format!("candidate-workspace-process-{mode}.log"));
            files.0.push(log.clone());
            let pid = run_candidate_workspace_process(params, mode, &manifest, &log);
            assert_ne!(
                pid,
                std::process::id(),
                "worker must be a real child process"
            );
            let output: serde_json::Value = serde_json::from_slice(
                &fs::read(&manifest).expect("worker must produce an evidence manifest"),
            )
            .unwrap();
            // Also fails if an accidentally renamed --exact filter runs zero tests.
            assert_eq!(output["completed_phase"], mode);
            assert_eq!(output["completed_pid"].as_u64(), Some(u64::from(pid)));
        }
    });
}

#[test]
#[ignore = "invoked only by the bounded multi-process candidate workspace parent test"]
fn candidate_workspace_process_worker() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let params: serde_json::Value = serde_json::from_str(
        &std::env::var(CANDIDATE_WORKSPACE_PROCESS_PARAMS_ENV)
            .expect("worker requires parent fixture params"),
    )
    .unwrap();
    let mode = std::env::var(CANDIDATE_WORKSPACE_PROCESS_MODE_ENV).expect("worker mode");
    let manifest_path = PathBuf::from(
        std::env::var_os(CANDIDATE_WORKSPACE_PROCESS_MANIFEST_ENV).expect("worker manifest path"),
    );
    let path = resolve_native_execution_store_path_from_params_v1(&params)
        .expect("explicit worker authority path");
    let chain_id = 98_917_205;
    let mut manifest = if mode == "create" {
        let parent = committed_block(
            &run_nov_native_candidate_execution_plan_v1(
                &genesis_plan(chain_id, vec![raw_fixture(chain_id, 501)]),
                &params,
            )
            .expect("first process executes the actual AOEM authority parent"),
        );
        let plan = successor_plan(&parent, vec![raw_fixture(chain_id, 502)]);
        let before =
            candidate_workspace_persistent_fingerprint(&path, &params, chain_id, &plan.tx_hashes);
        let ready = workspace::create_v1(&plan, &params)
            .expect("first process persists independent candidate input");
        assert_eq!(ready.status, workspace::WorkspaceStatusV1::Ready);
        assert_candidate_workspace_input_only(&ready);
        assert_eq!(
            candidate_workspace_persistent_fingerprint(&path, &params, chain_id, &plan.tx_hashes),
            before
        );
        serde_json::json!({"plan": plan, "ready": ready, "authority": before})
    } else {
        let mut manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(&manifest_path).expect("read evidence left by the previous exited process"),
        )
        .unwrap();
        let plan: NovNativeCandidateExecutionPlanV1 =
            serde_json::from_value(manifest["plan"].clone()).unwrap();
        let id: [u8; 32] =
            serde_json::from_value(manifest["ready"]["workspace_id"].clone()).unwrap();
        assert_eq!(
            candidate_workspace_persistent_fingerprint(&path, &params, chain_id, &plan.tx_hashes),
            manifest["authority"]
        );
        match mode.as_str() {
            "reopen_abort" => {
                assert_eq!(manifest["completed_phase"], "create");
                let loaded = workspace::load_v1(chain_id, id, &params)
                    .unwrap()
                    .expect("second process reopens Ready input");
                assert_eq!(serde_json::to_value(&loaded).unwrap(), manifest["ready"]);
                assert_eq!(
                    workspace::list_v1(chain_id, &params).unwrap(),
                    vec![loaded.clone()]
                );
                let replay = workspace::create_with_checkpoint_v1(&plan, &params, |_| {
                    panic!("ready replay after process exit must not reexecute or rewrite a checkpoint")
                }).unwrap();
                assert_eq!(replay, loaded);
                let aborted = workspace::abort_v1(chain_id, id, &params)
                    .expect("second process persists tombstone");
                assert_eq!(aborted.status, workspace::WorkspaceStatusV1::Aborted);
                assert_candidate_workspace_input_only(&aborted);
                manifest["aborted"] = serde_json::to_value(aborted).unwrap();
            }
            "reopen_aborted" => {
                assert_eq!(manifest["completed_phase"], "reopen_abort");
                let loaded = workspace::load_v1(chain_id, id, &params)
                    .unwrap()
                    .expect("third process reopens tombstone");
                assert_eq!(serde_json::to_value(&loaded).unwrap(), manifest["aborted"]);
                assert_eq!(loaded.status, workspace::WorkspaceStatusV1::Aborted);
                assert!(
                    workspace::create_v1(&plan, &params).is_err(),
                    "process restart must never revive aborted input"
                );
                assert_eq!(workspace::abort_v1(chain_id, id, &params).unwrap(), loaded);
                assert_eq!(workspace::list_v1(chain_id, &params).unwrap(), vec![loaded]);
            }
            _ => panic!("unknown candidate workspace worker mode: {mode}"),
        }
        assert_eq!(
            candidate_workspace_persistent_fingerprint(&path, &params, chain_id, &plan.tx_hashes),
            manifest["authority"]
        );
        manifest
    };
    manifest["completed_phase"] = serde_json::json!(mode);
    manifest["completed_pid"] = serde_json::json!(std::process::id());
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap())
        .expect("persist phase evidence for next process");
    // Returning allows the Rust test process to close all handles and terminate.
    // This proves clean process-exit recovery, not an OS power-loss crash claim.
    reset_native_aoem_semantic_ingress_session_v1();
}
