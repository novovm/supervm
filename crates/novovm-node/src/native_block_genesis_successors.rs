//! Unselected next-height candidates under the current finalized fresh block.
use super::*;

pub(super) struct FinalizedRecord {
    pub(super) block: NovNativeDurableBlockV1,
    pub(super) proof: NovNativeFreshFinalityProofV1,
    pub(super) execution: NovNativeIsolatedExecutionBindingV1,
    pub(super) commitment: [u8; 32],
}

pub(crate) struct FinalizedWorkspaceTipV1 {
    pub(crate) current: [u8; 32],
    pub(crate) previous: Option<[u8; 32]>,
}

pub(crate) struct FreshStartupTipV1 {
    pub(crate) block: NovNativeDurableBlockV1,
    pub(crate) execution: NovNativeIsolatedExecutionBindingV1,
    pub(crate) previous: Option<[u8; 32]>,
    pub(crate) authority: crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1,
    pub(crate) pending_promotion: bool,
}

pub(super) fn record_at(ledger: &NovNativeBlockLedgerV1, height: u64) -> Result<FinalizedRecord> {
    let (hash, execution, commitment, proof) = if height == 1 {
        let intent = promotion::read(ledger)?;
        (
            intent.block_hash,
            intent.execution.clone(),
            intent.commitment()?,
            finality::read(ledger)?,
        )
    } else {
        let intent = successor_finality::read_archive(ledger, height)?
            .context("finalized predecessor archive missing")?;
        let crate::native_block_seal::round_message::NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } = &intent.proof.witness else {
            bail!("finalized predecessor witness missing");
        };
        (
            decision.prepare.subject.block_hash,
            intent.execution.clone(),
            intent.commitment()?,
            intent.proof,
        )
    };
    let chain = proof.authority.chain_id;
    let record = ledger
        .load_candidate_record_inner_v1(chain, hash)?
        .context("finalized parent record missing")?;
    let block = ledger
        .load_candidate_block_for_record_inner_v1(&record)?
        .context("finalized parent body missing")?;
    if block.header.height != height
        || record.isolated_execution_binding.as_ref() != Some(&execution)
    {
        bail!("finalized parent record differs from archive");
    }
    Ok(FinalizedRecord {
        block,
        proof,
        execution,
        commitment,
    })
}

pub(super) fn tip_height(ledger: &NovNativeBlockLedgerV1) -> Result<u64> {
    let schema = ledger
        .db
        .get(KEY_SCHEMA_V1)?
        .context("ledger schema missing")?;
    if !has_successor_intent_schema(&schema) {
        return Ok(1);
    }
    let height = successor_promotion::read(ledger)?.height()?;
    if schema == SUCCESSOR_FINALIZED_SCHEMA.as_bytes() {
        Ok(height)
    } else {
        height.checked_sub(1).context("successor height underflow")
    }
}

pub(super) fn parent(ledger: &NovNativeBlockLedgerV1) -> Result<NovNativeDurableBlockV1> {
    Ok(record_at(ledger, tip_height(ledger)?)?.block)
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
    if p.height.checked_add(1) != Some(h.height)
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
    let mut parents = vec![record_at(ledger, 1)?.block];
    for (height, _) in successor_finality::archives(ledger)? {
        parents.push(record_at(ledger, height)?.block);
    }
    let mut keys = Vec::new();
    for parent in parents {
        keys.extend(candidate_keys(ledger, &parent)?);
    }
    Ok(keys)
}

fn candidate_keys(
    ledger: &NovNativeBlockLedgerV1,
    parent: &NovNativeDurableBlockV1,
) -> Result<Vec<Vec<u8>>> {
    let chain = parent.header.chain_id;
    let next = parent
        .header
        .height
        .checked_add(1)
        .context("candidate height overflow")?;
    let height = ledger.load_candidate_height_index_inner_v1(chain, next)?;
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
        candidate_height_index_key_v1(chain, next).into_bytes(),
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
        validate_child(parent, &block)?;
        keys.push(candidate_record_key_v1(chain, &hash).into_bytes());
        keys.push(candidate_artifact_key_v1(chain, &hash).into_bytes());
        keys.push(isolated_candidate::pin_key(chain, &hash).into_bytes());
    }
    Ok(keys)
}

