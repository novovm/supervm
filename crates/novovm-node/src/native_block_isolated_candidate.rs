//! Immutable local AOEM workspace binding. Does not grant signing or promotion.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Historical registration metadata, not proof that a workspace is still live.
pub struct NovNativeIsolatedExecutionBindingV1 {
    pub workspace_id: [u8; 32],
    pub plan_commitment: [u8; 32],
    pub output_digest: [u8; 32],
}

impl NovNativeIsolatedExecutionBindingV1 {
    pub(super) fn validate(&self) -> Result<()> {
        if self.workspace_id == [0; 32]
            || self.plan_commitment == [0; 32]
            || self.output_digest == [0; 32]
        {
            bail!("isolated execution binding contains a zero commitment");
        }
        Ok(())
    }
}

fn pin_key(chain: u64, hash: &[u8; 32]) -> String {
    format!(
        "{}/isolated-execution-binding",
        candidate_record_key_v1(chain, hash)
    )
}

impl NovNativeBlockLedgerV1 {
    /// The caller holds workspace and authority OS locks and has re-read both
    /// complete AOEM output and the current parent. The borrowed view cannot
    /// escape the callback; graph mutation is fenced for its entire lifetime.
    pub(crate) fn with_isolated_seal_scope_v1<T>(
        &self,
        block: &NovNativeDurableBlockV1,
        binding: &NovNativeIsolatedExecutionBindingV1,
        action: impl FnOnce(&Self) -> Result<T>,
    ) -> Result<T> {
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        let record = self
            .load_candidate_record_inner_v1(block.header.chain_id, block.header.block_hash)?
            .context("isolated signing candidate is not registered")?;
        if record.candidate_source != CANDIDATE_SOURCE_ISOLATED_V1
            || record.lifecycle_status != CANDIDATE_STATUS_ACTIVE_V1
            || record.isolated_execution_binding.as_ref() != Some(binding)
            || self
                .load_candidate_block_for_record_inner_v1(&record)?
                .as_ref()
                != Some(block)
        {
            bail!("isolated signing candidate no longer matches live execution");
        }
        let head = self
            .load_head_inner_v1(record.chain_id)?
            .context("isolated signing parent missing")?;
        if head.block_hash != record.parent_block_hash
            || head.height.checked_add(1) != Some(record.height)
            || self.load_prepared_inner_v1(record.chain_id)?.is_some()
        {
            bail!("isolated signing parent changed or authority preparation is unresolved");
        }
        let view = Self {
            path: self.path.clone(),
            db: Arc::clone(&self.db),
            write_lock: Arc::clone(&self.write_lock),
            read_only: true,
            isolated_seal_scope: Some(record),
        };
        action(&view)
    }

    pub(super) fn reject_orphaned_isolated_pin_v1(&self, chain: u64, hash: [u8; 32]) -> Result<()> {
        if self.db.get(pin_key(chain, &hash).as_bytes())?.is_some() {
            bail!("isolated candidate record disappeared while its evidence pin remains");
        }
        Ok(())
    }
    pub(super) fn verify_isolated_binding_pin_v1(
        &self,
        record: &NovNativeBlockCandidateRecordV1,
    ) -> Result<()> {
        if record.isolated_execution_binding.is_some()
            && self.db.get(KEY_SCHEMA_V1)?.as_deref() != Some(ISOLATED_LEDGER_SCHEMA_V1.as_bytes())
        {
            bail!("isolated candidate database capability marker was downgraded");
        }
        let pin = read_json_v1::<NovNativeIsolatedExecutionBindingV1>(
            &self.db,
            pin_key(record.chain_id, &record.block_hash).as_bytes(),
            "isolated execution pin",
        )?;
        if pin != record.isolated_execution_binding {
            bail!("isolated candidate binding or pin disappeared or changed");
        }
        Ok(())
    }

