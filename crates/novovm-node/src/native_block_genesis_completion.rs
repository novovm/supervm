//! Atomic first-block query projection after verified AOEM authority publication.
//! Immutable candidates and their signed headers are never rewritten as finalized.
use super::*;
use std::collections::BTreeMap;

const KEY_COMPLETED: &[u8] = b"native_block_ledger/v1/genesis/published-intent";

fn entries(ledger: &NovNativeBlockLedgerV1) -> Result<BTreeMap<Vec<u8>, Vec<u8>>> {
    let intent = promotion::read(ledger)?;
    let record = ledger
        .load_candidate_record_inner_v1(intent.chain_id, intent.block_hash)?
        .context("publication candidate missing")?;
    let block = ledger
        .load_candidate_block_for_record_inner_v1(&record)?
        .context("publication block missing")?;
    let h = &block.header;
    if h.height != 1 || h.parent_block_hash != [0; 32] {
        bail!("fresh publication supports only the first block");
    }
    let mut result = BTreeMap::new();
    result.insert(KEY_COMPLETED.to_vec(), intent.commitment()?.to_vec());
    result.insert(
        height_key_v1(h.chain_id, 1).into_bytes(),
        h.block_hash.to_vec(),
    );
    fn json<T: Serialize>(
        map: &mut BTreeMap<Vec<u8>, Vec<u8>>,
        key: String,
        value: &T,
    ) -> Result<()> {
        if map
            .insert(key.into_bytes(), serde_json::to_vec(value)?)
            .is_some()
        {
            bail!("duplicate fresh publication index");
        }
        Ok(())
    }
    json(&mut result, header_key_v1(h.chain_id, &h.block_hash), h)?;
    json(
        &mut result,
        body_key_v1(h.chain_id, &h.block_hash),
        &block.body,
    )?;
    json(
        &mut result,
        evidence_key_v1(h.chain_id, &h.block_hash),
        &block.execution_evidence,
    )?;
    let head = NovNativeBlockLedgerHeadV1 {
        schema: HEAD_SCHEMA_V1.into(),
        chain_id: h.chain_id,
        height: h.height,
        block_hash: h.block_hash,
        post_state_root: h.post_state_root,
        cumulative_receipt_root: h.cumulative_receipt_root,
        state_version: h.state_version,
        slot: h.slot,
        timestamp_unix_ms: h.timestamp_unix_ms,
        block_count: 1,
        cumulative_tx_count: u64::from(h.tx_count),
        cumulative_body_bytes: h.body_bytes,
        canonical_local: true,
        safe: false,
        finalized: false,
        proof_sealed: false,
    };
    json(&mut result, head_key_v1(h.chain_id), &head)?;
    for (i, (tx, receipt)) in block
        .body
        .tx_hashes
        .iter()
        .zip(&block.execution_evidence.per_block_receipt_commitments)
        .enumerate()
    {
        let tx_index = u32::try_from(i).context("publication tx index overflow")?;
        json(
            &mut result,
            tx_key_v1(h.chain_id, tx),
            &NovNativeBlockTxLocationV1 {
                schema: TX_LOCATION_SCHEMA_V1.into(),
                chain_id: h.chain_id,
                tx_hash: *tx,
                height: 1,
                block_hash: h.block_hash,
                tx_index,
                canonical_local: true,
            },
        )?;
        json(
            &mut result,
            receipt_key_v1(h.chain_id, tx),
            &NovNativeBlockReceiptLocationV1 {
                schema: RECEIPT_LOCATION_SCHEMA_V1.into(),
                chain_id: h.chain_id,
                tx_hash: *tx,
                height: 1,
                block_hash: h.block_hash,
                tx_index,
                receipt_commitment: *receipt,
                canonical_local: true,
                proof_sealed: false,
            },
        )?;
    }
    for (kind, id) in [
        ("batch", h.aoem_batch_id.as_str()),
        ("result", h.aoem_batch_result_id.as_str()),
    ] {
        json(
            &mut result,
            external_id_key_v1(h.chain_id, kind, id),
            &NovNativeBlockExternalIdIndexV1 {
                schema: EXTERNAL_ID_INDEX_SCHEMA_V1.into(),
                id_kind: kind.into(),
                exact_id: id.into(),
                chain_id: h.chain_id,
                height: 1,
                block_hash: h.block_hash,
            },
        )?;
    }
    Ok(result)
}

pub(super) fn validated_keys(ledger: &NovNativeBlockLedgerV1) -> Result<Vec<Vec<u8>>> {
    let expected = entries(ledger)?;
    for (key, bytes) in &expected {
        if ledger.db.get(key)?.as_deref() != Some(bytes.as_slice()) {
            bail!("fresh published ledger projection missing or changed; refusing repair");
        }
    }
    let intent = promotion::read(ledger)?;
    ledger
        .load_head_verified_inner_v1(intent.chain_id)?
        .context("published head missing")?;
    Ok(expected.into_keys().collect())
}

impl NovNativeBlockLedgerV1 {
    /// Read the fully verified first-block ledger projection. This checks durable
    /// ledger evidence, not live AOEM availability or chain-level finality.
    pub fn load_fresh_genesis_published_block_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<Option<NovNativeDurableBlockV1>> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("fresh published ledger missing")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("publication read lock poisoned"))?;
        load_verified(&ledger, genesis, namespace)?;
        if ledger.db.get(KEY_SCHEMA_V1)?.as_deref() != Some(PUBLISHED_SCHEMA.as_bytes()) {
            return Ok(None);
        }
        let intent = promotion::read(&ledger)?;
        ledger.load_by_hash_inner_v1(intent.chain_id, intent.block_hash)
    }

    /// Caller holds both AOEM locks and has just fully verified the published
    /// authority pointer/output. No public API accepts a claimed readback boolean.
    pub(crate) fn complete_fresh_genesis_ledger_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        expected_intent: &NovNativeFreshPromotionIntentV1,
    ) -> Result<()> {
        let existing = Self::open_existing_read_only_inner_v1(path, true)?
            .context("completion requires an existing ledger")?;
        drop(existing);
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("completion lock poisoned"))?;
        load_verified(&ledger, genesis, namespace)?;
        if promotion::read(&ledger)? != *expected_intent {
            bail!("completion intent differs from verified AOEM authority");
        }
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("completion schema missing")?;
        if schema == PUBLISHED_SCHEMA.as_bytes() {
            return Ok(());
        }
        if schema != PROMOTION_SCHEMA.as_bytes() {
            bail!("completion requires a promotion intent");
        }
        let mut batch = RocksDbWriteBatch::default();
        for (key, bytes) in entries(&ledger)? {
            if ledger.db.get(&key)?.is_some() {
                bail!("completion would overwrite existing ledger state");
            }
            batch.put(key, bytes);
        }
        batch.put(KEY_SCHEMA_V1, PUBLISHED_SCHEMA.as_bytes());
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, genesis, namespace)?;
        Ok(())
    }

    pub(crate) fn fresh_genesis_ledger_published_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<bool> {
        let ledger =
            Self::open_existing_read_only_inner_v1(path, true)?.context("fresh ledger missing")?;
        load_verified(&ledger, genesis, namespace)?;
        Ok(ledger.db.get(KEY_SCHEMA_V1)?.as_deref() == Some(PUBLISHED_SCHEMA.as_bytes()))
    }
}
