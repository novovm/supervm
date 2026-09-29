//! Publish existing immutable AOEM output, never rerun NOV business execution.
use super::*;

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
    run(chain, parent, id, genesis, params, true, |_| Ok(()))
}

pub fn verify_successor_authority_v1(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FreshSuccessorPublicationV1> {
    run(chain, parent, id, genesis, params, false, |_| Ok(()))
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
    run(chain, parent, id, genesis, params, true, checkpoint)
}

fn run(
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    allow_write: bool,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<FreshSuccessorPublicationV1> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("successor publication requires explicit native path")?;
    let authority_lock = acquire_nov_native_execution_store_write_lock_v1(&native_path)?;
    let ledger_path = nov_native_block_ledger_rocksdb_path_v1(&native_path);
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "successor namespace")?;
    let artifact = block_artifact::load_block_artifact_inner_v1(&workspace, id, params)?
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
    let parent_artifact = block_artifact::load_block_artifact_inner_v1(&workspace, parent, params)?
        .context("successor parent AOEM output missing")?;
    let parent_intent =
        NovNativeBlockLedgerV1::load_fresh_genesis_promotion_v1(&ledger_path, genesis, namespace)?;
    if parent_intent.execution.workspace_id != parent
        || parent_intent.execution.plan_commitment != parent_artifact.plan_commitment
        || parent_intent.execution.output_digest != parent_artifact.output_digest
        || NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(
            &ledger_path,
            genesis,
            namespace,
        )?
        .as_ref()
            != Some(parent_artifact.block())
    {
        bail!("successor parent output differs from published ledger");
    }
    let parent_target = publication_target(
        b"NVP1",
        namespace,
        genesis,
        parent_intent.commitment()?,
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
    if current == target {
        if evidence.as_deref() != Some(target.as_slice()) {
            bail!("published successor evidence missing; refusing repair");
        }
    } else {
        if !allow_write || current != parent_target {
            bail!("successor publication requires the exact live parent or completed target");
        }
        if evidence.as_ref().is_some_and(|value| value != &target) {
            bail!("successor publication has conflicting evidence");
        }
        let input = ready_input(&workspace, id)?;
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
        if block_artifact::load_block_artifact_inner_v1(&workspace, id, params)?.as_ref()
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
        ledger_publication_completed: false,
        finalized: false,
    })
}
