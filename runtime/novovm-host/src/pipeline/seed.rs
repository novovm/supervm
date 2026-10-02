//! Optional read locality, never a parent/nonce/signing capability. Only a
//! verified persistence completion installs a value in this driver-local slot.
//! Seed memory is charged separately; it never keeps a parent batch slot alive.

use super::*;
use crate::state::frontier::PostStateSeed;

pub(super) struct SeedCache {
    context: BatchContext,
    root: NodeHash,
    receipt: NodeHash,
    state_version: u64,
    seed: PostStateSeed,
    _lease: ByteLease,
}

struct ByteLease {
    usage: Arc<Mutex<Usage>>,
    bytes: usize,
}

impl Drop for ByteLease {
    fn drop(&mut self) {
        let mut usage = self.usage.lock().unwrap_or_else(|error| error.into_inner());
        usage.bytes -= self.bytes;
    }
}

impl SeedCache {
    /// Call only after the driver has checked the exact persistence reply.
    /// No payload is cloned. A missed opportunity must not fail a transaction.
    pub(super) fn try_install(
        seed: PostStateSeed,
        packet: &PreparedCandidate,
        config: &PipelineConfig,
        usage: &Arc<Mutex<Usage>>,
    ) -> Result<Option<Self>> {
        ensure!(
            seed.root() == packet.state_root(),
            "seed/packet poststate mismatch"
        );
        ensure!(
            seed.node_count() <= config.capture.nodes
                && seed.retained_bytes() <= config.capture.bytes,
            "seed exceeds its measured capture budget"
        );
        let bytes = seed.retained_bytes();
        let Some(state_version) = packet
            .context()
            .parent_state_version
            .checked_add(u64::try_from(packet.transaction_count())?)
        else {
            // No representable successor hint. Leave execution/persistence
            // and the consensus layer's independent version checks unchanged.
            return Ok(None);
        };
        let Ok(reserve) = BatchRequest::retained_reservation(
            config.authentication.body_bytes.min(config.plan.body_bytes),
            config,
        ) else {
            // An unusable maximum hint reservation must not reject an already
            // completed smaller ordinary batch.
            return Ok(None);
        };
        let mut used = match usage.try_lock() {
            Ok(used) => used,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => return Ok(None),
        };
        // Leave both a maximum background authentication/bind and its reserved
        // maximum ordinary batch possible even after current tickets drain.
        // Otherwise early-body admission could wait forever for cache bytes
        // whose release itself requires an admitted request. No limits widen.
        if bytes
            .checked_add(reserve)
            .is_none_or(|required| required > config.max_retained_bytes.saturating_sub(used.bytes))
            || reserve
                .checked_mul(2)
                .and_then(|n| n.checked_add(bytes))
                .is_none_or(|required| required > config.max_retained_bytes)
        {
            return Ok(None);
        }
        used.bytes += bytes;
        drop(used);
        Ok(Some(Self {
            context: *packet.context(),
            root: packet.state_root(),
            receipt: packet.receipt_batch_commitment(),
            state_version,
            seed,
            _lease: ByteLease {
                usage: usage.clone(),
                bytes,
            },
        }))
    }

    pub(super) fn for_context(&self, child: &BatchContext) -> Option<&PostStateSeed> {
        let parent = &self.context;
        // This selects useful immutable bytes, NOT a canonical parent. The
        // controller still independently checks parent_block_hash/round/QC.
        // Even identical roots in another candidate confer no extra authority.
        (child.chain_id == parent.chain_id
            && child.genesis_config_commitment == parent.genesis_config_commitment
            && child.protocol_commitment == parent.protocol_commitment
            && child.business_program == parent.business_program
            && child.semantic_version == parent.semantic_version
            && child.effect_contract == parent.effect_contract
            && child.receipt_codec == parent.receipt_codec
            && child.parent_height == parent.height
            && parent.height.checked_add(1) == Some(child.height)
            && child.parent_state_version == self.state_version
            && child.parent_state_root == self.root
            && child.parent_receipt_root == self.receipt)
            .then_some(&self.seed)
    }
}

#[cfg(test)]
mod tests;
