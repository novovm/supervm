//! NUAS adapts the original bounded pool, compute verifier and captured-state
//! reader. Admission is volatile; only the original plan/journal spends funds.
use super::*;
#[cfg(test)]
use novovm_protocol::uca_spend::SignedAssetSpendV2;
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};
use sha2::{Digest, Sha256};

pub(super) enum UcaAdmission {
    Accepted,
    Backpressured(AdmissionInput, AdmittedTransfer),
}

#[cfg(test)]
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
        let view = batch.view(*index)?;
        let intent = view.intent()?;
        ensure!(
            intent.chain_id == self.node.template.chain_id
                && intent.genesis == self.node.template.genesis_config_commitment,
            "UCA input chain/signing domain mismatch"
        );
        let id: Hash = Sha256::digest(intent.signing_bytes()?).into();
        if crate::native_pipeline::profile::asset_track(&id) {
            crate::native_pipeline::profile::asset_event(
                "row_seen",
                self.node.controller.context().height,
                || {
                    json!({
                        "source_ref":Arc::as_ptr(batch) as usize,"index":index,"tx":hex(&id),
                        "nonce":intent.nonce,"payer":hex(&intent.payer),"broadcast":broadcast
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
                // Full views include every action cell, public key and signature.
                if old.view(*row)? == view {
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
        mut input: AdmissionInput,
        mut admitted: AdmittedTransfer,
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
        let source_ref = Arc::as_ptr(batch) as usize;
        let source_index = *index;
        let id: Hash = Sha256::digest(intent.signing_bytes()?).into();
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
        // Reuse the original one-row signature-retention allowance. Future
        // gaps cannot consume the last legal bridge row's slot and bytes.
        let reserve = super::super::native_decode_limits(self.batch_size, true)
            .transaction_bytes
            .checked_mul(3)
            .and_then(|bytes| bytes.checked_add(512 + 4096))
            .context("bridge row retained bound overflow")?;
        #[cfg(test)]
        let reserve = self.admission.bridge_reserve_bytes.unwrap_or(reserve);
        let future_limit = limit.saturating_sub(reserve);
        let mut metadata = admitted.retained_metadata_bytes();
        let reserved = if self.pending.len() < MAX_POOL.saturating_sub(1)
            && canonical_len <= future_limit.saturating_sub(self.pending_bytes)
        {
            self.pending_retention
                .reserve(&input, metadata, future_limit)
        } else {
            Err(PoolBackpressure.into())
        };
        match reserved {
            Ok(()) => {}
            Err(error) if error.is::<PoolBackpressure>() => {
                // This scan is only at actual pressure, never once per hot
                // incoming row. Reservations grant priority, not spend authority.
                let mut expected = state.next_nonce();
                while self.reservations.contains(&(signer, expected)) {
                    expected = expected
                        .checked_add(1)
                        .context("pending nonce prefix overflow")?;
                }
                if nonce != expected
                    || self.pending.len() >= MAX_POOL
                    || canonical_len > limit.saturating_sub(self.pending_bytes)
                {
                    return Ok(UcaAdmission::Backpressured(input, admitted));
                }
                match self.pending_retention.reserve(&input, metadata, limit) {
                    Ok(()) => {}
                    Err(error) if error.is::<PoolBackpressure>() => {
                        self.nonces.bind_current_parent(parent);
                        let has_ready = self.pending.values().any(|entry| {
                            let known = if entry.signer == signer {
                                Some(state.next_nonce())
                            } else {
                                self.nonces.get(&entry.signer).copied()
                            };
                            known == Some(entry.nonce)
                        });
                        if has_ready {
                            return Ok(UcaAdmission::Backpressured(input, admitted));
                        }
                        // Only a real bridge blocked by its larger frame gets
                        // a compact original-APFL single-row projection.
                        let (projected, rebound) =
                            self.node.pipeline.project_admitted_uca_row(&admitted)?;
                        let AdmissionInput::Uca { batch, .. } = &projected else {
                            unreachable!("typed UCA projection");
                        };
                        let rebound_metadata = rebound.retained_metadata_bytes();
                        let retained = batch
                            .retained_bytes()?
                            .checked_add(rebound_metadata)
                            .context("bridge projection retained bytes overflow")?;
                        if retained > reserve {
                            return Ok(UcaAdmission::Backpressured(input, admitted));
                        }
                        if let Err(error) =
                            self.pending_retention
                                .reserve(&projected, rebound_metadata, limit)
                        {
                            if error.is::<PoolBackpressure>() {
                                return Ok(UcaAdmission::Backpressured(input, admitted));
                            }
                            return Err(error);
                        }
                        input = projected;
                        admitted = rebound;
                        metadata = rebound_metadata;
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
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
                        "source_ref":source_ref,"index":source_index,"tx":hex(&id),
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
        self.pool_revision = self.pool_revision.wrapping_add(1);
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

    pub(super) fn retained_uca_reference(
        &self,
        ordered: &[Hash],
    ) -> Result<Option<Arc<AssetSpendBatchV2>>> {
        let mut views = Vec::with_capacity(ordered.len());
        for id in ordered {
            let Some(entry) = self.pending.get(id) else {
                return Ok(None);
            };
            let AdmissionInput::Uca { batch, index } = &entry.input else {
                return Ok(None);
            };
            views.push(batch.view(*index)?);
        }
        // Receive limits, not this node's local selection target. The normal
        // body owner still charges and captures the complete rebuilt input.
        let limits = super::super::native_decode_limits(self.batch_size, true);
        Ok(Some(Arc::new(AssetSpendBatchV2::from_views(
            &views,
            AssetSpendBatchLimitsV2 {
                transactions: limits.transactions,
                transaction_bytes: limits.transaction_bytes,
                body_bytes: limits.body_bytes,
            },
        )?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "real AOEM/native admission; compact facts and exactly-once original APFL pool retention"]
    fn asset_pool_compact_facts_charge_one_source_without_owned_crypto_copies() -> Result<()> {
        super::super::super::uca_native_tests::with_uca_gossip_service_fixture(|nodes, signed| {
            let rpc = &mut nodes[0];
            let raws = signed
                .iter()
                .map(SignedAssetSpendV2::encode)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let batch = Arc::new(AssetSpendBatchV2::from_raw(
                &raws,
                AssetSpendBatchLimitsV2 {
                    transactions: 1024,
                    transaction_bytes: 8192,
                    body_bytes: 8 * 1024 * 1024,
                },
            )?);
            let payload = batch.retained_bytes()?;
            rpc.ingress_batch_boundary = false;
            rpc.incoming_from_client = false;
            rpc.incoming = Some((
                rpc.node
                    .controller
                    .uca_transactions_message(batch.clone())?,
                0,
            ));
            let deadline = Instant::now() + Duration::from_secs(15);
            while rpc.incoming.is_some() || rpc.admission.has_pending_native_admission() {
                ensure!(
                    Instant::now() < deadline,
                    "original compact pool admission timeout"
                );
                rpc.poll_admission()?;
                std::thread::yield_now();
            }
            assert_eq!(rpc.pending.len(), signed.len());
            let mut metadata = 0;
            for entry in rpc.pending.values() {
                let AdmissionInput::Uca {
                    batch: original,
                    index,
                } = &entry.input
                else {
                    anyhow::bail!("unexpected non-asset pool source");
                };
                assert!(Arc::ptr_eq(original, &batch));
                let admitted = entry.admitted.as_ref().context("pool fact missing")?;
                assert!(admitted.matches_uca_source(&batch, *index));
                let fact = admitted.checked_uca().unwrap();
                let expected = std::mem::size_of::<AdmittedTransfer>()
                    + std::mem::size_of::<crate::native_pipeline::ingress::batch::CheckedAdmission>(
                    )
                    + 2 * std::mem::size_of::<usize>()
                    + fact.intent().retained_heap_bytes();
                assert_eq!(admitted.retained_metadata_bytes(), expected);
                assert!(expected < original.transaction_bytes(*index)?);
                metadata += expected;
            }
            assert_eq!(rpc.pending_retention.owners.len(), 1);
            assert_eq!(rpc.pending_retention.metadata_bytes, metadata);
            assert_eq!(rpc.pending_retention.bytes, payload + metadata);
            let make_input = |spend: &SignedAssetSpendV2| -> Result<AdmissionInput> {
                Ok(AdmissionInput::Uca {
                    batch: Arc::new(AssetSpendBatchV2::from_raw(
                        &[spend.encode()?],
                        AssetSpendBatchLimitsV2 {
                            transactions: 1,
                            transaction_bytes: 8192,
                            body_bytes: 8192,
                        },
                    )?),
                    index: 0,
                })
            };
            let id = hash(&signed[0])?;
            let hits = rpc.authentication_cache_hits;
            assert_eq!(rpc.cached_uca(&make_input(&signed[0])?, true)?, Some(id));
            assert_eq!(rpc.authentication_cache_hits, hits + 1);
            assert!(rpc.gossip_queued.contains(&id));
            let mut changed_signature = signed[0].clone();
            changed_signature.signature[0] ^= 1;
            assert_eq!(
                hash(&changed_signature)?,
                id,
                "same intent is not the same signature fact"
            );
            assert!(rpc
                .cached_uca(&make_input(&changed_signature)?, false)?
                .is_none());
            let mut changed_key = signed[0].clone();
            changed_key.public_key[0] ^= 1;
            // A different key with an unchanged key reference is rejected at
            // the existing external structural boundary, before a cache lookup.
            assert_eq!(hash(&changed_key)?, id);
            assert!(make_input(&changed_key).is_err());
            changed_key.intent.spend_key_ref =
                novovm_protocol::uca_spend::spend_key_ref_v1(&changed_key.public_key)?;
            assert!(rpc.cached_uca(&make_input(&changed_key)?, false)?.is_none());
            let mut management = signed[0].clone();
            management.intent.amount = 0;
            management.intent.recipient = management.intent.payer.clone();
            management.intent.authority_update =
                Some(novovm_protocol::uca_spend::AssetAuthorityUpdateV2 {
                    use_recovery: false,
                    next_epoch: management.intent.authorization_epoch + 1,
                    active: true,
                    management_key_ref: [0x91; 32],
                    recovery_key_ref: None,
                    grants: Vec::new(),
                });
            assert!(rpc.cached_uca(&make_input(&management)?, false)?.is_none());
            let mut opening = signed[0].clone();
            let creation = novovm_protocol::uca_spend::AssetAccountCreationV2 {
                management_key_ref: [0x92; 32],
                recovery_key_ref: None,
                grants: Vec::new(),
            };
            opening.intent.recipient = creation
                .account(opening.intent.chain_id, &opening.intent.genesis)?
                .to_vec();
            opening.intent.recipient_creation = Some(creation);
            assert!(rpc.cached_uca(&make_input(&opening)?, false)?.is_none());
            let mut wrong_domain = signed[0].clone();
            wrong_domain.intent.genesis[0] ^= 1;
            assert!(rpc.cached_uca(&make_input(&wrong_domain)?, false).is_err());
            assert_eq!(rpc.authentication_cache_hits, hits + 1);
            assert_eq!(rpc.pending_retention.bytes, payload + metadata);
            for (_, entry) in std::mem::take(&mut rpc.pending) {
                rpc.pending_retention
                    .release(&entry.input, entry.retained_metadata_bytes());
            }
            assert_eq!(rpc.pending_retention.bytes, 0);
            assert_eq!(rpc.pending_retention.metadata_bytes, 0);
            assert!(rpc.pending_retention.owners.is_empty());
            Ok(())
        })
    }

    #[test]
    #[ignore = "real AOEM asset admission; reference reconstruction uses original pool, run alone"]
    fn asset_body_reference_reuses_pool_rows_without_local_target_or_authority_changes(
    ) -> Result<()> {
        super::super::super::uca_native_tests::with_uca_gossip_service_fixture(|nodes, signed| {
            let rpc = &mut nodes[0];
            for spend in signed {
                let raw = spend.encode()?;
                let batch = Arc::new(AssetSpendBatchV2::from_raw(
                    &[raw],
                    AssetSpendBatchLimitsV2 {
                        transactions: 1,
                        transaction_bytes: 8192,
                        body_bytes: 8192,
                    },
                )?);
                rpc.ingress_batch_boundary = false;
                rpc.incoming_from_client = false;
                rpc.incoming = Some((rpc.node.controller.uca_transactions_message(batch)?, 0));
                let deadline = Instant::now() + Duration::from_secs(15);
                while rpc.incoming.is_some() || rpc.admission.has_pending_native_admission() {
                    ensure!(Instant::now() < deadline, "original pool admission timeout");
                    rpc.poll_admission()?;
                    std::thread::yield_now();
                }
            }
            let hashes = signed.iter().map(hash).collect::<Result<Vec<_>>>()?;
            let retained = rpc.pending_retention.bytes;
            let parent = rpc.node.controller.parent();
            let executed = rpc.node.controller.executed();
            rpc.batch_size = 1;
            let batch = rpc
                .retained_uca_reference(&hashes)?
                .context("retained rows missing")?;
            assert_eq!(batch.len(), signed.len());
            for (index, spend) in signed.iter().enumerate() {
                assert!(batch.view(index)?.matches_signed(spend)?);
            }
            let reversed = hashes.iter().rev().copied().collect::<Vec<_>>();
            let reordered = rpc
                .retained_uca_reference(&reversed)?
                .context("reordered rows missing")?;
            for (index, spend) in signed.iter().rev().enumerate() {
                assert!(reordered.view(index)?.matches_signed(spend)?);
            }
            let mut missing = hashes.clone();
            missing[0] = [0xff; 32];
            assert!(rpc.retained_uca_reference(&missing)?.is_none());
            assert_eq!(rpc.pending_retention.bytes, retained);
            assert_eq!(rpc.pending.len(), signed.len());
            assert_eq!(rpc.node.controller.parent(), parent);
            assert_eq!(rpc.node.controller.executed(), executed);
            Ok(())
        })
    }
}
