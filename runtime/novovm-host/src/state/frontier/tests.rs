//! Pure input-boundary tests. No database, AOEM execution or finality is claimed.

use super::*;
use crate::state::tree::empty_root;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone, Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

#[derive(Default)]
struct SourceStatus {
    reads: AtomicUsize,
    disabled: AtomicBool,
    dropped: AtomicBool,
}

struct Source {
    memory: Memory,
    status: Arc<SourceStatus>,
}

impl StateNodeReader for Source {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.status.reads.fetch_add(1, Ordering::SeqCst);
        if self.status.disabled.load(Ordering::SeqCst) {
            anyhow::bail!("source reader disabled after capture");
        }
        self.memory.read_node(hash)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.status.dropped.store(true, Ordering::SeqCst);
    }
}

fn budget() -> CaptureBudget {
    CaptureBudget {
        keys: 128,
        nodes: 4096,
        bytes: 1024 * 1024,
    }
}

fn access(key: &[u8], may_put: bool, may_delete: bool) -> DeclaredAccess {
    DeclaredAccess {
        key: key.to_vec(),
        may_put,
        may_delete,
    }
}

fn put(key: &[u8], value: &[u8]) -> StateChange {
    StateChange::Put {
        key: key.to_vec(),
        value: value.to_vec(),
    }
}

fn delete(key: &[u8]) -> StateChange {
    StateChange::Delete { key: key.to_vec() }
}

fn parent(changes: &[StateChange]) -> (Memory, NodeHash) {
    let update = stage_state_update(&Memory::default(), empty_root(), changes).unwrap();
    (Memory(update.nodes().clone()), update.root())
}

fn assert_same_update(actual: &StagedStateUpdate, expected: &StagedStateUpdate) {
    assert_eq!(actual.parent_root(), expected.parent_root());
    assert_eq!(actual.root(), expected.root());
    assert_eq!(actual.nodes(), expected.nodes());
    assert_eq!(actual.loaded_nodes(), expected.loaded_nodes());
}

#[test]
fn owned_input_is_send_sync_and_static() {
    fn require_owned<T: Send + Sync + 'static>() {}
    require_owned::<OwnedStateInput>();
}

#[test]
fn batch_reads_preserve_absence_order_duplicates_and_source_independence() {
    let (memory, root) = parent(&[put(b"payer", b"100"), put(b"recipient", b"7")]);
    let status = Arc::new(SourceStatus::default());
    let source = Source {
        memory,
        status: Arc::clone(&status),
    };
    let owned = OwnedStateInput::capture(
        &source,
        root,
        &[
            access(b"payer", true, false),
            access(b"recipient", false, true),
            access(b"absent", false, false),
        ],
        budget(),
    )
    .unwrap();
    let reads = status.reads.load(Ordering::SeqCst);
    drop(source);
    assert!(status.dropped.load(Ordering::SeqCst));
    let keys = vec![
        b"recipient".to_vec(),
        b"absent".to_vec(),
        b"payer".to_vec(),
        b"payer".to_vec(),
    ];
    let expected = keys
        .iter()
        .map(|key| owned.read(key))
        .collect::<Result<Vec<_>>>()
        .unwrap();
    let returned = std::thread::spawn(move || {
        assert_eq!(owned.read_many(&keys).unwrap(), expected);
        assert!(owned.read_many(&[]).unwrap().is_empty());
        owned
    })
    .join()
    .unwrap();
    assert_eq!(status.reads.load(Ordering::SeqCst), reads);
    // Reading a projection never widens the original write permissions.
    assert!(returned.stage(&[put(b"recipient", b"8")]).is_err());
    assert!(returned.stage(&[delete(b"payer")]).is_err());
}

#[test]
fn batch_reads_do_not_expose_captured_deletion_siblings_or_a_successful_prefix() {
    let (memory, root) = parent(&[put(b"declared", b"yes"), put(b"secret", b"hidden")]);
    let owned =
        OwnedStateInput::capture(&memory, root, &[access(b"declared", true, true)], budget())
            .unwrap();
    // This sibling really was captured for a possible delete, but has no read
    // declaration. Physical availability is not access permission.
    assert_eq!(
        read_state_value(&OwnedNodeReader(&owned), root, b"secret").unwrap(),
        Some(b"hidden".to_vec())
    );
    let error = owned
        .read_many(&[b"declared".to_vec(), b"secret".to_vec()])
        .unwrap_err();
    assert!(error.to_string().contains("outside declared input"));
    assert!(owned.read_many(&[b"secret".to_vec()]).is_err());
    assert!(owned
        .read_many(&vec![b"declared".to_vec(); MAX_BATCH_READ_KEYS + 1])
        .unwrap_err()
        .to_string()
        .contains("key budget"));
}

