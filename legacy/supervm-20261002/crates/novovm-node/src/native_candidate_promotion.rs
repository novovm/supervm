//! Publish only the verified, pinned candidate output. No NOV business tasks in
//! AOEM and no re-execution. Ledger indexes may be completed under the same locks.
use super::*;
#[path = "native_candidate_startup.rs"]
mod startup;
use crate::native_block_ledger::NovNativeFreshFinalityProofV1;
pub(crate) use startup::{load_startup_artifact_v1, load_startup_successor_v1};
#[path = "native_candidate_retirement.rs"]
mod retirement;
#[path = "native_candidate_successor_publication.rs"]
mod successor;
pub use retirement::{retire_old_workspaces_v1, WorkspaceRetirementV1};
#[cfg(test)]
pub(crate) use retirement::{retire_with_checkpoint_v1, RetirementCheckpointV1};
pub use successor::{
    complete_successor_ledger_v1, finalize_successor_v1, publish_successor_authority_v1,
    resume_successor_promotion_v1, verify_successor_authority_v1, FreshSuccessorPublicationV1,
};
#[cfg(test)]
pub(crate) use successor::{
    complete_successor_with_checkpoint_v1, finalize_successor_with_checkpoint_v1,
    publish_successor_with_checkpoint_v1,
};

fn publication_target(
    magic: &[u8; 4],
    namespace: [u8; 32],
    genesis: [u8; 32],
    commitment: [u8; 32],
    id: [u8; 32],
    artifact: &IsolatedBlockArtifactV1,
) -> Vec<u8> {
    publication_target_fields(
        magic,
        namespace,
        genesis,
        commitment,
        id,
        artifact.output_digest,
        artifact.block(),
    )
}

pub(in super::super) fn publication_target_fields(
    magic: &[u8; 4],
    namespace: [u8; 32],
    genesis: [u8; 32],
    commitment: [u8; 32],
    id: [u8; 32],
    output_digest: [u8; 32],
    block: &NovNativeDurableBlockV1,
) -> Vec<u8> {
    let header = &block.header;
    let mut target = magic.to_vec();
    target.extend_from_slice(&header.chain_id.to_be_bytes());
    for part in [
        namespace,
        genesis,
        commitment,
        id,
        output_digest,
        header.block_hash,
        header.post_state_root,
        header.cumulative_receipt_root,
    ] {
        target.extend_from_slice(&part);
    }
    target.extend_from_slice(&header.state_version.to_be_bytes());
    target
}

enum PublicationScope<'a> {
    Authority,
    Ledger,
    Finality(&'a NovNativeFreshFinalityProofV1),
    Capture(&'a mut dyn FnMut(FinalizedGenesisParentV1) -> Result<()>),
}

/// Pin the complete successor decision and immutable execution under the live
/// parent's locks. This does not publish AOEM state, select a head or finalize.
pub fn prepare_successor_promotion_v1(
    chain: u64,
    parent_id: [u8; 32],
    candidate_id: [u8; 32],
    genesis: [u8; 32],
    proof: &NovNativeFreshFinalityProofV1,
    params: &serde_json::Value,
) -> Result<[u8; 32]> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    let candidate = block_artifact::load_block_artifact_inner_v1(&workspace, candidate_id, params)?
        .context("successor promotion requires complete AOEM output")?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("successor promotion requires explicit native path")?;
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "successor promotion namespace")?;
    // The first prepare uses the strict current-finalized rooted scope below.
    // An exact already-pinned intent is a recovery retry, not new permission to
    // register/sign while pending. Only this existing archive verifier's true
    // result selects the original cold, authority-locked callback; mismatched
    // proof/parent/candidate or corrupt evidence is an error, never a fallback.
    let existing_exact_intent = NovNativeBlockLedgerV1::verify_optional_successor_archive_v1(
        &nov_native_block_ledger_rocksdb_path_v1(&native_path),
        genesis,
        namespace,
        parent_id,
        candidate_id,
        proof,
    )?;
    let mut commitment = None;
    let mut prepare = |parent: live_parent::FinalizedParentViewV1| {
        parent.successor_seal_subject(&candidate, 0)?;
        commitment = Some(NovNativeBlockLedgerV1::stage_fresh_successor_promotion_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&native_path),
            genesis,
            namespace,
            parent_id,
            crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
                workspace_id: candidate_id,
                plan_commitment: candidate.plan_commitment,
                output_digest: candidate.output_digest,
            },
            proof.clone(),
        )?);
        Ok(())
    };
    if existing_exact_intent {
        with_cold_finalized_parent_locked(
            &mut workspace,
            parent_id,
            genesis,
            params,
            &mut prepare,
        )?;
    } else {
        with_finalized_parent_locked(&mut workspace, parent_id, genesis, params, &mut prepare)?;
    }
    commitment.context("successor promotion was not staged")
}

