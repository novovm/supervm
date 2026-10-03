//! A candidate is captured under the namespace lock, computed without that
//! lock, and durably completed only after fresh locked input revalidation.
//! None of these objects grants signing, current-parent, or publication rights.

use super::*;

struct ExecutionBinding {
    chain_id: u64,
    scope: [u8; 32],
    namespace: String,
    protocol: [u8; 32],
    lock_path: PathBuf,
    current_dir: PathBuf,
    runtime: novovm_exec::AoemRuntimeConfig,
}

impl ExecutionBinding {
    fn capture(workspace: &WorkspaceStore) -> Result<Self> {
        Ok(Self {
            chain_id: workspace.chain_id,
            scope: workspace.scope,
            namespace: workspace.namespace.clone(),
            protocol: workspace.protocol,
            lock_path: workspace.lock_path.clone(),
            current_dir: std::env::current_dir()?,
            runtime: workspace.runtime.clone(),
        })
    }

    fn validate(&self, workspace: &WorkspaceStore) -> Result<()> {
        if self.chain_id != workspace.chain_id
            || self.scope != workspace.scope
            || self.namespace != workspace.namespace
            || self.protocol != workspace.protocol
            || self.lock_path != workspace.lock_path
            || self.current_dir != std::env::current_dir()?
            || self.runtime != workspace.runtime
        {
            bail!("candidate execution job environment or storage binding changed");
        }
        Ok(())
    }
}

struct ExecutionSnapshot {
    binding: ExecutionBinding,
    input: Descriptor,
    existing: Option<OutputDescriptor>,
    // Includes the captured paths and ownership/domain parameters. Callers
    // cannot substitute parameters when running or completing this object.
    params: serde_json::Value,
}

pub(crate) enum ExecutionStartV1 {
    Complete(Box<ExecutionInfoV1>),
    Job(Box<ExecutionJobV1>),
}

impl ExecutionStartV1 {
    pub(crate) fn workspace_id(&self) -> [u8; 32] {
        match self {
            Self::Complete(info) => info.workspace_id,
            Self::Job(job) => job.snapshot.input.id,
        }
    }
}

/// Owned immutable input, not an AOEM handle or a namespace/authority guard.
/// The worker supplies its explicit storage-owner scope on its own thread.
pub(crate) struct ExecutionJobV1 {
    snapshot: ExecutionSnapshot,
    verified: VerifiedInput,
    captured: Option<Box<input_capture::CapturedInput>>,
}

/// Unpublished computed bytes and immutable tree changes. Durable writes still
/// run on the finishing owner; a successful computation is not a finality vote.
pub(crate) struct PreparedExecutionV1 {
    snapshot: ExecutionSnapshot,
    prepared: PreparedOutput,
    preview: IsolatedBlockArtifactV1,
    captured: Option<(Box<input_capture::CapturedInput>, VerifiedInput)>,
}

impl PreparedExecutionV1 {
    pub(crate) fn workspace_id(&self) -> [u8; 32] {
        self.snapshot.input.id
    }

    pub(crate) fn chain_id(&self) -> u64 {
        self.snapshot.binding.chain_id
    }

    pub(crate) fn preview_successor_subject_v1(
        &self,
        parent: &live_parent::FinalizedParentViewV1,
        round: u64,
    ) -> Result<crate::native_block_seal::NovNativeSealSubjectV1> {
        parent.successor_seal_subject(&self.preview, round)
    }
}

impl ExecutionJobV1 {
    pub(crate) fn run(self) -> Result<PreparedExecutionV1> {
        crate::native_fresh_timing::measure("candidate.pipeline.compute", || {
            self.run_with_probe(|_, _| Ok(()))
        })
    }

    fn run_with_probe(
        self,
        probe: impl FnOnce(&mut WorkspaceStore, &Descriptor) -> Result<()>,
    ) -> Result<PreparedExecutionV1> {
        let mut workspace = WorkspaceStore::open_computation(
            self.snapshot.binding.chain_id,
            &self.snapshot.params,
        )?;
        self.snapshot.binding.validate(&workspace)?;
        probe(&mut workspace, &self.snapshot.input)?;
        // All business computation, AOEM callbacks, and three-tree merging
        // happen here without the workspace OS lock. Candidate bytes/nodes
        // remain unstaged; the existing AOEM raw precommit may record auxiliary
        // evidence. Candidate persistence/reservations/completion stay in finish.
        let prepared = prepare_output(
            &workspace,
            &self.snapshot.input,
            &self.verified,
            self.snapshot.existing.as_ref(),
            &self.snapshot.params,
        )?;
        self.snapshot.binding.validate(&workspace)?;
        check_output_capacity(0, prepared.bytes().len())?;
        let preview = prepared.preview_artifact(&self.snapshot.input, &self.verified)?;
        Ok(PreparedExecutionV1 {
            snapshot: self.snapshot,
            prepared,
            preview,
            captured: self.captured.map(|captured| (captured, self.verified)),
        })
    }
}

