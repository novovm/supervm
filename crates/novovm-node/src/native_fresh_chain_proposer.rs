//! Bounded local selection from authenticated transport staging, not the legacy
//! pending executor. No ACK or pool admission is granted before authentication.
use super::*;
use crate::tx_ingress::candidate_workspace as workspace;
use sha2::{Digest, Sha256};

const MAX_SELECTED: usize = 16;

impl FreshChainLifecycleV1 {
    pub(super) fn propose_from_transactions(
        &mut self,
        events: Vec<ProductMainlineOverlayInboundV1>,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
        wall_ms: u64,
    ) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let config = self
            .config
            .as_ref()
            .context("automatic proposal identity missing")?;
        let height = config
            .height
            .checked_add(1)
            .context("proposal height overflow")?;
        if !config.propose_successors
            || config.authority.expected_leader(height, 0)? != config.local_validator_id
        {
            self.rejected = self.rejected.saturating_add(events.len() as u64);
            return Ok(());
        }
        let parent = workspace::load_finalized_genesis_parent_v1(
            config.chain_id,
            config
                .isolated_workspace_id
                .context("proposal parent missing")?,
            config
                .fresh_genesis_config_commitment
                .context("proposal genesis missing")?,
            &self.params,
        )?;
        if parent.block().header.block_hash != config.block_hash
            || parent.block().header.height != config.height
            || parent.finality_proof().authority != config.authority
        {
            bail!("automatic proposal parent differs from configured finalized authority");
        }
        let context = novovm_protocol::NovBlockExecutionContextV1 {
            chain_id: config.chain_id,
            block_height: height,
            parent_block_hash: config.block_hash,
            slot: parent
                .block()
                .header
                .slot
                .checked_add(1)
                .context("proposal slot overflow")?,
            timestamp_unix_ms: wall_ms.max(parent.block().header.timestamp_unix_ms),
        };
        let mut selected = Vec::new();
        for event in events {
            if selected.len() == MAX_SELECTED {
                break;
            }
            let raw = event.frame.payload;
            let hash = crate::tx_ingress::canonical_nov_native_tx_hash_from_payload_v1(&raw);
            let digest: [u8; 32] = Sha256::digest(&raw).into();
            if hash.ok() != Some(event.object_hash) || digest != event.payload_sha256 {
                self.rejected = self.rejected.saturating_add(1);
                continue;
            }
            selected.push(raw);
            // Authenticate the entire proposed prefix against the same verified
            // parent: signatures, identity, chain, exact nonce ordering and size.
            // This call has no pending/reservation/authority mutation.
            if parent
                .successor_plan(context, selected.clone(), &self.params)
                .is_err()
            {
                selected.pop();
                self.rejected = self.rejected.saturating_add(1);
            }
        }
        if selected.is_empty() {
            return Ok(());
        }
        let next = config.clone().prepare_fresh_successor(
            context.slot,
            context.timestamp_unix_ms,
            selected,
            &self.params,
        )?;
        let service = NovNativeSealServiceV1::open_configured(
            next.clone(),
            &self.ledger_path,
            &self.params,
            runtime,
            now,
        )?;
        // Preparation/registration is not a vote. The next lifecycle tick uses
        // the existing durable anti-equivocation and V3 confirmation scheduler.
        self.config = Some(next);
        self.service = Some(Box::new(service));
        self.publication = None;
        self.bodies = None;
        for queue in self.pending.values_mut() {
            queue.clear();
        }
        self.proposed_successors = self.proposed_successors.saturating_add(1);
        Ok(())
    }
}
