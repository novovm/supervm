//! In-memory final-write-set checks; these are not persistence/GC evidence.
use super::*;
use crate::native_state_tree::empty_root;
use std::cell::Cell;

type Records = BTreeMap<Vec<u8>, Vec<u8>>;

#[derive(Clone, Default)]
struct Memory {
    nodes: BTreeMap<NodeHash, Vec<u8>>,
    chunks: BTreeMap<(NodeHash, u32), Vec<u8>>,
    node_reads: Cell<usize>,
    chunk_reads: Cell<usize>,
    forbid_reads: Cell<bool>,
}

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.node_reads.set(self.node_reads.get() + 1);
        assert!(
            !self.forbid_reads.get(),
            "compaction must not read inherited nodes"
        );
        Ok(self.nodes.get(hash).cloned())
    }
}

impl StateRecordReader for Memory {
    fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
        self.chunk_reads.set(self.chunk_reads.get() + 1);
        assert!(
            !self.forbid_reads.get(),
            "compaction must not read inherited blobs"
        );
        Ok(self.chunks.get(&(hash, index)).cloned())
    }
}

impl Memory {
    fn apply(&mut self, update: &StagedRecordUpdate) {
        self.nodes.extend(update.nodes.clone());
        for (hash, bytes) in &update.blobs {
            for (index, chunk) in bytes.chunks(RECORD_CHUNK_BYTES_V1).enumerate() {
                self.chunks.insert((*hash, index as u32), chunk.to_vec());
            }
        }
    }
}

fn put(key: &[u8], value: &[u8]) -> RecordChange {
    RecordChange::Put {
        key: key.to_vec(),
        value: value.to_vec(),
    }
}

fn delete(key: &[u8]) -> RecordChange {
    RecordChange::Delete { key: key.to_vec() }
}

fn apply_reference(records: &mut Records, changes: &[RecordChange]) {
    for change in changes {
        match change {
            RecordChange::Put { key, value } => {
                records.insert(key.clone(), value.clone());
            }
            RecordChange::Delete { key } => {
                records.remove(key);
            }
        }
    }
}

fn scan(reader: &dyn StateRecordReader, root: NodeHash) -> Records {
    let mut records = Records::new();
    visit_records(
        reader,
        root,
        RecordScanBudget {
            max_nodes: 20_000,
            max_records: 10_000,
            max_bytes: 16 * 1024 * 1024,
        },
        &mut |key, value| {
            assert!(records.insert(key.to_vec(), value.to_vec()).is_none());
            Ok(())
        },
    )
    .unwrap();
    records
}

fn copy_update(update: &StagedRecordUpdate) -> StagedRecordUpdate {
    StagedRecordUpdate {
        parent_root: update.parent_root,
        root: update.root,
        nodes: update.nodes.clone(),
        blobs: update.blobs.clone(),
        loaded_nodes: update.loaded_nodes,
    }
}

fn compact_without_reads(
    memory: &Memory,
    update: StagedRecordUpdate,
) -> Result<StagedRecordUpdate> {
    memory.node_reads.set(0);
    memory.chunk_reads.set(0);
    memory.forbid_reads.set(true);
    let result = RecordOverlayV1 {
        reader: memory,
        update,
    }
    .finish_compacted();
    assert_eq!(memory.node_reads.get(), 0);
    assert_eq!(memory.chunk_reads.get(), 0);
    memory.forbid_reads.set(false);
    result
}

