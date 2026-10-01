//! Bounded local selection from authenticated transport staging, not the legacy
//! pending executor. No ACK or pool admission is granted before authentication.
use super::*;
use crate::tx_ingress::candidate_workspace as workspace;

impl FreshChainLifecycleV1 {
    pub(super) fn propose_from_pool(
        &mut self,
        runtime: &ProductMainlineOverlayRuntimeV1,
        now: Instant,
        wall_ms: u64,
    ) -> Result<()> {
        let Some(pool) = &self.pool else {
            self.proposal_window.clear();
            return Ok(());
        };
        if pool.is_empty() {
            self.proposal_window.clear();
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
        let Some(round) = self
            .pacemaker
            .as_ref()
            .and_then(pacemaker::ParentPacemaker::proposal_round)
        else {
            self.proposal_window.clear();
            return Ok(());
        };
        if !config.propose_successors
            || config.authority.scheduled_leader_v1(height, round)? != config.local_validator_id
        {
            self.proposal_window.clear();
            return Ok(());
        }
        let parent = workspace::load_finalized_parent_view_v1(
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
        if !clock::timestamp_allowed(parent.block().header.timestamp_unix_ms, wall_ms) {
            self.clock_waiting = true;
            self.proposal_window.clear();
            return Ok(());
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
        // Keep the pool's exact identity/nonce ordering, but derive and verify
        // each item's identity from raw bytes once. The immutable parent is
        // opened once and each distinct signer's starting nonce is read once.
        let selected = parent.select_ordered_transactions(
            pool.ordered().into_iter().map(|entry| entry.raw).collect(),
            config.proposal_max_transactions,
            &self.params,
        )?;
        // A collection window is only a scheduling hint. Every poll still
        // checks the live parent and selects/authenticates the current pool;
        // neither raw transactions nor signing authority are cached in it.
        if !self.proposal_window.ready(
            proposal_window::ProposalContext {
                parent_workspace_id: config
                    .isolated_workspace_id
                    .context("proposal parent missing")?,
                parent_block_hash: config.block_hash,
                authority_commitment: config.authority.authority_commitment,
                height,
                round,
            },
            selected.len(),
            config.proposal_max_transactions,
            now,
            config.proposal_collect,
        )? {
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
        if let Some(certificate) = self
            .pacemaker
            .as_ref()
            .and_then(pacemaker::ParentPacemaker::certificate)
        {
            service.admit_successor_new_view(certificate)?;
        }
        // Preparation/registration is not a vote. The next lifecycle tick uses
        // the existing durable anti-equivocation and V3 confirmation scheduler.
        self.config = Some(next);
        self.service = Some(Box::new(service));
        self.publication = None;
        self.bodies = None;
        self.pacemaker = None;
        self.proposal_window.clear();
        for queue in self.pending.values_mut() {
            queue.clear();
        }
        for queue in self.round_pending.values_mut() {
            queue.clear();
        }
        self.proposed_successors = self.proposed_successors.saturating_add(1);
        Ok(())
    }
}
