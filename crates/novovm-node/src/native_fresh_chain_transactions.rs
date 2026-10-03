use super::*;
use crate::product_mainline_overlay::{
    ProductMainlineOverlayEventV1 as TransportEvent,
    ProductMainlineOverlayRecipientAckDispositionV1 as ReceiptKind,
    ProductMainlineOverlayRecipientAckV1 as Receipt,
};
use crate::tx_ingress::fresh_pool::PendingTransaction;
use sha2::{Digest, Sha256};

impl FreshChainLifecycleV1 {
    /// A read from the immutable finalized parent, never the pool, speculative
    /// candidate or legacy Host JSON projection. Query errors are not balances.
    pub(crate) fn finalized_nov_balance(&self, account: &str) -> Result<serde_json::Value> {
        if self.halted {
            bail!("fresh lifecycle halted");
        }
        if self.candidate_storage_busy() {
            bail!("candidate durable stage busy; retry state query");
        }
        let parent = self
            .finalized_parent
            .as_ref()
            .context("finalized state unavailable")?;
        let balance = parent.with_records(&self.params, |reader| reader.nov_balance(account))?;
        let block = parent.block();
        Ok(serde_json::json!({
            "method":"nov_getAssetBalance", "account":account, "asset":"NOV",
            "found":balance.is_some(), "balance":balance.unwrap_or(0).to_string(),
            "finalized":true, "finalized_tip_height":block.header.height,
            "block_hash":crate::native_block_seal::hex_v1(&block.header.block_hash),
            "state_root":crate::native_block_seal::hex_v1(&block.header.post_state_root),
        }))
    }

    pub fn submit_raw_transaction(&mut self, raw: Vec<u8>) -> Result<serde_json::Value> {
        self.submit_raw_transactions(vec![raw])
            .pop()
            .expect("one submission result")
    }

    /// One owner-thread admission turn. This shares only immutable parent reads
    /// and the durable pool commit; it does not reserve execution nonces, charge
    /// fees, grant signing authority, or change any per-transaction wire rules.
    pub fn submit_raw_transactions(
        &mut self,
        raws: Vec<Vec<u8>>,
    ) -> Vec<Result<serde_json::Value>> {
        if raws.len() > crate::tx_ingress::fresh_pool::MAX_ENTRIES {
            return raws
                .into_iter()
                .map(|_| Err(anyhow::anyhow!("transaction admission batch limit")))
                .collect();
        }
        if self.halted || self.candidate_storage_busy() {
            return raws
                .into_iter()
                .map(|_| {
                    Err(anyhow::anyhow!(
                        "fresh lifecycle halted or durable stage busy"
                    ))
                })
                .collect();
        }
        let mut results: Vec<Option<Result<serde_json::Value>>> =
            (0..raws.len()).map(|_| None).collect();
        let mut entries = Vec::with_capacity(raws.len());
        for (index, raw) in raws.into_iter().enumerate() {
            match PendingTransaction::authenticate(raw, self.chain, &self.params) {
                Ok(entry) => entries.push((index, entry)),
                Err(error) => results[index] = Some(Err(error)),
            }
        }
        if entries.is_empty() {
            return results
                .into_iter()
                .map(|result| result.expect("authentication result"))
                .collect();
        }
        let checked = if let Some(parent) = &self.finalized_parent {
            parent.with_records(&self.params, |reader| {
                let mut live = Vec::new();
                let mut nonces = BTreeMap::new();
                for (index, entry) in entries {
                    if let Some(receipt) = reader.receipt(&entry.hash)? {
                        results[index] = Some(Ok(serde_json::json!({
                            "tx_hash":crate::native_block_seal::hex_v1(&entry.hash),
                            "status":"finalized", "receipt":receipt,
                            "finalized_tip_height":parent.block().header.height,
                        })));
                        continue;
                    }
                    let nonce = match nonces.entry(entry.identity.clone()) {
                        std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
                        std::collections::btree_map::Entry::Vacant(slot) => {
                            *slot.insert(reader.next_nonce(&entry.identity)?)
                        }
                    };
                    if nonce > entry.nonce {
                        results[index] =
                            Some(Err(anyhow::anyhow!("transaction nonce already consumed")));
                    } else {
                        live.push((index, entry));
                    }
                }
                Ok(live)
            })
        } else {
            Ok(entries)
        };
        let admission = checked.and_then(|live| {
            if live.is_empty() {
                return Ok(());
            }
            let pool = self
                .pool
                .as_mut()
                .context("durable transaction ingress is disabled")?;
            let (positions, entries): (Vec<_>, Vec<_>) = live
                .into_iter()
                .map(|(index, entry)| ((index, entry.hash), entry))
                .unzip();
            let retained = match pool.insert_batch(entries) {
                Ok(retained) => retained,
                Err(error) => {
                    self.halted = true;
                    self.proposal_window.clear();
                    return Err(error.context("transaction persistence failed; restart required"));
                }
            };
            for ((index, hash), retained) in positions.into_iter().zip(retained) {
                results[index] = Some(if retained {
                    Ok(serde_json::json!({
                        "tx_hash":crate::native_block_seal::hex_v1(&hash),
                        "status":"queued", "finalized":false,
                    }))
                } else {
                    Err(anyhow::anyhow!(
                        "transaction pool capacity or signer nonce conflict"
                    ))
                });
            }
            Ok(())
        });
        let failure = admission.err().map(|error| format!("{error:#}"));
        // A failed parent read or commit publishes no queued prefix. Already
        // verified finalized receipts remain independent read-only results;
        // parent read failures, unlike persistence failures, do not halt.
        results
            .into_iter()
            .map(|result| {
                result.unwrap_or_else(|| {
                    Err(anyhow::anyhow!(failure
                        .as_deref()
                        .unwrap_or("transaction admission result missing")
                        .to_owned()))
                })
            })
            .collect()
    }

