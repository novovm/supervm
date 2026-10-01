//! Recoverable byte records in the immutable state tree. This is a Host codec,
//! not NOV economics and not another authority ledger. Leaves bind a canonical
//! blob containing both the original key and value; large blobs use immutable
//! 512-byte AOEM chunks. A caller supplies an independently trusted parent root.
//! Full traversal is only for an explicit compatibility/materialization path.

use crate::native_state_tree::{
    read_state_value, stage_state_update, state_key_hash, visit_state_leaves, NodeHash,
    StateChange, StateNodeReader,
};
use anyhow::{bail, Context, Result};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

pub const STATE_RECORD_CODEC_V1: &str = "novovm-state-record-patricia-sha256/v1";
/// A per-value bound, never a limit on the number/total bytes of ledger records.
pub const MAX_RECORD_VALUE_BYTES_V1: usize = 8 * 1024 * 1024;
pub const RECORD_CHUNK_BYTES_V1: usize = 512;
const MAX_KEY_BYTES: usize = 256;
const BLOB_HEADER_BYTES: usize = 10;
const LEAF_BYTES: usize = 40;
const MAX_STAGE_CHANGES: usize = 4096;
// One staging call is bounded. An overlay can combine calls against one root.
const MAX_STAGE_BLOB_BYTES: usize = 16 * 1024 * 1024;
const MAX_BLOB_BYTES: usize = BLOB_HEADER_BYTES + MAX_KEY_BYTES + MAX_RECORD_VALUE_BYTES_V1;
pub type StateRecordVisitor<'a> = dyn FnMut(&[u8], &[u8]) -> Result<()> + 'a;

pub trait StateRecordReader: StateNodeReader {
    fn read_record_chunk(&self, blob_hash: NodeHash, chunk_index: u32) -> Result<Option<Vec<u8>>>;
}

