//! Atomic successor indexes/head, only after locked live AOEM verification.
use super::*;
use std::collections::BTreeMap;

const KEY_COMPLETED: &[u8] = b"native_block_ledger/v1/successor/published-intent";

fn block(ledger: &NovNativeBlockLedgerV1) -> Result<NovNativeDurableBlockV1> {
    let intent = successor_promotion::read(ledger)?;
    let crate::native_block_seal::round_message::NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } = intent.proof.witness else {
        bail!("successor publication decision missing");
    };
    let h = &decision.prepare.subject;
    let record = ledger
        .load_candidate_record_inner_v1(h.chain_id, h.block_hash)?
        .context("successor record missing")?;
    ledger
        .load_candidate_block_for_record_inner_v1(&record)?
        .context("successor block missing")
}

fn entries(ledger: &NovNativeBlockLedgerV1) -> Result<BTreeMap<Vec<u8>, Vec<u8>>> {
    let block = block(ledger)?;
    let parent_entries = completion::entries(ledger)?;
    let parent: NovNativeBlockLedgerHeadV1 = serde_json::from_slice(
        parent_entries
            .get(head_key_v1(block.header.chain_id).as_bytes())
            .context("parent head projection missing")?,
    )?;
    let mut entries = completion::block_entries(&block, Some(&parent))?;
    entries.insert(
        KEY_COMPLETED.to_vec(),
        successor_promotion::read(ledger)?.commitment()?.to_vec(),
    );
    Ok(entries)
}

pub(super) fn validated_keys(ledger: &NovNativeBlockLedgerV1) -> Result<Vec<Vec<u8>>> {
    let expected = entries(ledger)?;
    for (key, bytes) in &expected {
        if ledger.db.get(key)?.as_deref() != Some(bytes.as_slice()) {
            bail!("successor published index missing or changed; refusing repair");
        }
    }
    Ok(expected.into_keys().collect())
}

impl NovNativeBlockLedgerV1 {
    /// Historical ledger readback, not proof of current AOEM availability.
    pub fn load_fresh_successor_published_block_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<Option<NovNativeDurableBlockV1>> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor ledger missing")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("successor read lock poisoned"))?;
        load_verified(&ledger, genesis, namespace)?;
        if !ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .is_some_and(|schema| is_successor_published_schema(&schema))
        {
            return Ok(None);
        }
        let expected = block(&ledger)?;
        ledger.load_by_hash_inner_v1(expected.header.chain_id, expected.header.block_hash)
    }

    /// Only the coordinator may call after exact AOEM target/output readback,
    /// while retaining workspace and authority locks through this ledger write.
    pub(crate) fn complete_fresh_successor_ledger_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        commitment: [u8; 32],
    ) -> Result<()> {
        let probe = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor ledger missing")?;
        drop(probe);
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("successor completion lock poisoned"))?;
        load_verified(&ledger, genesis, namespace)?;
        if successor_promotion::read(&ledger)?.commitment()? != commitment {
            bail!("successor ledger target differs from verified AOEM authority");
        }
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("successor schema missing")?;
        if is_successor_published_schema(&schema) {
            return Ok(());
        }
        if schema != SUCCESSOR_INTENT_SCHEMA.as_bytes() {
            bail!("successor intent required");
        }
        let head_key = head_key_v1(block(&ledger)?.header.chain_id).into_bytes();
        let mut batch = RocksDbWriteBatch::default();
        for (key, bytes) in entries(&ledger)? {
            if key != head_key && ledger.db.get(&key)?.is_some() {
                bail!("successor completion would overwrite existing index");
            }
            batch.put(key, bytes);
        }
        batch.put(KEY_SCHEMA_V1, SUCCESSOR_PUBLISHED_SCHEMA.as_bytes());
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, genesis, namespace)?;
        Ok(())
    }
}
