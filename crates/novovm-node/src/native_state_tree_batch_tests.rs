// Reviewed reuse of runtime state/tree/batch/tests.rs at 4de28f3b.
// Work counters measure actual node encodings and hashes, not elapsed sleeps.
use super::*;

#[derive(Default, Clone)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn put(key: u32, value: u8) -> StateChange {
    StateChange::Put {
        key: key.to_be_bytes().to_vec(),
        value: vec![value],
    }
}

fn compare(
    memory: &Memory,
    parent: NodeHash,
    puts: &[Put<'_>],
) -> (NodeHash, BTreeMap<NodeHash, Vec<u8>>, usize, usize) {
    let mut batch = Planner::new(memory);
    let batch_root = batch.change_many(parent, puts, 0).unwrap();
    batch.retain_reachable(batch_root).unwrap();
    let mut ordered = Planner::new(memory);
    let mut ordered_root = parent;
    for put in puts {
        ordered_root = ordered
            .change(ordered_root, put.key, Some(put.value), 0)
            .unwrap();
    }
    ordered.retain_reachable(ordered_root).unwrap();
    assert_eq!(batch_root, ordered_root);
    assert_eq!(batch.nodes, ordered.nodes);
    (
        batch_root,
        batch.nodes,
        batch.staged_calls,
        ordered.staged_calls,
    )
}

fn original_order_put_calls(memory: &Memory, parent: NodeHash, changes: &[StateChange]) -> usize {
    let mut planner = Planner::new(memory);
    let mut root = parent;
    for change in changes {
        let StateChange::Put { key, value } = change else {
            panic!("work-count fixture contains only puts")
        };
        root = planner
            .change(root, digest_key(key).unwrap(), Some(value), 0)
            .unwrap();
    }
    planner.staged_calls
}

fn compare_mixed(
    memory: &Memory,
    parent: NodeHash,
    changes: &[StateChange],
) -> (StagedStateUpdate, usize) {
    let actual = stage_state_update(memory, parent, changes).unwrap();
    let mut ordered = Planner::new(memory);
    let mut root = parent;
    for change in changes {
        let (key, value) = match change {
            StateChange::Put { key, value } => (key, Some(value.as_slice())),
            StateChange::Delete { key } => (key, None),
        };
        root = ordered
            .change(root, digest_key(key).unwrap(), value, 0)
            .unwrap();
    }
    ordered.retain_reachable(root).unwrap();
    assert_eq!(actual.root(), root);
    assert_eq!(actual.nodes(), &ordered.nodes);
    (actual, ordered.staged_calls)
}

#[test]
fn large_put_runs_still_batch_with_intervening_deletes_and_ordered_duplicate_run() {
    let memory = Memory::default();
    let mut changes: Vec<_> = (0..1049).map(|key| put(key, 1)).collect();
    changes.push(StateChange::Delete {
        key: 90_000u32.to_be_bytes().to_vec(),
    });
    changes.extend((1049..1113).map(|key| put(key, 2)));
    changes.push(StateChange::Delete {
        key: 5u32.to_be_bytes().to_vec(),
    });
    changes.extend([put(5, 3), put(8, 4), put(5, 5)]);
    let (actual, ordered_calls) = compare_mixed(&memory, empty_root(), &changes);
    eprintln!(
        "state-tree-only mixed: max_batched_puts={} batch_stages={} ordered_stages={ordered_calls}",
        actual.max_batched_puts(),
        actual.staged_calls(),
    );
    assert_eq!(actual.max_batched_puts(), 1049);
    assert!(actual.staged_calls() < ordered_calls);
    let memory = Memory(actual.nodes);
    assert_eq!(
        read_state_value(&memory, actual.root, &5u32.to_be_bytes()).unwrap(),
        Some(vec![5])
    );
    assert_eq!(
        read_state_value(&memory, actual.root, &8u32.to_be_bytes()).unwrap(),
        Some(vec![4])
    );
}

#[test]
fn noop_runs_separated_by_absent_deletes_do_not_stage_anything() {
    let changes: Vec<_> = (0..1049).map(|key| put(key, 7)).collect();
    let initial = stage_state_update(&Memory::default(), empty_root(), &changes).unwrap();
    let memory = Memory(initial.nodes);
    let mut mixed = vec![StateChange::Delete {
        key: 90_000u32.to_be_bytes().to_vec(),
    }];
    mixed.extend(changes);
    mixed.push(StateChange::Delete {
        key: 90_001u32.to_be_bytes().to_vec(),
    });
    mixed.extend((0..64).map(|key| put(key, 7)));
    let (actual, ordered_calls) = compare_mixed(&memory, initial.root, &mixed);
    assert_eq!(actual.root(), initial.root);
    assert_eq!(actual.max_batched_puts(), 1049);
    assert_eq!(actual.staged_calls(), 0);
    assert_eq!(ordered_calls, 0);
    assert!(actual.nodes().is_empty());
}

#[test]
fn duplicate_run_fallback_and_delete_positions_are_preserved() {
    let memory = Memory::default();
    let changes = [put(1, 1), put(2, 2), put(1, 3)];
    let (actual, ordered_calls) = compare_mixed(&memory, empty_root(), &changes);
    assert_eq!(actual.max_batched_puts(), 0);
    assert_eq!(actual.staged_calls(), ordered_calls);

    let changes = [
        put(1, 1),
        put(2, 2),
        StateChange::Delete {
            key: 1u32.to_be_bytes().to_vec(),
        },
        put(1, 3),
        put(1, 4),
        StateChange::Delete {
            key: 2u32.to_be_bytes().to_vec(),
        },
        put(2, 5),
    ];
    let (actual, _) = compare_mixed(&memory, empty_root(), &changes);
    assert_eq!(actual.max_batched_puts(), 2, "never batch across a delete");
    let memory = Memory(actual.nodes);
    assert_eq!(
        read_state_value(&memory, actual.root, &1u32.to_be_bytes()).unwrap(),
        Some(vec![4])
    );
    assert_eq!(
        read_state_value(&memory, actual.root, &2u32.to_be_bytes()).unwrap(),
        Some(vec![5])
    );
}

#[test]
fn later_put_run_cannot_hide_a_delete_collapse_missing_parent_node() {
    let initial =
        stage_state_update(&Memory::default(), empty_root(), &[put(1, 1), put(2, 2)]).unwrap();
    let mut memory = Memory(initial.nodes);
    let Node::Branch {
        bit, left, right, ..
    } = Node::decode(&memory.0[&initial.root]).unwrap()
    else {
        panic!("expected branch")
    };
    let survivor = if bit_at(&digest_key(&2u32.to_be_bytes()).unwrap(), bit) {
        right
    } else {
        left
    };
    memory.0.remove(&survivor);
    let changes = [
        put(1, 1),
        StateChange::Delete {
            key: 1u32.to_be_bytes().to_vec(),
        },
        put(2, 9),
    ];
    let error = stage_state_update(&memory, initial.root, &changes).unwrap_err();
    assert!(error.to_string().contains("missing"));

    let changes: Vec<_> = (0..=MAX_BATCH_CHANGES / 2)
        .flat_map(|key| {
            [
                put(key as u32, 1),
                StateChange::Delete {
                    key: (key as u32).to_be_bytes().to_vec(),
                },
            ]
        })
        .collect();
    assert!(
        stage_state_update(&Memory::default(), empty_root(), &changes)
            .unwrap_err()
            .to_string()
            .contains("too many state changes")
    );
}

#[test]
fn each_shared_ancestor_is_staged_once_on_build_and_replacement() {
    let mut memory = Memory::default();
    let changes: Vec<_> = (0..512).map(|key| put(key, 7)).collect();
    let puts = unique_puts(&changes).unwrap().unwrap();
    let (root, nodes, batch_calls, _) = compare(&memory, empty_root(), &puts);
    // The pre-batch public path consumed this original key order, not the sorted
    // digest order used by the direct recursive correctness comparisons above.
    let ordered_calls = original_order_put_calls(&memory, empty_root(), &changes);
    eprintln!("state-tree-only build: batch_stages={batch_calls} ordered_stages={ordered_calls}");
    assert_eq!(nodes.len(), 1023);
    assert_eq!(batch_calls, nodes.len());
    assert!(ordered_calls > batch_calls);
    memory.0.extend(nodes);

    let changes: Vec<_> = (0..512).map(|key| put(key, 8)).collect();
    let puts = unique_puts(&changes).unwrap().unwrap();
    let (_, nodes, batch_calls, _) = compare(&memory, root, &puts);
    let ordered_calls = original_order_put_calls(&memory, root, &changes);
    eprintln!("state-tree-only replace: batch_stages={batch_calls} ordered_stages={ordered_calls}");
    assert_eq!(nodes.len(), 1023);
    assert_eq!(batch_calls, nodes.len());
    assert!(ordered_calls > batch_calls);
}

#[test]
fn all_noop_puts_stage_nothing_and_empty_batch_does_not_read_parent() {
    let mut memory = Memory::default();
    let changes: Vec<_> = (0..512).map(|key| put(key, 7)).collect();
    let puts = unique_puts(&changes).unwrap().unwrap();
    let (root, nodes, _, _) = compare(&memory, empty_root(), &puts);
    memory.0.extend(nodes);
    let (unchanged, nodes, batch_calls, ordered_calls) = compare(&memory, root, &puts);
    assert_eq!(unchanged, root);
    assert!(nodes.is_empty());
    assert_eq!(batch_calls, 0);
    assert_eq!(ordered_calls, 0);

    let missing_root = [9; 32];
    let mut planner = Planner::new(&memory);
    assert_eq!(
        planner.change_many(missing_root, &[], 0).unwrap(),
        missing_root
    );
    assert_eq!(planner.loaded_nodes, 0);
    assert_eq!(planner.staged_calls, 0);
}

#[test]
fn sorted_inputs_mixed_with_old_leaves_cover_shallower_and_deepest_splits() {
    // Synthetic digests exercise bit 0, byte boundaries and bit 255 without
    // weakening the public key hash or searching for improbable SHA prefixes.
    let mut keys = vec![[0; 32], [0xff; 32]];
    for bit in [0, 1, 7, 8, 31, 127, 128, 254, 255] {
        let mut key = [0; 32];
        key[bit / 8] = 0x80 >> (bit % 8);
        keys.push(key);
    }
    keys.sort();
    keys.dedup();
    for initial_count in 1..keys.len() {
        let mut memory = Memory::default();
        let initial: Vec<_> = keys[..initial_count]
            .iter()
            .map(|key| Put {
                key: *key,
                value: &[1],
            })
            .collect();
        let (root, nodes, _, _) = compare(&memory, empty_root(), &initial);
        memory.0.extend(nodes);
        for selected in [keys.clone(), keys[initial_count..].to_vec()] {
            let puts: Vec<_> = selected
                .iter()
                .map(|key| Put {
                    key: *key,
                    value: &[2],
                })
                .collect();
            compare(&memory, root, &puts);
        }
    }
    // Also put the existing root on the right of multiple new ancestors.
    let initial = [Put {
        key: [0xff; 32],
        value: &[3],
    }];
    let (root, nodes, _, _) = compare(&Memory::default(), empty_root(), &initial);
    let memory = Memory(nodes);
    let puts: Vec<_> = keys
        .iter()
        .map(|key| Put {
            key: *key,
            value: &[4],
        })
        .collect();
    compare(&memory, root, &puts);
}

#[test]
fn full_256_branch_depth_preserves_roots_and_noop_staging() {
    let mut keys = vec![[0; 32]];
    for bit in 0..256 {
        let mut key = [0; 32];
        key[bit / 8] = 0x80 >> (bit % 8);
        keys.push(key);
    }
    keys.sort();
    let puts: Vec<_> = keys
        .iter()
        .map(|key| Put {
            key: *key,
            value: &[1],
        })
        .collect();
    let (root, nodes, calls, _) = compare(&Memory::default(), empty_root(), &puts);
    assert_eq!(calls, 513);
    let memory = Memory(nodes);
    let (unchanged, nodes, calls, _) = compare(&memory, root, &puts);
    assert_eq!(unchanged, root);
    assert!(nodes.is_empty());
    assert_eq!(calls, 0);
    let puts: Vec<_> = keys
        .iter()
        .map(|key| Put {
            key: *key,
            value: &[2],
        })
        .collect();
    let (_, nodes, calls, _) = compare(&memory, root, &puts);
    assert_eq!(nodes.len(), 513);
    assert_eq!(calls, 513);
}

#[test]
fn missing_or_corrupt_existing_root_cannot_be_hidden_by_new_prefixes() {
    let initial = [Put {
        key: [0xff; 32],
        value: &[1],
    }];
    let (root, nodes, _, _) = compare(&Memory::default(), empty_root(), &initial);
    let mut corrupt = Memory(nodes);
    corrupt.0.get_mut(&root).unwrap()[0] ^= 0x80;
    let puts = [
        Put {
            key: [0; 32],
            value: &[2],
        },
        Put {
            key: [0x40; 32],
            value: &[3],
        },
    ];
    for memory in [Memory::default(), corrupt] {
        let mut planner = Planner::new(&memory);
        assert!(planner.change_many(root, &puts, 0).is_err());
        assert_eq!(planner.staged_calls, 0);
    }
}

#[test]
fn cache_hit_still_checks_each_incoming_parent_edge() {
    let memory = Memory::default();
    let mut planner = Planner::new(&memory);
    let key = [0; 32];
    let leaf = planner
        .stage(Node::Leaf {
            key,
            value: vec![1],
        })
        .unwrap();
    let mut sibling_key = [0; 32];
    sibling_key[0] = 0x40;
    let sibling = planner
        .stage(Node::Leaf {
            key: sibling_key,
            value: vec![1],
        })
        .unwrap();
    // The right subtree falsely references the leftmost leaf, through a second
    // authenticated edge. Its hash is valid and is already in the cache.
    let branch = planner
        .stage(Node::Branch {
            bit: 1,
            prefix: [0; 32],
            left: leaf,
            right: sibling,
        })
        .unwrap();
    let root = planner
        .stage(Node::Branch {
            bit: 0,
            prefix: [0; 32],
            left: leaf,
            right: branch,
        })
        .unwrap();
    let memory = Memory(planner.nodes);
    let mut planner = Planner::new(&memory);
    planner.load(&branch).unwrap();
    let mut right_key = [0; 32];
    right_key[0] = 0x80;
    let puts = [
        Put { key, value: &[2] },
        Put {
            key: right_key,
            value: &[3],
        },
    ];
    let error = planner.change_many(root, &puts, 0).unwrap_err();
    assert!(error.to_string().contains("authenticated parent path"));
}

#[test]
fn untouched_child_is_not_loaded_but_touched_child_stays_required() {
    let initial = [
        Put {
            key: [0; 32],
            value: &[1],
        },
        Put {
            key: [0xff; 32],
            value: &[2],
        },
    ];
    let (root, nodes, _, _) = compare(&Memory::default(), empty_root(), &initial);
    let mut memory = Memory(nodes);
    let Node::Branch { right, .. } = Node::decode(&memory.0[&root]).unwrap() else {
        panic!("expected branch")
    };
    memory.0.remove(&right);
    compare(
        &memory,
        root,
        &[Put {
            key: [0; 32],
            value: &[3],
        }],
    );
    let mut planner = Planner::new(&memory);
    assert!(planner.change_many(root, &initial, 0).is_err());
}

#[test]
fn deletes_and_duplicate_digests_are_not_coalesced() {
    assert!(unique_puts(&[put(1, 1), put(1, 2)]).unwrap().is_none());
    assert!(unique_puts(&[
        put(1, 1),
        StateChange::Delete {
            key: 1u32.to_be_bytes().to_vec()
        },
        put(2, 2),
    ])
    .unwrap()
    .is_none());
    let changes: Vec<_> = (0..64).rev().map(|key| put(key, 9)).collect();
    let puts = unique_puts(&changes).unwrap().unwrap();
    assert!(puts.windows(2).all(|pair| pair[0].key < pair[1].key));
}

#[test]
fn original_input_and_internal_resource_bounds_are_preserved() {
    let memory = Memory::default();
    for (key, value) in [
        (vec![1], Vec::new()),
        (vec![2; MAX_KEY_BYTES], vec![3; MAX_VALUE_BYTES]),
    ] {
        stage_state_update(&memory, empty_root(), &[StateChange::Put { key, value }]).unwrap();
    }
    for (key, value) in [
        (Vec::new(), vec![1]),
        (vec![1; MAX_KEY_BYTES + 1], vec![1]),
        (vec![1], vec![1; MAX_VALUE_BYTES + 1]),
    ] {
        assert!(
            stage_state_update(&memory, empty_root(), &[StateChange::Put { key, value }]).is_err()
        );
    }
    let changes: Vec<_> = (0..MAX_BATCH_CHANGES as u32)
        .map(|key| put(key, 1))
        .collect();
    let update = stage_state_update(&memory, empty_root(), &changes).unwrap();
    assert_eq!(update.nodes.len(), MAX_BATCH_CHANGES * 2 - 1);
    let mut too_many = changes;
    too_many.push(put(MAX_BATCH_CHANGES as u32, 1));
    assert!(stage_state_update(&memory, empty_root(), &too_many).is_err());

    let root = update.root;
    let memory = Memory(update.nodes);
    let mut planner = Planner::new(&memory);
    planner.loaded_nodes = MAX_READER_READS;
    let puts = [Put {
        key: [0; 32],
        value: &[1],
    }];
    assert!(planner
        .change_many(root, &puts, 0)
        .unwrap_err()
        .to_string()
        .contains("reader budget"));

    let mut planner = Planner::new(&memory);
    for index in 0..MAX_STAGED_NODES as u64 {
        let mut hash = [0; 32];
        hash[..8].copy_from_slice(&index.to_be_bytes());
        planner.nodes.insert(hash, Vec::new());
    }
    assert!(planner
        .change_many(empty_root(), &puts, 0)
        .unwrap_err()
        .to_string()
        .contains("staged-node budget"));
    assert_eq!(planner.nodes.len(), MAX_STAGED_NODES);
}
