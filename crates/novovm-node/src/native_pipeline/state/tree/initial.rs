//! Startup-only batching over the original immutable tree planner. The batch
//! budget limits one operation, never the total number of initialized accounts.
use super::*;

pub(crate) struct InitialStateBuilder {
    root: NodeHash,
    nodes: BTreeMap<NodeHash, Vec<u8>>,
    pending: Vec<StateChange>,
    loaded_nodes: usize,
    #[cfg(test)]
    staged_calls: usize,
    #[cfg(test)]
    max_batched_puts: usize,
}

impl StateNodeReader for InitialStateBuilder {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.nodes.get(hash).cloned())
    }
}

const _: () = assert!(InitialStateBuilder::CHANGES_PER_BATCH * 258 < MAX_STAGED_NODES);

impl InitialStateBuilder {
    // The original planner may retain intermediate ancestors for mixed or
    // repeated changes. A 256-bit path uses at most 256 branches, one leaf and
    // one collapse survivor: 128 * 258 < its unchanged 65,536-node budget.
    const CHANGES_PER_BATCH: usize = 128;

    pub(crate) fn new() -> Self {
        Self {
            root: empty_root(),
            nodes: BTreeMap::new(),
            pending: Vec::with_capacity(Self::CHANGES_PER_BATCH),
            loaded_nodes: 0,
            #[cfg(test)]
            staged_calls: 0,
            #[cfg(test)]
            max_batched_puts: 0,
        }
    }

    pub(crate) fn extend(&mut self, changes: impl IntoIterator<Item = StateChange>) -> Result<()> {
        for change in changes {
            self.pending.push(change);
            if self.pending.len() == Self::CHANGES_PER_BATCH {
                self.flush()?;
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let staged = stage_state_update(self, self.root, &self.pending)?;
        self.root = staged.root;
        self.loaded_nodes = self.loaded_nodes.saturating_add(staged.loaded_nodes);
        #[cfg(test)]
        {
            self.staged_calls += staged.staged_calls;
            self.max_batched_puts = self.max_batched_puts.max(staged.max_batched_puts);
        }
        self.nodes.extend(staged.nodes);
        self.pending.clear();
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<StagedStateUpdate> {
        self.flush()?;
        let mut reachable = BTreeSet::new();
        let mut stack = vec![self.root];
        while let Some(hash) = stack.pop() {
            if hash == empty_root() || !reachable.insert(hash) {
                continue;
            }
            let bytes = self
                .nodes
                .get(&hash)
                .context("initial staged tree node missing")?;
            if let Node::Branch { left, right, .. } = Node::decode(bytes)? {
                stack.extend([left, right]);
            }
        }
        self.nodes.retain(|hash, _| reachable.contains(hash));
        Ok(StagedStateUpdate {
            parent_root: empty_root(),
            root: self.root,
            nodes: self.nodes,
            loaded_nodes: self.loaded_nodes,
            #[cfg(test)]
            staged_calls: self.staged_calls,
            #[cfg(test)]
            max_batched_puts: self.max_batched_puts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batched_initialization_matches_one_original_tree_update() {
        let changes = (0u32..5000)
            .map(|n| StateChange::Put {
                key: n.to_be_bytes().to_vec(),
                value: n.to_le_bytes().to_vec(),
            })
            .collect::<Vec<_>>();
        let mut builder = InitialStateBuilder::new();
        builder.extend(changes.clone()).unwrap();
        let result = builder.finish().unwrap();
        struct Empty;
        impl StateNodeReader for Empty {
            fn read_node(&self, _: &NodeHash) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
        }
        let reference = stage_state_update(&Empty, empty_root(), &changes).unwrap();
        assert_eq!(result.root(), reference.root());
        assert_eq!(result.nodes(), reference.nodes());
        assert_eq!(
            result.max_batched_puts(),
            InitialStateBuilder::CHANGES_PER_BATCH
        );
    }

    #[test]
    fn large_mixed_initialization_matches_original_planner_chunks() -> Result<()> {
        const LEAVES: u64 = 131_072;
        let key = |kind: u8, index: u64| {
            let mut bytes = vec![kind];
            bytes.extend_from_slice(&index.to_be_bytes());
            bytes
        };
        let mut changes = Vec::new();
        for index in 0..LEAVES {
            changes.push(StateChange::Put {
                key: key(0, index),
                value: index.to_le_bytes().to_vec(),
            });
            if index.is_multiple_of(16) {
                changes.push(StateChange::Put {
                    key: key(1, index),
                    value: vec![7],
                });
                changes.push(StateChange::Delete { key: key(1, index) });
                changes.push(StateChange::Put {
                    key: key(0, index),
                    value: (index ^ 0xff).to_le_bytes().to_vec(),
                });
            }
        }
        let mut builder = InitialStateBuilder::new();
        builder.extend(changes.iter().cloned())?;
        let actual = builder.finish()?;

        struct OriginalNodes(BTreeMap<NodeHash, Vec<u8>>);
        impl StateNodeReader for OriginalNodes {
            fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
                Ok(self.0.get(hash).cloned())
            }
        }
        let mut reference = OriginalNodes(BTreeMap::new());
        let mut expected_root = empty_root();
        for chunk in changes.chunks(64) {
            let staged = stage_state_update(&reference, expected_root, chunk)?;
            expected_root = staged.root();
            reference.0.extend(staged.nodes);
        }
        assert_eq!(actual.root(), expected_root);
        assert_eq!(actual.nodes().len() as u64, LEAVES * 2 - 1);
        assert!(actual.nodes().len() > MAX_STAGED_NODES);
        assert!(actual.max_batched_puts() <= InitialStateBuilder::CHANGES_PER_BATCH);
        let actual_nodes = OriginalNodes(actual.nodes);
        for index in [0, 17, 65_535, 65_536, LEAVES - 1] {
            let expected = if index.is_multiple_of(16) {
                index ^ 0xff
            } else {
                index
            };
            assert_eq!(
                read_state_value(&actual_nodes, expected_root, &key(0, index))?,
                Some(expected.to_le_bytes().to_vec())
            );
            assert_eq!(
                read_state_value(&actual_nodes, expected_root, &key(1, index))?,
                None
            );
        }
        Ok(())
    }
}
