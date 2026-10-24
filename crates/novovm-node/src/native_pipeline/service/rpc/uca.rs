//! NUAS adapts the original bounded pool, compute verifier and captured-state
//! reader. Admission is volatile; only the original plan/journal spends funds.
use super::*;
use novovm_protocol::uca_spend::SignedUcaSpendV1;
use novovm_protocol::uca_spend_batch::{UcaSpendBatch, UcaSpendBatchLimits};
use sha2::{Digest, Sha256};

pub(super) fn hash(signed: &SignedUcaSpendV1) -> Result<Hash> {
    Ok(Sha256::digest(signed.intent.signing_bytes()?).into())
}

impl RpcLifecycle {
    pub(super) fn is_uca(&self) -> bool {
        self.node.template.business_program
            == crate::native_pipeline::business::nov_transfer_batch::uca_program_id()
    }

    pub(super) fn input_body_limit(&self) -> usize {
        if self.is_uca() {
            super::super::uca_body_byte_limit(self.batch_size)
        } else {
            super::super::body_byte_limit(self.batch_size)
        }
    }

    pub(super) fn cached_uca(
        &mut self,
        input: &AdmissionInput,
        broadcast: bool,
    ) -> Result<Option<Hash>> {
        ensure!(
            self.is_uca() && self.projection_error.is_none(),
            "UCA input/profile unavailable"
        );
        let AdmissionInput::Uca { batch, index } = input else {
            anyhow::bail!("UCA requires the explicit native structured contract");
        };
        let raw = batch.raw_transaction(*index)?;
        let signed = SignedUcaSpendV1::decode(&raw)?;
        ensure!(
            signed.intent.chain_id == self.node.template.chain_id
                && signed.intent.genesis == self.node.template.genesis_config_commitment,
            "UCA input chain/signing domain mismatch"
        );
        let id = hash(&signed)?;
        if let Some(entry) = self.pending.get(&id) {
            if let AdmissionInput::Uca {
                batch: old,
                index: row,
            } = &entry.input
            {
                // Intent hash excludes signatures. Exact bytes and hints are
                // required before skipping this pipeline's strict verification.
                if old.raw_transaction(*row)? == raw
                    && old.account_hints(*row)? == batch.account_hints(*index)?
                {
                    self.authentication_cache_hits += 1;
                    if broadcast {
                        self.queue_gossip(id);
                    }
                    return Ok(Some(id));
                }
            }
        }
        Ok(None)
    }

    /// Called only after a fresh exact-parent UCA read. Never creates or mutates
    /// an account/grant. Two different grants still reserve one account nonce.
    pub(super) fn admit_uca_checked(
        &mut self,
        input: AdmissionInput,
        admitted: AdmittedTransfer,
        state: &crate::native_pipeline::business::uca_spend_state::CapturedUcaSpendStateV1,
        broadcast: bool,
    ) -> Result<Hash> {
        ensure!(
            self.is_uca() && self.projection_error.is_none(),
            "UCA admission unavailable"
        );
        ensure!(
            self.node.pipeline.owns_admitted(&admitted),
            "foreign UCA verifier owner"
        );
        let checked = admitted
            .checked_uca()
            .context("UCA signature capability required")?;
        let intent = checked.intent();
        ensure!(state.matches_intent(intent), "UCA captured intent mismatch");
        let parent = self.node.controller.parent();
        ensure!(
            state.parent_root() == parent.state_root,
            "UCA admission parent changed"
        );
        ensure!(
            intent.chain_id == self.node.template.chain_id
                && intent.genesis == self.node.template.genesis_config_commitment,
            "UCA signature domain mismatch"
        );
        ensure!(
            state
                .static_failure(parent.height.checked_add(1).context("height overflow")?)
                .is_none(),
            "UCA authority rejected at published parent"
        );
        ensure!(intent.nonce >= state.next_nonce(), "UCA nonce replay");
        let debit = intent
            .amount
            .checked_add(intent.max_fee)
            .context("UCA debit overflow")?;
        ensure!(
            debit <= state.grant().max_debit_per_intent
                && debit <= state.grant().remaining_debit_budget,
            "UCA grant debit limit"
        );
        let AdmissionInput::Uca { batch, index } = &input else {
            anyhow::bail!("UCA source required");
        };
        ensure!(
            admitted.matches_uca_source(batch, *index),
            "UCA source capability mismatch"
        );
        let id = hash(checked.signed())?;
        if self.receipts.contains_key(&id) || self.pending.contains_key(&id) {
            return Ok(id);
        }
        let signer = intent.nonce_identity()?;
        let nonce = intent.nonce;
        ensure!(
            !self.reservations.contains(&(signer, nonce)),
            "UCA account nonce already reserved"
        );
        let canonical_len = input.encoded_len()?;
        ensure!(
            self.pending.len() < MAX_POOL
                && canonical_len <= MAX_POOL_BYTES.saturating_sub(self.pending_bytes),
            "UCA pool backpressure"
        );
        let metadata = admitted.retained_metadata_bytes();
        self.pending_retention
            .reserve(&input, metadata, MAX_POOL_BYTES)?;
        self.pending_bytes += canonical_len;
        self.pending.insert(
            id,
            Pending {
                input,
                admitted: Some(admitted),
                canonical_len,
                signer,
                nonce,
                gossip_missing: 0,
            },
        );
        self.reservations.insert((signer, nonce));
        self.order.push_back(id);
        if broadcast {
            self.queue_gossip(id);
        }
        Ok(id)
    }

    pub(super) fn selected_uca_batch(&self, selected: &[Hash]) -> Result<Arc<UcaSpendBatch>> {
        let mut raws = Vec::with_capacity(selected.len());
        let mut hints = Vec::with_capacity(selected.len());
        for id in selected {
            let entry = self.pending.get(id).context("selected UCA entry missing")?;
            let AdmissionInput::Uca { batch, index } = &entry.input else {
                anyhow::bail!("mixed pool contracts");
            };
            raws.push(batch.raw_transaction(*index)?);
            let (payer, recipient) = batch.account_hints(*index)?;
            hints.push((payer.to_vec(), recipient.to_vec()));
        }
        Ok(Arc::new(UcaSpendBatch::from_raw(
            &raws,
            &hints,
            UcaSpendBatchLimits {
                transactions: self.batch_size,
                transaction_bytes: 8192,
                body_bytes: self.input_body_limit(),
            },
        )?))
    }
}