#[test]
fn batch_read_failure_does_not_turn_missing_captured_content_into_absence() {
    let (memory, root) = parent(&[put(b"existing", b"value")]);
    let mut owned = OwnedStateInput::capture(
        &memory,
        root,
        &[
            access(b"existing", false, false),
            access(b"absent", false, false),
        ],
        budget(),
    )
    .unwrap();
    owned.nodes.remove(&root); // Test-only corruption of normally private data.
    assert!(owned.read_many(&[b"absent".to_vec()]).is_err());
    assert!(owned.read_many(&[b"existing".to_vec()]).is_err());
    assert!(owned
        .read_many(&[b"existing".to_vec(), b"not-declared".to_vec()])
        .unwrap_err()
        .to_string()
        .contains("outside declared input"));
}

#[test]
fn threaded_computation_uses_no_source_after_capture_and_drop() {
    let (memory, root) = parent(&[put(b"payer", b"100"), put(b"recipient", b"7")]);
    let changes = [
        put(b"payer", b"90"),
        delete(b"recipient"),
        put(b"new", b"17"),
        put(b"payer", b"89"),
    ];
    let expected = stage_state_update(&memory, root, &changes).unwrap();
    let status = Arc::new(SourceStatus::default());
    let source = Source {
        memory,
        status: status.clone(),
    };
    let input = OwnedStateInput::capture(
        &source,
        root,
        &[
            access(b"payer", true, false),
            access(b"recipient", false, true),
            access(b"new", true, false),
        ],
        budget(),
    )
    .unwrap();
    let reads_at_capture = status.reads.load(Ordering::SeqCst);
    assert!(reads_at_capture > 0);
    assert_eq!(reads_at_capture, input.captured_nodes());
    status.disabled.store(true, Ordering::SeqCst);
    drop(source);
    assert!(status.dropped.load(Ordering::SeqCst));

    // Only the owned input and changes move into this actual OS thread. There
    // is no source handle, path, callback or borrowing scope in the closure.
    let actual = std::thread::spawn(move || {
        assert_eq!(input.read(b"payer").unwrap(), Some(b"100".to_vec()));
        assert_eq!(input.read(b"new").unwrap(), None);
        input.stage(&changes)
    })
    .join()
    .unwrap()
    .unwrap();
    assert_same_update(&actual, &expected);
    assert_eq!(status.reads.load(Ordering::SeqCst), reads_at_capture);
}

#[test]
fn repeated_changes_preserve_order_and_exact_full_reader_result() {
    let (memory, root) = parent(&[
        put(b"a", b"a0"),
        put(b"b", b"b0"),
        put(b"untouched", b"stable"),
    ]);
    let original_nodes = memory.0.clone();
    let input = OwnedStateInput::capture(
        &memory,
        root,
        &[
            access(b"a", true, true),
            access(b"b", true, true),
            access(b"new", true, true),
        ],
        budget(),
    )
    .unwrap();
    let changes = [
        put(b"a", b"a1"),
        delete(b"a"),
        put(b"new", b"first"),
        delete(b"b"),
        put(b"a", b"last"),
        delete(b"new"),
        put(b"new", b"final"),
    ];
    for length in 0..=changes.len() {
        assert_same_update(
            &input.stage(&changes[..length]).unwrap(),
            &stage_state_update(&memory, root, &changes[..length]).unwrap(),
        );
    }
    let actual = input.stage(&changes).unwrap();
    let mut complete = memory.clone();
    complete.0.extend(actual.nodes().clone());
    assert_eq!(
        read_state_value(&complete, actual.root(), b"a").unwrap(),
        Some(b"last".to_vec())
    );
    assert_eq!(
        read_state_value(&complete, actual.root(), b"b").unwrap(),
        None
    );
    assert_eq!(
        read_state_value(&complete, actual.root(), b"new").unwrap(),
        Some(b"final".to_vec())
    );
    assert_eq!(
        read_state_value(&complete, actual.root(), b"untouched").unwrap(),
        Some(b"stable".to_vec())
    );
    assert_eq!(input.read(b"a").unwrap(), Some(b"a0".to_vec()));
    assert_eq!(input.parent_root(), root);
    assert_eq!(memory.0, original_nodes);
}

