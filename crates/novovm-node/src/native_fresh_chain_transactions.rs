use super::*;
use crate::product_mainline_overlay::{
    ProductMainlineOverlayEventV1 as TransportEvent,
    ProductMainlineOverlayRecipientAckDispositionV1 as ReceiptKind,
    ProductMainlineOverlayRecipientAckV1 as Receipt,
};
use crate::tx_ingress::fresh_pool::PendingTransaction;
use sha2::{Digest, Sha256};

impl FreshChainLifecycleV1 {
    pub fn submit_raw_transaction(&mut self, raw: Vec<u8>) -> Result<serde_json::Value> {
        if self.halted {
            bail!("fresh lifecycle halted");
        }
        let entry = PendingTransaction::authenticate(raw, self.chain, &self.params)?;
        if let Some(parent) = &self.finalized_parent {
            let (receipt, nonce) = parent.with_records(&self.params, |reader| {
                let receipt = reader.receipt(&entry.hash)?;
                let nonce = if receipt.is_none() {
                    Some(reader.next_nonce(&entry.identity)?)
                } else {
                    None
                };
                Ok((receipt, nonce))
            })?;
            if let Some(receipt) = receipt {
                return Ok(serde_json::json!({
                    "tx_hash":crate::native_block_seal::hex_v1(&entry.hash),
                    "status":"finalized", "receipt":receipt,
                    "finalized_tip_height":parent.block().header.height,
                }));
            }
            if nonce.is_some_and(|nonce| nonce > entry.nonce) {
                bail!("transaction nonce already consumed");
            }
        }
        let hash = entry.hash;
        match self
            .pool
            .as_mut()
            .context("durable transaction ingress is disabled")?
            .insert(entry)
        {
            // The immutable cached finalized view was already checked above.
            // Successful insertion guarantees queued status; no second DB read.
            Ok(true) => Ok(serde_json::json!({
                "tx_hash":crate::native_block_seal::hex_v1(&hash),
                "status":"queued", "finalized":false,
            })),
            Ok(false) => bail!("transaction pool capacity or signer nonce conflict"),
            Err(error) => {
                self.halted = true;
                self.proposal_window.clear();
                Err(error.context("transaction persistence failed; restart required"))
            }
        }
    }

    pub fn transaction_status(&self, hash: [u8; 32]) -> Result<serde_json::Value> {
        if self.halted {
            bail!("fresh lifecycle halted");
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