impl NovNativeBlockLedgerV1 {
    pub(crate) fn finalized_service_tip_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        anchor_height: u64,
        anchor_hash: [u8; 32],
        anchor_id: [u8; 32],
        anchor_previous: Option<[u8; 32]>,
    ) -> Result<Option<FinalizedWorkspaceTipV1>> {
        let Some(tip) = Self::fresh_startup_tip_v1(
            path,
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
        if tip.pending_promotion {
            bail!("startup follow requires pending promotion recovery first");
        }
        Ok(Some(FinalizedWorkspaceTipV1 {
            current: tip.execution.workspace_id,
            previous: tip.previous,
        }))
    }

    pub(crate) fn fresh_startup_tip_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        anchor_height: u64,
        anchor_hash: [u8; 32],
        anchor_id: [u8; 32],
        anchor_previous: Option<[u8; 32]>,
    ) -> Result<Option<FreshStartupTipV1>> {
        let ledger =
            Self::open_existing_read_only_inner_v1(path, true)?.context("fresh ledger missing")?;
        load_verified(&ledger, genesis, namespace)?;
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("fresh schema missing")?;
        if !is_finalized_schema(&schema) {
            return Ok(None);
        }
        let height = tip_height(&ledger)?;
        if height < anchor_height {
            return Ok(None);
        }
        let anchor = record_at(&ledger, anchor_height)?;
        if anchor.block.header.block_hash != anchor_hash
            || anchor.execution.workspace_id != anchor_id
        {
            bail!("configured startup anchor is not in the finalized chain");
        }
        let previous_anchor = if anchor_height > 1 {
            Some(
                record_at(&ledger, anchor_height - 1)?
                    .execution
                    .workspace_id,
            )
        } else {
            None
        };
        if previous_anchor != anchor_previous {
            bail!("configured startup predecessor differs from finalized ancestry");
        }
        let pending_promotion = schema != FINALIZED_SCHEMA.as_bytes()
            && schema != SUCCESSOR_FINALIZED_SCHEMA.as_bytes();
        let current = record_at(&ledger, height)?;
        let previous = if height > 1 {
            Some(record_at(&ledger, height - 1)?.execution.workspace_id)
        } else {
            None
        };
        Ok(Some(FreshStartupTipV1 {
            block: current.block,
            execution: current.execution,
            authority: current.proof.authority,
            previous,
            pending_promotion,
        }))
    }

    pub(crate) fn fresh_startup_successor_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        parent_height: u64,
        parent_hash: [u8; 32],
        hash: [u8; 32],
    ) -> Result<(NovNativeDurableBlockV1, NovNativeIsolatedExecutionBindingV1)> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("startup ledger missing")?;
        load_verified(&ledger, genesis, namespace)?;
        if tip_height(&ledger)? != parent_height {
            bail!("startup finalized frontier changed");
        }
        let parent = record_at(&ledger, parent_height)?.block;
        if parent.header.block_hash != parent_hash {
            bail!("startup parent binding changed");
        }
        let record = ledger
            .load_candidate_record_inner_v1(parent.header.chain_id, hash)?
            .context("startup pinned candidate is not registered")?;
        if record.lifecycle_status != CANDIDATE_STATUS_ACTIVE_V1 {
            bail!("startup pinned candidate is closed");
        }
        let execution = record
            .isolated_execution_binding
            .clone()
            .context("startup execution binding missing")?;
        let block = ledger
            .load_candidate_block_for_record_inner_v1(&record)?
            .context("startup candidate body missing")?;
        validate_child(&parent, &block)?;
        Ok((block, execution))
    }

    pub(crate) fn verify_retirable_fresh_candidate_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        height: u64,
        binding: &NovNativeIsolatedExecutionBindingV1,
    ) -> Result<Option<([u8; 32], [u8; 32])>> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("retirement ledger missing")?;
        load_verified(&ledger, genesis, namespace)?;
        let canonical = record_at(&ledger, height)?;
        let chain = canonical.block.header.chain_id;
        let index = ledger
            .load_candidate_height_index_inner_v1(chain, height)?
            .context("retirement height index missing")?;
        for hash in index.block_hashes {
            let record = ledger
                .load_candidate_record_inner_v1(chain, hash)?
                .context("retirement record missing")?;
            if record.isolated_execution_binding.as_ref() == Some(binding) {
                return Ok(Some((hash, canonical.commitment)));
            }
        }
        Ok(None)
    }

    pub(crate) fn load_fresh_finalized_execution_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        height: u64,
    ) -> Result<(
        NovNativeIsolatedExecutionBindingV1,
        [u8; 32],
        NovNativeDurableBlockV1,
    )> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("finalized execution ledger missing")?;
        load_verified(&ledger, genesis, namespace)?;
        let record = record_at(&ledger, height)?;
        Ok((record.execution, record.commitment, record.block))
    }

    pub(crate) fn fresh_successor_parent_workspace_v1(&self) -> Result<Option<[u8; 32]>> {
        self.ensure_schema_v1()?;
        if self.fresh_successor_parent_target.is_none() {
            return Ok(None);
        }
        Ok(Some(
            record_at(self, tip_height(self)?)?.execution.workspace_id,
        ))
    }

    pub(crate) fn fresh_successor_parent_target_v1(&self) -> Result<Option<[u8; 32]>> {
        self.ensure_schema_v1()?;
        Ok(self.fresh_successor_parent_target)
    }

    pub(crate) fn fresh_successor_height_v1(&self) -> Result<Option<u64>> {
        self.ensure_schema_v1()?;
        if self.fresh_successor_parent_target.is_none() {
            return Ok(None);
        }
        let record = self
            .isolated_seal_scope
            .as_ref()
            .context("successor candidate scope missing")?;
        Ok(Some(record.height))
    }

    pub(crate) fn fresh_round_height_v1(&self) -> Result<Option<u64>> {
        self.ensure_schema_v1()?;
        match self.fresh_parent_round_height {
            Some(height) => Ok(Some(height)),
            None => self.fresh_successor_height_v1(),
        }
    }

    pub(crate) fn with_fresh_parent_round_scope_v1<T>(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        block: &NovNativeDurableBlockV1,
        binding: &NovNativeIsolatedExecutionBindingV1,
        action: impl FnOnce(&Self) -> Result<T>,
    ) -> Result<T> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("parent round ledger missing")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("parent round ledger lock poisoned"))?;
        let config = load_verified(&ledger, genesis, namespace)?;
        if !ledger.db.get(KEY_SCHEMA_V1)?.is_some_and(|schema| {
            schema == FINALIZED_SCHEMA.as_bytes() || schema == SUCCESSOR_FINALIZED_SCHEMA.as_bytes()
        }) {
            bail!("parent round requires a fully finalized frontier");
        }
        let parent = record_at(&ledger, tip_height(&ledger)?)?;
        if &parent.block != block || &parent.execution != binding {
            bail!("parent round differs from the live finalized execution");
        }
        parent.proof.validated_decision_target(&config, block)?;
        let height = block
            .header
            .height
            .checked_add(1)
            .context("parent round height overflow")?;
        let view = Self {
            path: ledger.path.clone(),
            db: Arc::clone(&ledger.db),
            write_lock: Arc::clone(&ledger.write_lock),
            read_only: true,
            isolated_seal_scope: None,
            fresh_successor_parent_target: None,
            fresh_parent_round_height: Some(height),
            fresh_genesis_seal_scope: Some((config, namespace)),
        };
        action(&view)
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
        if !ledger.db.get(KEY_SCHEMA_V1)?.is_some_and(|s| {
            s == FINALIZED_SCHEMA.as_bytes() || s == SUCCESSOR_FINALIZED_SCHEMA.as_bytes()
        }) {
            bail!("successor signing requires finalized current block");
        }
        let parent = parent(&ledger)?;
        validate_child(&parent, block)?;
        let target = record_at(&ledger, parent.header.height)?
            .proof
            .validated_decision_target(&config, &parent)?;
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
            fresh_parent_round_height: None,
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
        if !ledger.db.get(KEY_SCHEMA_V1)?.is_some_and(|s| {
            s == FINALIZED_SCHEMA.as_bytes() || s == SUCCESSOR_FINALIZED_SCHEMA.as_bytes()
        }) {
            bail!("fresh successor requires finalized current ledger");
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
