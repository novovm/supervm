//! Reclaim derived snapshots only after immutable ledger finality supersedes them.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceRetirementV1 {
    pub finalized_height: u64,
    pub retired_workspaces: Vec<[u8; 32]>,
    pub snapshot_bytes_unreferenced: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetirementCheckpointV1 {
    IntentPersisted,
    PartialReclaim,
    SlotReleased,
}

struct Retirement {
    slot: usize,
    genesis: [u8; 32],
    height: u64,
    block_hash: [u8; 32],
    canonical_commitment: [u8; 32],
    input: Descriptor,
    output: OutputDescriptor,
}

impl Retirement {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"NCR1".to_vec();
        bytes.extend_from_slice(&(self.slot as u32).to_be_bytes());
        bytes.extend_from_slice(&self.genesis);
        bytes.extend_from_slice(&self.height.to_be_bytes());
        bytes.extend_from_slice(&self.block_hash);
        bytes.extend_from_slice(&self.canonical_commitment);
        bytes.extend_from_slice(&self.input.encode());
        bytes.extend_from_slice(&self.output.encode());
        bytes
    }

    fn decode(raw: &[u8], scope: &[u8; 32]) -> Result<Self> {
        const INPUT: usize = 4 + 4 + 32 + 8 + 32 + 32;
        if raw.len() != INPUT + DESCRIPTOR_BYTES + 76 || &raw[..4] != b"NCR1" {
            bail!("invalid retirement record codec");
        }
        let input = Descriptor::decode(&raw[INPUT..INPUT + DESCRIPTOR_BYTES], scope)?;
        let output = OutputDescriptor::decode(&raw[INPUT + DESCRIPTOR_BYTES..], &input)?;
        let result = Self {
            slot: u32::from_be_bytes(raw[4..8].try_into()?) as usize,
            genesis: raw[8..40].try_into()?,
            height: u64::from_be_bytes(raw[40..48].try_into()?),
            block_hash: raw[48..80].try_into()?,
            canonical_commitment: raw[80..112].try_into()?,
            input,
            output,
        };
        if result.slot >= MAX_WORKSPACES_V1 || result.height == 0 {
            bail!("invalid retirement bounds");
        }
        Ok(result)
    }

    fn binding(&self) -> crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
        crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
            workspace_id: self.input.id,
            plan_commitment: self.input.plan,
            output_digest: self.output.digest,
        }
    }
}