fn assert_equivalent(
    memory: &Memory,
    original: &StagedRecordUpdate,
    compact: &StagedRecordUpdate,
    expected: &Records,
) {
    assert_eq!(compact.root(), original.root());
    assert_eq!(compact.parent_root(), original.parent_root());
    assert_eq!(compact.loaded_nodes(), original.loaded_nodes());
    assert!(compact
        .nodes()
        .iter()
        .all(|(hash, bytes)| original.nodes().get(hash) == Some(bytes)));
    assert!(compact
        .blobs()
        .iter()
        .all(|(hash, bytes)| original.blobs().get(hash) == Some(bytes)));
    let parent = scan(memory, original.parent_root());
    let mut unpruned = memory.clone();
    unpruned.apply(original);
    let mut pruned = memory.clone();
    pruned.apply(compact);
    assert_eq!(scan(&unpruned, original.root()), *expected);
    assert_eq!(scan(&pruned, compact.root()), *expected);
    assert_eq!(scan(&unpruned, original.parent_root()), parent);
    assert_eq!(scan(&pruned, compact.parent_root()), parent);
    for (key, value) in expected {
        assert_eq!(
            read_record(&pruned, compact.root(), key).unwrap(),
            Some(value.clone())
        );
    }
}

#[test]
fn compacted_physical_like_multistage_257_changes_preserve_roots_and_old_parent() {
    let mut memory = Memory::default();
    let initial: Vec<_> = (0..64)
        .map(|id| put(format!("parent/{id}").as_bytes(), &[3; 900]))
        .collect();
    let parent = stage_record_update(&memory, empty_root(), &initial).unwrap();
    memory.apply(&parent);
    let old_nodes = memory.nodes.clone();
    let old_chunks = memory.chunks.clone();
    let mut expected = scan(&memory, parent.root());
    let mut overlay = RecordOverlayV1::new(&memory, parent.root());
    let changes: Vec<_> = (0u32..257)
        .map(|id| {
            put(
                format!("store/account_asset_balances/{id}/NOV").as_bytes(),
                &[id as u8; 700],
            )
        })
        .collect();
    for batch in changes.chunks(13) {
        overlay.stage(batch).unwrap();
        apply_reference(&mut expected, batch);
    }
    for id in 0u32..257 {
        let key = format!("store/account_asset_balances/{id}/NOV");
        let change = if id.is_multiple_of(3) {
            delete(key.as_bytes())
        } else {
            put(key.as_bytes(), &id.to_le_bytes())
        };
        overlay.stage(std::slice::from_ref(&change)).unwrap();
        apply_reference(&mut expected, &[change]);
    }
    let original = overlay.finish();
    let compact = compact_without_reads(&memory, copy_update(&original)).unwrap();
    assert!(compact.nodes().len() < original.nodes().len());
    assert!(compact.blobs().len() < original.blobs().len());
    assert_equivalent(&memory, &original, &compact, &expected);
    assert_eq!(memory.nodes, old_nodes);
    assert_eq!(memory.chunks, old_chunks);
}

#[test]
fn compacted_receipt_append_retains_all_256_receipts_but_not_intermediate_branches() {
    let memory = Memory::default();
    let mut overlay = RecordOverlayV1::new(&memory, empty_root());
    let mut expected = Records::new();
    for id in 0u32..256 {
        let change = put(format!("receipts/{id:064x}").as_bytes(), &[id as u8; 600]);
        overlay.stage(std::slice::from_ref(&change)).unwrap();
        apply_reference(&mut expected, &[change]);
    }
    let original = overlay.finish();
    let compact = compact_without_reads(&memory, copy_update(&original)).unwrap();
    assert!(compact.nodes().len() < original.nodes().len());
    assert_eq!(compact.blobs().len(), 256);
    assert_eq!(compact.blobs(), original.blobs());
    assert_equivalent(&memory, &original, &compact, &expected);
}