    pub fn transaction_status(&self, hash: [u8; 32]) -> Result<serde_json::Value> {
        if self.halted {
            bail!("fresh lifecycle halted");
        }
        if self.candidate_storage_busy() {
            bail!("candidate durable stage busy; retry state query");
        }
        let hex = crate::native_block_seal::hex_v1(&hash);
        if let Some(parent) = &self.finalized_parent {
            if let Some(receipt) =
                parent.with_records(&self.params, |reader| reader.receipt(&hash))?
            {
                return Ok(
                    serde_json::json!({"tx_hash":hex,"status":"finalized","receipt":receipt,"finalized_tip_height":parent.block().header.height}),
                );
            }
        }
        let status = if self.pool.as_ref().is_some_and(|pool| pool.contains(&hash)) {
            "queued"
        } else {
            "unknown"
        };
        Ok(serde_json::json!({"tx_hash":hex,"status":status,"finalized":false}))
    }

    /// These events only affect transport retries, never consensus or pool
    /// ownership. A new connection forgets receipt suppression conservatively.
    pub fn observe_transaction_transport_event(&mut self, event: &TransportEvent) {
        if self.halted {
            return;
        }
        match event {
            TransportEvent::RecipientAck { ack, .. } => {
                self.accept_transaction_receipt(ack);
            }
            TransportEvent::E2eSessionEstablished { remote_peer_id }
            | TransportEvent::PeerIsolated { remote_peer_id, .. } => {
                self.transaction_transport
                    .reset_peer_receipts(remote_peer_id);
            }
            TransportEvent::RelayDisconnected { .. }
            | TransportEvent::RelayRotated { .. }
            | TransportEvent::WorkerStopped
            | TransportEvent::WorkerFailed(_) => {
                self.transaction_transport.reset_all_receipts();
            }
            _ => {}
        }
    }

    fn accept_transaction_receipt(&mut self, receipt: &Receipt) -> bool {
        let Some(config) = &self.config else {
            return false;
        };
        if !config.transaction_transport_limits().durable_receipts
            || receipt.disposition != ReceiptKind::PendingTransactionPersisted
            || receipt.payload_class != ProductMainlineOverlayPayloadClassV1::NativeTransaction
        {
            return false;
        }
        let Some(local) = config
            .authority
            .transport_bindings
            .iter()
            .find(|binding| binding.validator_id == config.local_validator_id)
        else {
            return false;
        };
        let Some(entry) = self
            .pool
            .as_ref()
            .and_then(|pool| pool.get(&receipt.object_hash))
        else {
            return false;
        };
        if receipt
            .verify_route(
                self.chain,
                &local.transport_peer_id,
                &receipt.recipient_peer_id,
            )
            .is_err()
        {
            return false;
        }
        self.transaction_transport.acknowledge(
            &receipt.recipient_peer_id,
            entry,
            receipt.payload_sha256,
        )
    }

    pub(super) fn poll_transactions(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        if self.pool.is_none() {
            return Ok(());
        }
        let receipts_enabled = self
            .config
            .as_ref()
            .is_some_and(|config| config.transaction_transport_limits().durable_receipts);
        let (events, rate_rejected) = self.transaction_transport.drain(now);
        self.rejected = self.rejected.saturating_add(rate_rejected as u64);
        let mut authenticated = Vec::with_capacity(events.len());
        let mut bindings = Vec::with_capacity(events.len());
        for mut event in events {
            let digest: [u8; 32] = Sha256::digest(&event.frame.payload).into();
            let entry = PendingTransaction::authenticate(
                std::mem::take(&mut event.frame.payload),
                self.chain,
                &self.params,
            );
            let Ok(entry) = entry else {
                self.rejected = self.rejected.saturating_add(1);
                continue;
            };
            if entry.hash != event.object_hash || digest != event.payload_sha256 {
                self.rejected = self.rejected.saturating_add(1);
                continue;
            }
            if receipts_enabled
                && event.delivery_id
                    != crate::product_delivery_journal::product_delivery_id_v1(
                        self.chain,
                        event.payload_class.label(),
                        event.object_hash,
                        digest,
                        &event.source_peer_id,
                        &runtime.startup().local_peer_id,
                    )
            {
                self.rejected = self.rejected.saturating_add(1);
                continue;
            }
            authenticated.push(entry);
            // Retain bounded metadata only, not a second raw payload copy.
            bindings.push(event);
        }
        let pool = self.pool.as_mut().expect("pool exists");
        let admitted = pool.insert_live_batch_with_retention(
            authenticated,
            self.finalized_parent.as_ref(),
            &self.params,
        )?;
        self.rejected = self.rejected.saturating_add(admitted.rejected);
        if let Some(parent) = &self.finalized_parent {
            pool.reconcile_rooted(parent, &self.params)?;
        }
        if receipts_enabled {
            for (binding, retained) in bindings.iter().zip(admitted.retained) {
                if retained && pool.contains(&binding.object_hash) {
                    // Full raw equality was checked for this individual input.
                    // False/backpressure does not suppress any future ACK:
                    // the sender retries and exact durable duplicates re-ACK.
                    runtime.try_submit_pending_transaction_ack(binding)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn gossip_transactions(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        // Borrow the bounded pool, copying raw bytes only for selected sends.
        // The durable pool remains authoritative across transport backpressure.
        self.transaction_transport
            .gossip(&pool.ordered_refs(), now, |peer, entry| {
                runtime.try_submit_to_peer(
                    peer,
                    ProductMainlineOverlayPayloadClassV1::NativeTransaction,
                    entry.hash,
                    entry.raw.clone(),
                )
            })
    }
}
