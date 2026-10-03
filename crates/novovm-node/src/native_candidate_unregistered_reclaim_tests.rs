//! Real AOEM completion/reclamation against the existing signed Transfer fixture.
//! Checkpoint errors model response loss and process-local reopen, not a killed
//! process. No synthetic output, alternate executor or throughput claim.

use super::*;

type PrivateRecords = Vec<(Vec<u8>, Option<Vec<u8>>)>;

fn complete_fresh_candidate(
    plan: &NovNativeCandidateExecutionPlanV1,
    current: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<IsolatedBlockArtifactV1> {
    let captured = capture_execution_from_finalized_v1(plan, current, genesis, params)?;
    let id = captured.workspace_id();
    let ExecutionStartV1::Job(job) = captured else {
        bail!("reclaim fixture expected real recomputation, not cached completion");
    };
    let completed = finish_execution_v1(job.run()?)?;
    if !completed.execution_completed || completed.workspace_id != id {
        bail!("reclaim fixture did not complete real candidate execution");
    }
    load_block_artifact_v1(plan.context.chain_id, id, params)?
        .context("reclaim fixture durable artifact missing")
}

fn candidate_private_records(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<PrivateRecords> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let mut keys = [b'q', b'g', b'a', b'r', b'v', b'e', b'h']
        .map(|kind| workspace.key(kind, &id))
        .to_vec();
    if let Some((_, input)) = workspace
        .catalog()?
        .iter()
        .find(|(_, input)| input.id == id)
    {
        for index in 0..input.len.div_ceil(CHUNK_BYTES) {
            keys.push(workspace.chunk_key(&id, index));
        }
        if let Some(raw) = workspace.graph.get(&workspace.key(b'v', &id))? {
            let output = OutputDescriptor::decode(&raw, input)?;
            for index in 0..output.len.div_ceil(CHUNK_BYTES) {
                keys.push(output_chunk_key(&workspace, &id, index));
            }
        }
    }
    keys.push(native_aoem_owned_state_head_key_v1(
        chain,
        &workspace.namespace,
    ));
    for slot in 0..MAX_WORKSPACES_V1 {
        keys.push(workspace.slot_key(slot));
    }
    keys.into_iter()
        .map(|key| workspace.graph.get(&key).map(|value| (key, value)))
        .collect()
}

fn write_reclaim_fixture_fault(
    chain: u64,
    params: &serde_json::Value,
    key: &[u8],
    bytes: Vec<u8>,
) -> Result<()> {
    let workspace = WorkspaceStore::open(chain, params)?;
    let digest = sha256_bytes_v1(&[
        b"candidate-unregistered-reclaim-fixture-fault-v1\0",
        key,
        &bytes,
    ]);
    let write = AoemAtomicGraphWriteV1::Put {
        key: key.to_vec(),
        value: bytes,
    };
    workspace.graph.commit(AoemAtomicGraphRequestV1 {
        graph_id: u64::from_be_bytes(digest[..8].try_into()?).max(1),
        steps: vec![AoemAtomicGraphStepV1 {
            task_kind: 0,
            task_payload: vec![],
            writes: vec![write.clone()],
            event: None,
        }],
        completion_write: write,
    })?;
    Ok(())
}

pub(crate) fn exercise_unregistered_reclaim_for_test_v1(
    chain: u64,
    current: [u8; 32],
    genesis: [u8; 32],
    seed_plan: &NovNativeCandidateExecutionPlanV1,
    params: &serde_json::Value,
) -> Result<()> {
    let parent = load_finalized_parent_view_v1(chain, current, genesis, params)?;
    let parent_block = parent.block().clone();
    let parent_output = parent.output_digest();
    let parent_artifact = load_block_artifact_v1(chain, current, params)?
        .context("reclaim fixture finalized parent artifact missing")?;
    let baseline = list_v1(chain, params)?;
    let raw = seed_plan
        .raw_txs
        .first()
        .context("reclaim fixture needs a Transfer")?
        .clone();
    let make_plan = |offset: u64| {
        let mut context = seed_plan.context;
        context.timestamp_unix_ms = context
            .timestamp_unix_ms
            .checked_add(100 + offset)
            .context("reclaim fixture timestamp overflow")?;
        parent.successor_plan(context, vec![raw.clone()], params)
    };
    let assert_unchanged = || -> Result<()> {
        assert_eq!(list_v1(chain, params)?, baseline);
        let reloaded = load_finalized_parent_view_v1(chain, current, genesis, params)?;
        assert_eq!(reloaded.block(), &parent_block);
        assert_eq!(reloaded.output_digest(), parent_output);
        assert_eq!(
            load_block_artifact_v1(chain, current, params)?.as_ref(),
            Some(&parent_artifact)
        );
        Ok(())
    };
    let assert_reclaimed = |id| -> Result<()> {
        assert!(load_v1(chain, id, params)?.is_none());
        assert!(load_execution_v1(chain, id, params)?.is_none());
        assert!(load_block_artifact_v1(chain, id, params)?.is_none());
        let workspace = WorkspaceStore::open(chain, params)?;
        for kind in [b'q', b'g', b'a', b'r', b'v', b'e'] {
            assert!(workspace.graph.get(&workspace.key(kind, &id))?.is_none());
        }
        assert!(workspace.graph.get(&workspace.chunk_key(&id, 0))?.is_none());
        let mut suffix = id.to_vec();
        suffix.extend_from_slice(&0u32.to_be_bytes());
        assert!(workspace
            .graph
            .get(&workspace.key(b'o', &suffix))?
            .is_none());
        drop(workspace);
        assert_unchanged()
    };

    // The actual published parent, not just a fake authority marker, is pinned.
    let published_before = candidate_private_records(chain, current, params)?;
    let protected =
        reclaim_unregistered_workspaces_v1(chain, current, genesis, &[current], params)?;
    assert!(protected.is_empty());
    assert_eq!(
        candidate_private_records(chain, current, params)?,
        published_before
    );
    assert_unchanged()?;

    let mut ids = HashSet::new();
    for offset in 0..MAX_WORKSPACES_V1 + 2 {
        let plan = make_plan(offset as u64)?;
        let artifact = complete_fresh_candidate(&plan, current, genesis, params)?;
        let id = artifact.workspace_id;
        assert!(ids.insert(id), "cycle must persist a distinct candidate");
        assert_eq!(list_v1(chain, params)?.len(), baseline.len() + 1);
        assert!(load_v1(chain, id, params)?.is_some());
        let unregistered =
            with_verified_finalized_parent_round_v1(chain, current, genesis, params, |view| {
                view.load_candidate_record(chain, artifact.block().header.block_hash)
            })?;
        assert!(unregistered.is_none());
        if offset == 0 {
            // Wrong authority must reject before creating a deletion journal.
            let before = candidate_private_records(chain, id, params)?;
            let mut wrong_parent = current;
            wrong_parent[0] ^= 1;
            assert!(reclaim_unregistered_workspaces_v1(
                chain,
                wrong_parent,
                genesis,
                &[id],
                params,
            )
            .is_err());
            let mut wrong_genesis = genesis;
            wrong_genesis[0] ^= 1;
            assert!(reclaim_unregistered_workspaces_v1(
                chain,
                current,
                wrong_genesis,
                &[id],
                params,
            )
            .is_err());
            assert_eq!(candidate_private_records(chain, id, params)?, before);
        }
        assert_eq!(
            reclaim_unregistered_workspaces_v1(chain, current, genesis, &[id], params)?,
            vec![id],
        );
        assert_reclaimed(id)?;
    }
    assert_eq!(ids.len(), MAX_WORKSPACES_V1 + 2);

    // Identical plans reuse graph IDs. Capture-only replay cannot prove that
    // graph idempotence permits a second real write after the first was pruned.
    let replay_plan = make_plan(70)?;
    let mut previous = None;
    for _ in 0..2 {
        let artifact = complete_fresh_candidate(&replay_plan, current, genesis, params)?;
        if let Some(ref earlier) = previous {
            assert_eq!(&artifact, earlier);
        }
        let id = artifact.workspace_id;
        assert_eq!(
            reclaim_unregistered_workspaces_v1(chain, current, genesis, &[id], params)?,
            vec![id],
        );
        assert_reclaimed(id)?;
        previous = Some(artifact);
    }

    for (offset, stop) in [
        UnregisteredReclaimCheckpointV1::IntentPersisted,
        UnregisteredReclaimCheckpointV1::PartialReclaim,
        UnregisteredReclaimCheckpointV1::SlotReleased,
    ]
    .into_iter()
    .enumerate()
    {
        let artifact =
            complete_fresh_candidate(&make_plan(80 + offset as u64)?, current, genesis, params)?;
        let id = artifact.workspace_id;
        let error = reclaim_unregistered_with_checkpoint_v1(
            chain,
            current,
            genesis,
            &[id],
            params,
            |point| {
                if point == stop {
                    bail!("unregistered reclaim checkpoint response loss");
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("unregistered reclaim checkpoint response loss"));
        // The call returned and dropped its graph/namespace lock. Inspect a
        // newly opened handle; do not re-enter an API from its lock callback.
        {
            let reopened = WorkspaceStore::open(chain, params)?;
            let has_slot = reopened.catalog()?.iter().any(|(_, input)| input.id == id);
            let has_journal = reopened.graph.get(&reopened.key(b'q', &id))?.is_some();
            let completed = stop == UnregisteredReclaimCheckpointV1::SlotReleased;
            assert_eq!(has_slot, !completed);
            assert_eq!(has_journal, !completed);
            if stop == UnregisteredReclaimCheckpointV1::PartialReclaim {
                assert!(reopened.graph.get(&reopened.chunk_key(&id, 0))?.is_none());
            }
        }
        let resumed = reclaim_unregistered_workspaces_v1(chain, current, genesis, &[id], params)?;
        if stop == UnregisteredReclaimCheckpointV1::SlotReleased {
            assert!(resumed.is_empty());
        } else {
            assert_eq!(resumed, vec![id]);
        }
        assert_reclaimed(id)?;
    }

    let artifact = complete_fresh_candidate(&make_plan(90)?, current, genesis, params)?;
    let id = artifact.workspace_id;
    let error =
        reclaim_unregistered_with_checkpoint_v1(chain, current, genesis, &[id], params, |point| {
            if point == UnregisteredReclaimCheckpointV1::IntentPersisted {
                bail!("reclaim journal corruption setup");
            }
            Ok(())
        })
        .unwrap_err();
    assert!(format!("{error:#}").contains("reclaim journal corruption setup"));
    let (journal_key, original) = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let key = workspace.key(b'q', &id);
        let original = workspace
            .graph
            .get(&key)?
            .context("fixture reclaim journal missing")?;
        (key, original)
    };
    let mut damaged = original.clone();
    *damaged
        .first_mut()
        .context("fixture reclaim journal empty")? ^= 1;
    write_reclaim_fixture_fault(chain, params, &journal_key, damaged)?;
    let before = candidate_private_records(chain, id, params)?;
    let rejected = reclaim_unregistered_workspaces_v1(chain, current, genesis, &[id], params);
    let after = candidate_private_records(chain, id, params);
    // Restoration is only a fixture cleanup, never a production fallback.
    write_reclaim_fixture_fault(chain, params, &journal_key, original)?;
    assert!(
        rejected.is_err(),
        "corrupt journal must not authorize deletion"
    );
    assert_eq!(
        after?, before,
        "corrupt journal rejection changed private data"
    );
    assert_eq!(
        reclaim_unregistered_workspaces_v1(chain, current, genesis, &[id], params)?,
        vec![id],
    );
    assert_reclaimed(id)?;
    eprintln!("unregistered reclaim: 34 distinct real AOEM completions reclaimed without slot growth; same plan recomputed and persisted twice; 3 checkpoint response-loss reopen cases; corrupt journal and wrong parent/genesis rejected without deletion; published parent unchanged");
    Ok(())
}