#[test]
fn empty_noop_and_transient_overwrites_drop_only_unreachable_local_content() {
    let mut memory = Memory::default();
    let initial = stage_record_update(&memory, empty_root(), &[put(b"keep", b"old")]).unwrap();
    memory.apply(&initial);
    for root in [empty_root(), initial.root()] {
        let original = RecordOverlayV1::new(&memory, root).finish();
        let compact = compact_without_reads(&memory, copy_update(&original)).unwrap();
        assert!(compact.nodes().is_empty() && compact.blobs().is_empty());
        assert_equivalent(&memory, &original, &compact, &scan(&memory, root));
    }
    let mut overlay = RecordOverlayV1::new(&memory, initial.root());
    overlay
        .stage(&[put(b"keep", b"old"), delete(b"missing")])
        .unwrap();
    overlay.stage(&[put(b"temporary", b"one")]).unwrap();
    overlay.stage(&[put(b"temporary", b"two")]).unwrap();
    overlay.stage(&[delete(b"temporary")]).unwrap();
    let original = overlay.finish();
    assert_eq!(original.root(), initial.root());
    assert!(!original.nodes().is_empty() && !original.blobs().is_empty());
    let compact = compact_without_reads(&memory, copy_update(&original)).unwrap();
    assert!(compact.nodes().is_empty() && compact.blobs().is_empty());
    assert_equivalent(&memory, &original, &compact, &scan(&memory, initial.root()));
}

#[test]
fn collapse_to_inherited_leaf_does_not_require_a_staged_blob_or_read_the_parent() {
    let mut memory = Memory::default();
    let initial = stage_record_update(
        &memory,
        empty_root(),
        &[put(b"a", b"one"), put(b"b", b"two")],
    )
    .unwrap();
    memory.apply(&initial);
    let mut overlay = RecordOverlayV1::new(&memory, initial.root());
    overlay.stage(&[delete(b"a")]).unwrap();
    let original = overlay.finish();
    // Planner::change returns the existing survivor hash, not a copied leaf.
    assert!(original.nodes().is_empty());
    assert!(original.blobs().is_empty());
    assert!(memory.nodes.contains_key(&original.root()));
    let compact = compact_without_reads(&memory, copy_update(&original)).unwrap();
    assert_equivalent(
        &memory,
        &original,
        &compact,
        &Records::from([(b"b".to_vec(), b"two".to_vec())]),
    );
}

#[test]
fn overwrite_then_restore_parent_bytes_keeps_the_restaged_leaf_blob_pair_valid() {
    let mut memory = Memory::default();
    let initial = stage_record_update(&memory, empty_root(), &[put(b"a", b"old")]).unwrap();
    memory.apply(&initial);
    let mut overlay = RecordOverlayV1::new(&memory, initial.root());
    overlay.stage(&[put(b"a", b"new")]).unwrap();
    overlay.stage(&[put(b"a", b"old")]).unwrap();
    let original = overlay.finish();
    assert_eq!(original.root(), initial.root());
    assert!(original.nodes().contains_key(&initial.root()));
    let compact = compact_without_reads(&memory, copy_update(&original)).unwrap();
    assert_eq!(compact.nodes().len(), 1);
    assert_eq!(compact.blobs().len(), 1);
    assert_equivalent(&memory, &original, &compact, &scan(&memory, initial.root()));
}

#[test]
fn corrupt_node_and_blob_hashes_are_rejected_even_when_the_content_would_be_pruned() {
    let memory = Memory::default();
    let mut overlay = RecordOverlayV1::new(&memory, empty_root());
    overlay.stage(&[put(b"a", b"old")]).unwrap();
    let old_root = overlay.root();
    let old_blob = blob_hash(&encode_blob(b"a", b"old").unwrap());
    overlay.stage(&[put(b"a", b"new")]).unwrap();
    let original = overlay.finish();
    let new_blob = blob_hash(&encode_blob(b"a", b"new").unwrap());
    let compact = compact_without_reads(&memory, copy_update(&original)).unwrap();
    assert!(!compact.nodes().contains_key(&old_root));
    assert!(!compact.blobs().contains_key(&old_blob));
    for hash in [old_root, original.root()] {
        let mut broken = copy_update(&original);
        broken.nodes.get_mut(&hash).unwrap()[0] ^= 1;
        assert!(compact_without_reads(&memory, broken).is_err());
    }
    for hash in [old_blob, new_blob] {
        let mut broken = copy_update(&original);
        let bytes = broken.blobs.get_mut(&hash).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        assert!(compact_without_reads(&memory, broken).is_err());
    }
}

