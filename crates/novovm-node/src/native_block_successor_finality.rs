//! BFT finality binds the already archived complete decision, not mutable headers.
use super::*;

pub(super) const KEY_FINALIZED: &[u8] = b"native_block_ledger/v1/successor/finalized-intent";
const ARCHIVE_PREFIX: &str = "native_block_ledger/v1/successor/finalized/";

fn archive_key(height: u64) -> Vec<u8> {
    format!("{ARCHIVE_PREFIX}{height:016x}").into_bytes()
}

pub(super) fn read_archive(
    ledger: &NovNativeBlockLedgerV1,
    height: u64,
) -> Result<Option<successor_promotion::Intent>> {
    let archive: Option<successor_promotion::Intent> =
        read_json_v1(&ledger.db, &archive_key(height), "height finality archive")?;
    if let Some(intent) = &archive {
        if intent.height()? != height {
            bail!("finality archive height differs from signed decision");
        }
    }
    Ok(archive)
}

pub(super) fn archives(
    ledger: &NovNativeBlockLedgerV1,
) -> Result<Vec<(u64, successor_promotion::Intent)>> {
    let mut result = Vec::new();
    for entry in ledger.db.prefix_iterator(ARCHIVE_PREFIX.as_bytes()) {
        let (key, bytes) = entry?;
        if !key.starts_with(ARCHIVE_PREFIX.as_bytes()) {
            break;
        }
        let suffix = std::str::from_utf8(&key[ARCHIVE_PREFIX.len()..])?;
        let height = u64::from_str_radix(suffix, 16)?;
        if key.as_ref() != archive_key(height) || height != result.len() as u64 + 2 {
            bail!("finality history must be canonical and contiguous");
        }
        let intent: successor_promotion::Intent = serde_json::from_slice(&bytes)?;
        if intent.height()? != height {
            bail!("finality history height mismatch");
        }
        result.push((height, intent));
    }
    Ok(result)
}

pub(super) fn head_at(
    ledger: &NovNativeBlockLedgerV1,
    height: u64,
) -> Result<NovNativeBlockLedgerHeadV1> {
    let chain = promotion::read(ledger)?.chain_id;
    let key = head_key_v1(chain).into_bytes();
    let mut head: NovNativeBlockLedgerHeadV1 = serde_json::from_slice(
        completion::entries(ledger)?
            .get(&key)
            .context("first head missing")?,
    )?;
    for (known, _) in archives(ledger)? {
        if known > height {
            break;
        }
        let block = successors::record_at(ledger, known)?.block;
        head = serde_json::from_slice(
            completion::block_entries(&block, Some(&head))?
                .get(&key)
                .context("historical head missing")?,
        )?;
    }
    if head.height != height {
        bail!("historical head is not finalized");
    }
    Ok(head)
}

pub(super) fn validated_keys(
    ledger: &NovNativeBlockLedgerV1,
    config: &FreshGenesisConfigV1,
    namespace: [u8; 32],
) -> Result<Vec<Vec<u8>>> {
    let intent = successor_promotion::read(ledger)?;
    let commitment = intent.commitment()?;
    let schema = ledger.db.get(KEY_SCHEMA_V1)?.context("schema missing")?;
    let finalized = schema == SUCCESSOR_FINALIZED_SCHEMA.as_bytes();
    let mut keys = Vec::new();
    let chain = config.chain_id;
    let head_key = head_key_v1(chain).into_bytes();
    let mut head = head_at(ledger, 1)?;
    for (height, archived) in archives(ledger)? {
        archived.validate(ledger, config, namespace)?;
        let block = successors::record_at(ledger, height)?.block;
        let mut entries = completion::block_entries(&block, Some(&head))?;
        head = serde_json::from_slice(&entries.remove(&head_key).context("archive head missing")?)?;
        for (key, bytes) in entries {
            if ledger.db.get(&key)?.as_deref() != Some(bytes.as_slice()) {
                bail!("historical published index missing or changed");
            }
            keys.push(key);
        }
        keys.push(archive_key(height));
    }
    let height = intent.height()?;
    if finalized {
        if head.height != height
            || read_archive(ledger, height)?.as_ref() != Some(&intent)
            || ledger.db.get(KEY_FINALIZED)?.as_deref() != Some(&commitment[..])
        {
            bail!("height finality archive or pin differs from verified promotion");
        }
        keys.push(KEY_FINALIZED.to_vec());
    } else if head.height.checked_add(1) != Some(height) {
        bail!("active promotion does not follow complete finality history");
    }
    if !is_successor_published_schema(&schema) {
        if ledger.db.get(&head_key)?.as_deref() != Some(serde_json::to_vec(&head)?.as_slice()) {
            bail!("pending successor changed finalized head");
        }
        keys.push(head_key);
    }
    Ok(keys)
}

impl NovNativeBlockLedgerV1 {
    pub fn load_fresh_finalized_block_by_height_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        height: u64,
    ) -> Result<Option<(NovNativeDurableBlockV1, NovNativeFreshFinalityProofV1)>> {
        let Some(proof) = Self::load_fresh_finality_by_height_v1(path, genesis, namespace, height)?
        else {
            return Ok(None);
        };
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("finality ledger missing")?;
        let block = ledger
            .load_by_height_inner_v1(proof.authority.chain_id, height)?
            .context("finalized block body missing")?;
        let proposal = proof
            .witness
            .proposal()
            .context("finality proposal missing")?;
        if block.header.block_hash != proposal.subject.block_hash {
            bail!("finalized block differs from decision");
        }
        Ok(Some((block, proof)))
    }

    /// Historical finality lookup by height, never a live execution capability.
    /// Verifies the complete ledger before returning the immutable decision.
    pub fn load_fresh_finality_by_height_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        height: u64,
    ) -> Result<Option<NovNativeFreshFinalityProofV1>> {
        if height == 0 {
            bail!("execution finality starts at height one");
        }
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("finality ledger missing")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("finality read lock poisoned"))?;
        load_verified(&ledger, genesis, namespace)?;
        if height == 1 {
            return if ledger
                .db
                .get(KEY_SCHEMA_V1)?
                .is_some_and(|s| is_finalized_schema(&s))
            {
                finality::read(&ledger).map(Some)
            } else {
                Ok(None)
            };
        }
        Ok(read_archive(&ledger, height)?.map(|intent| intent.proof))
    }

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
            let height = successor_promotion::read(&ledger)?.height()?;
            return Ok(Some(
                read_archive(&ledger, height)?
                    .context("successor finality archive missing")?
                    .proof,
            ));
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
        let intent = successor_promotion::read(&ledger)?;
        let key = archive_key(intent.height()?);
        if ledger.db.get(&key)?.is_some() {
            bail!("finality cannot overwrite an existing height archive");
        }
        put_json_v1(&mut batch, &key, &intent, "height finality archive")?;
        batch.put(KEY_FINALIZED, commitment);
        batch.put(KEY_SCHEMA_V1, SUCCESSOR_FINALIZED_SCHEMA.as_bytes());
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, genesis, namespace)?;
        Ok(())
    }
}
