//! Independent comparisons against the original ordered planner. These do not
//! replace business, durable recovery, proof-relation or complete-path tests.
use super::*;
use crate::state::frontier::{CaptureBudget, DeclaredAccess, OwnedStateInput};
use std::cell::RefCell;

#[derive(Default, Clone)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn put(key: u32, value: u64) -> StateChange {
    StateChange::Put {
        key: key.to_be_bytes().to_vec(),
        value: value.to_be_bytes().to_vec(),
    }
}

// Deliberately keep the old sequence of change calls: comparing two invocations
// of the new public batch entry point would not be an independent oracle.
fn ordered(
    reader: &dyn StateNodeReader,
    parent_root: NodeHash,
    changes: &[StateChange],
) -> Result<StagedStateUpdate> {
    anyhow::ensure!(changes.len() <= MAX_BATCH_CHANGES, "too many state changes");
    let mut planner = Planner::new(reader);
    let mut root = parent_root;
    for change in changes {
        let (key, value) = match change {
            StateChange::Put { key, value } => {
                anyhow::ensure!(value.len() <= MAX_VALUE_BYTES, "oversized value");
                (key.as_slice(), Some(value.as_slice()))
            }
            StateChange::Delete { key } => (key.as_slice(), None),
        };
        root = planner.change(root, digest_key(key)?, value, 0)?;
    }
    planner.retain_reachable(root)?;
    Ok(StagedStateUpdate {
        parent_root,
        root,
        nodes: planner.nodes,
        loaded_nodes: planner.loaded_nodes,
        staged_calls: planner.staged_calls,
        max_batched_puts: planner.max_batched_puts,
    })
}

fn same(actual: &StagedStateUpdate, expected: &StagedStateUpdate) {
    assert_eq!(actual.parent_root, expected.parent_root);
    assert_eq!(actual.root, expected.root);
    assert_eq!(actual.nodes, expected.nodes);
}

#[test]
fn many_parent_versions_and_put_orders_match_original_root_and_node_bytes() {
    let mut memory = Memory::default();
    let mut root = empty_root();
    for round in 0..64u64 {
        let mut changes: Vec<_> = (0..96u32)
            .map(|key| {
                put(
                    (key * 37 + round as u32 * 13) % 257,
                    round * 96 + key as u64,
                )
            })
            .collect();
        let expected = ordered(&memory, root, &changes).unwrap();
        let forward = stage_state_update(&memory, root, &changes).unwrap();
        same(&forward, &expected);
        changes.reverse();
        same(
            &stage_state_update(&memory, root, &changes).unwrap(),
            &expected,
        );
        changes.rotate_left(round as usize % 96);
        same(
            &stage_state_update(&memory, root, &changes).unwrap(),
            &expected,
        );
        root = forward.root;
        memory.0.extend(forward.nodes);
    }
}

struct Observed<'a> {
    memory: &'a Memory,
    reads: RefCell<BTreeSet<NodeHash>>,
}

impl StateNodeReader for Observed<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.reads.borrow_mut().insert(*hash);
        self.memory.read_node(hash)
    }
}

#[test]
fn every_original_accessed_parent_node_remains_required_and_authenticated() {
    let mut memory = Memory::default();
    let initial: Vec<_> = (0..96).map(|key| put(key, key.into())).collect();
    let parent = ordered(&memory, empty_root(), &initial).unwrap();
    let root = parent.root;
    memory.0.extend(parent.nodes);
    let changes: Vec<_> = (0..8).map(|key| put(key * 11, 999)).collect();
    let observed = Observed {
        memory: &memory,
        reads: RefCell::default(),
    };
    let expected = ordered(&observed, root, &changes).unwrap();
    same(
        &stage_state_update(&memory, root, &changes).unwrap(),
        &expected,
    );
    for hash in observed.reads.into_inner() {
        let mut missing = memory.clone();
        missing.0.remove(&hash);
        assert!(ordered(&missing, root, &changes).is_err());
        assert!(stage_state_update(&missing, root, &changes).is_err());
        let mut corrupt = memory.clone();
        corrupt.0.get_mut(&hash).unwrap()[0] ^= 0x80;
        assert!(ordered(&corrupt, root, &changes).is_err());
        assert!(stage_state_update(&corrupt, root, &changes).is_err());
    }
}