fn forged_leaf(
    memory: &Memory,
    key: &[u8],
    descriptor: Vec<u8>,
    blobs: BTreeMap<NodeHash, Vec<u8>>,
) -> StagedRecordUpdate {
    let tree = stage_state_update(
        memory,
        empty_root(),
        &[StateChange::Put {
            key: key.to_vec(),
            value: descriptor,
        }],
    )
    .unwrap();
    StagedRecordUpdate {
        parent_root: empty_root(),
        root: tree.root(),
        nodes: tree.nodes().clone(),
        blobs,
        loaded_nodes: tree.loaded_nodes(),
    }
}

#[test]
fn reachable_leaf_wrong_blob_length_original_key_missing_blob_and_bad_codec_are_rejected() {
    let memory = Memory::default();
    let blob = encode_blob(b"a", b"value").unwrap();
    let hash = blob_hash(&blob);
    let other = encode_blob(b"other", b"value").unwrap();
    let other_hash = blob_hash(&other);
    let cases = [
        forged_leaf(
            &memory,
            b"a",
            encode_leaf(hash, blob.len() + 1),
            BTreeMap::from([(hash, blob.clone())]),
        ),
        forged_leaf(
            &memory,
            b"a",
            encode_leaf(other_hash, other.len()),
            BTreeMap::from([(other_hash, other)]),
        ),
        forged_leaf(
            &memory,
            b"a",
            encode_leaf(hash, blob.len()),
            BTreeMap::new(),
        ),
        forged_leaf(
            &memory,
            b"a",
            b"not-a-record-leaf".to_vec(),
            BTreeMap::from([(hash, blob)]),
        ),
    ];
    for broken in cases {
        assert!(compact_without_reads(&memory, broken).is_err());
    }
}

#[test]
fn malformed_unreachable_blob_with_its_own_correct_hash_is_not_silently_discarded() {
    let memory = Memory::default();
    let mut update = stage_record_update(&memory, empty_root(), &[put(b"a", b"good")]).unwrap();
    let malformed = b"NRB1 invalid length".to_vec();
    update.blobs.insert(blob_hash(&malformed), malformed);
    assert!(compact_without_reads(&memory, update).is_err());
}

#[test]
fn deterministic_multistage_differentials_preserve_inherited_subtrees_and_every_old_root() {
    fn next(seed: &mut u64) -> u64 {
        *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        *seed >> 17
    }
    let mut seed = 0x6e6f_765f_7472_6565;
    let mut full_memory = Memory::default();
    let mut compact_memory = Memory::default();
    let mut root = empty_root();
    let mut expected = Records::new();
    let mut history = vec![(root, expected.clone())];
    let mut removed_nodes = 0usize;
    for round in 0..64 {
        let mut overlay = RecordOverlayV1::new(&full_memory, root);
        for _ in 0..12 {
            let key = format!("account/{}", next(&mut seed) % 40).into_bytes();
            let change = if next(&mut seed).is_multiple_of(4) {
                delete(&key)
            } else {
                let value = vec![(next(&mut seed) % 256) as u8; (next(&mut seed) % 800) as usize];
                put(&key, &value)
            };
            overlay.stage(std::slice::from_ref(&change)).unwrap();
            apply_reference(&mut expected, &[change]);
        }
        let original = overlay.finish();
        let compact = compact_without_reads(&compact_memory, copy_update(&original)).unwrap();
        assert_eq!(compact.parent_root(), root);
        assert_eq!(compact.root(), original.root());
        assert_eq!(compact.loaded_nodes(), original.loaded_nodes());
        removed_nodes += original.nodes().len() - compact.nodes().len();
        full_memory.apply(&original);
        compact_memory.apply(&compact);
        root = original.root();
        assert_eq!(scan(&full_memory, root), expected, "full round {round}");
        assert_eq!(
            scan(&compact_memory, root),
            expected,
            "compact round {round}"
        );
        history.push((root, expected.clone()));
    }
    assert!(removed_nodes > 0);
    for (old_root, records) in history {
        assert_eq!(scan(&full_memory, old_root), records);
        assert_eq!(scan(&compact_memory, old_root), records);
    }
}