#[test]
fn deletion_capture_includes_the_other_leaf_needed_for_branch_collapse() {
    let (memory, root) = parent(&[put(b"left", b"one"), put(b"right", b"two")]);
    for (removed, survivor) in [
        (b"left".as_slice(), b"right".as_slice()),
        (b"right", b"left"),
    ] {
        let put_only =
            OwnedStateInput::capture(&memory, root, &[access(removed, true, false)], budget())
                .unwrap();
        let mut deleting =
            OwnedStateInput::capture(&memory, root, &[access(removed, false, true)], budget())
                .unwrap();
        assert_eq!(put_only.captured_nodes(), 2);
        assert_eq!(deleting.captured_nodes(), 3);
        assert!(deleting.read(survivor).is_err());
        assert!(deleting.stage(&[put(survivor, b"forbidden")]).is_err());
        assert!(deleting.stage(&[delete(survivor)]).is_err());
        let changes = [delete(removed)];
        let expected = stage_state_update(&memory, root, &changes).unwrap();
        assert_same_update(&deleting.stage(&changes).unwrap(), &expected);
        assert!(read_state_value(&memory, expected.root(), survivor)
            .unwrap()
            .is_some());

        // A capture of only the read path would be insufficient even though
        // the deleted leaf itself was successfully authenticated and read.
        let survivor_hash = *deleting
            .nodes
            .keys()
            .find(|hash| !put_only.nodes.contains_key(*hash))
            .unwrap();
        deleting.nodes.remove(&survivor_hash);
        let error = deleting.stage(&changes).unwrap_err();
        assert!(error.to_string().contains("outside captured frontier"));
    }
}

#[test]
fn authenticated_absence_is_not_unknown_key_or_missing_frontier_content() {
    let (memory, root) = parent(&[put(b"present", b"value")]);
    let mut input = OwnedStateInput::capture(
        &memory,
        root,
        &[
            access(b"absent", true, true),
            access(b"present", false, false),
        ],
        budget(),
    )
    .unwrap();
    assert_eq!(input.read(b"absent").unwrap(), None);
    assert!(input.read(b"undeclared").is_err());
    assert!(OwnedNodeReader(&input).read_node(&[0xff; 32]).is_err());
    input.nodes.remove(&root);
    assert!(
        input.read(b"absent").is_err(),
        "missing proof is never absence"
    );
}

#[test]
fn declared_permissions_and_unknown_writes_fail_without_partial_effects() {
    let (memory, root) = parent(&[put(b"readonly", b"1"), put(b"delete", b"2")]);
    let input = OwnedStateInput::capture(
        &memory,
        root,
        &[
            access(b"readonly", false, false),
            access(b"put", true, false),
            access(b"delete", false, true),
        ],
        budget(),
    )
    .unwrap();
    let nodes = input.nodes.clone();
    for denied in [
        put(b"readonly", b"3"),
        delete(b"readonly"),
        delete(b"put"),
        put(b"delete", b"3"),
        put(b"unknown", b"3"),
        delete(b"unknown"),
    ] {
        assert!(input
            .stage(&[put(b"put", b"allowed prefix"), denied])
            .is_err());
        assert_eq!(input.nodes, nodes);
        assert_eq!(input.read(b"put").unwrap(), None);
    }
    assert_same_update(
        &input
            .stage(&[put(b"put", b"3"), delete(b"delete")])
            .unwrap(),
        &stage_state_update(&memory, root, &[put(b"put", b"3"), delete(b"delete")]).unwrap(),
    );
}

#[test]
fn duplicate_empty_and_invalid_key_declarations_fail_before_reading_source() {
    let status = Arc::new(SourceStatus::default());
    let source = Source {
        memory: Memory::default(),
        status: status.clone(),
    };
    for declarations in [
        vec![],
        vec![access(b"same", true, false), access(b"same", false, true)],
        vec![access(b"", true, true)],
        vec![access(&[1; 257], true, true)],
    ] {
        assert!(OwnedStateInput::capture(&source, [8; 32], &declarations, budget()).is_err());
    }
    assert_eq!(status.reads.load(Ordering::SeqCst), 0);
}

