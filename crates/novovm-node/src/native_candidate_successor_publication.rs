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
    let artifact = block_artifact::load_block_artifact_inner_v1(workspace, id, params)?
        .context("successor publication output missing")?;
    let commitment = NovNativeBlockLedgerV1::verify_fresh_successor_promotion_target_v1(
        &ledger_path,
        genesis,
        namespace,
        parent,
        &crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
            workspace_id: id,
            plan_commitment: artifact.plan_commitment,
            output_digest: artifact.output_digest,
        },
    )?;
    let parent_artifact = block_artifact::load_block_artifact_inner_v1(workspace, parent, params)?
        .context("successor parent AOEM output missing")?;
    let (parent_execution, parent_commitment, parent_block) =
        NovNativeBlockLedgerV1::load_fresh_finalized_execution_v1(
            &ledger_path,
            genesis,
            namespace,
            parent_artifact.block().header.height,
        )?;
    if parent_execution.workspace_id != parent
        || parent_execution.plan_commitment != parent_artifact.plan_commitment
        || parent_execution.output_digest != parent_artifact.output_digest
        || &parent_block != parent_artifact.block()
    {
        bail!("successor parent output differs from published ledger");
    }
    let parent_target = publication_target(
        if parent_block.header.height == 1 {
            b"NVP1"
        } else {
            b"NVP2"
        },
        namespace,
        genesis,
        parent_commitment,
        parent,
        &parent_artifact,
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
    let published_block = NovNativeBlockLedgerV1::load_fresh_successor_published_block_v1(
        &ledger_path,
        genesis,
        namespace,
    )?;
    if published_block.is_some()
        && (published_block.as_ref() != Some(artifact.block()) || current != target)
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
        if block_artifact::load_block_artifact_inner_v1(workspace, id, params)?.as_ref()
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
    let ledger_publication_completed =
        NovNativeBlockLedgerV1::load_fresh_successor_published_block_v1(
            &ledger_path,
            genesis,
            namespace,
        )?
        .is_some();
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
    let finality =
        NovNativeBlockLedgerV1::load_fresh_successor_finality_v1(&ledger_path, genesis, namespace)?;
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
