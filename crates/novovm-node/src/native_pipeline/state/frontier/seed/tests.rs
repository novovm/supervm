//! Pure exact-frontier tests. No storage ACK or parent authority is fabricated.

use super::*;
use crate::native_pipeline::state::frontier::{BulkCapture, CaptureStep, DeclaredAccess};
use crate::native_pipeline::state::tree::{self, StateChange, StateNodeReader};
use sha2::{Digest, Sha256};

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn budget() -> CaptureBudget {
    CaptureBudget {
        keys: 256,
        nodes: 4096,
        bytes: 1024 * 1024,
    }
}

fn key(n: u32) -> Vec<u8> {
    n.to_be_bytes().to_vec()
}

fn put(n: u32, value: u8) -> StateChange {
    StateChange::Put {
        key: key(n),
        value: vec![value],
    }
}

fn declarations(count: u32) -> Vec<DeclaredAccess> {
    (0..count)
        .map(|n| DeclaredAccess {
            key: key(n),
            may_put: true,
            may_delete: true,
        })
        .collect()
}

fn parent(count: u32) -> (Memory, NodeHash) {
    let update = tree::stage_state_update(
        &Memory::default(),
        tree::empty_root(),
        &(0..count).map(|n| put(n, 7)).collect::<Vec<_>>(),
    )
    .unwrap();
    (Memory(update.nodes().clone()), update.root())
}

fn seeded_parent(count: u32) -> (Memory, PostStateSeed) {
    let (mut memory, root) = parent(count);
    let input =
        OwnedStateInput::capture(&memory, root, &declarations(count + 1), budget()).unwrap();
    let update = input
        .stage(&[
            put(0, 19),
            StateChange::Delete { key: key(1) },
            put(count, 23),
        ])
        .unwrap();
    memory.0.extend(update.nodes().clone());
    let seed = input
        .into_poststate_seed(update, budget())
        .unwrap()
        .unwrap();
    (memory, seed)
}

fn drive(
    capture: &mut BulkCapture,
    seed: &PostStateSeed,
    memory: &Memory,
    quantum: usize,
) -> Result<usize> {
    let mut reads = 0;
    for _ in 0..100_000 {
        match capture.advance_with_seed(quantum, Some(seed))? {
            CaptureStep::More => {}
            CaptureStep::Complete => return Ok(reads),
            CaptureStep::NeedRead => {
                let hashes = capture.next_request()?.expect("capture miss request");
                reads += hashes.len();
                capture.accept(
                    hashes
                        .iter()
                        .map(|hash| memory.0.get(hash).cloned())
                        .collect(),
                )?;
            }
        }
    }
    anyhow::bail!("seeded capture did not terminate")
}

