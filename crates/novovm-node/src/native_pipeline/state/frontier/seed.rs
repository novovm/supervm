//! Move-only content from a captured input or its staged poststate. This is
//! neither parent authority nor evidence of persistence. The pipeline must
//! separately bind successful durable completion before retaining a cache.

use super::{CaptureBudget, OwnedStateInput};
use crate::native_pipeline::state::tree::validate_state_node_bytes;
use crate::native_pipeline::state::tree::{NodeHash, StagedStateUpdate};
use anyhow::{ensure, Context, Result};
use std::collections::BTreeMap;

/// No source handle, declarations, signer or publication capability is kept.
/// Both maps are moved, not cloned. Duplicate hashes in the witness and delta
/// still occupy two allocations and are charged twice. Callers must keep the
/// measured bytes charged while this value is retained outside its batch.
pub(crate) struct StateReadSeed {
    root: NodeHash,
    witness: BTreeMap<NodeHash, Vec<u8>>,
    update: Option<StagedStateUpdate>,
    nodes: usize,
    bytes: usize,
}

impl OwnedStateInput {
    /// Move authenticated parent nodes without granting their old declarations
    /// or pretending that a read has produced/persisted a state transition.
    pub(crate) fn into_read_seed(self, budget: CaptureBudget) -> Result<Option<StateReadSeed>> {
        let nodes = self.nodes.len();
        let bytes = self.nodes.iter().try_fold(0usize, |total, (hash, value)| {
            validate_state_node_bytes(hash, value)?;
            total
                .checked_add(std::mem::size_of::<NodeHash>())
                .and_then(|n| n.checked_add(value.len()))
                .context("read seed byte count overflow")
        })?;
        if nodes > budget.nodes || bytes > budget.bytes {
            return Ok(None);
        }
        Ok(Some(StateReadSeed {
            root: self.root,
            witness: self.nodes,
            update: None,
            nodes,
            bytes,
        }))
    }

    /// Consume the exact old input and its tentative delta without copying
    /// their node payloads. Only node/byte fields of the local budget apply;
    /// old declarations are discarded and never inherited by a child.
    pub(crate) fn into_poststate_seed(
        self,
        update: StagedStateUpdate,
        budget: CaptureBudget,
    ) -> Result<Option<StateReadSeed>> {
        ensure!(
            self.root == update.parent_root(),
            "poststate seed delta belongs to another captured parent"
        );
        let nodes = self
            .nodes
            .len()
            .checked_add(update.nodes().len())
            .context("poststate seed node count overflow")?;
        let bytes =
            self.nodes
                .iter()
                .chain(update.nodes())
                .try_fold(0usize, |total, (hash, value)| {
                    validate_state_node_bytes(hash, value)?;
                    total
                        .checked_add(std::mem::size_of::<NodeHash>())
                        .and_then(|n| n.checked_add(value.len()))
                        .context("poststate seed byte count overflow")
                })?;
        // This is an optional optimization, not a stricter transaction limit.
        // Reject corruption above, but discard a sound over-budget seed.
        if nodes > budget.nodes || bytes > budget.bytes {
            return Ok(None);
        }
        Ok(Some(StateReadSeed {
            root: update.root(),
            witness: self.nodes,
            update: Some(update),
            nodes,
            bytes,
        }))
    }
}

impl StateReadSeed {
    pub(crate) fn root(&self) -> NodeHash {
        self.root
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes
    }

    /// Retained logical node content: every hash plus its exact value length.
    /// BTree allocation overhead is not presented as a total memory cap.
    pub(crate) fn retained_bytes(&self) -> usize {
        self.bytes
    }

    pub(super) fn node(&self, hash: &NodeHash) -> Option<&[u8]> {
        self.update
            .as_ref()
            .and_then(|update| update.nodes().get(hash))
            .or_else(|| self.witness.get(hash))
            .map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests;
