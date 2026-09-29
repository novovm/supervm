//! BFT finality binds the already archived complete decision, not mutable headers.
use super::*;

const KEY_FINALIZED: &[u8] = b"native_block_ledger/v1/successor/finalized-intent";

pub(super) fn validated_keys(ledger: &NovNativeBlockLedgerV1) -> Result<Vec<Vec<u8>>> {
    // The manifest validates both complete proofs and query projections before
    // reaching this pin. Never accept this key on its own.
    let commitment = successor_promotion::read(ledger)?.commitment()?;
    if ledger.db.get(KEY_FINALIZED)?.as_deref() != Some(&commitment[..]) {
        bail!("successor finality pin missing or changed");
    }
    Ok(vec![KEY_FINALIZED.to_vec()])
}

impl NovNativeBlockLedgerV1 {
    /// Historical durable proof query. Live AOEM verification belongs to the
    /// workspace coordinator; this is BFT finality, not a ZK execution proof.
    pub fn load_fresh_successor_finality_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<Option<NovNativeFreshFinalityProofV1>> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor finality ledger missing")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("successor finality read lock poisoned"))?;
        load_verified(&ledger, genesis, namespace)?;
        if ledger.db.get(KEY_SCHEMA_V1)?.as_deref() == Some(SUCCESSOR_FINALIZED_SCHEMA.as_bytes()) {
            return Ok(Some(successor_promotion::read(&ledger)?.proof));
        }
        Ok(None)
    }

    /// Caller retains workspace and authority locks after live AOEM readback.
    pub(crate) fn finalize_fresh_successor_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        commitment: [u8; 32],
    ) -> Result<()> {
        let probe = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor finality ledger missing")?;
        drop(probe);
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("successor finality lock poisoned"))?;
        load_verified(&ledger, genesis, namespace)?;
        if successor_promotion::read(&ledger)?.commitment()? != commitment {
            bail!("successor finality differs from live published authority");
        }
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("successor finality schema missing")?;
        if schema == SUCCESSOR_FINALIZED_SCHEMA.as_bytes() {
            return Ok(());
        }
        if schema != SUCCESSOR_PUBLISHED_SCHEMA.as_bytes() {
            bail!("successor finality requires complete published ledger");
        }
        let mut batch = RocksDbWriteBatch::default();
        batch.put(KEY_FINALIZED, commitment);
        batch.put(KEY_SCHEMA_V1, SUCCESSOR_FINALIZED_SCHEMA.as_bytes());
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, genesis, namespace)?;
        Ok(())
    }
}