pub fn retire_old_workspaces_v1(
    chain: u64,
    current: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<WorkspaceRetirementV1> {
    retire_with_checkpoint_v1(chain, current, genesis, params, |_| Ok(()))
}

pub(crate) fn retire_with_checkpoint_v1(
    chain: u64,
    current: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    checkpoint: impl Fn(RetirementCheckpointV1) -> Result<()>,
) -> Result<WorkspaceRetirementV1> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    // Capture verifies current execution and all predecessor proofs. Reacquire
    // the authority lock and compare the exact target before any deletion.
    let image = capture_finalized_parent_locked(&mut workspace, current, genesis, params)?;
    let height = image.block().header.height;
    let mut report = WorkspaceRetirementV1 {
        finalized_height: height,
        retired_workspaces: Vec::new(),
        snapshot_bytes_unreferenced: 0,
    };
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("retirement requires explicit native path")?;
    let _authority = acquire_nov_native_execution_store_write_lock_v1(&native_path)?;
    let ledger = nov_native_block_ledger_rocksdb_path_v1(&native_path);
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "retirement namespace")?;
    let (binding, commitment, block) = NovNativeBlockLedgerV1::load_fresh_finalized_execution_v1(
        &ledger, genesis, namespace, height,
    )?;
    let artifact = block_artifact::load_block_artifact_inner_v1(&workspace, current, params)?
        .context("retirement current output missing")?;
    if binding.workspace_id != current
        || binding.plan_commitment != artifact.plan_commitment
        || binding.output_digest != image.output_digest()
        || &block != image.block()
        || artifact.block() != &block
    {
        bail!("retirement authority changed after verification");
    }
    let protected_parent = if height > 1 {
        Some(
            NovNativeBlockLedgerV1::load_fresh_finalized_execution_v1(
                &ledger,
                genesis,
                namespace,
                height - 1,
            )?
            .0
            .workspace_id,
        )
    } else {
        None
    };
    let target = publication_target(
        if height == 1 { b"NVP1" } else { b"NVP2" },
        namespace,
        genesis,
        commitment,
        current,
        &artifact,
    );
    let head_key = native_aoem_owned_state_head_key_v1(chain, &workspace.namespace);
    if workspace.graph.get(&head_key)?.as_deref() != Some(target.as_slice()) {
        bail!("retirement requires the exact current authority");
    }
    for (slot, input) in workspace.catalog()? {
        if input.id == current || Some(input.id) == protected_parent {
            continue;
        }
        let key = workspace.key(b'g', &input.id);
        let existing = workspace.graph.get(&key)?;
        let retirement = if let Some(raw) = &existing {
            Retirement::decode(raw, &workspace.scope)?
        } else {
            if !matches!(
                workspace.status(slot, &input)?,
                WorkspaceStatusV1::Ready | WorkspaceStatusV1::Aborted
            ) {
                continue;
            }
            let payload = workspace.read_payload(&input)?;
            let candidate_height = payload.plan.context.block_height;
            if candidate_height == 0 || candidate_height > height {
                continue;
            }
            let Some(raw_output) = workspace.graph.get(&workspace.key(b'v', &input.id))? else {
                continue;
            };
            let output = OutputDescriptor::decode(&raw_output, &input)?;
            if !is_complete(&workspace, &input, &output)? {
                continue;
            }
            read_output(&workspace, &input, &output, &payload, params)?
                .context("retirement output missing")?;
            let execution = crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
                workspace_id: input.id,
                plan_commitment: input.plan,
                output_digest: output.digest,
            };
            let Some((block_hash, canonical_commitment)) =
                NovNativeBlockLedgerV1::verify_retirable_fresh_candidate_v1(
                    &ledger,
                    genesis,
                    namespace,
                    candidate_height,
                    &execution,
                )?
            else {
                continue;
            };
            Retirement {
                slot,
                genesis,
                height: candidate_height,
                block_hash,
                canonical_commitment,
                input: input.clone(),
                output,
            }
        };
        if retirement.slot != slot
            || retirement.input != input
            || retirement.genesis != genesis
            || retirement.height > height
            || input.id == current
            || Some(input.id) == protected_parent
        {
            bail!("retirement record is outside the verified obsolete workspace");
        }
        let canonical = NovNativeBlockLedgerV1::verify_retirable_fresh_candidate_v1(
            &ledger,
            genesis,
            namespace,
            retirement.height,
            &retirement.binding(),
        )?
        .context("retirement candidate binding disappeared")?;
        if canonical != (retirement.block_hash, retirement.canonical_commitment) {
            bail!("retirement finality changed");
        }
        let marker = AoemAtomicGraphWriteV1::Put {
            key: key.clone(),
            value: retirement.encode(),
        };
        if existing.is_none() {
            workspace.commit(b'G', &input, vec![marker.clone()], marker.clone())?;
        }
        checkpoint(RetirementCheckpointV1::IntentPersisted)?;
        // The journal survives partial deletion. Keep the catalog slot occupied
        // until every input/output chunk and its obsolete markers are removed.
        workspace.commit(
            b'D',
            &input,
            vec![AoemAtomicGraphWriteV1::Delete {
                key: workspace.chunk_key(&input.id, 0),
            }],
            marker,
        )?;
        checkpoint(RetirementCheckpointV1::PartialReclaim)?;
        let mut writes = Vec::new();
        for index in 0..input.len.div_ceil(CHUNK_BYTES) {
            writes.push(AoemAtomicGraphWriteV1::Delete {
                key: workspace.chunk_key(&input.id, index),
            });
        }
        for index in 0..retirement.output.len.div_ceil(CHUNK_BYTES) {
            writes.push(AoemAtomicGraphWriteV1::Delete {
                key: output_chunk_key(&workspace, &input.id, index),
            });
        }
        for kind in [b'v', b'e', b'r', b'a'] {
            writes.push(AoemAtomicGraphWriteV1::Delete {
                key: workspace.key(kind, &input.id),
            });
        }
        workspace.commit(
            b'R',
            &input,
            writes.clone(),
            AoemAtomicGraphWriteV1::Delete {
                key: workspace.slot_key(slot),
            },
        )?;
        checkpoint(RetirementCheckpointV1::SlotReleased)?;
        for write in writes {
            let AoemAtomicGraphWriteV1::Delete { key } = write else {
                unreachable!()
            };
            if workspace.graph.get(&key)?.is_some() {
                bail!("retired snapshot deletion readback mismatch");
            }
        }
        if workspace.graph.get(&workspace.slot_key(slot))?.is_some()
            || workspace.graph.get(&key)?.as_deref() != Some(retirement.encode().as_slice())
            || workspace.graph.get(&head_key)?.as_deref() != Some(target.as_slice())
        {
            bail!("retirement completion or authority readback mismatch");
        }
        report.retired_workspaces.push(input.id);
        report.snapshot_bytes_unreferenced += input.len + retirement.output.len;
    }
    Ok(report)
}