#[derive(Debug, Clone)]
pub enum RecordChange {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

/// Only constructed by staging checked records against a trusted parent.
/// Nodes/blobs may include content already durable, or superseded during an
/// overlay's intermediate stages. No GC or authority publication occurs here.
#[derive(Debug)]
pub struct StagedRecordUpdate {
    parent_root: NodeHash,
    root: NodeHash,
    nodes: BTreeMap<NodeHash, Vec<u8>>,
    blobs: BTreeMap<NodeHash, Vec<u8>>,
    loaded_nodes: usize,
}

impl StagedRecordUpdate {
    pub fn parent_root(&self) -> NodeHash {
        self.parent_root
    }
    pub fn root(&self) -> NodeHash {
        self.root
    }
    pub fn nodes(&self) -> &BTreeMap<NodeHash, Vec<u8>> {
        &self.nodes
    }
    pub fn blobs(&self) -> &BTreeMap<NodeHash, Vec<u8>> {
        &self.blobs
    }
    pub fn loaded_nodes(&self) -> usize {
        self.loaded_nodes
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RecordScanBudget {
    pub max_nodes: usize,
    pub max_records: usize,
    /// Maximum canonical blob bytes decoded during this explicit scan.
    pub max_bytes: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RecordScanStats {
    pub nodes: usize,
    pub records: usize,
    pub bytes: usize,
}

fn blob_hash(bytes: &[u8]) -> NodeHash {
    let mut digest = Sha256::new();
    digest.update(b"novovm-state-record-blob-v1\0");
    digest.update(bytes);
    digest.finalize().into()
}

fn decode_blob(bytes: &[u8]) -> Result<(&[u8], &[u8])> {
    if bytes.len() < BLOB_HEADER_BYTES || &bytes[..4] != b"NRB1" {
        bail!("invalid state record blob codec");
    }
    let key_len = usize::from(u16::from_be_bytes(bytes[4..6].try_into()?));
    let value_len = usize::try_from(u32::from_be_bytes(bytes[6..10].try_into()?))?;
    if key_len == 0
        || key_len > MAX_KEY_BYTES
        || value_len > MAX_RECORD_VALUE_BYTES_V1
        || bytes.len() != BLOB_HEADER_BYTES + key_len + value_len
    {
        bail!("invalid state record blob length");
    }
    Ok((
        &bytes[BLOB_HEADER_BYTES..BLOB_HEADER_BYTES + key_len],
        &bytes[BLOB_HEADER_BYTES + key_len..],
    ))
}

/// Validate immutable blob content before admitting writes to AOEM.
pub(crate) fn validate_record_blob(hash: &NodeHash, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_BLOB_BYTES || blob_hash(bytes) != *hash {
        bail!("state record blob content hash mismatch");
    }
    decode_blob(bytes)?;
    Ok(())
}

fn encode_blob(key: &[u8], value: &[u8]) -> Result<Vec<u8>> {
    state_key_hash(key)?;
    if value.len() > MAX_RECORD_VALUE_BYTES_V1 {
        bail!("state record value exceeds per-record limit");
    }
    let mut bytes = Vec::with_capacity(BLOB_HEADER_BYTES + key.len() + value.len());
    bytes.extend_from_slice(b"NRB1");
    bytes.extend_from_slice(&(key.len() as u16).to_be_bytes());
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(value);
    Ok(bytes)
}

fn encode_leaf(hash: NodeHash, length: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(LEAF_BYTES);
    bytes.extend_from_slice(b"NRL1");
    bytes.extend_from_slice(&hash);
    bytes.extend_from_slice(&(length as u32).to_be_bytes());
    bytes
}

fn decode_leaf(bytes: &[u8]) -> Result<(NodeHash, usize)> {
    if bytes.len() != LEAF_BYTES || &bytes[..4] != b"NRL1" {
        bail!("invalid state record leaf codec");
    }
    let length = usize::try_from(u32::from_be_bytes(bytes[36..40].try_into()?))?;
    if !(BLOB_HEADER_BYTES + 1..=MAX_BLOB_BYTES).contains(&length) {
        bail!("invalid state record descriptor length");
    }
    Ok((bytes[4..36].try_into()?, length))
}

fn load_blob(reader: &dyn StateRecordReader, hash: NodeHash, length: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .context("reserve state record buffer")?;
    for index in 0..length.div_ceil(RECORD_CHUNK_BYTES_V1) {
        let chunk = reader
            .read_record_chunk(hash, u32::try_from(index)?)?
            .context("state record chunk missing")?;
        let expected = (length - bytes.len()).min(RECORD_CHUNK_BYTES_V1);
        if chunk.len() != expected {
            bail!("state record chunk length mismatch");
        }
        bytes.extend_from_slice(&chunk);
    }
    validate_record_blob(&hash, &bytes)?;
    Ok(bytes)
}

pub fn read_record(
    reader: &dyn StateRecordReader,
    root: NodeHash,
    key: &[u8],
) -> Result<Option<Vec<u8>>> {
    let Some(leaf) = read_state_value(reader, root, key)? else {
        return Ok(None);
    };
    let (hash, length) = decode_leaf(&leaf)?;
    let bytes = load_blob(reader, hash, length)?;
    let (stored_key, value) = decode_blob(&bytes)?;
    if stored_key != key {
        bail!("state record original key differs from authenticated lookup");
    }
    Ok(Some(value.to_vec()))
}

/// Stream verified original keys and values. If any later callback/read fails,
/// the caller must discard the partial view; callbacks must not publish writes.
pub fn visit_records(
    reader: &dyn StateRecordReader,
    root: NodeHash,
    budget: RecordScanBudget,
    visitor: &mut StateRecordVisitor<'_>,
) -> Result<RecordScanStats> {
    let mut stats = RecordScanStats::default();
    stats.nodes = visit_state_leaves(reader, root, budget.max_nodes, &mut |key_hash, leaf| {
        let (hash, length) = decode_leaf(leaf)?;
        let next_bytes = stats
            .bytes
            .checked_add(length)
            .context("record scan byte count overflow")?;
        if stats.records >= budget.max_records || next_bytes > budget.max_bytes {
            bail!("state record scan exceeds resource budget");
        }
        let bytes = load_blob(reader, hash, length)?;
        let (key, value) = decode_blob(&bytes)?;
        if state_key_hash(key)? != key_hash {
            bail!("state record original key differs from authenticated leaf");
        }
        visitor(key, value)?;
        stats.records += 1;
        stats.bytes = next_bytes;
        Ok(())
    })?;
    Ok(stats)
}

pub fn stage_record_update(
    reader: &dyn StateRecordReader,
    parent_root: NodeHash,
    changes: &[RecordChange],
) -> Result<StagedRecordUpdate> {
    if changes.len() > MAX_STAGE_CHANGES {
        bail!("too many record changes in one stage");
    }
    let mut byte_count = 0usize;
    // Validate all per-call bounds before allocating duplicate encoded blobs.
    for change in changes {
        let (key, value_len) = match change {
            RecordChange::Put { key, value } => (key, value.len()),
            RecordChange::Delete { key } => (key, 0),
        };
        state_key_hash(key)?;
        if value_len > MAX_RECORD_VALUE_BYTES_V1 {
            bail!("state record value exceeds per-record limit");
        }
        byte_count = byte_count
            .checked_add(BLOB_HEADER_BYTES + key.len() + value_len)
            .context("record staging byte count overflow")?;
        if byte_count > MAX_STAGE_BLOB_BYTES {
            bail!("record staging exceeds per-call byte budget");
        }
    }
    let mut blobs = BTreeMap::new();
    let mut tree_changes = Vec::with_capacity(changes.len());
    for change in changes {
        tree_changes.push(match change {
            RecordChange::Put { key, value } => {
                let blob = encode_blob(key, value)?;
                let hash = blob_hash(&blob);
                let leaf = encode_leaf(hash, blob.len());
                match blobs.entry(hash) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(blob);
                    }
                    std::collections::btree_map::Entry::Occupied(entry) if entry.get() != &blob => {
                        bail!("immutable record hash collision");
                    }
                    std::collections::btree_map::Entry::Occupied(_) => {}
                }
                StateChange::Put {
                    key: key.clone(),
                    value: leaf,
                }
            }
            RecordChange::Delete { key } => StateChange::Delete { key: key.clone() },
        });
    }
    let tree = stage_state_update(reader, parent_root, &tree_changes)?;
    Ok(StagedRecordUpdate {
        parent_root,
        root: tree.root(),
        nodes: tree.nodes().clone(),
        blobs,
        loaded_nodes: tree.loaded_nodes(),
    })
}

/// An in-memory overlay for sequential staging against one trusted root. It is
/// never persisted as a separate database or head. The caller bounds the total
/// transaction/candidate workload; each stage has independent resource limits.
pub struct RecordOverlayV1<'a> {
    reader: &'a dyn StateRecordReader,
    update: StagedRecordUpdate,
}

impl<'a> RecordOverlayV1<'a> {
    pub fn new(reader: &'a dyn StateRecordReader, parent_root: NodeHash) -> Self {
        Self {
            reader,
            update: StagedRecordUpdate {
                parent_root,
                root: parent_root,
                nodes: BTreeMap::new(),
                blobs: BTreeMap::new(),
                loaded_nodes: 0,
            },
        }
    }