/// An immutable, verified point-in-time parent image, not permission to sign or
/// publish a child. A child publisher must recheck that its parent is still live.
/// No Deserialize/public constructor: caller-provided state is not a verified parent.
pub struct FinalizedGenesisParentV1 {
    block: NovNativeDurableBlockV1,
    store: NovNativeExecutionStoreV1,
    pub(in super::super) record_state: Option<state_records::StoreRef>,
    batch_result: novovm_exec::NovovmAoemNativeTxBatchResultV1,
    genesis: fresh_genesis::FreshGenesisConfigV1,
    workspace_id: [u8; 32],
    output_digest: [u8; 32],
    proof: NovNativeFreshFinalityProofV1,
}

impl FinalizedGenesisParentV1 {
    /// Build the historical next-height signing subject. Does not sign,
    /// register, transmit or grant live publication permission.
    pub fn successor_seal_subject(
        &self,
        candidate: &IsolatedBlockArtifactV1,
        round: u64,
    ) -> Result<crate::native_block_seal::NovNativeSealSubjectV1> {
        let block = candidate.block();
        let h = &block.header;
        let p = &self.block.header;
        let compiled = self.genesis.compile()?;
        let expected_parent = NovNativePreparedAoemParentV1 {
            batch_id: p.aoem_batch_id.clone(),
            batch_result_id: p.aoem_batch_result_id.clone(),
            state_root: p.post_state_root,
            state_root_codec: p.post_state_root_codec.clone(),
            cumulative_receipt_root: p.cumulative_receipt_root,
            receipt_root_codec: p.cumulative_receipt_root_codec.clone(),
            state_version: p.state_version,
        };
        if candidate.fresh_genesis_identity() != Some(&compiled.identity())
            || h.chain_id != p.chain_id
            || p.height.checked_add(1) != Some(h.height)
            || h.parent_block_hash != p.block_hash
            || h.pre_state_root != p.post_state_root
            || h.aoem_parent.as_ref() != Some(&expected_parent)
            || p.state_version.checked_add(u64::from(h.tx_count)) != Some(h.state_version)
            || h.slot <= p.slot
            || h.timestamp_unix_ms < p.timestamp_unix_ms
        {
            bail!("successor seal subject does not extend verified finalized execution");
        }
        let target = self
            .proof
            .validated_decision_target(&self.genesis, &self.block)?;
        crate::native_block_seal::subject_from_block_profile_v1(
            block,
            compiled.validator_set(),
            round,
            target,
            compiled.identity().anchor(),
            self.genesis.protocol_config_commitment,
            crate::native_block_seal::NOV_NATIVE_BLOCK_SEAL_FRESH_SUCCESSOR_PROOF_V1,
        )
    }

