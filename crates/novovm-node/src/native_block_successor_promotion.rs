//! Durable unique successor publication target. No authority or head change.
use super::*;
use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;

const KEY: &[u8] = b"native_block_ledger/v1/successor/promotion";
const PIN: &[u8] = b"native_block_ledger/v1/successor/promotion-pin";

/// Owned results from one fully verified, mutex-protected ledger read. These
/// historical facts are not a live capability and must not be cached as one.
pub(crate) struct VerifiedSuccessorPublicationV1 {
    pub(crate) commitment: [u8; 32],
    pub(crate) parent_archive: FinalizedRecordArchiveV1,
    pub(crate) published_block: Option<NovNativeDurableBlockV1>,
    pub(crate) finality: Option<NovNativeFreshFinalityProofV1>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    genesis: [u8; 32],
    namespace: [u8; 32],
    parent_workspace: [u8; 32],
    pub(super) execution: NovNativeIsolatedExecutionBindingV1,
    pub(super) proof: NovNativeFreshFinalityProofV1,
}

/// Only constructed after full certificate verification in this traversal.
/// Carries historical linkage, never a live signing/publication capability.
pub(super) struct VerifiedHistoricalParentV1 {
    genesis: [u8; 32],
    namespace: [u8; 32],
    workspace: [u8; 32],
    block: NovNativeDurableBlockV1,
    target: [u8; 32],
}

impl VerifiedHistoricalParentV1 {
    pub(super) fn genesis(
        ledger: &NovNativeBlockLedgerV1,
        validation: &FreshGenesisValidationV1<'_>,
        namespace: [u8; 32],
    ) -> Result<Self> {
        let parent = successors::record_at(ledger, 1)?;
        #[cfg(test)]
        HISTORY_PARENT_CHECKS.with(|counts| {
            let (checked, reused) = counts.get();
            counts.set((checked + 1, reused));
        });
        let target = parent
            .proof
            .validated_decision_target_with_validation(validation, &parent.block)?;
        Ok(Self {
            genesis: validation.compiled().config_commitment(),
            namespace,
            workspace: parent.execution.workspace_id,
            block: parent.block,
            target,
        })
    }

