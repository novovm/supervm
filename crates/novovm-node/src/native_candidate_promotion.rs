//! Publish only the verified, pinned candidate output. No NOV business tasks in
//! AOEM and no re-execution. Ledger indexes may be completed under the same locks.
use super::*;
use crate::native_block_ledger::NovNativeFreshFinalityProofV1;

enum PublicationScope<'a> {
    Authority,
    Ledger,
    Finality(&'a NovNativeFreshFinalityProofV1),
}

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
    BeforeLedgerCommit,
    AfterLedgerCommit,
}

fn uncertain_authority_locks() -> &'static Mutex<Vec<NovNativeExecutionStoreWriteLockV1>> {
    static LOCKS: OnceLock<Mutex<Vec<NovNativeExecutionStoreWriteLockV1>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Recover the exact archived decision across intent, AOEM and ledger boundaries.
/// A different archive or target never replaces a previously pinned intent.
pub fn resume_genesis_promotion_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    seal_path: &Path,
    ledger_path: &Path,
    params: &serde_json::Value,
) -> Result<GenesisPromotionPublicationV1> {
    let existing = {
        let workspace = WorkspaceStore::open(chain, params)?;
        let path = resolve_native_execution_store_path_from_params_v1(params)
            .context("promotion requires explicit storage")?;
        if fs::canonicalize(nov_native_block_ledger_rocksdb_path_v1(&path))?
            != fs::canonicalize(ledger_path)?
        {
            bail!("promotion params resolve to a different service ledger");
        }
        NovNativeBlockLedgerV1::optional_fresh_genesis_promotion_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&path),
            genesis,
            parse_fixed_hex_32_v1(&workspace.namespace, "promotion namespace")?,
        )?
    };
    if let Some(intent) = existing {
        let store = crate::native_block_seal::NovNativeBlockSealStoreV1::open_existing_read_only(
            seal_path,
        )?
        .context("promotion decision archive missing")?;
        let decision = store
            .load_decision_certificate_by_height_v3(chain, 1, 1)?
            .context("promotion decision missing")?;
        if decision != intent.decision || intent.execution.workspace_id != id {
            bail!("promotion recovery differs from pinned decision/workspace");
        }
    } else {
        block_artifact::prepare_genesis_promotion_v1(chain, id, genesis, seal_path, params)?;
    }
    complete_genesis_promotion_v1(chain, id, genesis, params)
}

/// Explicit operator/coordinator action only. A separate durable ledger intent
/// must already exist; caller booleans or an unarchived remote QC are insufficient.
pub fn publish_genesis_promotion_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<GenesisPromotionPublicationV1> {
    run(
        chain,
        id,
        genesis,
        params,
        true,
        PublicationScope::Authority,
        |_| Ok(()),
    )
}

/// Publish AOEM authority and atomically complete the durable block query indexes.
/// This is not activation of continuous consensus or a finality attestation.
pub fn complete_genesis_promotion_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<GenesisPromotionPublicationV1> {
    run(
        chain,
        id,
        genesis,
        params,
        true,
        PublicationScope::Ledger,
        |_| Ok(()),
    )
}

/// Verify the live published output and durably attach the complete BFT witness.
pub fn finalize_genesis_promotion_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    proof: &NovNativeFreshFinalityProofV1,
    params: &serde_json::Value,
) -> Result<GenesisPromotionPublicationV1> {
    run(
        chain,
        id,
        genesis,
        params,
        false,
        PublicationScope::Finality(proof),
        |_| Ok(()),
    )
}

/// Full readback without AOEM writes or repairs, under both persistence locks.
pub fn verify_genesis_promotion_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<GenesisPromotionPublicationV1> {
    run(
        chain,
        id,
        genesis,
        params,
        false,
        PublicationScope::Authority,
        |_| Ok(()),
    )
}

#[cfg(test)]
pub(crate) fn publish_with_checkpoint_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<GenesisPromotionPublicationV1> {
    run(
        chain,
        id,
        genesis,
        params,
        true,
        PublicationScope::Authority,
        checkpoint,
    )
}

#[cfg(test)]
pub(crate) fn complete_with_checkpoint_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<GenesisPromotionPublicationV1> {
    run(
        chain,
        id,
        genesis,
        params,
        true,
        PublicationScope::Ledger,
        checkpoint,
    )
}

fn run(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    allow_write: bool,
    scope: PublicationScope<'_>,
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
    let ledger_path = nov_native_block_ledger_rocksdb_path_v1(&native_path);
    if matches!(scope, PublicationScope::Ledger) {
        checkpoint(PromotionCheckpointV1::BeforeLedgerCommit)?;
        NovNativeBlockLedgerV1::complete_fresh_genesis_ledger_v1(
            &ledger_path,
            genesis,
            namespace,
            &intent,
        )?;
        checkpoint(PromotionCheckpointV1::AfterLedgerCommit)?;
    }
    if let PublicationScope::Finality(proof) = scope {
        NovNativeBlockLedgerV1::finalize_fresh_genesis_v1(&ledger_path, genesis, namespace, proof)?;
    }
    let ledger_publication_completed = NovNativeBlockLedgerV1::fresh_genesis_ledger_published_v1(
        &ledger_path,
        genesis,
        namespace,
    )?;
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
        ledger_publication_completed,
        finalized: NovNativeBlockLedgerV1::load_fresh_genesis_finality_v1(
            &ledger_path,
            genesis,
            namespace,
        )?
        .is_some(),
    })
}
