use super::*;
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

    pub(super) fn poll_transactions(&mut self, now: Instant) -> Result<()> {
        if self.pool.is_none() {
            return Ok(());
        }
        let (events, rate_rejected) = self.transaction_transport.drain(now);
        self.rejected = self.rejected.saturating_add(rate_rejected as u64);
        for event in events {
            let digest: [u8; 32] = Sha256::digest(&event.frame.payload).into();
            let entry =
                PendingTransaction::authenticate(event.frame.payload, self.chain, &self.params);
            let Ok(entry) = entry else {
                self.rejected = self.rejected.saturating_add(1);
                continue;
            };
            if entry.hash != event.object_hash || digest != event.payload_sha256 {
                self.rejected = self.rejected.saturating_add(1);
                continue;
            }
            if !self.pool.as_mut().expect("pool exists").insert(entry)? {
                self.rejected = self.rejected.saturating_add(1);
            }
        }
        let pool = self.pool.as_mut().expect("pool exists");
        if let Some(parent) = &self.finalized_parent {
            pool.reconcile_rooted(parent, &self.params)?;
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
