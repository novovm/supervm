use super::*;
use crate::tx_ingress::fresh_pool::PendingTransaction;
use sha2::{Digest, Sha256};

impl FreshChainLifecycleV1 {
    pub fn submit_raw_transaction(&mut self, raw: Vec<u8>) -> Result<serde_json::Value> {
        if self.halted {
            bail!("fresh lifecycle halted");
        }
        let entry = PendingTransaction::authenticate(raw, self.chain, &self.params)?;
        let status = self.transaction_status(entry.hash)?;
        if status["status"] == "finalized" {
            return Ok(status);
        }
        if let Some(parent) = &self.finalized_parent {
            if parent
                .state()
                .module_state
                .native_auth_next_nonces
                .get(&entry.identity)
                .copied()
                .unwrap_or(0)
                > entry.nonce
            {
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
            Ok(true) => self.transaction_status(hash),
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
            if let Some(receipt) = parent.state().receipts.get(&hex) {
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

    pub(super) fn poll_transactions(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
    ) -> Result<()> {
        if self.pool.is_none() {
            return Ok(());
        }
        let limit = self
            .config
            .as_ref()
            .context("transaction configuration missing")?
            .ingress_per_source_per_second;
        let mut events = Vec::new();
        for queue in self.pending.values_mut() {
            let mut remaining = VecDeque::new();
            while let Some(event) = queue.pop_front() {
                if event.payload_class == ProductMainlineOverlayPayloadClassV1::NativeTransaction {
                    events.push(event);
                } else {
                    remaining.push_back(event);
                }
            }
            *queue = remaining;
        }
        for event in events {
            let budget = self
                .transaction_budgets
                .get_mut(&event.source_peer_id)
                .context("transaction source missing")?;
            if now.duration_since(budget.0) >= Duration::from_secs(1) {
                *budget = (now, 0);
            }
            if budget.1 >= limit {
                self.rejected = self.rejected.saturating_add(1);
                continue;
            }
            budget.1 += 1;
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
            pool.reconcile(parent)?;
        }
        if now < self.next_gossip {
            return Ok(());
        }
        self.next_gossip = now + Duration::from_secs(1);
        let entries = pool.ordered();
        if entries.is_empty() {
            return Ok(());
        }
        for _ in 0..entries.len().min(4) {
            let entry = &entries[self.gossip_cursor % entries.len()];
            self.gossip_cursor = (self.gossip_cursor + 1) % entries.len();
            for peer in self.pending.keys() {
                runtime.try_submit_to_peer(
                    peer,
                    ProductMainlineOverlayPayloadClassV1::NativeTransaction,
                    entry.hash,
                    entry.raw.clone(),
                )?;
            }
        }
        Ok(())
    }
}