#[test]
fn seed_moves_both_maps_and_charges_every_hash_and_duplicate_allocation() {
    fn owned<T: Send + Sync + 'static>() {}
    owned::<PostStateSeed>();
    let (memory, root) = parent(8);
    let mut input = OwnedStateInput::capture(&memory, root, &declarations(8), budget()).unwrap();
    let update = input.stage(&[put(0, 9)]).unwrap();
    // A deliberate test-only duplicate proves accounting is by actual retained
    // allocations, not by the set union of hashes or only the poststate delta.
    let (&duplicate, value) = update.nodes().iter().next().unwrap();
    input.nodes.insert(duplicate, value.clone());
    let witness_pointer = input.nodes[&duplicate].as_ptr();
    let update_pointer = update.nodes()[&duplicate].as_ptr();
    let nodes = input.nodes.len() + update.nodes().len();
    let bytes = input
        .nodes
        .values()
        .chain(update.nodes().values())
        .map(|value| 32 + value.len())
        .sum();
    let seed = input
        .into_poststate_seed(
            update,
            CaptureBudget {
                keys: 0,
                nodes,
                bytes,
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(seed.node_count(), nodes);
    assert_eq!(seed.retained_bytes(), bytes);
    assert_eq!(seed.witness[&duplicate].as_ptr(), witness_pointer);
    assert_eq!(seed.update.nodes()[&duplicate].as_ptr(), update_pointer);
}

#[test]
fn seed_limits_are_inclusive_and_optional_but_corruption_is_an_error() {
    let (_, sample) = seeded_parent(16);
    let exact = CaptureBudget {
        keys: 0,
        nodes: sample.node_count(),
        bytes: sample.retained_bytes(),
    };
    for (limits, retained) in [
        (exact, true),
        (
            CaptureBudget {
                nodes: exact.nodes - 1,
                ..exact
            },
            false,
        ),
        (
            CaptureBudget {
                bytes: exact.bytes - 1,
                ..exact
            },
            false,
        ),
    ] {
        let (memory, root) = parent(16);
        let input = OwnedStateInput::capture(&memory, root, &declarations(17), budget()).unwrap();
        let update = input
            .stage(&[put(0, 19), StateChange::Delete { key: key(1) }, put(16, 23)])
            .unwrap();
        assert_eq!(
            input.into_poststate_seed(update, limits).unwrap().is_some(),
            retained
        );
    }
    let (memory, root) = parent(16);
    let mut input = OwnedStateInput::capture(&memory, root, &declarations(16), budget()).unwrap();
    let update = input.stage(&[put(0, 9)]).unwrap();
    input.nodes.get_mut(&root).unwrap()[0] ^= 1;
    assert!(input
        .into_poststate_seed(
            update,
            CaptureBudget {
                nodes: 0,
                bytes: 0,
                keys: 0
            }
        )
        .is_err());
    let input = OwnedStateInput::capture(&memory, root, &declarations(16), budget()).unwrap();
    let wrong = tree::stage_state_update(&Memory::default(), tree::empty_root(), &[]).unwrap();
    assert!(input.into_poststate_seed(wrong, budget()).is_err());
}

#[test]
fn complete_seed_recaptures_put_delete_and_absence_without_any_source() {
    let (memory, seed) = seeded_parent(64);
    let declared = declarations(66);
    let expected = OwnedStateInput::capture(&memory, seed.root(), &declared, budget()).unwrap();
    drop(memory);
    for quantum in [1, 7, 256] {
        let mut capture = BulkCapture::new(seed.root(), &declared, budget()).unwrap();
        assert_eq!(
            drive(&mut capture, &seed, &Memory::default(), quantum).unwrap(),
            0
        );
        assert_eq!(capture.seed_hits(), expected.captured_nodes());
        let actual = capture.finish().unwrap();
        assert_eq!(actual.nodes, expected.nodes);
        assert_eq!(actual.bytes, expected.bytes);
        assert_eq!(actual.read(&key(0)).unwrap(), Some(vec![19]));
        assert_eq!(actual.read(&key(1)).unwrap(), None);
        assert_eq!(actual.read(&key(64)).unwrap(), Some(vec![23]));
        assert_eq!(actual.read(&key(65)).unwrap(), None);
        let changes = [put(1, 31), StateChange::Delete { key: key(0) }];
        assert_eq!(
            actual.stage(&changes).unwrap().root(),
            expected.stage(&changes).unwrap().root()
        );
    }
}

#[test]
fn seed_does_not_inherit_permissions_or_bypass_child_capture_bounds() {
    let (memory, seed) = seeded_parent(32);
    let declared = vec![DeclaredAccess {
        key: key(0),
        may_put: false,
        may_delete: false,
    }];
    let expected = OwnedStateInput::capture(&memory, seed.root(), &declared, budget()).unwrap();
    let exact = CaptureBudget {
        keys: 1,
        nodes: expected.captured_nodes(),
        bytes: expected.captured_bytes(),
    };
    for (limits, accepted) in [
        (exact, true),
        (
            CaptureBudget {
                nodes: exact.nodes - 1,
                ..exact
            },
            false,
        ),
        (
            CaptureBudget {
                bytes: exact.bytes - 1,
                ..exact
            },
            false,
        ),
    ] {
        let mut capture = BulkCapture::new(seed.root(), &declared, limits).unwrap();
        assert_eq!(
            drive(&mut capture, &seed, &Memory::default(), 3).is_ok(),
            accepted
        );
        if accepted {
            let input = capture.finish().unwrap();
            assert_eq!(input.read(&key(0)).unwrap(), Some(vec![19]));
            assert!(input.read(&key(2)).is_err());
            assert!(input.stage(&[put(0, 8)]).is_err());
        } else {
            assert!(capture.advance_with_seed(1, Some(&seed)).is_err());
            assert!(capture.finish().is_err());
        }
    }
}

#[test]
fn partial_seed_uses_bulk_storage_misses_and_never_invents_absence() {
    let (mut memory, root) = parent(64);
    let declared = declarations(1);
    let input = OwnedStateInput::capture(&memory, root, &declared, budget()).unwrap();
    let update = input.stage(&[put(0, 22)]).unwrap();
    memory.0.extend(update.nodes().clone());
    let seed = input
        .into_poststate_seed(update, budget())
        .unwrap()
        .unwrap();
    let child = declarations(64);
    let mut capture = BulkCapture::new(seed.root(), &child, budget()).unwrap();
    assert!(drive(&mut capture, &seed, &memory, 7).unwrap() > 0);
    let input = capture.finish().unwrap();
    assert_eq!(input.read(&key(63)).unwrap(), Some(vec![7]));
    let mut missing = BulkCapture::new(seed.root(), &child, budget()).unwrap();
    let error = drive(&mut missing, &seed, &Memory::default(), 7).unwrap_err();
    assert!(error.to_string().contains("source node missing"));
    assert!(missing.finish().is_err());
}

#[test]
fn wrong_seed_root_is_only_a_cache_miss() {
    let (_, seed) = seeded_parent(8);
    let (memory, root) = parent(16);
    let declared = declarations(16);
    let mut capture = BulkCapture::new(root, &declared, budget()).unwrap();
    assert!(drive(&mut capture, &seed, &memory, 1).unwrap() > 0);
    assert_eq!(capture.seed_hits(), 0);
    let input = capture.finish().unwrap();
    assert_eq!(input.parent_root(), root);
    assert_eq!(input.read(&key(0)).unwrap(), Some(vec![7]));
}

fn hash_node(bytes: &[u8]) -> NodeHash {
    let mut hash = Sha256::new();
    hash.update(b"novovm-state-patricia-v1:node\0");
    hash.update(bytes);
    hash.finalize().into()
}

#[test]
fn seeded_shared_node_is_rechecked_at_each_incoming_edge() {
    let (mut memory, root) = parent(64);
    let original = memory.0[&root].clone();
    let left: NodeHash = original[35..67].try_into().unwrap();
    let right: NodeHash = original[67..99].try_into().unwrap();
    let mut wrong_left = memory.0[&left].clone();
    assert_eq!(wrong_left[0], 2);
    wrong_left[35..67].copy_from_slice(&right);
    let wrong_left_hash = hash_node(&wrong_left);
    memory.0.insert(wrong_left_hash, wrong_left);
    let mut wrong_root = original;
    wrong_root[35..67].copy_from_slice(&wrong_left_hash);
    let wrong_root_hash = hash_node(&wrong_root);
    memory.0.insert(wrong_root_hash, wrong_root);
    // Test-only forged internal witness: each content hash is sound, but an
    // incoming edge is invalid. A seed can never stand in for child traversal.
    let input = OwnedStateInput {
        root: wrong_root_hash,
        access: BTreeMap::new(),
        bytes: memory.0.values().map(Vec::len).sum(),
        nodes: memory.0,
    };
    let update = tree::stage_state_update(&Memory::default(), wrong_root_hash, &[]).unwrap();
    let seed = input
        .into_poststate_seed(update, budget())
        .unwrap()
        .unwrap();
    let mut capture = BulkCapture::new(wrong_root_hash, &declarations(64), budget()).unwrap();
    assert!(drive(&mut capture, &seed, &Memory::default(), 512).is_err());
    assert!(capture.finish().is_err());
}
