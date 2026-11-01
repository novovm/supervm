//! Optional read locality, never a parent/nonce/signing capability. The ONE
//! slot is shared by admission capture and the original candidate driver.
//! Seed memory is charged separately; it never keeps a parent batch slot alive.

use super::*;
use crate::native_pipeline::state::frontier::StateReadSeed;
use novovm_consensus::round_bft::journal::ParentPoint;

pub(super) type SeedSlot = Arc<Mutex<Option<Arc<SeedCache>>>>;

/// An exact-parent, pipeline-owned reference. Keeping a reader alive keeps the
/// original measured byte lease alive even when the single slot is replaced.
pub(crate) struct ReadSeedLease {
    cache: Arc<SeedCache>,
    owner: Arc<()>,
    parent: ParentPoint,
    context: BatchContext,
}

pub(super) struct SeedCache {
    context: BatchContext,
    root: NodeHash,
    receipt: NodeHash,
    state_version: u64,
    captured_parent: Option<ParentPoint>,
    seed: StateReadSeed,
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
        seed: StateReadSeed,
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
        let Some(state_version) = packet
            .context()
            .parent_state_version
            .checked_add(u64::try_from(packet.transaction_count())?)
        else {
            // No representable successor hint. Leave execution/persistence
            // and the consensus layer's independent version checks unchanged.
            return Ok(None);
        };
        let Some(lease) = Self::reserve(&seed, config, usage)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            context: *packet.context(),
            root: packet.state_root(),
            receipt: packet.receipt_batch_commitment(),
            state_version,
            captured_parent: None,
            seed,
            _lease: lease,
        }))
    }

    fn reserve(
        seed: &StateReadSeed,
        config: &PipelineConfig,
        usage: &Arc<Mutex<Usage>>,
    ) -> Result<Option<ByteLease>> {
        let bytes = seed.retained_bytes();
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
        Ok(Some(ByteLease {
            usage: usage.clone(),
            bytes,
        }))
    }

    fn try_install_capture(
        seed: StateReadSeed,
        context: BatchContext,
        parent: ParentPoint,
        config: &PipelineConfig,
        usage: &Arc<Mutex<Usage>>,
    ) -> Result<Option<Self>> {
        ensure!(
            context_matches_parent(&context, &parent) && seed.root() == parent.state_root,
            "captured read seed parent mismatch"
        );
        ensure!(
            seed.node_count() <= config.capture.nodes
                && seed.retained_bytes() <= config.capture.bytes,
            "captured read seed exceeds its measured budget"
        );
        let Some(lease) = Self::reserve(&seed, config, usage)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            context,
            root: parent.state_root,
            receipt: parent.receipt_batch_commitment,
            state_version: parent.state_version,
            captured_parent: Some(parent),
            seed,
            _lease: lease,
        }))
    }

    fn parent_height(&self) -> u64 {
        self.captured_parent
            .map_or(self.context.height, |p| p.height)
    }

    pub(super) fn for_context(&self, child: &BatchContext) -> Option<&StateReadSeed> {
        let parent = &self.context;
        if let Some(captured) = &self.captured_parent {
            return (same_domain(child, parent) && context_matches_parent(child, captured))
                .then_some(&self.seed);
        }
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

fn same_domain(left: &BatchContext, right: &BatchContext) -> bool {
    left.chain_id == right.chain_id
        && left.genesis_config_commitment == right.genesis_config_commitment
        && left.protocol_commitment == right.protocol_commitment
        && left.business_program == right.business_program
        && left.semantic_version == right.semantic_version
        && left.effect_contract == right.effect_contract
        && left.receipt_codec == right.receipt_codec
}

fn context_matches_parent(context: &BatchContext, parent: &ParentPoint) -> bool {
    context.parent_height == parent.height
        && parent.height.checked_add(1) == Some(context.height)
        && context.parent_block_hash == parent.block_hash
        && context.parent_state_root == parent.state_root
        && context.parent_receipt_root == parent.receipt_batch_commitment
        && context.parent_state_version == parent.state_version
}

impl CandidatePipeline {
    pub(crate) fn capture_read_seed(
        &self,
        context: BatchContext,
        parent: ParentPoint,
    ) -> Option<ReadSeedLease> {
        if !context_matches_parent(&context, &parent) {
            return None;
        }
        let slot = self.seed.try_lock().ok()?;
        let cache = slot.as_ref()?;
        if cache.captured_parent.is_some_and(|known| known != parent)
            || cache.for_context(&context).is_none()
        {
            return None;
        }
        Some(ReadSeedLease {
            cache: cache.clone(),
            owner: self.identity.clone(),
            parent,
            context,
        })
    }

    pub(crate) fn read_seed_material<'a>(
        &self,
        lease: &'a ReadSeedLease,
        context: &BatchContext,
        parent: &ParentPoint,
    ) -> Option<&'a StateReadSeed> {
        (Arc::ptr_eq(&lease.owner, &self.identity)
            && lease.parent == *parent
            && same_domain(&lease.context, context)
            && context_matches_parent(context, parent))
        .then(|| lease.cache.for_context(context))
        .flatten()
    }

    /// Only verified read material is retained; this does not publish a parent
    /// or an admission decision. Cache pressure is an ordinary optimization miss.
    pub(crate) fn retain_capture_read_seed(
        &self,
        input: crate::native_pipeline::state::frontier::OwnedStateInput,
        context: BatchContext,
        parent: ParentPoint,
    ) -> Result<()> {
        ensure!(
            context_matches_parent(&context, &parent) && input.parent_root() == parent.state_root,
            "captured read seed binding mismatch"
        );
        let Some(seed) = input.into_read_seed(self.config.capture)? else {
            return Ok(());
        };
        let mut slot = match self.seed.try_lock() {
            Ok(slot) => slot,
            Err(_) => return Ok(()),
        };
        if slot.as_ref().is_some_and(|old| {
            old.parent_height() > parent.height
                || old.for_context(&context).is_some() && old.seed.node_count() >= seed.node_count()
        }) {
            return Ok(());
        }
        let Some(cache) =
            SeedCache::try_install_capture(seed, context, parent, &self.config, &self.usage)?
        else {
            return Ok(());
        };
        let old = slot.replace(Arc::new(cache));
        drop(slot);
        drop(old);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
