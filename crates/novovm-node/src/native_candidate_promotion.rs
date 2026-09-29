//! Publish only the verified, pinned candidate output. No NOV business tasks in
//! AOEM and no re-execution. Ledger finality/index publication remains separate.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenesisPromotionPublicationV1 {
    pub chain_id: u64,
    pub block_hash: [u8; 32],
    pub intent_commitment: [u8; 32],
    pub workspace_id: [u8; 32],
    pub state_root: [u8; 32],
    pub receipt_root: [u8; 32],
    pub state_version: u64,
    pub aoem_authority_published: bool,
    pub aoem_readback_verified: bool,
    pub ledger_publication_completed: bool,
    pub finalized: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromotionCheckpointV1 {
    BeforePublication,
    AfterPublication,
}

fn uncertain_authority_locks() -> &'static Mutex<Vec<NovNativeExecutionStoreWriteLockV1>> {
    static LOCKS: OnceLock<Mutex<Vec<NovNativeExecutionStoreWriteLockV1>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Explicit operator/coordinator action only. A separate durable ledger intent
/// must already exist; caller booleans or an unarchived remote QC are insufficient.
pub fn publish_genesis_promotion_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<GenesisPromotionPublicationV1> {
    run(chain, id, genesis, params, true, |_| Ok(()))
}

/// Full readback without AOEM writes or repairs, under both persistence locks.
pub fn verify_genesis_promotion_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<GenesisPromotionPublicationV1> {
    run(chain, id, genesis, params, false, |_| Ok(()))
}

#[cfg(test)]
pub(crate) fn publish_with_checkpoint_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<GenesisPromotionPublicationV1> {
    run(chain, id, genesis, params, true, checkpoint)
}

fn run(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    allow_write: bool,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<GenesisPromotionPublicationV1> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    let artifact = block_artifact::load_block_artifact_inner_v1(&workspace, id, params)?
        .context("promotion requires complete durable execution output")?;
    let identity = artifact
        .fresh_genesis_identity()
        .context("promotion requires a fresh first candidate")?;
    if identity.config_commitment() != genesis || identity.chain_id() != chain {
        bail!("promotion genesis identity mismatch");
    }
    let input = ready_input(&workspace, id)?;
    let payload = workspace.read_payload(&input)?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("promotion requires an explicit native store path")?;
    let authority_lock = acquire_nov_native_execution_store_write_lock_v1(&native_path)?;
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "promotion namespace")?;
    let intent = NovNativeBlockLedgerV1::load_fresh_genesis_promotion_v1(
        &nov_native_block_ledger_rocksdb_path_v1(&native_path),
        genesis,
        namespace,
    )?;
    if intent.chain_id != chain
        || intent.block_hash != artifact.block().header.block_hash
        || intent.execution.workspace_id != id
        || intent.execution.plan_commitment != artifact.plan_commitment
        || intent.execution.output_digest != artifact.output_digest
    {
        bail!("promotion intent differs from verified AOEM output");
    }
    let header = &artifact.block().header;
    let commitment = intent.commitment()?;
    // Points to existing immutable AOEM candidate chunks. Their input/output
    // markers, signatures, receipt/state roots and complete bytes were verified
    // above. The intent fences abort, so the new authority cannot lose its source.
    let mut target = b"NVP1".to_vec();
    target.extend_from_slice(&chain.to_be_bytes());
    for part in [
        namespace,
        genesis,
        commitment,
        id,
        artifact.output_digest,
        header.block_hash,
        header.post_state_root,
        header.cumulative_receipt_root,
    ] {
        target.extend_from_slice(&part);
    }
    target.extend_from_slice(&header.state_version.to_be_bytes());
    let head_key = native_aoem_owned_state_head_key_v1(chain, &workspace.namespace);
    let evidence_key = workspace.key(b'h', &id);
    let current = workspace
        .graph
        .get(&head_key)?
        .context("promotion authority head is missing")?;
    let evidence = workspace.graph.get(&evidence_key)?;
    if current == target {
        if evidence.as_deref() != Some(target.as_slice()) {
            bail!("published promotion evidence missing or changed; refusing repair");
        }
    } else {
        if !allow_write {
            bail!("promotion authority has not been published");
        }
        if evidence.as_ref().is_some_and(|bytes| bytes != &target) {
            bail!("promotion contains another publication target");
        }
        let live = fresh_genesis::publication::read_snapshot_v1(
            &workspace.graph,
            chain,
            &workspace.namespace,
            genesis,
        )?;
        let captured = payload
            .genesis
            .as_ref()
            .context("promotion archived genesis missing")?;
        if serde_json::to_value(live)? != serde_json::to_value(captured)? {
            bail!("promotion current authority differs from captured genesis");
        }
        checkpoint(PromotionCheckpointV1::BeforePublication)?;
        let publication = workspace.commit(
            b'H',
            &input,
            vec![AoemAtomicGraphWriteV1::Put {
                key: evidence_key.clone(),
                value: target.clone(),
            }],
            AoemAtomicGraphWriteV1::Put {
                key: head_key.clone(),
                value: target.clone(),
            },
        );
        if let Err(error) = publication {
            uncertain_authority_locks()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(authority_lock);
            return Err(error).context(
                "authority publication outcome uncertain; authority lock retained until exit",
            );
        }
        checkpoint(PromotionCheckpointV1::AfterPublication)?;
    }
    let verified = (|| -> Result<()> {
        if workspace.graph.get(&head_key)?.as_deref() != Some(target.as_slice())
            || workspace.graph.get(&evidence_key)?.as_deref() != Some(target.as_slice())
        {
            bail!("AOEM promotion publication readback mismatch");
        }
        let recovered = block_artifact::load_block_artifact_inner_v1(&workspace, id, params)?
            .context("published promotion output missing")?;
        if recovered != artifact {
            bail!("published promotion output changed");
        }
        Ok(())
    })();
    if let Err(error) = verified {
        if current != target {
            uncertain_authority_locks()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(authority_lock);
        }
        return Err(error).context("promotion readback failed; inspect before further publication");
    }
    Ok(GenesisPromotionPublicationV1 {
        chain_id: chain,
        block_hash: header.block_hash,
        intent_commitment: commitment,
        workspace_id: id,
        state_root: header.post_state_root,
        receipt_root: header.cumulative_receipt_root,
        state_version: header.state_version,
        aoem_authority_published: true,
        aoem_readback_verified: true,
        ledger_publication_completed: false,
        finalized: false,
    })
}