    /// Only the workspace loader calls this while holding its OS lock and the
    /// authority lock, after a complete AOEM readback. No remote/RPC entrypoint.
    pub(crate) fn register_isolated_candidate_v1(
        &self,
        block: NovNativeDurableBlockV1,
        binding: NovNativeIsolatedExecutionBindingV1,
        namespace: &str,
        protocol: &str,
    ) -> Result<NovNativeBlockCandidateRecordV1> {
        validate_durable_block_v1(&block)?;
        binding.validate()?;
        let _guard = self.lock_writes_v1()?;
        self.ensure_schema_v1()?;
        let ownership = self
            .load_aoem_ownership()?
            .context("isolated candidate ledger ownership missing")?;
        if ownership.chain_id != block.header.chain_id
            || ownership.namespace_digest != namespace
            || ownership.protocol_config_commitment != protocol
        {
            bail!("isolated candidate ledger ownership mismatch");
        }
        let chain = block.header.chain_id;
        let hash = block.header.block_hash;
        if self.load_prepared_inner_v1(chain)?.is_some() {
            bail!("isolated registration refuses unresolved authoritative execution");
        }
        let existing = self.load_candidate_record_inner_v1(chain, hash)?;
        if let Some(record) = &existing {
            if record.lifecycle_status != CANDIDATE_STATUS_ACTIVE_V1 {
                bail!("isolated registration cannot revive an aborted candidate");
            }
            if self
                .load_candidate_block_for_record_inner_v1(record)?
                .as_ref()
                != Some(&block)
            {
                bail!("isolated registration conflicts with stored artifact");
            }
            if record.candidate_source == CANDIDATE_SOURCE_ISOLATED_V1 {
                if record.isolated_execution_binding.as_ref() != Some(&binding) {
                    bail!("isolated execution binding is immutable");
                }
                return Ok(record.clone());
            }
            if record.candidate_source != CANDIDATE_SOURCE_OBSERVED_V1 {
                bail!("isolated registration cannot replace selected execution");
            }
        }
        if read_json_v1::<NovNativeIsolatedExecutionBindingV1>(
            &self.db,
            pin_key(chain, &hash).as_bytes(),
            "isolated pin before registration",
        )?
        .is_some()
        {
            bail!("isolated binding exists without its original record");
        }
        let head = self
            .load_head_inner_v1(chain)?
            .context("isolated candidate requires a local parent")?;
        if head.block_hash != block.header.parent_block_hash
            || head.height.checked_add(1) != Some(block.header.height)
        {
            bail!("isolated registration parent is not current local execution head");
        }
        self.validate_candidate_parent_inner_v1(&block)?;
        if let Some(old) = read_json_v1::<NovNativeDurableBlockV1>(
            &self.db,
            candidate_artifact_key_v1(chain, &hash).as_bytes(),
            "existing isolated artifact",
        )? {
            if old != block {
                bail!("isolated candidate artifact changed");
            }
        }
        let mut record =
            candidate_record_from_block_v1(&block, CANDIDATE_SOURCE_OBSERVED_V1, false, false)?;
        record.candidate_source = CANDIDATE_SOURCE_ISOLATED_V1.to_string();
        record.local_aoem_readback_verified = true;
        record.isolated_execution_binding = Some(binding.clone());
        if let Some(old) = existing {
            record.revision = old
                .revision
                .checked_add(1)
                .context("candidate revision overflow")?;
        }
        validate_candidate_record_v1(&record)?;
        let mut batch = RocksDbWriteBatch::default();
        // Old binaries reject this marker at open/prepare, before authority
        // mutation. Registration is the explicit, one-way opt-in boundary.
        batch.put(KEY_SCHEMA_V1, ISOLATED_LEDGER_SCHEMA_V1.as_bytes());
        put_json_v1(
            &mut batch,
            candidate_artifact_key_v1(chain, &hash).as_bytes(),
            &block,
            "isolated block artifact",
        )?;
        put_json_v1(
            &mut batch,
            pin_key(chain, &hash).as_bytes(),
            &binding,
            "isolated execution pin",
        )?;
        self.stage_candidate_graph_record_v1(&mut batch, &record)?;
        write_sync_v1(&self.db, batch)?;
        let readback = self
            .load_candidate_record_inner_v1(chain, hash)?
            .context("isolated registration readback missing")?;
        if readback != record {
            bail!("isolated registration readback mismatch");
        }
        Ok(readback)
    }
}