#[test]
fn detached_sparse_frontier_supports_bulk_puts_without_authority_or_extra_reads() {
    let mut memory = Memory::default();
    let initial: Vec<_> = (0..512).map(|key| put(key, key.into())).collect();
    let parent = ordered(&memory, empty_root(), &initial).unwrap();
    let root = parent.root;
    memory.0.extend(parent.nodes);
    let changes: Vec<_> = (0..48).map(|key| put(key * 17, 999)).collect();
    let declarations: Vec<_> = changes
        .iter()
        .map(|change| {
            let StateChange::Put { key, .. } = change else {
                unreachable!()
            };
            DeclaredAccess {
                key: key.clone(),
                may_put: true,
                may_delete: false,
            }
        })
        .collect();
    let input = OwnedStateInput::capture(
        &memory,
        root,
        &declarations,
        CaptureBudget {
            keys: declarations.len(),
            nodes: 65_536,
            bytes: 16 * 1024 * 1024,
        },
    )
    .unwrap();
    let expected = ordered(&memory, root, &changes).unwrap();
    drop(memory);
    same(&input.stage(&changes).unwrap(), &expected);
    assert!(input.stage(&[put(123_456, 1)]).is_err());
    assert!(input
        .stage(&[StateChange::Delete {
            key: declarations[0].key.clone()
        }])
        .is_err());
}

#[test]
fn duplicate_and_delete_sequences_keep_the_original_ordered_contract() {
    let memory = Memory::default();
    for changes in [
        vec![put(1, 1), put(2, 2), put(1, 3)],
        vec![
            put(1, 1),
            StateChange::Delete {
                key: 1u32.to_be_bytes().to_vec(),
            },
            put(1, 2),
        ],
        vec![
            put(1, 1),
            put(2, 2),
            StateChange::Delete {
                key: 1u32.to_be_bytes().to_vec(),
            },
        ],
    ] {
        same(
            &stage_state_update(&memory, empty_root(), &changes).unwrap(),
            &ordered(&memory, empty_root(), &changes).unwrap(),
        );
    }
}

#[test]
fn unique_noop_puts_still_check_parent_edges_and_do_not_restage() {
    let mut memory = Memory::default();
    let initial: Vec<_> = (0..128).map(|key| put(key, key.into())).collect();
    let parent = ordered(&memory, empty_root(), &initial).unwrap();
    let root = parent.root;
    memory.0.extend(parent.nodes);
    let noop = stage_state_update(&memory, root, &initial).unwrap();
    assert_eq!(noop.root, root);
    assert!(noop.nodes.is_empty());

    let Node::Branch {
        bit,
        prefix,
        left,
        right,
    } = Node::decode(memory.0.get(&root).unwrap()).unwrap()
    else {
        unreachable!()
    };
    let swapped = Node::Branch {
        bit,
        prefix,
        left: right,
        right: left,
    }
    .encode();
    let bad_root = digest_node(&swapped);
    memory.0.insert(bad_root, swapped);
    // Every content hash is valid. What is wrong is the authenticated placement,
    // which must not be hidden by reuse of the same value or shared subtree.
    assert!(ordered(&memory, bad_root, &initial).is_err());
    assert!(stage_state_update(&memory, bad_root, &initial).is_err());
}

#[test]
fn unique_put_batch_preserves_all_input_bounds_including_late_invalid_items() {
    let memory = Memory::default();
    let mut changes: Vec<_> = (0..32).map(|key| put(key, key.into())).collect();
    for invalid in [
        StateChange::Put {
            key: vec![],
            value: vec![1],
        },
        StateChange::Put {
            key: vec![1; MAX_KEY_BYTES + 1],
            value: vec![1],
        },
        StateChange::Put {
            key: b"valid-key".to_vec(),
            value: vec![1; MAX_VALUE_BYTES + 1],
        },
    ] {
        changes.push(invalid);
        assert!(stage_state_update(&memory, empty_root(), &changes).is_err());
        changes.pop();
    }
    let excessive: Vec<_> = (0..=MAX_BATCH_CHANGES as u32)
        .map(|key| put(key, 1))
        .collect();
    assert!(stage_state_update(&memory, empty_root(), &excessive).is_err());
    assert!(memory.0.is_empty());
    // The pre-existing empty-update contract does not authenticate a parent.
    let empty = stage_state_update(&memory, [99; 32], &[]).unwrap();
    assert_eq!(empty.root, [99; 32]);
    assert!(empty.nodes.is_empty());
}
