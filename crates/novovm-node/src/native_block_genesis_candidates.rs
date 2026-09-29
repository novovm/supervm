//! First candidates under a pinned fresh genesis; no selected head or signing.
use super::*;

fn validate_first(
    block: &NovNativeDurableBlockV1,
    config: &FreshGenesisConfigV1,
    initial_root: [u8; 32],
) -> Result<()> {
    validate_durable_block_v1(block)?;
    if block.header.chain_id != config.chain_id
        || block.header.height != 1
        || block.header.parent_block_hash != [0; 32]
        || block.header.aoem_parent.is_some()
        || block.header.pre_state_root != initial_root
        || block.header.timestamp_unix_ms < config.timestamp_unix_ms
        || block.header.state_version != 1
    {
        bail!("isolated first candidate does not extend the pinned fresh genesis");
    }
    Ok(())
}

/// Validate the complete graph and return an exact key allowlist. Reject orphan
/// artifacts/pins/index entries, selected state and capability downgrades rather
/// than accepting arbitrary keys merely because they have a candidate prefix.
pub(super) fn validated_keys(
    ledger: &NovNativeBlockLedgerV1,
    config: &FreshGenesisConfigV1,
) -> Result<Vec<Vec<u8>>> {
    let chain = config.chain_id;
    let initial_root = config.compile()?.state_root();
    let height = ledger
        .load_candidate_height_index_inner_v1(chain, 1)?
        .context("fresh genesis candidate height index missing")?;
    let children = ledger
        .load_candidate_children_index_inner_v1(chain, [0; 32])?
        .context("fresh genesis candidate children index missing")?;
    if height.block_hashes.is_empty() || height.block_hashes != children.block_hashes {
        bail!("fresh genesis candidate indexes disagree or are empty");
    }
    let mut keys = vec![
        KEY_CANDIDATE_GRAPH_SCHEMA_V1.to_vec(),
        candidate_height_index_key_v1(chain, 1).into_bytes(),
        candidate_children_index_key_v1(chain, &[0; 32]).into_bytes(),
    ];
    for hash in height.block_hashes {
        let record = ledger
            .load_candidate_record_inner_v1(chain, hash)?
            .context("fresh genesis candidate record missing")?;
        if record.candidate_source != CANDIDATE_SOURCE_ISOLATED_V1
            || record.execution_selected_local
            || record.lifecycle_status != CANDIDATE_STATUS_ACTIVE_V1
        {
            bail!("fresh genesis graph contains an unsupported candidate state");
        }
        let block = ledger
            .load_candidate_block_for_record_inner_v1(&record)?
            .context("fresh genesis candidate artifact missing")?;
        validate_first(&block, config, initial_root)?;
        keys.push(candidate_record_key_v1(chain, &hash).into_bytes());
        keys.push(candidate_artifact_key_v1(chain, &hash).into_bytes());
        keys.push(isolated_candidate::pin_key(chain, &hash).into_bytes());
    }
    Ok(keys)
}

impl NovNativeBlockLedgerV1 {
    pub(crate) fn fresh_genesis_seal_config_v1(
        &self,
        chain: u64,
    ) -> Result<Option<(&FreshGenesisConfigV1, [u8; 32])>> {
        self.ensure_schema_v1()?;
        match &self.fresh_genesis_seal_scope {
            Some((config, namespace)) if config.chain_id == chain => Ok(Some((config, *namespace))),
            Some(_) => bail!("fresh genesis signing chain mismatch"),
            None => Ok(None),
        }
    }

    /// Coordinator holds workspace and authority locks; this method holds the
    /// ledger lock through the callback. The read-only view cannot escape it.
    pub(crate) fn with_fresh_genesis_seal_scope_v1<T>(
        path: &Path,
        expected: [u8; 32],
        namespace: [u8; 32],
        block: &NovNativeDurableBlockV1,
        binding: &NovNativeIsolatedExecutionBindingV1,
        action: impl FnOnce(&Self) -> Result<T>,
    ) -> Result<T> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("first signing candidate requires existing ledger")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("genesis signing ledger lock poisoned"))?;
        let config = load_verified(&ledger, expected, namespace)?;
        let record = ledger
            .load_candidate_record_inner_v1(config.chain_id, block.header.block_hash)?
            .context("first signing candidate is not registered")?;
        if record.isolated_execution_binding.as_ref() != Some(binding)
            || ledger
                .load_candidate_block_for_record_inner_v1(&record)?
                .as_ref()
                != Some(block)
        {
            bail!("first signing candidate does not match verified live execution");
        }
        let view = Self {
            path: ledger.path.clone(),
            db: Arc::clone(&ledger.db),
            write_lock: Arc::clone(&ledger.write_lock),
            read_only: true,
            isolated_seal_scope: Some(record),
            fresh_genesis_seal_scope: Some((config, namespace)),
        };
        action(&view)
    }

    /// Workspace coordinator only, under workspace then authority OS locks,
    /// after verifying live genesis and complete AOEM candidate output. Records
    /// historical execution; it never grants signing or selects an execution.
    pub(crate) fn register_fresh_genesis_candidate_v1(
        path: &Path,
        expected: [u8; 32],
        namespace: [u8; 32],
        block: NovNativeDurableBlockV1,
        binding: NovNativeIsolatedExecutionBindingV1,
    ) -> Result<NovNativeBlockCandidateRecordV1> {
        binding.validate()?;
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("fresh genesis candidate requires a reserved ledger")?;
        // Require an existing database before obtaining a writable handle;
        // validate its complete reservation under the write lock below.
        drop(ledger);
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("genesis candidate ledger lock poisoned"))?;
        let config = load_verified(&ledger, expected, namespace)?;
        validate_first(&block, &config, config.compile()?.state_root())?;
        let chain = config.chain_id;
        let hash = block.header.block_hash;
        if let Some(record) = ledger.load_candidate_record_inner_v1(chain, hash)? {
            if record.isolated_execution_binding.as_ref() != Some(&binding)
                || ledger
                    .load_candidate_block_for_record_inner_v1(&record)?
                    .as_ref()
                    != Some(&block)
            {
                bail!("fresh genesis candidate registration cannot replace pinned execution");
            }
            return Ok(record);
        }
        let mut record =
            candidate_record_from_block_v1(&block, CANDIDATE_SOURCE_OBSERVED_V1, false, false)?;
        record.candidate_source = CANDIDATE_SOURCE_ISOLATED_V1.to_string();
        record.local_aoem_readback_verified = true;
        record.isolated_execution_binding = Some(binding.clone());
        let mut batch = RocksDbWriteBatch::default();
        batch.put(KEY_SCHEMA_V1, CANDIDATES_SCHEMA.as_bytes());
        put_json_v1(
            &mut batch,
            candidate_artifact_key_v1(chain, &hash).as_bytes(),
            &block,
            "fresh genesis candidate artifact",
        )?;
        put_json_v1(
            &mut batch,
            isolated_candidate::pin_key(chain, &hash).as_bytes(),
            &binding,
            "fresh genesis candidate binding",
        )?;
        ledger.stage_candidate_graph_record_v1(&mut batch, &record)?;
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, expected, namespace)?;
        let readback = ledger
            .load_candidate_record_inner_v1(chain, hash)?
            .context("fresh genesis candidate readback missing")?;
        if readback != record {
            bail!("fresh genesis candidate readback mismatch");
        }
        Ok(readback)
    }
}
