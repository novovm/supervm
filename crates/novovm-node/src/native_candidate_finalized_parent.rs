//! Historical, proof-bound parent input. Never a live authority capability.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FinalizedParentSnapshot {
    pub(super) config: fresh_genesis::FreshGenesisConfigV1,
    pub(super) block: NovNativeDurableBlockV1,
    pub(super) store: NovNativeExecutionStoreV1,
    pub(super) proof: crate::native_block_ledger::NovNativeFreshFinalityProofV1,
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
            .validate_archived_certificate(&self.config, &self.block)?;
        verify_native_nonce_identity_scheme_v2(&self.store)?;
        verify_production_native_execution_store_authority_domain_v2(
            &self.store,
            workspace.chain_id,
            &workspace.namespace,
        )?;
        verify_native_business_protocol_config_v1(&self.store)?;
        let h = &self.block.header;
        let profile = self.config.root_codec_profile()?;
        if h.post_state_root_codec != profile.state_root_codec()
            || h.cumulative_receipt_root_codec != profile.receipt_root_codec()
        {
            bail!("finalized parent root codecs differ from approved fresh genesis profile");
        }
        let (state_root, receipt_root) = match profile {
            crate::native_root_codecs::NativeRootCodecProfileV1::LegacyWireV1 => (
                parse_fixed_hex_32_v1(
                    &native_semantic_ledger_state_digest_v1(&self.store.module_state),
                    "finalized parent legacy state root",
                )?,
                parse_fixed_hex_32_v1(
                    &native_execution_receipt_root_v2(&self.store)?,
                    "finalized parent legacy receipt root",
                )?,
            ),
            crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1 => (
                native_record_commitment::consensus_state_root_v1(&self.store.module_state)?,
                native_record_commitment::cumulative_receipt_root_v1(&self.store)?,
            ),
        };
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
            || state_root != h.post_state_root
            || receipt_root != h.cumulative_receipt_root
            || self.store.module_state.aoem_semantic_ledger_sequence != h.state_version
            || plan.aoem_parent.as_ref() != Some(&expected)
            || plan.pre_state_root != h.post_state_root
            || h.height.checked_add(1) != Some(plan.context.block_height)
            || plan.context.parent_block_hash != h.block_hash
            || plan.context.slot <= h.slot
            || plan.context.timestamp_unix_ms < h.timestamp_unix_ms
        {
            bail!("finalized parent state, proof and successor plan disagree");
        }
        Ok(())
    }
}

/// Stage the next height from the currently published, finalized block.
/// The original API name is retained for callers; no height is inferred from it.
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
    {
        // Authentication precedes maintenance. Recheck the live parent after
        // reacquiring the workspace; cleanup never grants a stale plan authority.
        drop(workspace);
        retire_old_workspaces_v1(
            plan.context.chain_id,
            parent_workspace_id,
            genesis_commitment,
            params,
        )?;
        workspace = WorkspaceStore::open(plan.context.chain_id, params)?;
        let current = execution::capture_finalized_parent_locked(
            &mut workspace,
            parent_workspace_id,
            genesis_commitment,
            params,
        )?;
        if current.output_digest() != parent.output_digest()
            || current.block() != parent.block()
            || current.successor_plan(plan.context, plan.raw_txs.clone(), params)? != *plan
        {
            bail!("successor parent changed during workspace retirement");
        }
    }
    let id = workspace_id(&workspace.scope, &plan.plan_commitment);
    let existing_version = workspace
        .catalog()?
        .into_iter()
        .find_map(|(_, descriptor)| (descriptor.id == id).then_some(descriptor.version));
    // Old reservations retain their exact input schema and parent digest. A
    // physical-only historical output or mixed Execute plan is explicitly cold.
    let has_three_roots = parent
        .record_state
        .as_ref()
        .map(state_records::StoreRef::rooted_parts)
        .transpose()?
        .flatten()
        .is_some();
    if existing_version != Some(DescriptorVersion::Ncw1)
        && parent.genesis_config().root_codec_profile()?
            == crate::native_root_codecs::NativeRootCodecProfileV1::RecordTreeV1
        && has_three_roots
        && plan_contains_only_transfers(plan)?
    {
        let payload = LightPayload {
            schema: LIGHT_SCHEMA.into(),
            plan: plan.clone(),
            finalized_parent: rooted_parent::RootedParentSnapshot::capture_from_verified_full(
                &parent, &workspace, params,
            )?,
            record_state: parent.record_state.clone(),
        };
        if let Some(info) = stage_light_payload(&mut workspace, &payload, |_| Ok(()))? {
            return Ok(info);
        }
        // Only the typed pre-reservation 8 MiB error can reach this branch.
        // No invalid reference, partial NCW2 or storage error may fall back.
    } else if existing_version == Some(DescriptorVersion::Ncw2) {
        bail!("existing NCW2 input cannot be replayed through a cold parent path");
    }
    let payload = Payload {
        schema: SCHEMA.to_owned(),
        plan: plan.clone(),
        parent_block: None,
        parent_snapshot: None,
        genesis: None,
        finalized_parent: Some(FinalizedParentSnapshot::capture(&parent)),
        record_state: parent.record_state.clone(),
    };
    validate_payload(&payload, &workspace)?;
    stage_payload(&mut workspace, &payload, |_| Ok(()))
}