#[test]
fn key_node_and_byte_budgets_are_inclusive_and_count_unique_content() {
    let (memory, root) = parent(&[put(b"a", b"1"), put(b"b", b"2"), put(b"c", b"3")]);
    let declarations = [
        access(b"a", true, true),
        access(b"b", true, true),
        access(b"c", true, true),
    ];
    let reference = OwnedStateInput::capture(&memory, root, &declarations, budget()).unwrap();
    let exact = CaptureBudget {
        keys: declarations.len(),
        nodes: reference.captured_nodes(),
        bytes: reference.captured_bytes(),
    };
    let status = Arc::new(SourceStatus::default());
    let source = Source {
        memory,
        status: status.clone(),
    };
    let input = OwnedStateInput::capture(&source, root, &declarations, exact).unwrap();
    assert_eq!(input.nodes, reference.nodes);
    assert_eq!(
        input.captured_bytes(),
        input.nodes.values().map(Vec::len).sum::<usize>()
    );
    assert_eq!(status.reads.load(Ordering::SeqCst), input.captured_nodes());
    for (limited, reason) in [
        (
            CaptureBudget {
                keys: exact.keys - 1,
                ..exact
            },
            "declaration budget",
        ),
        (
            CaptureBudget {
                nodes: exact.nodes - 1,
                ..exact
            },
            "node budget",
        ),
        (
            CaptureBudget {
                bytes: exact.bytes - 1,
                ..exact
            },
            "byte budget",
        ),
    ] {
        let error = OwnedStateInput::capture(&source, root, &declarations, limited)
            .err()
            .unwrap();
        assert!(error.to_string().contains(reason), "{error:#}");
    }
}

#[test]
fn empty_parent_needs_no_source_and_supports_insert_delete_and_noop() {
    let status = Arc::new(SourceStatus::default());
    status.disabled.store(true, Ordering::SeqCst);
    let source = Source {
        memory: Memory::default(),
        status: status.clone(),
    };
    let input = OwnedStateInput::capture(
        &source,
        empty_root(),
        &[access(&[7; 256], true, true)],
        CaptureBudget {
            keys: 1,
            nodes: 0,
            bytes: 0,
        },
    )
    .unwrap();
    let key = [7; 256];
    assert_eq!(input.captured_nodes(), 0);
    assert_eq!(input.captured_bytes(), 0);
    assert_eq!(input.read(&key).unwrap(), None);
    for changes in [
        vec![],
        vec![delete(&key)],
        vec![put(&key, b"new")],
        vec![put(&key, b"new"), delete(&key)],
    ] {
        assert_same_update(
            &input.stage(&changes).unwrap(),
            &stage_state_update(&Memory::default(), empty_root(), &changes).unwrap(),
        );
    }
    assert_eq!(
        input
            .stage(&[put(&key, b"new"), delete(&key)])
            .unwrap()
            .root(),
        empty_root()
    );
    assert_eq!(status.reads.load(Ordering::SeqCst), 0);
}

#[test]
fn capture_rejects_missing_corrupt_and_failing_source_instead_of_absence() {
    let (memory, root) = parent(&[put(b"a", b"1")]);
    let declarations = [access(b"a", true, true)];
    let mut missing = memory.clone();
    missing.0.remove(&root);
    assert!(OwnedStateInput::capture(&missing, root, &declarations, budget()).is_err());
    let mut corrupt = memory.clone();
    corrupt.0.get_mut(&root).unwrap()[0] ^= 1;
    assert!(OwnedStateInput::capture(&corrupt, root, &declarations, budget()).is_err());
    let status = Arc::new(SourceStatus::default());
    status.disabled.store(true, Ordering::SeqCst);
    let failed = Source { memory, status };
    let error = OwnedStateInput::capture(&failed, root, &declarations, budget())
        .err()
        .unwrap();
    assert!(error.to_string().contains("source reader disabled"));
}

#[test]
fn deterministic_mixed_changes_match_full_reader_across_128_parent_plans() {
    let mut random = 0x93d7_44bc_812e_05a1u64;
    let mut next = || {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        random >> 32
    };
    for case in 0..128u16 {
        let initial = (0..12u16)
            .map(|key| put(&key.to_be_bytes(), &case.wrapping_add(key).to_be_bytes()))
            .collect::<Vec<_>>();
        let (memory, root) = parent(&initial);
        let declarations = (0..18u16)
            .map(|key| access(&key.to_be_bytes(), true, true))
            .collect::<Vec<_>>();
        let input = OwnedStateInput::capture(&memory, root, &declarations, budget()).unwrap();
        let mut changes = Vec::new();
        for _ in 0..48 {
            let key = (next() % 18) as u16;
            changes.push(if next().is_multiple_of(3) {
                delete(&key.to_be_bytes())
            } else {
                put(&key.to_be_bytes(), &(next() as u16).to_be_bytes())
            });
        }
        let expected = stage_state_update(&memory, root, &changes).unwrap();
        let actual = input
            .stage(&changes)
            .unwrap_or_else(|error| panic!("case {case}: {error:#}"));
        assert_same_update(&actual, &expected);
        assert_eq!(input.parent_root(), root);
    }
}
