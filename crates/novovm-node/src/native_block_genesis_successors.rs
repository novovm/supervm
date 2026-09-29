//! Unselected height-two candidates under a finalized fresh first block.
use super::*;

pub(super) fn parent(ledger: &NovNativeBlockLedgerV1) -> Result<NovNativeDurableBlockV1> {
    let intent = promotion::read(ledger)?;
    let record = ledger
        .load_candidate_record_inner_v1(intent.chain_id, intent.block_hash)?
        .context("finalized parent record missing")?;
    ledger
        .load_candidate_block_for_record_inner_v1(&record)?
        .context("finalized parent body missing")
}

pub(super) fn validate_child(
    parent: &NovNativeDurableBlockV1,
    child: &NovNativeDurableBlockV1,
) -> Result<()> {
    validate_durable_block_v1(child)?;
    let p = &parent.header;
    let h = &child.header;
    let expected = NovNativePreparedAoemParentV1 {
        batch_id: p.aoem_batch_id.clone(),
        batch_result_id: p.aoem_batch_result_id.clone(),
        state_root: p.post_state_root,
        state_root_codec: p.post_state_root_codec.clone(),
        cumulative_receipt_root: p.cumulative_receipt_root,
        receipt_root_codec: p.cumulative_receipt_root_codec.clone(),
        state_version: p.state_version,
    };
    if p.height != 1
        || h.height != 2
        || h.chain_id != p.chain_id
        || h.parent_block_hash != p.block_hash
        || h.pre_state_root != p.post_state_root
        || h.aoem_parent.as_ref() != Some(&expected)
        || p.state_version.checked_add(u64::from(h.tx_count)) != Some(h.state_version)
        || h.slot <= p.slot
        || h.timestamp_unix_ms < p.timestamp_unix_ms
    {
        bail!("fresh successor does not extend the finalized parent");
    }
    Ok(())
}

pub(super) fn validated_keys(ledger: &NovNativeBlockLedgerV1) -> Result<Vec<Vec<u8>>> {
    let parent = parent(ledger)?;
    let chain = parent.header.chain_id;
    let height = ledger.load_candidate_height_index_inner_v1(chain, 2)?;
    let children =
        ledger.load_candidate_children_index_inner_v1(chain, parent.header.block_hash)?;
    let (height, children) = match (height, children) {
        (None, None) => return Ok(Vec::new()),
        (Some(height), Some(children)) => (height, children),
        _ => bail!("fresh successor index missing"),
    };
    if height.block_hashes.is_empty() || height.block_hashes != children.block_hashes {
        bail!("fresh successor indexes disagree");
    }
    let mut keys = vec![
        candidate_height_index_key_v1(chain, 2).into_bytes(),
        candidate_children_index_key_v1(chain, &parent.header.block_hash).into_bytes(),
    ];
    for hash in height.block_hashes {
        let record = ledger
            .load_candidate_record_inner_v1(chain, hash)?
            .context("fresh successor record missing")?;
        if record.candidate_source != CANDIDATE_SOURCE_ISOLATED_V1
            || record.execution_selected_local
            || record.lifecycle_status != CANDIDATE_STATUS_ACTIVE_V1
        {
            bail!("fresh successor has unsupported selection or lifecycle state");
        }
        let block = ledger
            .load_candidate_block_for_record_inner_v1(&record)?
            .context("fresh successor artifact missing")?;
        validate_child(&parent, &block)?;
        keys.push(candidate_record_key_v1(chain, &hash).into_bytes());
        keys.push(candidate_artifact_key_v1(chain, &hash).into_bytes());
        keys.push(isolated_candidate::pin_key(chain, &hash).into_bytes());
    }
    Ok(keys)
}

impl NovNativeBlockLedgerV1 {
    pub(crate) fn fresh_successor_parent_workspace_v1(&self) -> Result<Option<[u8; 32]>> {
        self.ensure_schema_v1()?;
        if self.fresh_successor_parent_target.is_none() {
            return Ok(None);
        }
        Ok(Some(promotion::read(self)?.execution.workspace_id))
    }