pub(crate) fn begin_execution_v1(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
) -> Result<ExecutionStartV1> {
    begin_with_checkpoint(chain_id, id, params, &|_| Ok(()))
}

/// New async successors remain entirely owned input until the completion's
/// current parent/round/subject fence admits them. Exact existing reservations
/// keep their original staged recovery path; no error selects this branch.
pub(crate) fn capture_execution_from_finalized_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    parent: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<ExecutionStartV1> {
    auth::require_new_successor_execution(&plan.raw_txs)?;
    let mut workspace = WorkspaceStore::open(plan.context.chain_id, params)?;
    match input_capture::capture_locked(&mut workspace, plan, parent, genesis, params)? {
        Some(input) => Ok(ExecutionStartV1::Job(Box::new(ExecutionJobV1 {
            snapshot: ExecutionSnapshot {
                binding: ExecutionBinding::capture(&workspace)?,
                input: input.descriptor,
                existing: None,
                params: params.clone(),
            },
            verified: input.verified,
            captured: Some(Box::new(input.captured)),
        }))),
        None => {
            drop(workspace);
            let input = create_from_finalized_genesis_v1(plan, parent, genesis, params)?;
            begin_execution_v1(plan.context.chain_id, input.workspace_id, params)
        }
    }
}

pub(super) fn begin_with_checkpoint(
    chain_id: u64,
    id: [u8; 32],
    params: &serde_json::Value,
    checkpoint: &impl Fn(ExecutionCheckpointV1) -> Result<()>,
) -> Result<ExecutionStartV1> {
    let mut workspace = WorkspaceStore::open(chain_id, params)?;
    let input = ready_input(&workspace, id)?;
    let verified = workspace.read_input(&input)?;
    let existing = catalog(&workspace)?
        .into_iter()
        .find(|(known, _)| *known == id)
        .map(|(_, output)| output);
    if let Some(descriptor) = &existing {
        let complete = is_complete(&workspace, &input, descriptor)?;
        if let Some(output) = read_output_view(&workspace, &input, descriptor, &verified, params)? {
            if !complete {
                publish(&mut workspace, &input, descriptor)?;
                checkpoint(ExecutionCheckpointV1::Completed)?;
            }
            return Ok(ExecutionStartV1::Complete(Box::new(info(
                &input,
                descriptor,
                output,
                verified.root_codec_profile()?,
            )?)));
        }
        if complete {
            bail!("completed candidate output has missing chunks");
        }
    }
    Ok(ExecutionStartV1::Job(Box::new(ExecutionJobV1 {
        snapshot: ExecutionSnapshot {
            binding: ExecutionBinding::capture(&workspace)?,
            input,
            existing,
            params: params.clone(),
        },
        verified,
        captured: None,
    }))) // workspace, its provider handle and its OS lock are dropped here.
}

fn revalidate_input(
    workspace: &WorkspaceStore,
    snapshot: &ExecutionSnapshot,
) -> Result<VerifiedInput> {
    snapshot.binding.validate(workspace)?;
    let current = ready_input(workspace, snapshot.input.id)?;
    if current != snapshot.input {
        bail!("candidate execution input changed while computation was in flight");
    }
    // The exact descriptor binds every input byte and the parent reference;
    // re-read it, including its historical proof/source validation. An abort,
    // retirement, missing chunk or changed bytes cannot authorize persistence.
    workspace.read_input(&current)
}

pub(crate) fn finish_execution_v1(prepared: PreparedExecutionV1) -> Result<ExecutionInfoV1> {
    finish_with_checkpoint(prepared, &|_| Ok(()))
}

