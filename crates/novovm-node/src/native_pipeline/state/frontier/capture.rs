//! Incremental content-addressed frontier capture. No source handles, synchronous
//! proxy reads, root replays after a miss, or whole-tree enumeration are used.

use super::{CaptureBudget, DeclaredAccess, OwnedStateInput, PostStateSeed};
use crate::native_pipeline::state::tree::{validate_state_node_bytes, CaptureCursor, NodeHash};
use anyhow::{ensure, Context, Result};
use std::collections::{BTreeMap, VecDeque};

const BULK_KEYS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureStep {
    /// More bounded CPU work is ready; no blocking read is needed yet.
    More,
    /// Submit the next unique hash batch or wait for its existing reply.
    NeedRead,
    /// Every path and every required sibling edge has been checked.
    Complete,
}

/// The caller supplies an independently trusted parent and explicit access
/// declarations. Content hashes validate replies, not parent authority. Exactly
/// one read batch may be outstanding; any failed operation invalidates capture.
pub struct BulkCapture {
    state: OwnedStateInput,
    budget: CaptureBudget,
    cursors: Vec<CaptureCursor>,
    ready: VecDeque<usize>,
    waiting: BTreeMap<NodeHash, Vec<usize>>,
    request: Option<Vec<NodeHash>>,
    complete: usize,
    failed: bool,
    seed_hits: usize,
    #[cfg(test)]
    edge_steps: usize,
}

impl BulkCapture {
    pub fn new(
        root: NodeHash,
        declarations: &[DeclaredAccess],
        budget: CaptureBudget,
    ) -> Result<Self> {
        ensure!(
            !declarations.is_empty() && declarations.len() <= budget.keys,
            "input capture declaration budget exceeded or empty"
        );
        let mut access = BTreeMap::new();
        let mut cursors = Vec::with_capacity(declarations.len());
        let mut ready = VecDeque::new();
        let mut complete = 0;
        for declaration in declarations {
            let cursor = CaptureCursor::new(root, &declaration.key, declaration.may_delete)?;
            ensure!(
                access
                    .insert(
                        declaration.key.clone(),
                        (declaration.may_put, declaration.may_delete)
                    )
                    .is_none(),
                "input capture declarations must have unique keys"
            );
            if cursor.needed_hash().is_some() {
                ready.push_back(cursors.len());
            } else {
                complete += 1;
            }
            cursors.push(cursor);
        }
        Ok(Self {
            state: OwnedStateInput {
                root,
                access,
                nodes: BTreeMap::new(),
                bytes: 0,
            },
            budget,
            cursors,
            ready,
            waiting: BTreeMap::new(),
            request: None,
            complete,
            failed: false,
            seed_hits: 0,
            #[cfg(test)]
            edge_steps: 0,
        })
    }

    pub fn advance(&mut self, max_edge_steps: usize) -> Result<CaptureStep> {
        self.advance_with_seed(max_edge_steps, None)
    }

    /// Newly retained node values, not visits, reused edges or authority checks.
    pub(crate) fn seed_hits(&self) -> usize {
        self.seed_hits
    }

