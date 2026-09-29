//! Durable unique successor publication target. No authority or head change.
use super::*;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;

const KEY: &[u8] = b"native_block_ledger/v1/successor/promotion";
const PIN: &[u8] = b"native_block_ledger/v1/successor/promotion-pin";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    genesis: [u8; 32],
    namespace: [u8; 32],
    parent_workspace: [u8; 32],
    pub(super) execution: NovNativeIsolatedExecutionBindingV1,
    pub(super) proof: NovNativeFreshFinalityProofV1,
}

impl Intent {
    pub(super) fn commitment(&self) -> Result<[u8; 32]> {
        let mut hash = Sha256::new();
        hash.update(b"novovm-fresh-successor-promotion-intent-v1\0");
        hash.update(serde_json::to_vec(self)?);
        Ok(hash.finalize().into())
    }

    fn validate(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        config: &FreshGenesisConfigV1,
        namespace: [u8; 32],
    ) -> Result<()> {
        let compiled = config.compile()?;
        if self.genesis != compiled.config_commitment()
            || self.namespace != namespace
            || self.parent_workspace != promotion::read(ledger)?.execution.workspace_id
        {
            bail!("successor promotion domain or parent mismatch");
        }
        self.execution.validate()?;
        let Message::DecisionCertificateV3 { decision, .. } = &self.proof.witness else {
            bail!("successor promotion requires full V3 decision witness");
        };
        let subject = &decision.prepare.subject;
        let record = ledger
            .load_candidate_record_inner_v1(config.chain_id, subject.block_hash)?
            .context("successor promotion candidate missing")?;
        if record.isolated_execution_binding.as_ref() != Some(&self.execution) {
            bail!("successor promotion execution binding changed");
        }
        let block = ledger
            .load_candidate_block_for_record_inner_v1(&record)?
            .context("successor promotion body missing")?;
        let parent = successors::parent(ledger)?;
        successors::validate_child(&parent, &block)?;
        let target = finality::read(ledger)?.validated_decision_target(config, &parent)?;
        let expected = crate::native_block_seal::subject_from_block_profile_v1(
            &block,
            compiled.validator_set(),
            subject.round,
            target,
            compiled.identity().anchor(),
            config.protocol_config_commitment,
            crate::native_block_seal::NOV_NATIVE_BLOCK_SEAL_FRESH_SUCCESSOR_PROOF_V1,
        )?;
        if expected != *subject {
            bail!("successor promotion decision differs from executed block");
        }
        let authority = NovNativeSealEpochAuthorityV1::derive_operator_pinned_fresh_genesis_epoch(
            config,
            self.genesis,
            self.proof.authority.transport_bindings.clone(),
        )?;
        if authority != self.proof.authority {
            bail!("successor promotion authority differs from genesis");
        }
        let source = authority
            .transport_bindings
            .first()
            .context("successor authority empty")?;
        self.proof
            .witness
            .validate_authenticated(&authority, 2, &source.transport_peer_id)?;
        Ok(())
    }
}

pub(super) fn read(ledger: &NovNativeBlockLedgerV1) -> Result<Intent> {
    let intent: Intent = read_json_v1(&ledger.db, KEY, "successor promotion")?
        .context("successor promotion intent missing")?;
    if ledger.db.get(PIN)?.as_deref() != Some(&intent.commitment()?[..]) {
        bail!("successor promotion pin missing or changed");
    }
    Ok(intent)
}

pub(super) fn validated_keys(
    ledger: &NovNativeBlockLedgerV1,
    config: &FreshGenesisConfigV1,
    namespace: [u8; 32],
) -> Result<Vec<Vec<u8>>> {
    read(ledger)?.validate(ledger, config, namespace)?;
    Ok(vec![KEY.to_vec(), PIN.to_vec()])
}

impl NovNativeBlockLedgerV1 {
    pub(crate) fn verify_fresh_successor_promotion_target_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        parent_workspace: [u8; 32],
        execution: &NovNativeIsolatedExecutionBindingV1,
    ) -> Result<[u8; 32]> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor promotion ledger missing")?;
        load_verified(&ledger, genesis, namespace)?;
        if !ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .is_some_and(|schema| has_successor_intent_schema(&schema))
        {
            bail!("successor publication requires a durable intent");
        }
        let intent = read(&ledger)?;
        if intent.parent_workspace != parent_workspace || intent.execution != *execution {
            bail!("successor publication differs from pinned target");
        }
        intent.commitment()
    }

    /// The coordinator retains workspace and authority locks and has verified
    /// live parent publication and the complete candidate AOEM artifact.
    pub(crate) fn stage_fresh_successor_promotion_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        parent_workspace: [u8; 32],
        execution: NovNativeIsolatedExecutionBindingV1,
        proof: NovNativeFreshFinalityProofV1,
    ) -> Result<[u8; 32]> {
        let probe = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor ledger missing")?;
        drop(probe);
        let ledger = Self::open_inner_v1(path, true)?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("successor promotion lock poisoned"))?;
        let config = load_verified(&ledger, genesis, namespace)?;
        let intent = Intent {
            genesis,
            namespace,
            parent_workspace,
            execution,
            proof,
        };
        intent.validate(&ledger, &config, namespace)?;
        let schema = ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .context("successor schema missing")?;
        if has_successor_intent_schema(&schema) {
            if read(&ledger)? != intent {
                bail!("another successor promotion target is already durable");
            }
            return intent.commitment();
        }
        if schema != FINALIZED_SCHEMA.as_bytes() {
            bail!("successor promotion requires finalized parent");
        }
        let commitment = intent.commitment()?;
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(&mut batch, KEY, &intent, "successor promotion")?;
        batch.put(PIN, commitment);
        batch.put(KEY_SCHEMA_V1, SUCCESSOR_INTENT_SCHEMA.as_bytes());
        write_sync_v1(&ledger.db, batch)?;
        load_verified(&ledger, genesis, namespace)?;
        if read(&ledger)? != intent {
            bail!("successor promotion readback mismatch");
        }
        Ok(commitment)
    }

    pub(crate) fn refuse_pending_successor_promotion_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
    ) -> Result<()> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor ledger missing")?;
        load_verified(&ledger, genesis, namespace)?;
        if ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .is_some_and(|schema| has_successor_intent_schema(&schema))
        {
            bail!("successor promotion requires recovery before abort");
        }
        Ok(())
    }
}