pub(super) fn finish_with_checkpoint(
    computed: PreparedExecutionV1,
    checkpoint: &impl Fn(ExecutionCheckpointV1) -> Result<()>,
) -> Result<ExecutionInfoV1> {
    let _span = crate::native_fresh_timing::Span::start("candidate.pipeline.finish");
    let PreparedExecutionV1 {
        snapshot,
        prepared,
        preview: _,
        captured,
    } = computed;
    if let Some((captured, _)) = &captured {
        let checked =
            WorkspaceStore::open_computation(snapshot.binding.chain_id, &snapshot.params)?;
        snapshot.binding.validate(&checked)?;
        drop(checked);
        // Maintenance happens only for an accepted completion, before acquiring
        // the finishing namespace lock. Recheck the live parent under that lock.
        retire_old_workspaces_v1(
            snapshot.binding.chain_id,
            captured.parent,
            captured.genesis,
            &snapshot.params,
        )?;
    }
    let mut workspace = WorkspaceStore::open(snapshot.binding.chain_id, &snapshot.params)?;
    if let Some((captured, verified)) = &captured {
        snapshot.binding.validate(&workspace)?;
        crate::native_fresh_timing::measure("candidate.finish.stage_input", || {
            input_capture::validate_and_stage(
                &mut workspace,
                &snapshot.input,
                captured,
                verified,
                &snapshot.params,
            )
        })?;
    }
    let verified = revalidate_input(&workspace, &snapshot)?;
    let input = &snapshot.input;
    let descriptor = prepared.descriptor(input);
    let bytes = prepared.bytes();
    check_output_capacity(0, bytes.len())?;
    let outputs = catalog(&workspace)?;
    let existing = outputs
        .iter()
        .find(|(known, _)| *known == input.id)
        .map(|(_, output)| output);
    if snapshot
        .existing
        .as_ref()
        .is_some_and(|old| existing != Some(old))
    {
        bail!("candidate output reservation changed while computation was in flight");
    }
    if let Some(previous) = existing {
        if previous != &descriptor {
            bail!("incomplete candidate output recomputation differs from reserved bytes");
        }
        let complete = is_complete(&workspace, input, previous)?;
        if let Some(output) =
            read_output_view(&workspace, input, previous, &verified, &snapshot.params)?
        {
            // A concurrent exact completion is idempotent, not permission to
            // recompute or overwrite a different output reservation.
            if !complete {
                publish(&mut workspace, input, previous)?;
                checkpoint(ExecutionCheckpointV1::Completed)?;
            }
            return info(input, previous, output, verified.root_codec_profile()?);
        }
        if complete {
            bail!("completed candidate output has missing chunks");
        }
    } else {
        let total = outputs
            .iter()
            .try_fold(0usize, |total, (_, output)| total.checked_add(output.len))
            .context("candidate output capacity overflow")?;
        check_output_capacity(total, bytes.len())?;
        let reservation = AoemAtomicGraphWriteV1::Put {
            key: workspace.key(b'v', &input.id),
            value: descriptor.encode(),
        };
        workspace.commit(b'V', input, vec![reservation.clone()], reservation)?;
    }
    checkpoint(ExecutionCheckpointV1::OutputReserved)?;
    crate::native_fresh_timing::measure("candidate.finish.persist_output", || {
        prepared.persist(&workspace)
    })?;
    let writes = bytes
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(index, chunk)| AoemAtomicGraphWriteV1::Put {
            key: output_chunk_key(&workspace, &input.id, index),
            value: chunk.to_vec(),
        })
        .collect::<Vec<_>>();
    let reservation = AoemAtomicGraphWriteV1::Put {
        key: workspace.key(b'v', &input.id),
        value: descriptor.encode(),
    };
    workspace.commit(b'P', input, writes[..1].to_vec(), reservation.clone())?;
    checkpoint(ExecutionCheckpointV1::PartialOutput)?;
    workspace.commit(b'O', input, writes, reservation)?;
    checkpoint(ExecutionCheckpointV1::OutputWritten)?;
    let readback = crate::native_fresh_timing::measure("candidate.finish.readback", || {
        read_output_view(&workspace, input, &descriptor, &verified, &snapshot.params)
    })?
    .context("candidate output readback incomplete")?;
    publish(&mut workspace, input, &descriptor)?;
    checkpoint(ExecutionCheckpointV1::Completed)?;
    info(input, &descriptor, readback, verified.root_codec_profile()?)
}

#[cfg(test)]
#[path = "native_candidate_execution_pipeline_tests.rs"]
mod tests;
#[cfg(test)]
pub(crate) use tests::exercise_execution_pipeline_for_test_v1;
