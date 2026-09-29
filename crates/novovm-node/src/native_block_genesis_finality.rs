//! First-height BFT finality, separate from immutable signed execution headers.
use super::*;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1;
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;

const KEY_PROOF: &[u8] = b"native_block_ledger/v1/genesis/finality-proof";
const KEY_PIN: &[u8] = b"native_block_ledger/v1/genesis/finality-pin";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NovNativeFreshFinalityProofV1 {
    pub authority: NovNativeSealEpochAuthorityV1,
    pub witness: NovNativeSealRoundMessageV1,
}

impl NovNativeFreshFinalityProofV1 {
    /// Stable across valid signer subsets and confirmation rounds. The complete
    /// witness is still required: a prepare QC alone is not finality.
    pub(crate) fn validated_decision_target(
        &self,
        config: &FreshGenesisConfigV1,
        block: &NovNativeDurableBlockV1,
    ) -> Result<[u8; 32]> {
        self.validate_archived_certificate(config, block)?;
        let NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } = &self.witness
        else {
            bail!("parent requires a complete decision witness");
        };
        crate::native_block_seal::commit_v3::decision_target_v3(
            &decision.prepare,
            config.compile()?.validator_set(),
        )
    }

    /// Verify archived first-block evidence without treating it as live authority.
    pub(crate) fn validate_archived_block(
        &self,
        config: &FreshGenesisConfigV1,
        block: &NovNativeDurableBlockV1,
    ) -> Result<()> {
        if block.header.height != 1 {
            bail!("first-block finality requires height one");
        }
        self.validate_archived_certificate(config, block)
    }

    /// Verify the pinned quorum's certificate for this exact historical block.
    /// This alone does not verify ancestry or current state ownership. Live
    /// capture must also verify the complete ledger and AOEM publication.
    pub(crate) fn validate_archived_certificate(
        &self,
        config: &FreshGenesisConfigV1,
        block: &NovNativeDurableBlockV1,
    ) -> Result<()> {
        let compiled = config.compile()?;
        let rebuilt = NovNativeSealEpochAuthorityV1::derive_operator_pinned_fresh_genesis_epoch(
            config,
            compiled.config_commitment(),
            self.authority.transport_bindings.clone(),
        )?;
        if rebuilt != self.authority {
            bail!("archived finality authority differs from genesis");
        }
        let NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } = &self.witness
        else {
            bail!("archived finality requires a V3 decision witness");
        };
        let subject = &decision.prepare.subject;
        let (parent_target, profile) = if block.header.height == 1 {
            (
                [0; 32],
                crate::native_block_seal::NOV_NATIVE_BLOCK_SEAL_FRESH_GENESIS_PROOF_V1,
            )
        } else if block.header.height > 1 && subject.justify_qc_hash != [0; 32] {
            (
                subject.justify_qc_hash,
                crate::native_block_seal::NOV_NATIVE_BLOCK_SEAL_FRESH_SUCCESSOR_PROOF_V1,
            )
        } else {
            bail!("archived successor certificate requires a parent decision target");
        };
        let expected = crate::native_block_seal::subject_from_block_profile_v1(
            block,
            compiled.validator_set(),
            subject.round,
            parent_target,
            compiled.identity().anchor(),
            config.protocol_config_commitment,
            profile,
        )?;
        if expected != *subject {
            bail!("archived finality does not bind parent block");
        }
        let source = self
            .authority
            .transport_bindings
            .first()
            .context("archived finality has no members")?;
        self.witness.validate_authenticated(
            &self.authority,
            block.header.height,
            &source.transport_peer_id,
        )?;
        Ok(())
    }

    fn pin(&self) -> Result<[u8; 32]> {
        let mut hash = Sha256::new();
        hash.update(b"novovm-fresh-bft-finality-proof-v1\0");
        hash.update(serde_json::to_vec(self)?);
        Ok(hash.finalize().into())
    }

    fn validate(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        config: &FreshGenesisConfigV1,
    ) -> Result<()> {
        let intent = promotion::read(ledger)?;
        let NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } = &self.witness
        else {
            bail!("finality requires a complete V3 decision witness, not a prepare QC");
        };
        if **decision != intent.decision {
            bail!("finality witness differs from selected promotion");
        }
        let record = ledger
            .load_candidate_record_inner_v1(intent.chain_id, intent.block_hash)?
            .context("finality candidate missing")?;
        let block = ledger
            .load_candidate_block_for_record_inner_v1(&record)?
            .context("finality candidate body missing")?;
        self.validate_archived_block(config, &block)
    }
}

pub(super) fn read(ledger: &NovNativeBlockLedgerV1) -> Result<NovNativeFreshFinalityProofV1> {
    let proof: NovNativeFreshFinalityProofV1 =
        read_json_v1(&ledger.db, KEY_PROOF, "fresh finality proof")?
            .context("fresh finality proof missing")?;
    if ledger.db.get(KEY_PIN)?.as_deref() != Some(&proof.pin()?[..]) {
        bail!("fresh finality proof pin missing or changed");
    }
    Ok(proof)
}

pub(super) fn validated_keys(
    ledger: &NovNativeBlockLedgerV1,
    config: &FreshGenesisConfigV1,
) -> Result<Vec<Vec<u8>>> {
    read(ledger)?.validate(ledger, config)?;
    Ok(vec![KEY_PROOF.to_vec(), KEY_PIN.to_vec()])
}

impl NovNativeBlockLedgerV1 {
    /// The coordinator has just verified live AOEM authority under both locks.
    pub(crate) fn finalize_fresh_genesis_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        proof: &NovNativeFreshFinalityProofV1,
    ) -> Result<()> {
        let existing = Self::open_existing_read_only_inner_v1(path, true)?
            .context("finality ledger missing")?;
        drop(existing);
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("finality ledger lock poisoned"))?;
        let config = load_verified(&ledger, genesis, namespace)?;
        proof.validate(&ledger, &config)?;
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("finality schema missing")?;
        if is_finalized_schema(&schema) {
            if read(&ledger)? != *proof {
                bail!("finality evidence cannot be replaced");
            }
            return Ok(());
        }
        if schema != PUBLISHED_SCHEMA.as_bytes() {
            bail!("finality requires complete published ledger");
        }
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(&mut batch, KEY_PROOF, proof, "fresh finality proof")?;
        batch.put(KEY_PIN, proof.pin()?);
        batch.put(KEY_SCHEMA_V1, FINALIZED_SCHEMA.as_bytes());
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, genesis, namespace)?;
        Ok(())
    }

    /// Durable evidence query; callers needing live AOEM must use the workspace
    /// verifier. Signed header flags remain historical and are never rewritten.
    pub fn load_fresh_genesis_finality_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<Option<NovNativeFreshFinalityProofV1>> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("finality ledger missing")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("finality read lock poisoned"))?;
        load_verified(&ledger, genesis, namespace)?;
        if ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .is_some_and(|schema| is_finalized_schema(&schema))
        {
            return read(&ledger).map(Some);
        }
        Ok(None)
    }
}