    /// Optional immutable content source, never inherited access or authority.
    /// A different poststate root is an ordinary cache miss. Each hit is copied
    /// only after this child's own bounds are checked, and its cursor still
    /// authenticates every incoming edge. The seed's separate retained-byte
    /// reservation must remain held by its owner for the duration of this call.
    pub(crate) fn advance_with_seed(
        &mut self,
        max_edge_steps: usize,
        seed: Option<&PostStateSeed>,
    ) -> Result<CaptureStep> {
        let result = self.advance_inner(max_edge_steps, seed);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn advance_inner(
        &mut self,
        max_edge_steps: usize,
        seed: Option<&PostStateSeed>,
    ) -> Result<CaptureStep> {
        ensure!(!self.failed, "input capture already failed");
        ensure!(
            max_edge_steps > 0,
            "capture edge-step budget must be nonzero"
        );
        if self.request.is_some() {
            return Ok(CaptureStep::NeedRead);
        }
        let seed = seed.filter(|seed| seed.root() == self.state.root);
        for _ in 0..max_edge_steps {
            let Some(index) = self.ready.pop_front() else {
                break;
            };
            let hash = self.cursors[index]
                .needed_hash()
                .context("completed cursor queued")?;
            if !self.state.nodes.contains_key(&hash) {
                if let Some(bytes) = seed.and_then(|seed| seed.node(&hash)) {
                    validate_state_node_bytes(&hash, bytes)?;
                    let total = self
                        .state
                        .bytes
                        .checked_add(bytes.len())
                        .context("input capture byte count overflow")?;
                    ensure!(
                        total <= self.budget.bytes,
                        "input capture byte budget exceeded"
                    );
                    ensure!(
                        self.state.nodes.len() < self.budget.nodes,
                        "input capture node budget exceeded"
                    );
                    self.state.nodes.insert(hash, bytes.to_vec());
                    self.state.bytes = total;
                    {
                        self.seed_hits += 1;
                    }
                }
            }
            if let Some(bytes) = self.state.nodes.get(&hash) {
                self.cursors[index].advance(bytes)?;
                #[cfg(test)]
                {
                    self.edge_steps += 1;
                }
                if self.cursors[index].needed_hash().is_none() {
                    self.complete += 1;
                } else {
                    self.ready.push_back(index);
                }
            } else {
                self.waiting.entry(hash).or_default().push(index);
            }
        }
        Ok(if self.complete == self.cursors.len() {
            CaptureStep::Complete
        } else if self.ready.is_empty() {
            CaptureStep::NeedRead
        } else {
            CaptureStep::More
        })
    }

    /// A repeated call before accept returns the SAME outstanding batch. It is
    /// safe to retain it across I/O admission backpressure, not to submit twice.
    pub fn next_request(&mut self) -> Result<Option<Vec<NodeHash>>> {
        let result = self.next_request_inner();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn next_request_inner(&mut self) -> Result<Option<Vec<NodeHash>>> {
        ensure!(!self.failed, "input capture already failed");
        if let Some(request) = &self.request {
            return Ok(Some(request.clone()));
        }
        if self.waiting.is_empty() {
            return Ok(None);
        }
        let remaining = self
            .budget
            .nodes
            .checked_sub(self.state.nodes.len())
            .context("input capture node budget exceeded")?;
        ensure!(remaining > 0, "input capture node budget exceeded");
        let request: Vec<_> = self
            .waiting
            .keys()
            .take(BULK_KEYS.min(remaining))
            .copied()
            .collect();
        self.request = Some(request.clone());
        Ok(Some(request))
    }

    /// Values must match the outstanding request's order exactly. Missing
    /// storage content is never a proof of an absent application key. The whole
    /// reply is checked before retaining any prefix or resuming a path.
    pub fn accept(&mut self, values: Vec<Option<Vec<u8>>>) -> Result<()> {
        let result = self.accept_inner(values);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn accept_inner(&mut self, values: Vec<Option<Vec<u8>>>) -> Result<()> {
        ensure!(!self.failed, "input capture already failed");
        let request = self
            .request
            .as_ref()
            .context("capture received an unsolicited reply")?;
        ensure!(
            values.len() == request.len(),
            "capture bulk response count mismatch"
        );
        let mut total = self.state.bytes;
        for (hash, value) in request.iter().zip(&values) {
            let bytes = value
                .as_ref()
                .context("input capture source node missing")?;
            validate_state_node_bytes(hash, bytes)?;
            total = total
                .checked_add(bytes.len())
                .context("input capture byte count overflow")?;
            ensure!(
                total <= self.budget.bytes,
                "input capture byte budget exceeded"
            );
        }
        ensure!(
            self.state
                .nodes
                .len()
                .checked_add(request.len())
                .is_some_and(|n| n <= self.budget.nodes),
            "input capture node budget exceeded"
        );
        for (hash, value) in self
            .request
            .take()
            .expect("request checked")
            .into_iter()
            .zip(values)
        {
            self.state.nodes.insert(hash, value.expect("reply checked"));
            self.ready.extend(
                self.waiting
                    .remove(&hash)
                    .expect("requested hash has waiting cursors"),
            );
        }
        self.state.bytes = total;
        Ok(())
    }

    pub fn finish(self) -> Result<OwnedStateInput> {
        ensure!(
            !self.failed
                && self.complete == self.cursors.len()
                && self.ready.is_empty()
                && self.waiting.is_empty()
                && self.request.is_none(),
            "input capture incomplete or failed"
        );
        Ok(self.state)
    }
}

#[cfg(test)]
mod tests;
