//! Owned, bounded input for an execution batch. Capture reads a trusted parent;
//! execution reads only this object. It cannot reopen storage or acquire a
//! workspace lock. Missing captured data is an error, not an absent account.
//!
//! This is an input boundary, NOT transaction authentication, a business proof,
//! durable completion, or permission to publish. The immutable tree algorithm
//! is migrated separately; no legacy crate is a dependency.

use super::tree::{
    capture_update_path_v1, read_state_value, read_state_values, stage_state_update,
    validate_state_node_bytes, NodeHash, StagedStateUpdate, StateChange, StateNodeReader,
    MAX_BATCH_READ_KEYS,
};
use anyhow::{bail, Context, Result};
use std::cell::RefCell;
use std::collections::BTreeMap;

mod capture;
pub use capture::{BulkCapture, CaptureStep};
mod seed;
pub(crate) use seed::PostStateSeed;
mod witness;

#[derive(Clone, Debug)]
pub struct DeclaredAccess {
    pub key: Vec<u8>,
    pub may_put: bool,
    pub may_delete: bool,
}

/// Local admission budgets, not consensus rules or ledger-size limits.
#[derive(Clone, Copy, Debug)]
pub struct CaptureBudget {
    pub keys: usize,
    pub nodes: usize,
    pub bytes: usize,
}

/// All fields are private; callers cannot replace the root or widen access.
/// The type has no storage path, source reader, callbacks or authority token.
/// The low-level reader is deliberately not exposed:
///
/// ```compile_fail
/// use novovm_host::state::{frontier::OwnedStateInput, tree::StateNodeReader};
/// fn bypass(input: &OwnedStateInput) -> &dyn StateNodeReader { input }
/// ```
pub struct OwnedStateInput {
    root: NodeHash,
    access: BTreeMap<Vec<u8>, (bool, bool)>,
    nodes: BTreeMap<NodeHash, Vec<u8>>,
    bytes: usize,
}

impl OwnedStateInput {
    pub fn capture(
        source: &dyn StateNodeReader,
        trusted_parent: NodeHash,
        declarations: &[DeclaredAccess],
        budget: CaptureBudget,
    ) -> Result<Self> {
        if declarations.is_empty() || declarations.len() > budget.keys {
            bail!("input capture declaration budget exceeded or empty");
        }
        let mut access = BTreeMap::new();
        for declaration in declarations {
            // Validate before retaining an attacker-controlled allocation.
            super::tree::state_key_hash(&declaration.key)?;
            if access
                .insert(
                    declaration.key.clone(),
                    (declaration.may_put, declaration.may_delete),
                )
                .is_some()
            {
                bail!("input capture declarations must have unique keys");
            }
        }
        let capture = CaptureReader {
            source,
            budget,
            data: RefCell::new(CapturedNodes::default()),
        };
        for declaration in declarations {
            capture_update_path_v1(
                &capture,
                trusted_parent,
                &declaration.key,
                declaration.may_delete,
            )?;
        }
        let captured = capture.data.into_inner();
        Ok(Self {
            root: trusted_parent,
            access,
            nodes: captured.nodes,
            bytes: captured.bytes,
        })
    }

    pub fn parent_root(&self) -> NodeHash {
        self.root
    }

    pub fn captured_nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn captured_bytes(&self) -> usize {
        self.bytes
    }

    pub fn read(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if !self.access.contains_key(key) {
            bail!("execution read outside declared input");
        }
        read_state_value(&OwnedNodeReader(self), self.root, key)
    }

    /// Read at most 4096 declared keys from this immutable parent in one shared
    /// traversal. Results retain input order and duplicates. All permissions
    /// are checked before traversing any node; no partial result escapes on an
    /// error. Captured sibling bytes never grant access to undeclared keys.
    pub fn read_many(&self, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>> {
        if keys.len() > MAX_BATCH_READ_KEYS {
            bail!("execution read batch exceeds key budget");
        }
        for key in keys {
            if !self.access.contains_key(key) {
                bail!("execution read outside declared input");
            }
        }
        read_state_values(&OwnedNodeReader(self), self.root, keys)
    }

    /// Pure isolated effects. Repeated changes retain their supplied order.
    /// Every call starts at the captured parent, not at the result of a previous
    /// call. Supply one ordered batch; this object is not a mutable overlay.
    /// Callers must persist and independently verify completion later; this
    /// function does not write storage, receipts, finality or an authority head.
    pub fn stage(&self, changes: &[StateChange]) -> Result<StagedStateUpdate> {
        for change in changes {
            let (key, put) = match change {
                StateChange::Put { key, .. } => (key, true),
                StateChange::Delete { key } => (key, false),
            };
            let &(may_put, may_delete) = self
                .access
                .get(key)
                .context("execution write outside declared input")?;
            if (put && !may_put) || (!put && !may_delete) {
                bail!("execution change exceeds declared permission");
            }
        }
        stage_state_update(&OwnedNodeReader(self), self.root, changes)
    }
}

// Do not expose StateNodeReader on OwnedStateInput: callers could otherwise
// bypass declared permissions through the generic tree API and read/change an
// undeclared sibling that was captured solely for deletion path compression.
struct OwnedNodeReader<'a>(&'a OwnedStateInput);

impl StateNodeReader for OwnedNodeReader<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.0
            .nodes
            .get(hash)
            .cloned()
            .map(Some)
            .context("execution requested a node outside captured frontier")
    }
}

#[derive(Default)]
struct CapturedNodes {
    nodes: BTreeMap<NodeHash, Vec<u8>>,
    bytes: usize,
}

struct CaptureReader<'a> {
    source: &'a dyn StateNodeReader,
    budget: CaptureBudget,
    data: RefCell<CapturedNodes>,
}

impl StateNodeReader for CaptureReader<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        if let Some(bytes) = self.data.borrow().nodes.get(hash) {
            return Ok(Some(bytes.clone()));
        }
        if self.data.borrow().nodes.len() >= self.budget.nodes {
            bail!("input capture node budget exceeded");
        }
        let bytes = self
            .source
            .read_node(hash)?
            .context("input capture source node missing")?;
        validate_state_node_bytes(hash, &bytes)?;
        let mut captured = self.data.borrow_mut();
        let next_bytes = captured
            .bytes
            .checked_add(bytes.len())
            .context("input capture byte count overflow")?;
        if next_bytes > self.budget.bytes {
            bail!("input capture byte budget exceeded");
        }
        captured.bytes = next_bytes;
        captured.nodes.insert(*hash, bytes.clone());
        Ok(Some(bytes))
    }
}

#[cfg(test)]
mod tests;
