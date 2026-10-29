//! NUAS adapts the original bounded pool, compute verifier and captured-state
//! reader. Admission is volatile; only the original plan/journal spends funds.
use super::*;
use novovm_protocol::uca_spend::SignedAssetSpendV2;
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};
use sha2::{Digest, Sha256};

pub(super) enum UcaAdmission {
    Accepted,
    Backpressured(AdmissionInput, AdmittedTransfer),
}

pub(super) fn hash(signed: &SignedAssetSpendV2) -> Result<Hash> {
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
        let signed = SignedAssetSpendV2::decode(&raw)?;
        ensure!(
            signed.intent.chain_id == self.node.template.chain_id
                && signed.intent.genesis == self.node.template.genesis_config_commitment,
            "UCA input chain/signing domain mismatch"
        );
        let id = hash(&signed)?;
        if crate::native_pipeline::profile::asset_track(&id) {
            crate::native_pipeline::profile::asset_event(
                "row_seen",
                self.node.controller.context().height,
                || {
                    json!({
                        "source_ref":Arc::as_ptr(batch) as usize,"index":index,"tx":hex(&id),
                        "nonce":signed.intent.nonce,"payer":hex(&signed.intent.payer),"broadcast":broadcast
                    })
                },
            );
        }
        if let Some(entry) = self.pending.get(&id) {
            if let AdmissionInput::Uca {
                batch: old,
                index: row,
            } = &entry.input
            {
                // The canonical v2 bytes include the concrete asset accounts.
                if old.raw_transaction(*row)? == raw {
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
        state: &crate::native_pipeline::business::uca_spend_state::CapturedAssetSpendStateV2,
        broadcast: bool,
    ) -> Result<UcaAdmission> {
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
        if intent.authority_update.is_none() {
            let grant = state.grant().context("asset spend grant missing")?;
            ensure!(
                debit <= grant.max_debit_per_intent && debit <= grant.remaining_debit_budget,
                "asset grant debit limit"
            );
        }
        let AdmissionInput::Uca { batch, index } = &input else {
            anyhow::bail!("UCA source required");
        };
        ensure!(
            admitted.matches_uca_source(batch, *index),
            "UCA source capability mismatch"
        );
        let id = hash(checked.signed())?;
        if self.receipts.contains_key(&id) || self.pending.contains_key(&id) {
            return Ok(UcaAdmission::Accepted);
        }
        let signer = intent.nonce_identity()?;
        let nonce = intent.nonce;
        ensure!(
            !self.reservations.contains(&(signer, nonce)),
            "UCA account nonce already reserved"
        );
        let canonical_len = input.encoded_len()?;
        let limit = MAX_POOL_BYTES;
        #[cfg(test)]
        let limit = self.admission.pool_byte_limit.unwrap_or(limit);
        if self.pending.len() >= MAX_POOL
            || canonical_len > limit.saturating_sub(self.pending_bytes)
        {
            return Ok(UcaAdmission::Backpressured(input, admitted));
        }
        let metadata = admitted.retained_metadata_bytes();
        if let Err(error) = self.pending_retention.reserve(&input, metadata, limit) {
            if error.is::<PoolBackpressure>() {
                return Ok(UcaAdmission::Backpressured(input, admitted));
            }
            return Err(error);
        }
        self.nonces.bind_current_parent(parent);
        if let Err(error) =
            self.nonces
                .observe_present_at_parent(parent, signer, state.next_nonce())
        {
            self.pending_retention.release(&input, metadata);
            return Err(error);
        }
        self.pending_bytes += canonical_len;
        if crate::native_pipeline::profile::asset_track(&id) {
            crate::native_pipeline::profile::asset_event(
                "row_pool_insert",
                parent.height + 1,
                || {
                    json!({
                        "source_ref":Arc::as_ptr(batch) as usize,"index":index,"tx":hex(&id),
                        "nonce":nonce,"signer":hex(&signer),"parent_height":parent.height,
                        "parent_root":hex(&parent.state_root),"parent_nonce":state.next_nonce()
                    })
                },
            );
        }
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
        Ok(UcaAdmission::Accepted)
    }

    pub(super) fn selected_uca_batch(&self, selected: &[Hash]) -> Result<Arc<AssetSpendBatchV2>> {
        let mut views = Vec::with_capacity(selected.len());
        for id in selected {
            let entry = self.pending.get(id).context("selected UCA entry missing")?;
            let AdmissionInput::Uca { batch, index } = &entry.input else {
                anyhow::bail!("mixed pool contracts");
            };
            views.push(batch.view(*index)?);
        }
        Ok(Arc::new(AssetSpendBatchV2::from_views(
            &views,
            AssetSpendBatchLimitsV2 {
                transactions: self.batch_size,
                transaction_bytes: 8192,
                body_bytes: self.input_body_limit(),
            },
        )?))
    }
}