    pub(super) fn block(&self) -> &NovNativeDurableBlockV1 {
        &self.block
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static HISTORY_PARENT_CHECKS: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
    pub(super) static HISTORY_CURRENT_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Intent {
    pub(super) fn height(&self) -> Result<u64> {
        let Message::DecisionCertificateV3 { decision, .. } = &self.proof.witness else {
            bail!("successor archive requires a full decision witness");
        };
        Ok(decision.prepare.subject.height)
    }

    pub(super) fn commitment(&self) -> Result<[u8; 32]> {
        let mut hash = Sha256::new();
        hash.update(b"novovm-fresh-successor-promotion-intent-v1\0");
        hash.update(serde_json::to_vec(self)?);
        Ok(hash.finalize().into())
    }

    pub(super) fn validate(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        config: &FreshGenesisConfigV1,
        namespace: [u8; 32],
    ) -> Result<()> {
        self.validate_with_validation(ledger, &FreshGenesisValidationV1::new(config)?, namespace)
    }

    pub(super) fn validate_with_validation(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        validation: &FreshGenesisValidationV1<'_>,
        namespace: [u8; 32],
    ) -> Result<()> {
        self.validate_with_parent(ledger, validation, namespace, None)
            .map(|_| ())
    }

    pub(super) fn validate_next_in_history(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        validation: &FreshGenesisValidationV1<'_>,
        namespace: [u8; 32],
        parent: &VerifiedHistoricalParentV1,
    ) -> Result<VerifiedHistoricalParentV1> {
        let block = self.validate_with_parent(ledger, validation, namespace, Some(parent))?;
        let Message::DecisionCertificateV3 { decision, .. } = &self.proof.witness else {
            bail!("verified historical successor lost its decision witness");
        };
        // Keep the public target derivation's own prepare-QC check. We only
        // avoid validating this complete witness again as the next parent.
        let target = crate::native_block_seal::commit_v3::decision_target_v3(
            &decision.prepare,
            validation.compiled().validator_set(),
        )?;
        Ok(VerifiedHistoricalParentV1 {
            genesis: validation.compiled().config_commitment(),
            namespace,
            workspace: self.execution.workspace_id,
            block,
            target,
        })
    }

    fn validate_with_parent(
        &self,
        ledger: &NovNativeBlockLedgerV1,
        validation: &FreshGenesisValidationV1<'_>,
        namespace: [u8; 32],
        verified_parent: Option<&VerifiedHistoricalParentV1>,
    ) -> Result<NovNativeDurableBlockV1> {
        let config = validation.config();
        let compiled = validation.compiled();
        if self.genesis != compiled.config_commitment() || self.namespace != namespace {
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
        let loaded_parent = if verified_parent.is_none() {
            Some(successors::record_at(
                ledger,
                subject
                    .height
                    .checked_sub(1)
                    .context("successor height underflow")?,
            )?)
        } else {
            None
        };
        let (parent_block, parent_workspace) = if let Some(parent) = verified_parent {
            if parent.genesis != self.genesis || parent.namespace != namespace {
                bail!("verified historical parent domain differs from successor");
            }
            #[cfg(test)]
            HISTORY_PARENT_CHECKS.with(|counts| {
                let (checked, reused) = counts.get();
                counts.set((checked, reused + 1));
            });
            (&parent.block, parent.workspace)
        } else {
            let parent = loaded_parent
                .as_ref()
                .context("cold successor parent missing")?;
            (&parent.block, parent.execution.workspace_id)
        };
        if self.parent_workspace != parent_workspace {
            bail!("successor promotion parent workspace mismatch");
        }
        successors::validate_child(parent_block, &block)?;
        let target = match verified_parent {
            Some(parent) => parent.target,
            None => {
                #[cfg(test)]
                HISTORY_PARENT_CHECKS.with(|counts| {
                    let (checked, reused) = counts.get();
                    counts.set((checked + 1, reused));
                });
                loaded_parent
                    .as_ref()
                    .context("cold successor parent missing")?
                    .proof
                    .validated_decision_target_with_validation(validation, parent_block)?
            }
        };
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
        let authority =
            NovNativeSealEpochAuthorityV1::derive_operator_pinned_validated_fresh_genesis_epoch(
                validation,
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
        #[cfg(test)]
        HISTORY_CURRENT_CHECKS.with(|count| count.set(count.get() + 1));
        self.proof.witness.validate_authenticated(
            &authority,
            subject.height,
            &source.transport_peer_id,
        )?;
        Ok(block)
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
    validation: &FreshGenesisValidationV1<'_>,
    namespace: [u8; 32],
) -> Result<Vec<Vec<u8>>> {
    read(ledger)?.validate_with_validation(ledger, validation, namespace)?;
    Ok(vec![KEY.to_vec(), PIN.to_vec()])
}

/// Requires the complete ledger validation before use. Keep the existing
/// exact-intent checks shared with the non-bundled publication paths.
fn verify_target(
    ledger: &NovNativeBlockLedgerV1,
    parent_workspace: [u8; 32],
    execution: &NovNativeIsolatedExecutionBindingV1,
) -> Result<Intent> {
    if !ledger
        .db
        .get(KEY_SCHEMA_V1)?
        .is_some_and(|schema| has_successor_intent_schema(&schema))
    {
        bail!("successor publication requires a durable intent");
    }
    let intent = read(ledger)?;
    if intent.parent_workspace != parent_workspace || intent.execution != *execution {
        bail!("successor publication differs from pinned target");
    }
    Ok(intent)
}

impl NovNativeBlockLedgerV1 {
    /// Read all ledger facts needed by the coordinator's read-only Verify scope
    /// under one ledger mutex and one complete history/QC/key verification.
    ///
    /// The coordinator retains workspace then authority locks throughout its
    /// own AOEM checks and readback. This getter releases the non-reentrant
    /// ledger mutex before returning, so subsequent source verification may
    /// safely read historical archives. Only those same-call, read-only checks
    /// may reuse the result; mutating/recovery scopes keep their old getters.
    pub(crate) fn load_verified_successor_publication_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        parent_workspace: [u8; 32],
        execution: &NovNativeIsolatedExecutionBindingV1,
    ) -> Result<VerifiedSuccessorPublicationV1> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor promotion ledger missing")?;
        let _guard = ledger
            .write_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("successor publication read lock poisoned"))?;
        let config = load_verified(&ledger, genesis, namespace)?;
        let intent = verify_target(&ledger, parent_workspace, execution)?;
        let parent_height = intent
            .height()?
            .checked_sub(1)
            .filter(|height| *height > 0)
            .context("successor publication requires a finalized parent height")?;
        // has_successor_intent_schema is a strict subset of is_finalized_schema:
        // the first-block finality gate of load_fresh_finalized_archive_v1 is
        // already satisfied. Later records still require their height archive.
        let parent = successors::record_at(&ledger, parent_height)?;
        Ok(VerifiedSuccessorPublicationV1 {
            commitment: intent.commitment()?,
            parent_archive: FinalizedRecordArchiveV1 {
                config,
                block: parent.block,
                proof: parent.proof,
                execution: parent.execution,
                commitment: parent.commitment,
            },
            published_block: successor_completion::read_published_block(&ledger)?,
            finality: successor_finality::read_finality(&ledger)?,
        })
    }

    pub(crate) fn fresh_successor_archived_parent_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        candidate: [u8; 32],
    ) -> Result<[u8; 32]> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor archive ledger missing")?;
        load_verified(&ledger, genesis, namespace)?;
        if ledger.db.get(KEY_SCHEMA_V1)?.as_deref() != Some(SUCCESSOR_FINALIZED_SCHEMA.as_bytes()) {
            bail!("next parent requires finalized successor ledger");
        }
        let intent = read(&ledger)?;
        if intent.execution.workspace_id != candidate {
            bail!("next parent differs from finalized successor");
        }
        Ok(intent.parent_workspace)
    }