    /// Build and authenticate the next-height input against this exact image.
    /// No pending admission, nonce reservation, execution or authority writes.
    pub fn successor_plan(
        &self,
        context: novovm_protocol::NovBlockExecutionContextV1,
        raw_txs: Vec<Vec<u8>>,
        params: &serde_json::Value,
    ) -> Result<NovNativeCandidateExecutionPlanV1> {
        if raw_txs.is_empty()
            || raw_txs.len() > crate::native_block_ledger::NOV_NATIVE_BLOCK_LEDGER_MAX_TXS_V1
            || raw_txs
                .iter()
                .try_fold(0usize, |total, raw| total.checked_add(raw.len()))
                .is_none_or(|total| {
                    total > crate::native_block_ledger::NOV_NATIVE_BLOCK_LEDGER_MAX_BODY_BYTES_V1
                })
        {
            bail!("successor transaction batch exceeds block bounds");
        }
        let h = &self.block.header;
        if context.chain_id != h.chain_id
            || h.height.checked_add(1) != Some(context.block_height)
            || context.parent_block_hash != h.block_hash
            || context.slot <= h.slot
            || context.timestamp_unix_ms < h.timestamp_unix_ms
        {
            bail!("successor context does not extend the verified finalized parent");
        }
        let hashes = raw_txs
            .iter()
            .map(|raw| canonical_nov_native_tx_hash_from_payload_v1(raw))
            .collect::<Result<Vec<_>>>()?;
        let plan = NovNativeCandidateExecutionPlanV1::new(
            context,
            self.genesis.protocol_config_commitment,
            h.post_state_root,
            Some(NovNativePreparedAoemParentV1 {
                batch_id: h.aoem_batch_id.clone(),
                batch_result_id: h.aoem_batch_result_id.clone(),
                state_root: h.post_state_root,
                state_root_codec: h.post_state_root_codec.clone(),
                cumulative_receipt_root: h.cumulative_receipt_root,
                receipt_root_codec: h.cumulative_receipt_root_codec.clone(),
                state_version: h.state_version,
            }),
            hashes,
            raw_txs,
        )?;
        authenticate_plan(&plan, &self.store, params)?;
        Ok(plan)
    }

    pub fn block(&self) -> &NovNativeDurableBlockV1 {
        &self.block
    }
    pub fn batch_result(&self) -> &novovm_exec::NovovmAoemNativeTxBatchResultV1 {
        &self.batch_result
    }
    pub fn genesis_config(&self) -> &fresh_genesis::FreshGenesisConfigV1 {
        &self.genesis
    }
    pub fn workspace_id(&self) -> [u8; 32] {
        self.workspace_id
    }
    pub fn output_digest(&self) -> [u8; 32] {
        self.output_digest
    }
    pub fn finality_proof(&self) -> &NovNativeFreshFinalityProofV1 {
        &self.proof
    }
    /// Complete historical state, including authenticated nonce reservations and
    /// cumulative receipts. This accessor never turns the image into live authority.
    pub fn state(&self) -> &NovNativeExecutionStoreV1 {
        &self.store
    }
}

pub fn load_finalized_genesis_parent_v1(
    chain: u64,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FinalizedGenesisParentV1> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    capture_finalized_parent_locked(&mut workspace, id, genesis, params)
}

/// Resolve only a fully finalized descendant of the operator's exact anchor.
/// Historical snapshots may be retired; immutable ledger bindings remain required.
pub fn load_latest_finalized_parent_v1(
    chain: u64,
    genesis: [u8; 32],
    anchor_height: u64,
    anchor_hash: [u8; 32],
    anchor_id: [u8; 32],
    anchor_previous: Option<[u8; 32]>,
    params: &serde_json::Value,
) -> Result<Option<(FinalizedGenesisParentV1, Option<[u8; 32]>)>> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    let path = resolve_native_execution_store_path_from_params_v1(params)
        .context("startup follow requires explicit native path")?;
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "startup namespace")?;
    let Some(tip) = NovNativeBlockLedgerV1::finalized_service_tip_v1(
        &nov_native_block_ledger_rocksdb_path_v1(&path),
        genesis,
        namespace,
        anchor_height,
        anchor_hash,
        anchor_id,
        anchor_previous,
    )?
    else {
        return Ok(None);
    };
    Ok(Some((
        capture_finalized_parent_locked(&mut workspace, tip.current, genesis, params)?,
        tip.previous,
    )))
}