    pub(crate) fn fresh_successor_parent_target_v1(&self) -> Result<Option<[u8; 32]>> {
        self.ensure_schema_v1()?;
        Ok(self.fresh_successor_parent_target)
    }

    /// Coordinator holds live workspace/authority locks throughout this callback.
    pub(crate) fn with_fresh_successor_seal_scope_v1<T>(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        block: &NovNativeDurableBlockV1,
        binding: &NovNativeIsolatedExecutionBindingV1,
        action: impl FnOnce(&Self) -> Result<T>,
    ) -> Result<T> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor signing ledger missing")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("successor signing lock poisoned"))?;
        let config = load_verified(&ledger, genesis, namespace)?;
        if ledger.db.get(KEY_SCHEMA_V1)?.as_deref() != Some(FINALIZED_SCHEMA.as_bytes()) {
            bail!("successor signing requires finalized first block");
        }
        let parent = parent(&ledger)?;
        validate_child(&parent, block)?;
        let target = finality::read(&ledger)?.validated_decision_target(&config, &parent)?;
        let record = ledger
            .load_candidate_record_inner_v1(config.chain_id, block.header.block_hash)?
            .context("successor signing candidate is not registered")?;
        if record.isolated_execution_binding.as_ref() != Some(binding)
            || ledger
                .load_candidate_block_for_record_inner_v1(&record)?
                .as_ref()
                != Some(block)
        {
            bail!("successor signing scope differs from verified AOEM output");
        }
        let view = Self {
            path: ledger.path.clone(),
            db: Arc::clone(&ledger.db),
            write_lock: Arc::clone(&ledger.write_lock),
            read_only: true,
            isolated_seal_scope: Some(record),
            fresh_genesis_seal_scope: Some((config, namespace)),
            fresh_successor_parent_target: Some(target),
        };
        action(&view)
    }

    /// Only the workspace coordinator may call this while holding live-parent
    /// workspace and authority locks. This does not authorize any signature.
    pub(crate) fn register_fresh_successor_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        block: NovNativeDurableBlockV1,
        binding: NovNativeIsolatedExecutionBindingV1,
    ) -> Result<NovNativeBlockCandidateRecordV1> {
        binding.validate()?;
        let existing = Self::open_existing_read_only_inner_v1(path, true)?
            .context("fresh successor ledger missing")?;
        drop(existing);
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("successor ledger lock poisoned"))?;
        load_verified(&ledger, genesis, namespace)?;
        if ledger.db.get(KEY_SCHEMA_V1)?.as_deref() != Some(FINALIZED_SCHEMA.as_bytes()) {
            bail!("fresh successor requires finalized first-block ledger");
        }
        validate_child(&parent(&ledger)?, &block)?;
        let chain = block.header.chain_id;
        let hash = block.header.block_hash;
        if let Some(record) = ledger.load_candidate_record_inner_v1(chain, hash)? {
            if record.isolated_execution_binding.as_ref() != Some(&binding)
                || ledger
                    .load_candidate_block_for_record_inner_v1(&record)?
                    .as_ref()
                    != Some(&block)
            {
                bail!("fresh successor cannot replace pinned execution");
            }
            return Ok(record);
        }
        let mut record =
            candidate_record_from_block_v1(&block, CANDIDATE_SOURCE_OBSERVED_V1, false, false)?;
        record.candidate_source = CANDIDATE_SOURCE_ISOLATED_V1.into();
        record.local_aoem_readback_verified = true;
        record.isolated_execution_binding = Some(binding.clone());
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(
            &mut batch,
            candidate_artifact_key_v1(chain, &hash).as_bytes(),
            &block,
            "fresh successor artifact",
        )?;
        put_json_v1(
            &mut batch,
            isolated_candidate::pin_key(chain, &hash).as_bytes(),
            &binding,
            "fresh successor binding",
        )?;
        ledger.stage_candidate_graph_record_v1(&mut batch, &record)?;
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, genesis, namespace)?;
        let stored = ledger
            .load_candidate_record_inner_v1(chain, hash)?
            .context("successor readback missing")?;
        if stored != record {
            bail!("successor readback changed");
        }
        Ok(stored)
    }
}