    pub(crate) fn verify_optional_successor_archive_v1(
        path: &Path,
        genesis: [u8; 32],
        namespace: [u8; 32],
        parent: [u8; 32],
        candidate: [u8; 32],
        proof: &NovNativeFreshFinalityProofV1,
    ) -> Result<bool> {
        let ledger = Self::open_existing_read_only_inner_v1(path, true)?
            .context("successor archive ledger missing")?;
        load_verified(&ledger, genesis, namespace)?;
        if !ledger
            .db
            .get(KEY_SCHEMA_V1)?
            .is_some_and(|schema| has_successor_intent_schema(&schema))
        {
            return Ok(false);
        }
        let intent = read(&ledger)?;
        if ledger.db.get(KEY_SCHEMA_V1)?.as_deref() == Some(SUCCESSOR_FINALIZED_SCHEMA.as_bytes())
            && intent.execution.workspace_id == parent
            && intent.height()?.checked_add(1)
                == Some(match &proof.witness {
                    Message::DecisionCertificateV3 { decision, .. } => {
                        decision.prepare.subject.height
                    }
                    _ => bail!("successor recovery requires full decision"),
                })
        {
            return Ok(false);
        }
        if intent.parent_workspace != parent
            || intent.execution.workspace_id != candidate
            || intent.proof != *proof
        {
            bail!("successor recovery archive differs from pinned intent");
        }
        Ok(true)
    }

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
        verify_target(&ledger, parent_workspace, execution)?.commitment()
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
            let previous = read(&ledger)?;
            if previous == intent {
                return intent.commitment();
            }
            if schema != SUCCESSOR_FINALIZED_SCHEMA.as_bytes()
                || previous.height()?.checked_add(1) != Some(intent.height()?)
                || previous.execution.workspace_id != parent_workspace
            {
                bail!("another successor promotion target is already durable");
            }
        } else if schema != FINALIZED_SCHEMA.as_bytes() {
            bail!("successor promotion requires finalized parent");
        }
        let commitment = intent.commitment()?;
        let mut batch = RocksDbWriteBatch::default();
        put_json_v1(&mut batch, KEY, &intent, "successor promotion")?;
        batch.put(PIN, commitment);
        batch.delete(successor_completion::KEY_COMPLETED);
        batch.delete(successor_finality::KEY_FINALIZED);
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
