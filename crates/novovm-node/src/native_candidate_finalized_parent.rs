//! Historical, proof-bound parent input. Never a live authority capability.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FinalizedParentSnapshot {
    pub(super) config: fresh_genesis::FreshGenesisConfigV1,
    block: NovNativeDurableBlockV1,
    pub(super) store: NovNativeExecutionStoreV1,
    proof: crate::native_block_ledger::NovNativeFreshFinalityProofV1,
}

impl FinalizedParentSnapshot {
    fn capture(parent: &FinalizedGenesisParentV1) -> Self {
        Self {
            config: parent.genesis_config().clone(),
            block: parent.block().clone(),
            store: parent.state().clone(),
            proof: parent.finality_proof().clone(),
        }
    }

    pub(super) fn validate(
        &self,
        plan: &NovNativeCandidateExecutionPlanV1,
        workspace: &WorkspaceStore,
    ) -> Result<()> {
        self.proof
            .validate_archived_block(&self.config, &self.block)?;
        verify_native_nonce_identity_scheme_v2(&self.store)?;
        verify_production_native_execution_store_authority_domain_v2(
            &self.store,
            workspace.chain_id,
            &workspace.namespace,
        )?;
        verify_native_business_protocol_config_v1(&self.store)?;
        let h = &self.block.header;
        let expected = NovNativePreparedAoemParentV1 {
            batch_id: h.aoem_batch_id.clone(),
            batch_result_id: h.aoem_batch_result_id.clone(),
            state_root: h.post_state_root,
            state_root_codec: h.post_state_root_codec.clone(),
            cumulative_receipt_root: h.cumulative_receipt_root,
            receipt_root_codec: h.cumulative_receipt_root_codec.clone(),
            state_version: h.state_version,
        };
        if self.config.chain_id != workspace.chain_id
            || self.config.protocol_config_commitment != workspace.protocol
            || h.height != 1
            || h.post_state_root_codec != NOVOVM_NATIVE_STATE_ROOT_CODEC_V3
            || h.cumulative_receipt_root_codec != NOVOVM_NATIVE_RECEIPT_ROOT_CODEC_V2
            || native_semantic_ledger_state_digest_v1(&self.store.module_state)
                != to_hex(&h.post_state_root)
            || native_execution_receipt_root_v2(&self.store)? != to_hex(&h.cumulative_receipt_root)
            || self.store.module_state.aoem_semantic_ledger_sequence != h.state_version
            || plan.aoem_parent.as_ref() != Some(&expected)
            || plan.pre_state_root != h.post_state_root
            || plan.context.block_height != 2
            || plan.context.parent_block_hash != h.block_hash
            || plan.context.slot <= h.slot
            || plan.context.timestamp_unix_ms < h.timestamp_unix_ms
        {
            bail!("finalized parent state, proof and successor plan disagree");
        }
        Ok(())
    }
}

/// Stage height two from the currently published, finalized first block.
/// Revalidates live authority even on replay; execution remains isolated.
pub fn create_from_finalized_genesis_v1(
    plan: &NovNativeCandidateExecutionPlanV1,
    parent_workspace_id: [u8; 32],
    genesis_commitment: [u8; 32],
    params: &serde_json::Value,
) -> Result<WorkspaceInfoV1> {
    plan.validate()?;
    let mut workspace = WorkspaceStore::open(plan.context.chain_id, params)?;
    let parent = execution::capture_finalized_parent_locked(
        &mut workspace,
        parent_workspace_id,
        genesis_commitment,
        params,
    )?;
    if parent.successor_plan(plan.context, plan.raw_txs.clone(), params)? != *plan {
        bail!("successor input differs from live finalized parent");
    }
    let payload = Payload {
        schema: SCHEMA.to_owned(),
        plan: plan.clone(),
        parent_block: None,
        parent_snapshot: None,
        genesis: None,
        finalized_parent: Some(FinalizedParentSnapshot::capture(&parent)),
    };
    validate_payload(&payload, &workspace)?;
    stage_payload(&mut workspace, &payload, |_| Ok(()))
}
