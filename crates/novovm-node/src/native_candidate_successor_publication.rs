//! Publish existing immutable AOEM output, never rerun NOV business execution.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Verify,
    Authority,
    Ledger,
    Finality,
}

/// Resume only the same local archive and explicitly configured ledger.
pub fn resume_successor_promotion_v1(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    proof: &NovNativeFreshFinalityProofV1,
    ledger_path: &Path,
    params: &serde_json::Value,
) -> Result<FreshSuccessorPublicationV1> {
    let existing = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let path = resolve_native_execution_store_path_from_params_v1(params)
            .context("successor recovery requires explicit native path")?;
        if fs::canonicalize(nov_native_block_ledger_rocksdb_path_v1(&path))?
            != fs::canonicalize(ledger_path)?
        {
            bail!("successor recovery resolves to a different service ledger");
        }
        NovNativeBlockLedgerV1::verify_optional_successor_archive_v1(
            ledger_path,
            genesis,
            parse_fixed_hex_32_v1(&workspace.namespace, "successor namespace")?,
            parent,
            id,
            proof,
        )?
    };
    if !existing {
        prepare_successor_promotion_v1(chain, parent, id, genesis, proof, params)?;
    }
    complete_successor_ledger_v1(chain, parent, id, genesis, params)?;
    finalize_successor_v1(chain, parent, id, genesis, params)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FreshSuccessorPublicationV1 {
    pub chain_id: u64,
    pub block_hash: [u8; 32],
    pub workspace_id: [u8; 32],
    pub intent_commitment: [u8; 32],
    pub state_root: [u8; 32],
    pub receipt_root: [u8; 32],
    pub state_version: u64,
    pub aoem_authority_published: bool,
    pub aoem_readback_verified: bool,
    pub ledger_publication_completed: bool,
    pub finalized: bool,
}