pub(in super::super) fn capture_finalized_parent_locked(
    workspace: &mut WorkspaceStore,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<FinalizedGenesisParentV1> {
    let artifact = block_artifact::load_block_artifact_inner_v1(workspace, id, params)?
        .context("finalized parent output missing")?;
    if artifact.block().header.height > 1 {
        return successor::capture_finalized_parent(workspace, id, genesis, params);
    }
    let mut captured = None;
    run_locked(
        workspace,
        id,
        genesis,
        params,
        false,
        PublicationScope::Capture(&mut |parent| {
            captured = Some(parent);
            Ok(())
        }),
        |_| Ok(()),
    )?;
    captured.context("verified finalized parent was not captured")
}

fn with_finalized_parent_locked(
    workspace: &mut WorkspaceStore,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    action: &mut dyn FnMut(live_parent::FinalizedParentViewV1) -> Result<()>,
) -> Result<()> {
    if live_parent::try_with_rooted_finalized_parent_view_locked(
        workspace, id, genesis, params, action,
    )? {
        return Ok(());
    }
    with_cold_finalized_parent_locked(workspace, id, genesis, params, action)
}

fn with_cold_finalized_parent_locked(
    workspace: &mut WorkspaceStore,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    action: &mut dyn FnMut(live_parent::FinalizedParentViewV1) -> Result<()>,
) -> Result<()> {
    // Only recognized legacy/physical-only output reaches this compatibility
    // branch from the normal live scope. prepare also uses it for a separately
    // verified exact-intent retry. Both original implementations hold authority
    // across Capture; never release a captured snapshot's guard before action.
    let artifact = block_artifact::load_block_artifact_inner_v1(workspace, id, params)?
        .context("finalized parent output missing")?;
    let mut cold_action = |parent| {
        action(live_parent::FinalizedParentViewV1::from_verified_cold(
            parent,
        ))
    };
    if artifact.block().header.height > 1 {
        successor::with_finalized_parent(workspace, id, genesis, params, &mut cold_action)
    } else {
        run_locked(
            workspace,
            id,
            genesis,
            params,
            false,
            PublicationScope::Capture(&mut cold_action),
            |_| Ok(()),
        )?;
        Ok(())
    }
}

/// Persist a next-height candidate only while its parent remains live and
/// finalized. No signing, authority mutation or candidate selection.
pub fn register_finalized_successor_v1(
    chain: u64,
    parent_id: [u8; 32],
    candidate_id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
) -> Result<crate::native_block_ledger::NovNativeBlockCandidateRecordV1> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    let candidate = block_artifact::load_block_artifact_inner_v1(&workspace, candidate_id, params)?
        .context("successor registration requires complete AOEM output")?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("successor registration requires explicit native path")?;
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "successor namespace")?;
    let mut registered = None;
    with_finalized_parent_locked(&mut workspace, parent_id, genesis, params, &mut |parent| {
        parent.successor_seal_subject(&candidate, 0)?;
        registered = Some(NovNativeBlockLedgerV1::register_fresh_successor_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&native_path),
            genesis,
            namespace,
            candidate.block().clone(),
            crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
                workspace_id: candidate_id,
                plan_commitment: candidate.plan_commitment,
                output_digest: candidate.output_digest,
            },
        )?);
        Ok(())
    })?;
    registered.context("successor registration did not complete")
}

/// Live signing scope, not a stored capability. The callback must not re-enter
/// workspace/authority APIs or mutate this ledger through another handle.
pub fn with_verified_finalized_successor_v1<T>(
    chain: u64,
    parent_id: [u8; 32],
    candidate_id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    action: impl FnOnce(&NovNativeBlockLedgerV1) -> Result<T>,
) -> Result<T> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    let candidate = block_artifact::load_block_artifact_inner_v1(&workspace, candidate_id, params)?
        .context("successor signing requires complete AOEM output")?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("successor signing requires explicit native path")?;
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "successor namespace")?;
    let mut action = Some(action);
    let mut result = None;
    with_finalized_parent_locked(&mut workspace, parent_id, genesis, params, &mut |parent| {
        parent.successor_seal_subject(&candidate, 0)?;
        result = Some(NovNativeBlockLedgerV1::with_fresh_successor_seal_scope_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&native_path),
            genesis,
            namespace,
            candidate.block(),
            &crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
                workspace_id: candidate_id,
                plan_commitment: candidate.plan_commitment,
                output_digest: candidate.output_digest,
            },
            action.take().context("successor action already consumed")?,
        )?);
        Ok(())
    })?;
    result.context("successor signing scope did not run")
}

