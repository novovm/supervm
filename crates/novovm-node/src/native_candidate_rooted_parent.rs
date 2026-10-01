//! Historical finalized parent metadata, never a partial Store or live authority.
//! The exact published output bytes bind physical roots/statistics to the local
//! finalized execution archive. Validating a parent never replays ancestor deltas.

use super::*;
use crate::native_block_ledger::{
    NovNativeFreshFinalityProofV1, NovNativeIsolatedExecutionBindingV1,
};
use crate::native_root_codecs::NativeRootCodecProfileV1;
use serde_json::value::RawValue;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RootedParentSnapshot {
    pub(super) config: fresh_genesis::FreshGenesisConfigV1,
    pub(super) block: NovNativeDurableBlockV1,
    pub(super) proof: NovNativeFreshFinalityProofV1,
    pub(super) binding: NovNativeIsolatedExecutionBindingV1,
    pub(super) promotion_commitment: [u8; 32],
    /// Original DocV2/V3 bytes. Hashing an extracted StoreRef would not prove
    /// that it belonged to the output pinned by the existing publication.
    pub(super) source_output: Box<RawValue>,
    /// NCW2's replaceable null field, not a default/fabricated complete Store.
    pub(super) store: (),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishedOutput<T> {
    schema: String,
    workspace_id: [u8; 32],
    input_digest: [u8; 32],
    expected_output_commitment: String,
    batch_result: novovm_exec::NovovmAoemNativeTxBatchResultV1,
    store: T,
}

impl RootedParentSnapshot {
    /// Historical validation deliberately does not require this parent to remain
    /// the live head. Admission/signing/publication must separately recheck the
    /// exact current authority while holding the existing workspace/authority locks.
    pub(super) fn validate(
        &self,
        workspace: &WorkspaceStore,
        plan: &NovNativeCandidateExecutionPlanV1,
        normalized_ref: &state_records::StoreRef,
        params: &serde_json::Value,
    ) -> Result<()> {
        self.validate_with_archive(workspace, plan, normalized_ref, params, None)
    }

    /// The optional archive must have been fully verified during this same
    /// locked operation. It only replaces a duplicate historical ledger read;
    /// source identity, publication evidence, roots and successor binding are
    /// still checked below. Never retain it as future signing authority.
    pub(super) fn validate_with_archive(
        &self,
        workspace: &WorkspaceStore,
        plan: &NovNativeCandidateExecutionPlanV1,
        normalized_ref: &state_records::StoreRef,
        params: &serde_json::Value,
        verified_archive: Option<&crate::native_block_ledger::FinalizedRecordArchiveV1>,
    ) -> Result<()> {
        plan.validate()?;
        let source = match verified_archive {
            Some(archive) => self.verify_source_with_archive(workspace, params, archive)?,
            None => self.verify_source(workspace, params)?,
        };
        // NCW2 identity links point parent_root at root. The original output's
        // links refer to its preceding block, so compare roots/statistics rather
        // than incorrectly demanding that these two encodings match bytewise.
        if normalized_ref.rooted_parts()? != source.rooted_parts()? {
            bail!("rooted parent input reference differs from its published output");
        }
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
        if plan.context.chain_id != workspace.chain_id
            || plan.protocol_config_commitment != workspace.protocol
            || plan.aoem_parent.as_ref() != Some(&expected)
            || plan.pre_state_root != h.post_state_root
            || h.height.checked_add(1) != Some(plan.context.block_height)
            || plan.context.parent_block_hash != h.block_hash
            || plan.context.slot <= h.slot
            || plan.context.timestamp_unix_ms < h.timestamp_unix_ms
        {
            bail!("rooted finalized parent does not bind the successor plan");
        }
        Ok(())
    }

    /// Explicit compatibility boundary for Execute/export/promotion. This does
    /// materialize the complete Store and never hands out a sparse fake image.
    pub(super) fn materialize_cold(
        &self,
        workspace: &WorkspaceStore,
        params: &serde_json::Value,
    ) -> Result<finalized_parent::FinalizedParentSnapshot> {
        self.verify_source(workspace, params)?;
        let (output, _): (PublishedOutput<NovNativeExecutionStoreV1>, _) =
            state_records::decode(workspace, self.source_output.get().as_bytes(), &["store"])?;
        verify_native_nonce_identity_scheme_v2(&output.store)?;
        verify_production_native_execution_store_authority_domain_v2(
            &output.store,
            workspace.chain_id,
            &workspace.namespace,
        )?;
        verify_native_business_protocol_config_v1(&output.store)?;
        if output.store.module_state.aoem_semantic_ledger_sequence
            != self.block.header.state_version
        {
            bail!("rooted parent cold state version differs from its finalized block");
        }
        Ok(finalized_parent::FinalizedParentSnapshot {
            config: self.config.clone(),
            block: self.block.clone(),
            store: output.store,
            proof: self.proof.clone(),
        })
    }

    pub(super) fn verify_source(
        &self,
        workspace: &WorkspaceStore,
        params: &serde_json::Value,
    ) -> Result<state_records::StoreRef> {
        self.verify_source_inner(workspace, params, None)
    }

    /// Reuse an archive already fully verified by a ledger getter during this
    /// locked capture. It does not skip config, QC, publication or root checks;
    /// only the duplicate ledger-history read is avoided.
    pub(super) fn verify_source_with_archive(
        &self,
        workspace: &WorkspaceStore,
        params: &serde_json::Value,
        archive: &crate::native_block_ledger::FinalizedRecordArchiveV1,
    ) -> Result<state_records::StoreRef> {
        self.verify_source_inner(workspace, params, Some(archive))
    }

    fn verify_source_inner(
        &self,
        workspace: &WorkspaceStore,
        params: &serde_json::Value,
        verified_archive: Option<&crate::native_block_ledger::FinalizedRecordArchiveV1>,
    ) -> Result<state_records::StoreRef> {
        let compiled = self.config.compile()?;
        let profile = compiled.root_codec_profile();
        if profile != NativeRootCodecProfileV1::RecordTreeV1
            || self.config.chain_id != workspace.chain_id
            || self.config.protocol_config_commitment != workspace.protocol
            || self.block.header.chain_id != workspace.chain_id
            || self.block.header.post_state_root_codec != profile.state_root_codec()
            || self.block.header.cumulative_receipt_root_codec != profile.receipt_root_codec()
            || self.binding.workspace_id == [0; 32]
            || self.binding.plan_commitment == [0; 32]
            || self.binding.output_digest == [0; 32]
            || self.promotion_commitment == [0; 32]
        {
            bail!("rooted parent profile/domain/execution binding mismatch");
        }
        let bytes = self.source_output.get().as_bytes();
        if bytes.is_empty()
            || bytes.len() > MAX_PAYLOAD_BYTES_V1
            || execution::output_digest(bytes) != self.binding.output_digest
        {
            bail!("rooted parent source bytes differ from its published output digest");
        }
        crate::native_block_ledger::validate_durable_block_v1(&self.block)?;
        self.proof
            .validate_archived_certificate(&self.config, &self.block)?;
        let genesis = compiled.config_commitment();
        let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "rooted parent namespace")?;
        if native_aoem_owned_state_namespace_digest_v1(params, workspace.chain_id)
            != workspace.namespace
        {
            bail!("rooted parent lookup resolves to a different authority namespace");
        }
        let loaded_archive;
        let archive = if let Some(archive) = verified_archive {
            archive
        } else {
            let native_path = resolve_native_execution_store_path_from_params_v1(params)
                .context("rooted parent verification requires an explicit native path")?;
            loaded_archive = NovNativeBlockLedgerV1::load_fresh_finalized_archive_v1(
                &nov_native_block_ledger_rocksdb_path_v1(&native_path),
                genesis,
                namespace,
                self.block.header.height,
            )?;
            &loaded_archive
        };
        if serde_json::to_vec(&archive.config)? != serde_json::to_vec(&self.config)? {
            bail!("rooted parent configuration differs from its approved local archive");
        }
        if archive.execution != self.binding
            || archive.commitment != self.promotion_commitment
            || archive.block != self.block
            || archive.proof != self.proof
        {
            bail!("rooted parent differs from the immutable finalized execution archive");
        }
        // h evidence survives workspace retirement. The current mutable head is
        // deliberately not accepted as a substitute for missing historical evidence.
        let expected_publication = execution::publication_target_fields(
            if self.block.header.height == 1 {
                b"NVP1"
            } else {
                b"NVP2"
            },
            namespace,
            genesis,
            self.promotion_commitment,
            self.binding.workspace_id,
            self.binding.output_digest,
            &self.block,
        );
        if workspace
            .graph
            .get(&workspace.key(b'h', &self.binding.workspace_id))?
            .as_deref()
            != Some(expected_publication.as_slice())
        {
            bail!("rooted parent publication evidence is missing or changed");
        }
        let source = state_records::decode_published_output_metadata::<PublishedOutput<()>>(
            workspace,
            bytes,
            &["store"],
        )?;
        self.validate_output_metadata(&source.inline, &source.state)?;
        Ok(source.state)
    }

    /// The live capture also requires the retained current workspace's exact
    /// input bytes and plan. This is bounded document verification, not a Store
    /// decode, and does not recursively read predecessor workspace chunks.
    pub(super) fn validate_live_input(&self, workspace: &WorkspaceStore) -> Result<()> {
        let (slot, input) = workspace
            .catalog()?
            .into_iter()
            .find(|(_, input)| input.id == self.binding.workspace_id)
            .context("live parent input descriptor missing")?;
        if workspace.status(slot, &input)? != WorkspaceStatusV1::Ready
            || input.plan != self.binding.plan_commitment
        {
            bail!("live parent input is not ready or its plan differs from finality");
        }
        let mut bytes = Vec::with_capacity(input.len);
        for index in 0..input.len.div_ceil(CHUNK_BYTES) {
            let chunk = workspace
                .graph
                .get(&workspace.chunk_key(&input.id, index))?
                .context("live parent input chunk missing")?;
            if chunk.len() != CHUNK_BYTES.min(input.len - bytes.len()) {
                bail!("live parent input chunk length mismatch");
            }
            bytes.extend_from_slice(&chunk);
        }
        if input.version.payload_digest(&bytes) != input.payload {
            bail!("live parent input digest mismatch");
        }
        if input.version == DescriptorVersion::Ncw2 {
            // Recompute every NCW2 descriptor field, including parent_snapshot,
            // without validating/replaying ancestors or materializing a Store.
            let document = state_records::decode_metadata::<LightPayload>(
                workspace,
                &bytes,
                &["finalized_parent", "store"],
            )?;
            if input != describe_light(&document.inline, &bytes, &document.state, &workspace.scope)?
            {
                bail!("live parent NCW2 descriptor does not bind its complete input metadata");
            }
        }
        // NCW1's parent_snapshot digest includes the complete typed parent
        // Store and cannot be recomputed here without cold materialization.
        // Its original input bytes remain bound by the finalized output digest;
        // the new parent roots come from that output, not this legacy field.
        // Read only the inline plan. The source document and input descriptor
        // were pinned by the fully verified publication; never invent a Store.
        #[derive(Deserialize)]
        struct InlineDocument {
            inline: Box<RawValue>,
        }
        #[derive(Deserialize)]
        struct PlanMetadata {
            schema: String,
            plan: NovNativeCandidateExecutionPlanV1,
        }
        #[derive(Deserialize)]
        struct Version {
            schema: String,
        }
        let version: Version = serde_json::from_slice(&bytes)?;
        let metadata: PlanMetadata = match version.schema.as_str() {
            "novovm-candidate-record-document/v1" | "novovm-candidate-record-document/v2" => {
                let document: InlineDocument = serde_json::from_slice(&bytes)?;
                serde_json::from_str(document.inline.get())?
            }
            SCHEMA if input.version == DescriptorVersion::Ncw1 => serde_json::from_slice(&bytes)?,
            _ => bail!("unsupported live parent input document schema"),
        };
        let expected_schema = match input.version {
            DescriptorVersion::Ncw1 => SCHEMA,
            DescriptorVersion::Ncw2 => LIGHT_SCHEMA,
        };
        metadata.plan.validate_against_block(&self.block)?;
        if metadata.schema != expected_schema
            || metadata.plan.protocol_config_commitment != workspace.protocol
            || metadata.plan.plan_commitment != input.plan
            || metadata.plan.pre_state_root != input.parent_state
            || metadata.plan.context.parent_block_hash != input.parent_block
        {
            bail!("live parent input metadata differs from its descriptor or finalized block");
        }
        let output_document: InlineDocument = serde_json::from_str(self.source_output.get())?;
        let output: PublishedOutput<()> = serde_json::from_str(output_document.inline.get())?;
        if output.input_digest != input.payload {
            bail!("live parent output does not bind its exact input digest");
        }
        Ok(())
    }

    fn validate_output_metadata(
        &self,
        output: &PublishedOutput<()>,
        reference: &state_records::StoreRef,
    ) -> Result<()> {
        let (_, state_root, receipt_root, _, _) = reference
            .rooted_parts()?
            .context("rooted parent source output has no three-root bundle")?;
        let h = &self.block.header;
        let result = &output.batch_result;
        let metadata = &result.snapshot_metadata;
        if output.schema != execution::OUTPUT_SCHEMA
            || output.workspace_id != self.binding.workspace_id
            || output.input_digest == [0; 32]
            || output.expected_output_commitment != h.aoem_expected_output_commitment
            || result.batch_id != h.aoem_batch_id
            || result.batch_result_id != h.aoem_batch_result_id
            || parse_fixed_hex_32_v1(&result.state_delta_root, "rooted parent state")? != state_root
            || parse_fixed_hex_32_v1(&result.receipt_root, "rooted parent receipts")?
                != receipt_root
            || state_root != h.post_state_root
            || receipt_root != h.cumulative_receipt_root
            || metadata.snapshot_version != 2
            || metadata.state_version != h.state_version
            || metadata.persistence_owner != "aoem_runtime"
            || metadata.backend.trim().is_empty()
            || metadata.backend.trim().eq_ignore_ascii_case("none")
            || result.per_tx_receipts.len() != self.block.body.tx_hashes.len()
            || parse_fixed_hex_32_v1(
                &native_aoem_execution_evidence_with_profile_v1(
                    result,
                    NativeRootCodecProfileV1::RecordTreeV1,
                )?,
                "rooted parent execution evidence",
            )? != h.aoem_evidence_commitment
        {
            bail!("rooted parent output metadata differs from its finalized block");
        }
        Ok(())
    }
}