pub fn publish_successor_authority_v1(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FreshSuccessorPublicationV1> {
    run(chain, parent, id, genesis, params, Scope::Authority, |_| {
        Ok(())
    })
}

pub fn verify_successor_authority_v1(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FreshSuccessorPublicationV1> {
    run(
        chain,
        parent,
        id,
        genesis,
        params,
        Scope::Verify,
        |_| Ok(()),
    )
}

pub fn complete_successor_ledger_v1(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FreshSuccessorPublicationV1> {
    run(
        chain,
        parent,
        id,
        genesis,
        params,
        Scope::Ledger,
        |_| Ok(()),
    )
}

pub fn finalize_successor_v1(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FreshSuccessorPublicationV1> {
    run(chain, parent, id, genesis, params, Scope::Finality, |_| {
        Ok(())
    })
}

#[cfg(test)]
pub(crate) fn finalize_successor_with_checkpoint_v1(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<FreshSuccessorPublicationV1> {
    run(
        chain,
        parent,
        id,
        genesis,
        params,
        Scope::Finality,
        checkpoint,
    )
}

#[cfg(test)]
pub(crate) fn complete_successor_with_checkpoint_v1(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<FreshSuccessorPublicationV1> {
    run(
        chain,
        parent,
        id,
        genesis,
        params,
        Scope::Ledger,
        checkpoint,
    )
}

#[cfg(test)]
pub(crate) fn publish_successor_with_checkpoint_v1(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<FreshSuccessorPublicationV1> {
    run(
        chain,
        parent,
        id,
        genesis,
        params,
        Scope::Authority,
        checkpoint,
    )
}

fn run(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    scope: Scope,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<FreshSuccessorPublicationV1> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    run_locked(
        &mut workspace,
        parent,
        id,
        genesis,
        params,
        scope,
        checkpoint,
        None,
    )
}

pub(super) fn capture_finalized_parent(
    workspace: &mut WorkspaceStore,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FinalizedGenesisParentV1> {
    let mut captured = None;
    with_finalized_parent(workspace, id, genesis, params, &mut |parent| {
        captured = Some(parent);
        Ok(())
    })?;
    captured.context("finalized successor parent was not captured")
}

pub(super) fn with_finalized_parent(
    workspace: &mut WorkspaceStore,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    action: &mut dyn FnMut(FinalizedGenesisParentV1) -> Result<()>,
) -> Result<()> {
    let path = resolve_native_execution_store_path_from_params_v1(params)
        .context("finalized parent requires explicit native path")?;
    let parent = NovNativeBlockLedgerV1::fresh_successor_archived_parent_v1(
        &nov_native_block_ledger_rocksdb_path_v1(&path),
        genesis,
        parse_fixed_hex_32_v1(&workspace.namespace, "successor namespace")?,
        id,
    )?;
    run_locked(
        workspace,
        parent,
        id,
        genesis,
        params,
        Scope::Verify,
        |_| Ok(()),
        Some(action),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_locked(
    workspace: &mut WorkspaceStore,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    scope: Scope,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
    capture: Option<&mut dyn FnMut(FinalizedGenesisParentV1) -> Result<()>>,
) -> Result<FreshSuccessorPublicationV1> {
    let chain = workspace.chain_id;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("successor publication requires explicit native path")?;
    let authority_lock = acquire_nov_native_execution_store_write_lock_v1(&native_path)?;
    let ledger_path = nov_native_block_ledger_rocksdb_path_v1(&native_path);
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "successor namespace")?;
    // Only public read-only Verify can reuse one complete artifact validation
    // under these uninterrupted workspace/authority locks. It cannot publish,
    // reach a checkpoint, or invoke a signing/mutating capture callback. Other
    // scopes retain their original read/commit/readback order below.
    let (artifact, binding, verified) = if scope == Scope::Verify && capture.is_none() {
        let input = ready_input(workspace, id)?;
        let descriptor = catalog(workspace)?
            .into_iter()
            .find_map(|(known, descriptor)| (known == id).then_some(descriptor))
            .context("successor publication output missing")?;
        if !is_complete(workspace, &input, &descriptor)? {
            bail!("successor publication output missing");
        }
        // This descriptor binding is only a lookup constraint, not trusted
        // output evidence. The ledger must match it and the complete artifact
        // validator below must independently reproduce it from durable bytes.
        let binding = crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
            workspace_id: id,
            plan_commitment: input.plan,
            output_digest: descriptor.digest,
        };
        let verified: crate::native_block_ledger::VerifiedSuccessorPublicationV1 =
            NovNativeBlockLedgerV1::load_verified_successor_publication_v1(
                &ledger_path,
                genesis,
                namespace,
                parent,
                &binding,
            )?;
        // The getter has released its non-reentrant ledger mutex. Reuse only
        // its direct-parent archive; input/output, QC/source, and exact delta
        // checks remain mandatory, with no error-to-cold fallback.
        let artifact = block_artifact::load_block_artifact_with_parent_archive_v1(
            workspace,
            id,
            params,
            Some(&verified.parent_archive),
        )?
        .context("successor publication output missing")?;
        if artifact.workspace_id != binding.workspace_id
            || artifact.plan_commitment != binding.plan_commitment
            || artifact.output_digest != binding.output_digest
        {
            bail!("successor publication artifact differs from completed binding");
        }
        (artifact, binding, Some(verified))
    } else {
        let artifact = block_artifact::load_block_artifact_inner_v1(workspace, id, params)?
            .context("successor publication output missing")?;
        let binding = crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
            workspace_id: id,
            plan_commitment: artifact.plan_commitment,
            output_digest: artifact.output_digest,
        };
        (artifact, binding, None)
    };
    let commitment = match &verified {
        Some(verified) => verified.commitment,
        None => NovNativeBlockLedgerV1::verify_fresh_successor_promotion_target_v1(
            &ledger_path,
            genesis,
            namespace,
            parent,
            &binding,
        )?,
    };
    let parent_height = artifact
        .block()
        .header
        .height
        .checked_sub(1)
        .filter(|height| *height > 0)
        .context("successor publication requires a finalized parent height")?;
    // The child intent is already verified and authority remains held. Use the
    // historical finalized archive here: a strict live-tip capture would reject
    // this legitimate pending intent or its already-published retry.
    let loaded_parent_archive;
    let parent_archive = match &verified {
        Some(verified) => &verified.parent_archive,
        None => {
            loaded_parent_archive = NovNativeBlockLedgerV1::load_fresh_finalized_archive_v1(
                &ledger_path,
                genesis,
                namespace,
                parent_height,
            )?;
            &loaded_parent_archive
        }
    };
    let parent_block = &parent_archive.block;
    if parent_archive.execution.workspace_id != parent
        || parent_block.header.height != parent_height
        || parent_block.header.chain_id != chain
        || parent_block.header.chain_id != artifact.block().header.chain_id
        || parent_block.header.block_hash != artifact.block().header.parent_block_hash
    {
        bail!("successor parent output differs from published ledger");
    }
    // Rooted capture binds the exact completed output, Ready input/plan, QC,
    // publication evidence, and prepared roots without materializing its Store.
    // Only an explicitly identified old format takes the original cold path;
    // a corrupt rooted source is an error, never a reason to downgrade.
    if live_parent::capture_rooted_archive(workspace, parent_archive, params)?.is_none() {
        let parent_artifact =
            block_artifact::load_block_artifact_inner_v1(workspace, parent, params)?
                .context("successor parent AOEM output missing")?;
        if parent_archive.execution.plan_commitment != parent_artifact.plan_commitment
            || parent_archive.execution.output_digest != parent_artifact.output_digest
            || parent_block != parent_artifact.block()
        {
            bail!("successor parent output differs from published ledger");
        }
    }
    let parent_target = publication_target_fields(
        if parent_block.header.height == 1 {
            b"NVP1"
        } else {
            b"NVP2"
        },
        namespace,
        genesis,
        parent_archive.commitment,
        parent,
        parent_archive.execution.output_digest,
        parent_block,
    );
    let target = publication_target(b"NVP2", namespace, genesis, commitment, id, &artifact);
    let head_key = native_aoem_owned_state_head_key_v1(chain, &workspace.namespace);
    let parent_evidence = workspace.key(b'h', &parent);
    let evidence_key = workspace.key(b'h', &id);
    if workspace.graph.get(&parent_evidence)?.as_deref() != Some(parent_target.as_slice()) {
        bail!("successor parent publication evidence missing or changed");
    }
    let current = workspace
        .graph
        .get(&head_key)?
        .context("successor authority missing")?;
    let evidence = workspace.graph.get(&evidence_key)?;
    let loaded_published_block;
    let published_block = match &verified {
        Some(verified) => verified.published_block.as_ref(),
        None => {
            loaded_published_block =
                NovNativeBlockLedgerV1::load_fresh_successor_published_block_v1(
                    &ledger_path,
                    genesis,
                    namespace,
                )?;
            loaded_published_block.as_ref()
        }
    };
    if published_block.is_some() && (published_block != Some(artifact.block()) || current != target)
    {
        bail!("published successor ledger differs from current AOEM authority");
    }
    if current == target {
        if evidence.as_deref() != Some(target.as_slice()) {
            bail!("published successor evidence missing; refusing repair");
        }
    } else {
        if matches!(scope, Scope::Verify | Scope::Finality) || current != parent_target {
            bail!("successor publication requires the exact live parent or completed target");
        }
        if evidence.as_ref().is_some_and(|value| value != &target) {
            bail!("successor publication has conflicting evidence");
        }
        let input = ready_input(workspace, id)?;
        checkpoint(PromotionCheckpointV1::BeforePublication)?;
        if let Err(error) = workspace.commit(
            b'J',
            &input,
            vec![AoemAtomicGraphWriteV1::Put {
                key: evidence_key.clone(),
                value: target.clone(),
            }],
            AoemAtomicGraphWriteV1::Put {
                key: head_key.clone(),
                value: target.clone(),
            },
        ) {
            uncertain_authority_locks()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(authority_lock);
            return Err(error).context(
                "successor publication outcome uncertain; authority lock retained until exit",
            );
        }
        checkpoint(PromotionCheckpointV1::AfterPublication)?;
    }
    let readback = (|| -> Result<()> {
        if workspace.graph.get(&head_key)?.as_deref() != Some(target.as_slice())
            || workspace.graph.get(&evidence_key)?.as_deref() != Some(target.as_slice())
            || workspace.graph.get(&parent_evidence)?.as_deref() != Some(parent_target.as_slice())
        {
            bail!("successor publication readback mismatch");
        }
        // Read-only Verify has already fully validated this immutable artifact
        // once, with both locks still held and no write/checkpoint/callback in
        // between. Replaying its three-tree transition again adds no new state
        // boundary. Preserve full readback for every mutating/recovery scope
        // and for capture callbacks; live authority/evidence reads above are
        // retained in all cases. Nothing is cached across calls.
        if verified.is_none()
            && block_artifact::load_block_artifact_inner_v1(workspace, id, params)?.as_ref()
                != Some(&artifact)
        {
            bail!("published successor AOEM output changed");
        }
        Ok(())
    })();
    if let Err(error) = readback {
        if current != target {
            uncertain_authority_locks()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(authority_lock);
        }
        return Err(error);
    }
    if scope == Scope::Ledger {
        checkpoint(PromotionCheckpointV1::BeforeLedgerCommit)?;
        NovNativeBlockLedgerV1::complete_fresh_successor_ledger_v1(
            &ledger_path,
            genesis,
            namespace,
            commitment,
        )?;
        checkpoint(PromotionCheckpointV1::AfterLedgerCommit)?;
    }
    let ledger_publication_completed = match &verified {
        Some(verified) => verified.published_block.is_some(),
        None => NovNativeBlockLedgerV1::load_fresh_successor_published_block_v1(
            &ledger_path,
            genesis,
            namespace,
        )?
        .is_some(),
    };
    if scope == Scope::Finality {
        checkpoint(PromotionCheckpointV1::BeforeFinalityCommit)?;
        NovNativeBlockLedgerV1::finalize_fresh_successor_v1(
            &ledger_path,
            genesis,
            namespace,
            commitment,
        )?;
        checkpoint(PromotionCheckpointV1::AfterFinalityCommit)?;
    }
    let finality = match verified {
        Some(verified) => verified.finality,
        None => NovNativeBlockLedgerV1::load_fresh_successor_finality_v1(
            &ledger_path,
            genesis,
            namespace,
        )?,
    };
    let finalized = finality.is_some();
    if let Some(capture) = capture {
        let proof = finality.context("next parent requires complete successor finality")?;
        if !ledger_publication_completed {
            bail!("next parent ledger is incomplete");
        }
        let input = ready_input(workspace, id)?;
        let payload = workspace.read_payload(&input)?;
        let descriptor = catalog(workspace)?
            .into_iter()
            .find_map(|(known, descriptor)| (known == id).then_some(descriptor))
            .context("finalized successor output descriptor missing")?;
        if descriptor.digest != artifact.output_digest {
            bail!("finalized successor output changed during capture");
        }
        let output = read_output(workspace, &input, &descriptor, &payload, params)?
            .context("finalized successor output missing")?;
        let config = payload
            .finalized_parent
            .as_ref()
            .context("successor parent genesis missing")?
            .config
            .clone();
        proof.validate_archived_certificate(&config, artifact.block())?;
        capture(FinalizedGenesisParentV1 {
            block: artifact.block().clone(),
            store: output.store,
            record_state: output.record_state,
            batch_result: output.batch_result,
            genesis: config,
            workspace_id: id,
            output_digest: artifact.output_digest,
            proof,
        })?;
    }
    let h = &artifact.block().header;
    Ok(FreshSuccessorPublicationV1 {
        chain_id: chain,
        block_hash: h.block_hash,
        workspace_id: id,
        intent_commitment: commitment,
        state_root: h.post_state_root,
        receipt_root: h.cumulative_receipt_root,
        state_version: h.state_version,
        aoem_authority_published: true,
        aoem_readback_verified: true,
        ledger_publication_completed,
        finalized,
    })
}
