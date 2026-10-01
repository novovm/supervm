// Included with the existing isolated AOEM candidate fixtures. These seed the
// pre-record-document physical format, never a production migration switch.
#[test]
fn candidate_workspace_legacy_inline_input_ready_and_partial_replay() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_920_701;
    with_plan_runtime(|path, params| {
        let parent = committed_block(
            &run_nov_native_candidate_execution_plan_v1(
                &genesis_plan(chain, vec![raw_fixture(chain, 1901)]),
                params,
            )
            .unwrap(),
        );
        for (index, stage) in [
            workspace::CheckpointV1::Reserved,
            workspace::CheckpointV1::PartialPayload,
            workspace::CheckpointV1::PayloadWritten,
            workspace::CheckpointV1::Ready,
        ]
        .into_iter()
        .enumerate()
        {
            let plan = successor_plan(&parent, vec![raw_fixture(chain, 1910 + index as u64)]);
            let authority =
                candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
            let original =
                workspace::seed_legacy_inline_input_for_test_v1(&plan, params, stage, false)
                    .unwrap();
            let replay = workspace::create_v1(&plan, params).unwrap();
            assert_eq!(replay.workspace_id, original.workspace_id);
            assert_eq!(replay.status, workspace::WorkspaceStatusV1::Ready);
            assert_eq!(
                workspace::create_v1(&plan, params).unwrap().workspace_id,
                original.workspace_id
            );
            assert_eq!(
                candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes),
                authority
            );
        }
        let bad = successor_plan(&parent, vec![raw_fixture(chain, 1919)]);
        workspace::seed_legacy_inline_input_for_test_v1(
            &bad,
            params,
            workspace::CheckpointV1::Reserved,
            true,
        )
        .unwrap();
        assert!(workspace::create_v1(&bad, params)
            .unwrap_err()
            .to_string()
            .contains("captured parent"));
    });
}

#[test]
fn candidate_workspace_legacy_inline_output_partial_and_complete_recovery() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_920_702;
    with_plan_runtime(|path, params| {
        let parent = committed_block(
            &run_nov_native_candidate_execution_plan_v1(
                &genesis_plan(chain, vec![raw_fixture(chain, 1921)]),
                params,
            )
            .unwrap(),
        );
        for (index, stage) in [
            workspace::ExecutionCheckpointV1::OutputReserved,
            workspace::ExecutionCheckpointV1::PartialOutput,
            workspace::ExecutionCheckpointV1::OutputWritten,
            workspace::ExecutionCheckpointV1::Completed,
        ]
        .into_iter()
        .enumerate()
        {
            let plan = successor_plan(&parent, vec![raw_fixture(chain, 1930 + index as u64)]);
            let ready = workspace::create_v1(&plan, params).unwrap();
            let authority =
                candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
            let digest = workspace::seed_legacy_inline_output_for_test_v1(
                chain,
                ready.workspace_id,
                params,
                stage,
                false,
            )
            .unwrap();
            let fully_written = matches!(
                stage,
                workspace::ExecutionCheckpointV1::OutputWritten
                    | workspace::ExecutionCheckpointV1::Completed
            );
            let recovered = workspace::execute_with_checkpoint_v1(
                chain,
                ready.workspace_id,
                params,
                |checkpoint| {
                    if fully_written {
                        assert_eq!(
                            checkpoint,
                            workspace::ExecutionCheckpointV1::Completed,
                            "fully written legacy output must not re-execute"
                        );
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert_candidate_workspace_execution_complete(&recovered);
            assert_eq!(
                recovered.output_digest, digest,
                "old physical output commitment must remain exact"
            );
            assert_eq!(
                workspace::execute_v1(chain, ready.workspace_id, params)
                    .unwrap()
                    .output_digest,
                digest
            );
            assert_eq!(
                candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes),
                authority
            );
        }
        let bad = successor_plan(&parent, vec![raw_fixture(chain, 1939)]);
        let ready = workspace::create_v1(&bad, params).unwrap();
        workspace::seed_legacy_inline_output_for_test_v1(
            chain,
            ready.workspace_id,
            params,
            workspace::ExecutionCheckpointV1::OutputReserved,
            true,
        )
        .unwrap();
        assert!(workspace::execute_v1(chain, ready.workspace_id, params)
            .unwrap_err()
            .to_string()
            .contains("reserved bytes"));
    });
}
