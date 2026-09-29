//! Durable first-block publication target. No authority/head/index publication yet.
use super::*;
use crate::native_block_seal::commit_v3::NovNativeSealDecisionCertificateV3;

const KEY_PROMOTION: &[u8] = b"native_block_ledger/v1/genesis/promotion";
const KEY_PROMOTION_PIN: &[u8] = b"native_block_ledger/v1/genesis/promotion-pin";
const INTENT_SCHEMA: &str = "novovm-fresh-genesis-promotion-intent/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeFreshPromotionIntentV1 {
    schema: String,
    pub(crate) chain_id: u64,
    pub(crate) genesis_commitment: [u8; 32],
    pub(crate) namespace: [u8; 32],
    pub(crate) block_hash: [u8; 32],
    pub(crate) execution: NovNativeIsolatedExecutionBindingV1,
    pub(crate) decision: NovNativeSealDecisionCertificateV3,
}

impl NovNativeFreshPromotionIntentV1 {
    pub fn commitment(&self) -> Result<[u8; 32]> {
        let mut hash = Sha256::new();
        hash.update(b"novovm-fresh-promotion-intent-v1\0");
        hash.update(serde_json::to_vec(self)?);
        Ok(hash.finalize().into())
    }

    fn validate(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        config: &FreshGenesisConfigV1,
        expected: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<()> {
        if self.schema != INTENT_SCHEMA
            || self.chain_id != config.chain_id
            || self.genesis_commitment != expected
            || self.namespace != namespace
        {
            bail!("fresh promotion intent domain mismatch");
        }
        self.execution.validate()?;
        let record = ledger
            .load_candidate_record_inner_v1(self.chain_id, self.block_hash)?
            .context("fresh promotion candidate missing")?;
        if record.isolated_execution_binding.as_ref() != Some(&self.execution) {
            bail!("fresh promotion output binding changed");
        }
        let block = ledger
            .load_candidate_block_for_record_inner_v1(&record)?
            .context("fresh promotion block missing")?;
        let compiled = config.compile()?;
        self.decision.verify(compiled.validator_set())?;
        let subject = &self.decision.prepare.subject;
        let rebuilt = crate::native_block_seal::subject_from_block_profile_v1(
            &block,
            compiled.validator_set(),
            subject.round,
            [0; 32],
            compiled.identity().anchor(),
            config.protocol_config_commitment,
            crate::native_block_seal::NOV_NATIVE_BLOCK_SEAL_FRESH_GENESIS_PROOF_V1,
        )?;
        if rebuilt != *subject {
            bail!("fresh promotion decision differs from executed candidate");
        }
        Ok(())
    }
}

pub(super) fn read(ledger: &NovNativeBlockLedgerV1) -> Result<NovNativeFreshPromotionIntentV1> {
    let intent: NovNativeFreshPromotionIntentV1 =
        read_json_v1(&ledger.db, KEY_PROMOTION, "fresh promotion")?
            .context("fresh promotion intent missing")?;
    if ledger.db.get(KEY_PROMOTION_PIN)?.as_deref() != Some(&intent.commitment()?[..]) {
        bail!("fresh promotion intent pin missing or changed");
    }
    Ok(intent)
}

pub(super) fn validated_keys(
    ledger: &NovNativeBlockLedgerV1,
    config: &FreshGenesisConfigV1,
    expected: [u8; 32],
    namespace: [u8; 32],
) -> Result<Vec<Vec<u8>>> {
    read(ledger)?.validate(ledger, config, expected, namespace)?;
    Ok(vec![KEY_PROMOTION.to_vec(), KEY_PROMOTION_PIN.to_vec()])
}

impl NovNativeBlockLedgerV1 {
    pub(crate) fn optional_fresh_genesis_promotion_v1(
        path: &Path,
        expected: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<Option<NovNativeFreshPromotionIntentV1>> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("fresh promotion ledger missing")?;
        load_verified(&ledger, expected, namespace)?;
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("promotion schema missing")?;
        if schema == PROMOTION_SCHEMA.as_bytes() || is_published_schema(&schema) {
            return read(&ledger).map(Some);
        }
        Ok(None)
    }

    pub(crate) fn load_fresh_genesis_promotion_v1(
        path: &Path,
        expected: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<NovNativeFreshPromotionIntentV1> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("fresh promotion ledger missing")?;
        load_verified(&ledger, expected, namespace)?;
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("promotion schema missing")?;
        if schema != PROMOTION_SCHEMA.as_bytes() && !is_published_schema(&schema) {
            bail!("fresh promotion requires a durable intent before authority publication");
        }
        read(&ledger)
    }

    /// Called under workspace and authority locks after live AOEM verification.
    /// The new capability marker fences old readers/signers before any publication.
    pub(crate) fn stage_fresh_genesis_promotion_v1(
        path: &Path,
        expected: [u8; 32],
        namespace: [u8; 32],
        block_hash: [u8; 32],
        execution: NovNativeIsolatedExecutionBindingV1,
        decision: NovNativeSealDecisionCertificateV3,
    ) -> Result<NovNativeFreshPromotionIntentV1> {
        let probe = Self::open_existing_read_only_inner_v1(path, true)?
            .context("fresh promotion requires an existing candidate ledger")?;
        drop(probe);
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("promotion ledger lock poisoned"))?;
        let config = load_verified(&ledger, expected, namespace)?;
        let intent = NovNativeFreshPromotionIntentV1 {
            schema: INTENT_SCHEMA.into(),
            chain_id: config.chain_id,
            genesis_commitment: expected,
            namespace,
            block_hash,
            execution,
            decision,
        };
        intent.validate(&ledger, &config, expected, namespace)?;
        if ledger.db.get(KEY_SCHEMA_V1)?.is_some_and(|schema| {
            schema == PROMOTION_SCHEMA.as_bytes() || is_published_schema(&schema)
        }) {
            if read(&ledger)? != intent {
                bail!("another fresh promotion target is already durable");
            }
            return Ok(intent);
        }
        if ledger.db.get(KEY_SCHEMA_V1)?.as_deref() != Some(CANDIDATES_SCHEMA.as_bytes()) {
            bail!("fresh promotion requires registered candidates");
        }
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(&mut batch, KEY_PROMOTION, &intent, "fresh promotion intent")?;
        batch.put(KEY_PROMOTION_PIN, intent.commitment()?);
        batch.put(KEY_SCHEMA_V1, PROMOTION_SCHEMA.as_bytes());
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, expected, namespace)?;
        let recovered = read(&ledger)?;
        if recovered != intent {
            bail!("fresh promotion intent readback differs from target");
        }
        Ok(recovered)
    }

    pub(crate) fn refuse_pending_fresh_promotion_v1(
        path: &Path,
        expected: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<()> {
        let ledger =
            Self::open_existing_read_only_inner_v1(path, true)?.context("fresh ledger missing")?;
        load_verified(&ledger, expected, namespace)?;
        if ledger.db.get(KEY_SCHEMA_V1)?.is_some_and(|schema| {
            schema == PROMOTION_SCHEMA.as_bytes() || is_published_schema(&schema)
        }) {
            bail!(
                "pending fresh promotion requires recovery before signing, registration or abort"
            );
        }
        Ok(())
    }
}