pub fn with_verified_finalized_parent_round_v1<T>(
    chain: u64,
    parent_id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    action: impl FnOnce(&NovNativeBlockLedgerV1) -> Result<T>,
) -> Result<T> {
    let mut workspace = WorkspaceStore::open(chain, params)?;
    let input = ready_input(&workspace, parent_id)?;
    let native_path = resolve_native_execution_store_path_from_params_v1(params)
        .context("parent round requires explicit native path")?;
    let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "parent round namespace")?;
    let mut action = Some(action);
    let mut result = None;
    with_finalized_parent_locked(&mut workspace, parent_id, genesis, params, &mut |parent| {
        if parent.workspace_id() != input.id {
            bail!("parent round input differs from finalized live execution");
        }
        result = Some(NovNativeBlockLedgerV1::with_fresh_parent_round_scope_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&native_path),
            genesis,
            namespace,
            parent.block(),
            &crate::native_block_ledger::NovNativeIsolatedExecutionBindingV1 {
                workspace_id: parent_id,
                plan_commitment: input.plan,
                output_digest: parent.output_digest(),
            },
            action
                .take()
                .context("parent round action already consumed")?,
        )?);
        Ok(())
    })?;
    result.context("parent round scope did not run")
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
    BeforeFinalityCommit,
    AfterFinalityCommit,
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
    run_locked(
        &mut workspace,
        id,
        genesis,
        params,
        allow_write,
        scope,
        checkpoint,
    )
}

// The caller owns the workspace OS lock. Keep the authority lock acquisition
// inside this function so all paths preserve workspace -> authority ordering.
fn run_locked(
    workspace: &mut WorkspaceStore,
    id: [u8; 32],
    genesis: [u8; 32],
    params: &serde_json::Value,
    allow_write: bool,
    scope: PublicationScope<'_>,
    checkpoint: impl Fn(PromotionCheckpointV1) -> Result<()>,
) -> Result<GenesisPromotionPublicationV1> {
    let chain = workspace.chain_id;
    let artifact = block_artifact::load_block_artifact_inner_v1(workspace, id, params)?
        .context("promotion requires complete durable execution output")?;
    let identity = artifact
        .fresh_genesis_identity()
        .context("promotion requires a fresh first candidate")?;
    if identity.config_commitment() != genesis || identity.chain_id() != chain {
        bail!("promotion genesis identity mismatch");
    }
    let input = ready_input(workspace, id)?;
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
    let target = publication_target(b"NVP1", namespace, genesis, commitment, id, &artifact);
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
        let recovered = block_artifact::load_block_artifact_inner_v1(workspace, id, params)?
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
    if let PublicationScope::Finality(proof) = &scope {
        NovNativeBlockLedgerV1::finalize_fresh_genesis_v1(&ledger_path, genesis, namespace, proof)?;
    }
    let ledger_publication_completed = NovNativeBlockLedgerV1::fresh_genesis_ledger_published_v1(
        &ledger_path,
        genesis,
        namespace,
    )?;
    let finality =
        NovNativeBlockLedgerV1::load_fresh_genesis_finality_v1(&ledger_path, genesis, namespace)?;
    let finalized = finality.is_some();
    if let PublicationScope::Capture(action) = scope {
        let proof = finality.context("next-height parent requires durable BFT finality")?;
        if !ledger_publication_completed {
            bail!("next-height parent ledger is incomplete");
        }
        let descriptor = catalog(workspace)?
            .into_iter()
            .find_map(|(known, descriptor)| (known == id).then_some(descriptor))
            .context("finalized parent output descriptor missing")?;
        if descriptor.digest != artifact.output_digest {
            bail!("finalized parent output changed during capture");
        }
        let output = read_output(workspace, &input, &descriptor, &payload, params)?
            .context("finalized parent output missing")?;
        let config = payload
            .genesis
            .as_ref()
            .context("finalized first parent genesis missing")?
            .config
            .clone();
        action(FinalizedGenesisParentV1 {
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
        finalized,
    })
}
