//! Historical finalized parent metadata, never a partial Store or live authority.
//! The exact published output bytes bind physical roots/statistics to the local
//! finalized execution archive. Validating a parent never replays ancestor deltas.

use super::*;
use crate::native_block_ledger::{
    NovNativeFreshFinalityProofV1, NovNativeIsolatedExecutionBindingV1,
};
use crate::native_root_codecs::NativeRootCodecProfileV1;
use serde_json::value::RawValue;

#[derive(Serialize, Deserialize)]
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
    /// The caller has cold-verified and captured the current finalized parent
    /// under the existing locks. This copies only its bounded output document,
    /// not its historical Store. It does not initialize or publish any state.
    pub(super) fn capture_from_verified_full(
        parent: &FinalizedGenesisParentV1,
        workspace: &WorkspaceStore,
        params: &serde_json::Value,
    ) -> Result<Self> {
        let native_path = resolve_native_execution_store_path_from_params_v1(params)
            .context("rooted parent capture requires an explicit native path")?;
        let ledger_path = nov_native_block_ledger_rocksdb_path_v1(&native_path);
        let reference = parent
            .record_state
            .as_ref()
            .context("rooted parent capture requires an existing three-root output")?;
        reference
            .rooted_parts()?
            .context("rooted parent capture cannot upgrade a physical-only output")?;
        let config = parent.genesis_config().clone();
        let genesis = config.compile()?.config_commitment();
        let namespace = parse_fixed_hex_32_v1(&workspace.namespace, "rooted parent namespace")?;
        let (binding, promotion_commitment, block) =
            NovNativeBlockLedgerV1::load_fresh_finalized_execution_v1(
                &ledger_path,
                genesis,
                namespace,
                parent.block().header.height,
            )?;
        if binding.workspace_id != parent.workspace_id()
            || binding.output_digest != parent.output_digest()
            || &block != parent.block()
        {
            bail!("rooted parent capture differs from the verified finalized execution");
        }
        let bytes = execution::read_completed_output_bytes(workspace, parent.workspace_id())?;
        if execution::output_digest(&bytes) != binding.output_digest {
            bail!("rooted parent capture output digest mismatch");
        }
        let source_output = RawValue::from_string(String::from_utf8(bytes.clone())?)?;
        if source_output.get().as_bytes() != bytes {
            bail!("rooted parent capture changed the original output bytes");
        }
        let snapshot = Self {
            config,
            block,
            proof: parent.finality_proof().clone(),
            binding,
            promotion_commitment,
            source_output,
            store: (),
        };
        let source = snapshot.verify_source(workspace, params)?;
        if source.rooted_parts()? != reference.rooted_parts()? {
            bail!("rooted parent captured source differs from the verified full parent roots");
        }
        Ok(snapshot)
    }

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
        plan.validate()?;
        let source = self.verify_source(workspace, params)?;
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

    fn verify_source(
        &self,
        workspace: &WorkspaceStore,
        params: &serde_json::Value,
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
        let native_path = resolve_native_execution_store_path_from_params_v1(params)
            .context("rooted parent verification requires an explicit native path")?;
        let ledger_path = nov_native_block_ledger_rocksdb_path_v1(&native_path);
        let archive = NovNativeBlockLedgerV1::load_fresh_finalized_archive_v1(
            &ledger_path,
            genesis,
            namespace,
            self.block.header.height,
        )?;
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
