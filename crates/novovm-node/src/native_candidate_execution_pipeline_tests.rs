//! Uses the existing real NCW2 candidate fixture. No sleeps, synthetic worker
//! overlap, alternative business executor, or extra AOEM computation.

use super::*;

#[test]
fn execution_pipeline_messages_are_send_without_unsafe() {
    fn assert_send<T: Send>() {}
    assert_send::<ExecutionJobV1>();
    assert_send::<PreparedExecutionV1>();
}

fn write_fault(workspace: &WorkspaceStore, key: &[u8], value: Option<Vec<u8>>) -> Result<()> {
    let digest = sha256_bytes_v1(&[
        b"candidate-pipeline-test-fault-v1\0",
        key,
        &serde_json::to_vec(&value)?,
    ]);
    let write = match value {
        Some(value) => AoemAtomicGraphWriteV1::Put {
            key: key.to_vec(),
            value,
        },
        None => AoemAtomicGraphWriteV1::Delete { key: key.to_vec() },
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

fn assert_fault_rejected(
    prepared: &PreparedExecutionV1,
    key: &[u8],
    fault: Option<Vec<u8>>,
    expected_error: &str,
) -> Result<()> {
    let snapshot = &prepared.snapshot;
    let workspace = WorkspaceStore::open(snapshot.binding.chain_id, &snapshot.params)?;
    let saved = workspace.graph.get(key)?;
    let output_keys = [
        workspace.key(b'v', &snapshot.input.id),
        workspace.key(b'e', &snapshot.input.id),
        output_chunk_key(&workspace, &snapshot.input.id, 0),
    ];
    let before = output_keys
        .iter()
        .map(|key| workspace.graph.get(key))
        .collect::<Result<Vec<_>>>()?;
    write_fault(&workspace, key, fault.clone())?;
    // Exercise the exact guard that finish calls before it reads/reserves any
    // output or persists staged nodes. Restoration is fixture-only, never a
    // production recovery fallback on invalid input.
    let rejected = revalidate_input(&workspace, snapshot).err();
    let after = output_keys
        .iter()
        .map(|key| workspace.graph.get(key))
        .collect::<Result<Vec<_>>>();
    let unchanged_fault = workspace.graph.get(key);
    write_fault(&workspace, key, saved.clone())?;
    if workspace.graph.get(key)? != saved {
        bail!("pipeline fixture did not restore its fault target");
    }
    let error = rejected.context("pipeline finish guard accepted a changed/aborted input")?;
    if !format!("{error:#}").contains(expected_error)
        || after? != before
        || unchanged_fault? != fault
    {
        bail!("pipeline finish guard failed for wrong reason or changed output: {error:#}");
    }
    Ok(())
}

pub(crate) fn exercise_execution_pipeline_for_test_v1(
    chain: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<ExecutionInfoV1> {
    let ExecutionStartV1::Job(job) = begin_execution_v1(chain, id, params)? else {
        bail!("pipeline fixture requires a first, uncomputed candidate");
    };
    if job.snapshot.existing.is_some() {
        bail!("pipeline fixture requires no reserved output");
    }
    let contender = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&job.snapshot.binding.lock_path)?;
    if let Err(error) = contender.try_lock() {
        bail!("begin retained the namespace lock: {error:?}");
    }
    // Deliberately keep this independently acquired lock through run(). If
    // computation tries to reacquire the namespace lock, this fixture fails;
    // no timeout or sleep is used to manufacture apparent concurrency.
    let prepared = job.run_with_probe(|workspace, input| {
        if workspace.lock.is_some() {
            bail!("computation view owns a namespace lock");
        }
        let key = workspace.key(b'v', &input.id);
        let before = workspace.graph.get(&key)?;
        let forbidden = AoemAtomicGraphWriteV1::Put {
            key: key.clone(),
            value: vec![1],
        };
        let error = workspace
            .commit(b'V', input, vec![forbidden.clone()], forbidden)
            .err()
            .context("computation view accepted a workspace metadata commit")?;
        if !format!("{error:#}").contains("without its lock")
            || workspace.graph.get(&key)? != before
        {
            bail!("computation metadata write guard failed: {error:#}");
        }
        Ok(())
    })?;
    drop(contender);
    if prepared.chain_id() != chain || prepared.workspace_id() != id {
        bail!("pipeline completion identity changed");
    }
    let preview = prepared.preview.clone();
    let (chunk_key, corrupt_chunk, abort_key, abort_marker, retire_key) = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let (slot, input) = workspace
            .catalog()?
            .into_iter()
            .find(|(_, input)| input.id == id)
            .context("pipeline fixture input missing")?;
        let chunk_key = workspace.chunk_key(&id, 0);
        let mut corrupt_chunk = workspace
            .graph
            .get(&chunk_key)?
            .context("pipeline fixture first chunk missing")?;
        *corrupt_chunk
            .first_mut()
            .context("pipeline fixture first chunk empty")? ^= 1;
        (
            chunk_key,
            corrupt_chunk,
            workspace.key(b'a', &id),
            workspace.marker(b'a', slot, &input),
            workspace.key(b'g', &id),
        )
    };
    assert_fault_rejected(
        &prepared,
        &chunk_key,
        Some(corrupt_chunk),
        "payload digest mismatch",
    )?;
    assert_fault_rejected(
        &prepared,
        &abort_key,
        Some(abort_marker),
        "ready, non-aborted",
    )?;
    assert_fault_rejected(&prepared, &retire_key, Some(vec![1]), "ready, non-aborted")?;
    let mut checkpoints = Vec::new();
    // RefCell only observes the existing Fn callback on this owner thread.
    let actual = {
        let checkpoints_ref = std::cell::RefCell::new(&mut checkpoints);
        finish_with_checkpoint(prepared, &|point| {
            checkpoints_ref.borrow_mut().push(point);
            Ok(())
        })?
    };
    if checkpoints
        != [
            ExecutionCheckpointV1::OutputReserved,
            ExecutionCheckpointV1::PartialOutput,
            ExecutionCheckpointV1::OutputWritten,
            ExecutionCheckpointV1::Completed,
        ]
    {
        bail!("pipeline changed the first execution checkpoint order");
    }
    let ExecutionStartV1::Complete(recovered) = begin_execution_v1(chain, id, params)? else {
        bail!("completed pipeline candidate was scheduled for re-execution");
    };
    if *recovered != actual {
        bail!("pipeline completed recovery changed execution information");
    }
    if load_block_artifact_v1(chain, id, params)?.as_ref() != Some(&preview) {
        bail!("computed preview differs from the independently read durable artifact");
    }
    eprintln!("candidate pipeline: begin lock released; computation metadata writes denied; changed/aborted/retiring input rejected; exact completion recovered");
    Ok(actual)
}