    pub fn root(&self) -> NodeHash {
        self.update.root
    }

    pub fn stage(&mut self, changes: &[RecordChange]) -> Result<()> {
        let next = stage_record_update(self, self.root(), changes)?;
        let loaded_nodes = self
            .update
            .loaded_nodes
            .checked_add(next.loaded_nodes)
            .context("record overlay node count overflow")?;
        // Check before changing the overlay, so errors leave the previous view.
        for (existing, incoming) in [
            (&self.update.nodes, &next.nodes),
            (&self.update.blobs, &next.blobs),
        ] {
            for (hash, bytes) in incoming {
                if existing.get(hash).is_some_and(|old| old != bytes) {
                    bail!("immutable record overlay content differs");
                }
            }
        }
        self.update.nodes.extend(next.nodes);
        self.update.blobs.extend(next.blobs);
        self.update.root = next.root;
        self.update.loaded_nodes = loaded_nodes;
        Ok(())
    }

    pub fn finish(self) -> StagedRecordUpdate {
        self.update
    }
}

impl StateNodeReader for RecordOverlayV1<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        match self.update.nodes.get(hash) {
            Some(bytes) => Ok(Some(bytes.clone())),
            None => self.reader.read_node(hash),
        }
    }
}

impl StateRecordReader for RecordOverlayV1<'_> {
    fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
        match self.update.blobs.get(&hash) {
            Some(bytes) => {
                let start = usize::try_from(index)?
                    .checked_mul(RECORD_CHUNK_BYTES_V1)
                    .context("record chunk offset overflow")?;
                if start >= bytes.len() {
                    return Ok(None);
                }
                Ok(Some(
                    bytes[start..(start + RECORD_CHUNK_BYTES_V1).min(bytes.len())].to_vec(),
                ))
            }
            None => self.reader.read_record_chunk(hash, index),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_state_tree::empty_root;

    #[derive(Default)]
    struct Memory {
        nodes: BTreeMap<NodeHash, Vec<u8>>,
        chunks: BTreeMap<(NodeHash, u32), Vec<u8>>,
    }
    impl StateNodeReader for Memory {
        fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
            Ok(self.nodes.get(hash).cloned())
        }
    }
    impl StateRecordReader for Memory {
        fn read_record_chunk(&self, hash: NodeHash, index: u32) -> Result<Option<Vec<u8>>> {
            Ok(self.chunks.get(&(hash, index)).cloned())
        }
    }
    impl Memory {
        fn apply(&mut self, update: StagedRecordUpdate) -> NodeHash {
            self.nodes.extend(update.nodes);
            for (hash, bytes) in update.blobs {
                for (index, chunk) in bytes.chunks(RECORD_CHUNK_BYTES_V1).enumerate() {
                    self.chunks.insert((hash, index as u32), chunk.to_vec());
                }
            }
            update.root
        }
    }
    fn put(key: &[u8], value: &[u8]) -> RecordChange {
        RecordChange::Put {
            key: key.to_vec(),
            value: value.to_vec(),
        }
    }
    fn scan(reader: &dyn StateRecordReader, root: NodeHash) -> Result<BTreeMap<Vec<u8>, Vec<u8>>> {
        let mut records = BTreeMap::new();
        visit_records(
            reader,
            root,
            RecordScanBudget {
                max_nodes: 1_000_000,
                max_records: 100_000,
                max_bytes: 64 * 1024 * 1024,
            },
            &mut |key, value| {
                if records.insert(key.to_vec(), value.to_vec()).is_some() {
                    bail!("duplicate restored key");
                }
                Ok(())
            },
        )?;
        Ok(records)
    }

    #[test]
    fn records_preserve_original_keys_large_values_and_old_roots() {
        let mut memory = Memory::default();
        let expected = BTreeMap::from([
            (b"empty".to_vec(), vec![]),
            (b"boundary".to_vec(), vec![3; 494]), // Exactly one 512-byte blob.
            (b"large".to_vec(), vec![7; 5_003]),
            (vec![0xff; 256], vec![0, 0xff, 4]),
        ]);
        let changes: Vec<_> = expected
            .iter()
            .map(|(key, value)| put(key, value))
            .collect();
        let update = stage_record_update(&memory, empty_root(), &changes).unwrap();
        assert!(update.blobs().values().any(|blob| blob.len() == 512));
        let root = memory.apply(update);
        assert_eq!(scan(&memory, root).unwrap(), expected);
        for (key, value) in &expected {
            assert_eq!(
                read_record(&memory, root, key).unwrap(),
                Some(value.clone())
            );
        }
        assert_eq!(read_record(&memory, root, b"absent").unwrap(), None);
        let updated = stage_record_update(
            &memory,
            root,
            &[
                put(b"large", b"replacement"),
                RecordChange::Delete {
                    key: b"boundary".to_vec(),
                },
            ],
        )
        .unwrap();
        let next = memory.apply(updated);
        assert_eq!(scan(&memory, root).unwrap(), expected);
        let mut changed = expected;
        changed.insert(b"large".to_vec(), b"replacement".to_vec());
        changed.remove(b"boundary".as_slice());
        assert_eq!(scan(&memory, next).unwrap(), changed);
    }

    #[test]
    fn chunk_loss_corruption_truncation_and_false_original_key_fail_closed() {
        let mut memory = Memory::default();
        let staged =
            stage_record_update(&memory, empty_root(), &[put(b"a", &vec![4; 1200])]).unwrap();
        let hash = *staged.blobs.keys().next().unwrap();
        let root = memory.apply(staged);
        let middle = memory.chunks.remove(&(hash, 1)).unwrap();
        assert!(read_record(&memory, root, b"a")
            .unwrap_err()
            .to_string()
            .contains("missing"));
        assert!(scan(&memory, root).is_err());
        memory.chunks.insert((hash, 1), middle.clone());
        memory.chunks.get_mut(&(hash, 1)).unwrap()[0] ^= 1;
        assert!(read_record(&memory, root, b"a").is_err());
        memory
            .chunks
            .insert((hash, 1), middle[..middle.len() - 1].to_vec());
        assert!(read_record(&memory, root, b"a").is_err());
        memory.chunks.insert((hash, 1), middle);
        let forged_blob = encode_blob(b"other", b"value").unwrap();
        let forged_hash = blob_hash(&forged_blob);
        memory.chunks.insert((forged_hash, 0), forged_blob.clone());
        let forged = stage_state_update(
            &memory,
            root,
            &[StateChange::Put {
                key: b"a".to_vec(),
                value: encode_leaf(forged_hash, forged_blob.len()),
            }],
        )
        .unwrap();
        let forged_root = forged.root();
        memory.nodes.extend(forged.nodes().clone());
        assert!(read_record(&memory, forged_root, b"a").is_err());
        assert!(scan(&memory, forged_root).is_err());
    }

    #[test]
    fn record_codec_rejects_bad_lengths_and_trailing_bytes_before_use() {
        assert!(encode_blob(b"", b"a").is_err());
        assert!(encode_blob(&vec![1; 257], b"a").is_err());
        assert!(decode_leaf(&[]).is_err());
        let mut leaf = encode_leaf([1; 32], 20);
        leaf[36..40].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode_leaf(&leaf).is_err());
        let mut blob = encode_blob(b"x", b"value").unwrap();
        blob.push(0);
        assert!(validate_record_blob(&blob_hash(&blob), &blob).is_err());
        blob.pop();
        blob[4..6].copy_from_slice(&0u16.to_be_bytes());
        assert!(validate_record_blob(&blob_hash(&blob), &blob).is_err());
        let bytes = vec![8; MAX_RECORD_VALUE_BYTES_V1];
        assert!(encode_blob(b"cap", &bytes).is_ok());
        let too_large = vec![8; MAX_RECORD_VALUE_BYTES_V1 + 1];
        assert!(
            stage_record_update(&Memory::default(), empty_root(), &[put(b"cap", &too_large)])
                .is_err()
        );
    }

    #[test]
    fn overlay_batches_match_direct_staging_and_reference_map() {
        let memory = Memory::default();
        let changes: Vec<_> = (0u32..257)
            .map(|key| put(&key.to_be_bytes(), &vec![key as u8; 700]))
            .collect();
        let direct = stage_record_update(&memory, empty_root(), &changes).unwrap();
        let reversed = stage_record_update(
            &memory,
            empty_root(),
            &changes.iter().rev().cloned().collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(direct.root(), reversed.root());
        let mut overlay = RecordOverlayV1::new(&memory, empty_root());
        for batch in changes.chunks(13) {
            overlay.stage(batch).unwrap();
        }
        assert_eq!(overlay.root(), direct.root());
        let mut expected = scan(&overlay, overlay.root()).unwrap();
        for index in 0u32..100 {
            let key = index.to_be_bytes().to_vec();
            let change = if index % 2 == 0 {
                expected.remove(&key);
                RecordChange::Delete { key }
            } else {
                expected.insert(key.clone(), vec![9; 3]);
                put(&key, &[9; 3])
            };
            overlay.stage(&[change]).unwrap();
        }
        assert_eq!(scan(&overlay, overlay.root()).unwrap(), expected);
        let before_error = overlay.root();
        assert!(overlay.stage(&[put(&[], b"invalid")]).is_err());
        assert_eq!(overlay.root(), before_error);
        let staged = overlay.finish();
        assert_eq!(staged.parent_root(), empty_root());
        let mut persisted = Memory::default();
        let root = persisted.apply(staged);
        assert_eq!(scan(&persisted, root).unwrap(), expected);
    }

    #[test]
    fn scan_budgets_fail_instead_of_returning_a_partial_success() {
        let mut memory = Memory::default();
        let staged =
            stage_record_update(&memory, empty_root(), &[put(b"a", b"b"), put(b"c", b"d")])
                .unwrap();
        let root = memory.apply(staged);
        for budget in [
            RecordScanBudget {
                max_nodes: 2,
                max_records: 2,
                max_bytes: 100,
            },
            RecordScanBudget {
                max_nodes: 3,
                max_records: 1,
                max_bytes: 100,
            },
            RecordScanBudget {
                max_nodes: 3,
                max_records: 2,
                max_bytes: 12,
            },
        ] {
            assert!(visit_records(&memory, root, budget, &mut |_, _| Ok(())).is_err());
        }
        let stats = visit_records(
            &memory,
            root,
            RecordScanBudget {
                max_nodes: 3,
                max_records: 2,
                max_bytes: 24,
            },
            &mut |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(
            stats,
            RecordScanStats {
                nodes: 3,
                records: 2,
                bytes: 24
            }
        );
        assert!(visit_records(
            &memory,
            root,
            RecordScanBudget {
                max_nodes: 3,
                max_records: 2,
                max_bytes: 24,
            },
            &mut |_, _| bail!("consumer rejected")
        )
        .is_err());
    }

    #[test]
    #[ignore = "large record-state fixture; explicitly run"]
    fn record_state_above_eight_mib_keeps_single_update_bounded() {
        const KEYS: u32 = 16_384;
        let mut memory = Memory::default();
        let mut overlay = RecordOverlayV1::new(&memory, empty_root());
        for start in (0..KEYS).step_by(128) {
            let changes: Vec<_> = (start..start + 128)
                .map(|index| put(&index.to_be_bytes(), &[7; 600]))
                .collect();
            overlay.stage(&changes).unwrap();
        }
        let before = memory.apply(overlay.finish());
        let stats = visit_records(
            &memory,
            before,
            RecordScanBudget {
                max_nodes: 2 * KEYS as usize - 1,
                max_records: KEYS as usize,
                max_bytes: 16 * 1024 * 1024,
            },
            &mut |_, value| {
                assert_eq!(value, &[7; 600]);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(stats.records, KEYS as usize);
        assert_eq!(stats.bytes, KEYS as usize * (BLOB_HEADER_BYTES + 4 + 600));
        assert!(stats.bytes > 8 * 1024 * 1024);
        let update =
            stage_record_update(&memory, before, &[put(&13u32.to_be_bytes(), &[9; 600])]).unwrap();
        assert!(update.loaded_nodes() <= 257);
        assert!(update.nodes().len() <= 257);
        assert_eq!(update.blobs().len(), 1);
        eprintln!("record capacity: {} records, {} blob bytes; one change loads {} nodes and stages {} nodes/{} blob", stats.records, stats.bytes, update.loaded_nodes(), update.nodes().len(), update.blobs().len());
        let after = memory.apply(update);
        assert_eq!(
            read_record(&memory, before, &13u32.to_be_bytes()).unwrap(),
            Some(vec![7; 600])
        );
        assert_eq!(
            read_record(&memory, after, &13u32.to_be_bytes()).unwrap(),
            Some(vec![9; 600])
        );
        assert_eq!(
            read_record(&memory, after, &14u32.to_be_bytes()).unwrap(),
            Some(vec![7; 600])
        );
    }
}
