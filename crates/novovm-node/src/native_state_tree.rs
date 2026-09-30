//! Immutable, content-addressed candidate state nodes.
//!
//! This codec is NOT the existing native state-root codec. It is not admitted
//! by the current block/parent protocol until that versioned integration is
//! complete. Updates stage bounded nodes only; the caller must persist them in
//! AOEM and verify durable completion before publishing any root. Nodes must
//! not live in, or be deleted with, temporary candidate workspace chunks.
//! Parent roots must already be trusted and structurally validated. This module
//! verifies accessed paths, not an arbitrary imported tree in its entirety;
//! an empty update does not validate its parent. No tree scanning or GC occurs.

use anyhow::{bail, Context, Result};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub type NodeHash = [u8; 32];
pub const STATE_TREE_CODEC_V1: &str = "novovm-state-patricia-sha256/v1";
const MAX_KEY_BYTES: usize = 256;
const MAX_VALUE_BYTES: usize = 256;
const MAX_BATCH_CHANGES: usize = 4096;
const MAX_STAGED_NODES: usize = 65_536;
const MAX_CACHED_NODES: usize = 4096;
// At most 256 branches, one leaf, and one collapse survivor per changed key.
// This limits one operation's reads, never the size of the persisted ledger.
const MAX_READER_READS: usize = MAX_BATCH_CHANGES * 258;

/// The provider must read immutable node bytes by their exact content hash.
pub trait StateNodeReader {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>>;
}

#[derive(Debug, Clone)]
pub enum StateChange {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

#[derive(Debug)]
pub struct StagedStateUpdate {
    parent_root: NodeHash,
    root: NodeHash,
    /// Reachable staged nodes; some content may already exist in the provider.
    /// These writes do not publish authority.
    nodes: BTreeMap<NodeHash, Vec<u8>>,
    loaded_nodes: usize,
}

impl StagedStateUpdate {
    pub fn parent_root(&self) -> NodeHash {
        self.parent_root
    }

    pub fn root(&self) -> NodeHash {
        self.root
    }

    pub fn nodes(&self) -> &BTreeMap<NodeHash, Vec<u8>> {
        &self.nodes
    }

    pub fn loaded_nodes(&self) -> usize {
        self.loaded_nodes
    }
}

#[derive(Clone)]
enum Node {
    Leaf {
        key: NodeHash,
        value: Vec<u8>,
    },
    Branch {
        bit: u16,
        prefix: NodeHash,
        left: NodeHash,
        right: NodeHash,
    },
}

pub fn empty_root() -> NodeHash {
    Sha256::digest(b"novovm-state-patricia-v1:empty\0").into()
}

fn digest_key(key: &[u8]) -> Result<NodeHash> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        bail!("state key must contain 1..=256 bytes");
    }
    let mut hash = Sha256::new();
    hash.update(b"novovm-state-patricia-v1:key\0");
    hash.update((key.len() as u64).to_be_bytes());
    hash.update(key);
    Ok(hash.finalize().into())
}

fn digest_node(bytes: &[u8]) -> NodeHash {
    let mut hash = Sha256::new();
    hash.update(b"novovm-state-patricia-v1:node\0");
    hash.update(bytes);
    hash.finalize().into()
}

/// Validate one content-addressed node's bytes and local canonical encoding.
/// This does not prove child availability, parent-path validity, or root trust.
pub fn validate_state_node_bytes(hash: &NodeHash, bytes: &[u8]) -> Result<()> {
    decode_state_node(hash, bytes).map(|_| ())
}

fn decode_state_node(hash: &NodeHash, bytes: &[u8]) -> Result<Node> {
    if bytes.len() > 35 + MAX_VALUE_BYTES || digest_node(bytes) != *hash {
        bail!("state node content hash or byte bound mismatch");
    }
    Node::decode(bytes)
}

fn bit_at(key: &NodeHash, bit: u16) -> bool {
    key[usize::from(bit / 8)] & (0x80 >> (bit % 8)) != 0
}

fn prefix_of(key: &NodeHash, length: u16) -> NodeHash {
    let mut result = *key;
    for bit in length..256 {
        result[usize::from(bit / 8)] &= !(0x80 >> (bit % 8));
    }
    result
}

fn common_prefix(left: &NodeHash, right: &NodeHash) -> u16 {
    for (index, (a, b)) in left.iter().zip(right).enumerate() {
        let different = a ^ b;
        if different != 0 {
            return index as u16 * 8 + different.leading_zeros() as u16;
        }
    }
    256
}

impl Node {
    fn representative(&self) -> NodeHash {
        match self {
            Self::Leaf { key, .. } => *key,
            Self::Branch { prefix, .. } => *prefix,
        }
    }

    fn encode(&self) -> Vec<u8> {
        match self {
            Self::Leaf { key, value } => {
                let mut bytes = vec![1];
                bytes.extend_from_slice(key);
                bytes.extend_from_slice(&(value.len() as u16).to_be_bytes());
                bytes.extend_from_slice(value);
                bytes
            }
            Self::Branch {
                bit,
                prefix,
                left,
                right,
            } => {
                let mut bytes = vec![2];
                bytes.extend_from_slice(&bit.to_be_bytes());
                bytes.extend_from_slice(prefix);
                bytes.extend_from_slice(left);
                bytes.extend_from_slice(right);
                bytes
            }
        }
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        match bytes.first() {
            Some(1) if bytes.len() >= 35 => {
                let length = usize::from(u16::from_be_bytes(bytes[33..35].try_into()?));
                if length > MAX_VALUE_BYTES || bytes.len() != 35 + length {
                    bail!("invalid state leaf value length");
                }
                Ok(Self::Leaf {
                    key: bytes[1..33].try_into()?,
                    value: bytes[35..].to_vec(),
                })
            }
            Some(2) if bytes.len() == 99 => {
                let bit = u16::from_be_bytes(bytes[1..3].try_into()?);
                let prefix: NodeHash = bytes[3..35].try_into()?;
                let left: NodeHash = bytes[35..67].try_into()?;
                let right: NodeHash = bytes[67..99].try_into()?;
                if bit >= 256
                    || prefix_of(&prefix, bit) != prefix
                    || left == empty_root()
                    || right == empty_root()
                    || left == right
                {
                    bail!("non-canonical state branch");
                }
                Ok(Self::Branch {
                    bit,
                    prefix,
                    left,
                    right,
                })
            }
            _ => bail!("invalid state node encoding"),
        }
    }

    fn validate_path(&self, key: &NodeHash, minimum_bit: u16) -> Result<()> {
        if common_prefix(&self.representative(), key) < minimum_bit
            || matches!(self, Self::Branch { bit, .. } if *bit < minimum_bit)
        {
            bail!("state node violates its authenticated parent path");
        }
        Ok(())
    }
}

struct Planner<'a> {
    reader: &'a dyn StateNodeReader,
    nodes: BTreeMap<NodeHash, Vec<u8>>,
    cache: BTreeMap<NodeHash, Node>,
    loaded_nodes: usize,
}

impl<'a> Planner<'a> {
    fn new(reader: &'a dyn StateNodeReader) -> Self {
        Self {
            reader,
            nodes: BTreeMap::new(),
            cache: BTreeMap::new(),
            loaded_nodes: 0,
        }
    }

    fn load(&mut self, hash: &NodeHash) -> Result<Node> {
        if let Some(bytes) = self.nodes.get(hash) {
            return decode_state_node(hash, bytes);
        }
        if let Some(node) = self.cache.get(hash) {
            return Ok(node.clone());
        }
        if self.loaded_nodes >= MAX_READER_READS {
            bail!("state update exceeds reader budget");
        }
        self.loaded_nodes += 1;
        let bytes = self
            .reader
            .read_node(hash)?
            .context("state node is missing")?;
        let node = decode_state_node(hash, &bytes)?;
        if self.cache.len() < MAX_CACHED_NODES {
            self.cache.insert(*hash, node.clone());
        }
        Ok(node)
    }

    fn stage(&mut self, node: Node) -> Result<NodeHash> {
        let bytes = node.encode();
        let hash = digest_node(&bytes);
        if !self.nodes.contains_key(&hash) && self.nodes.len() >= MAX_STAGED_NODES {
            bail!("state update exceeds staged-node budget");
        }
        self.nodes.insert(hash, bytes);
        Ok(hash)
    }

    fn change(
        &mut self,
        current: NodeHash,
        key: NodeHash,
        value: Option<&[u8]>,
        minimum_bit: u16,
    ) -> Result<NodeHash> {
        if current == empty_root() {
            return match value {
                Some(value) => self.stage(Node::Leaf {
                    key,
                    value: value.to_vec(),
                }),
                None => Ok(current),
            };
        }
        let node = self.load(&current)?;
        node.validate_path(&key, minimum_bit)?;
        let representative = node.representative();
        let common = common_prefix(&key, &representative);
        let node_bit = match &node {
            Node::Leaf { .. } => 256,
            Node::Branch { bit, .. } => *bit,
        };
        if common < node_bit {
            let Some(value) = value else {
                return Ok(current);
            };
            let new_leaf = self.stage(Node::Leaf {
                key,
                value: value.to_vec(),
            })?;
            let (left, right) = if bit_at(&key, common) {
                (current, new_leaf)
            } else {
                (new_leaf, current)
            };
            return self.stage(Node::Branch {
                bit: common,
                prefix: prefix_of(&key, common),
                left,
                right,
            });
        }
        match node {
            Node::Leaf {
                key: previous,
                value: old_value,
            } => {
                if previous != key {
                    bail!("state leaf comparison invariant");
                }
                match value {
                    Some(value) if value == old_value => Ok(current),
                    Some(value) => self.stage(Node::Leaf {
                        key,
                        value: value.to_vec(),
                    }),
                    None => Ok(empty_root()),
                }
            }
            Node::Branch {
                bit,
                prefix,
                mut left,
                mut right,
            } => {
                if bit_at(&key, bit) {
                    let next = self.change(right, key, value, bit + 1)?;
                    if next == right {
                        return Ok(current);
                    }
                    right = next;
                } else {
                    let next = self.change(left, key, value, bit + 1)?;
                    if next == left {
                        return Ok(current);
                    }
                    left = next;
                }
                if left == empty_root() || right == empty_root() {
                    let survives_right = left == empty_root();
                    let survivor = if survives_right { right } else { left };
                    let mut child_path = prefix;
                    if survives_right {
                        child_path[usize::from(bit / 8)] |= 0x80 >> (bit % 8);
                    }
                    self.load(&survivor)?.validate_path(&child_path, bit + 1)?;
                    return Ok(survivor);
                }
                self.stage(Node::Branch {
                    bit,
                    prefix,
                    left,
                    right,
                })
            }
        }
    }

    fn retain_reachable(&mut self, root: NodeHash) -> Result<()> {
        let mut stack = vec![root];
        let mut reachable = BTreeSet::new();
        while let Some(hash) = stack.pop() {
            let Some(bytes) = self.nodes.get(&hash) else {
                continue;
            };
            if !reachable.insert(hash) {
                continue;
            }
            if let Node::Branch { left, right, .. } = Node::decode(bytes)? {
                stack.push(left);
                stack.push(right);
            }
        }
        self.nodes.retain(|hash, _| reachable.contains(hash));
        Ok(())
    }
}

/// Construct a new root without mutating the provider or any authority pointer.
/// Repeated changes to the same key are applied in the supplied order.
/// `parent_root` must be an already validated, durably completed root, not an
/// untrusted import. Only accessed paths are checked; untouched subtrees are not
/// scanned. In particular, an empty change set does not validate its parent.
pub fn stage_state_update(
    reader: &dyn StateNodeReader,
    parent_root: NodeHash,
    changes: &[StateChange],
) -> Result<StagedStateUpdate> {
    if changes.len() > MAX_BATCH_CHANGES {
        bail!("too many state changes");
    }
    let mut planner = Planner::new(reader);
    let mut root = parent_root;
    for change in changes {
        let (key, value) = match change {
            StateChange::Put { key, value } => {
                if value.len() > MAX_VALUE_BYTES {
                    bail!("state leaf value exceeds 256 bytes");
                }
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
    })
}

/// Read from an already trusted, structurally validated root. Missing or corrupt
/// accessed nodes are errors; a valid path that excludes the key returns None.
pub fn read_state_value(
    reader: &dyn StateNodeReader,
    root: NodeHash,
    key: &[u8],
) -> Result<Option<Vec<u8>>> {
    let key = digest_key(key)?;
    let mut planner = Planner::new(reader);
    let mut current = root;
    let mut minimum_bit = 0;
    while current != empty_root() {
        let node = planner.load(&current)?;
        node.validate_path(&key, minimum_bit)?;
        match node {
            Node::Leaf { key: stored, value } => return Ok((stored == key).then_some(value)),
            Node::Branch {
                bit,
                prefix,
                left,
                right,
            } => {
                if common_prefix(&key, &prefix) < bit {
                    return Ok(None);
                }
                current = if bit_at(&key, bit) { right } else { left };
                minimum_bit = bit + 1;
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Memory(BTreeMap<NodeHash, Vec<u8>>);
    impl StateNodeReader for Memory {
        fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
            Ok(self.0.get(hash).cloned())
        }
    }
    fn put(index: u32, value: u8) -> StateChange {
        StateChange::Put {
            key: index.to_be_bytes().to_vec(),
            value: vec![value],
        }
    }
    fn persist(memory: &mut Memory, update: StagedStateUpdate) -> NodeHash {
        memory.0.extend(update.nodes);
        update.root
    }

    #[test]
    fn roots_are_order_independent_and_old_versions_remain_readable() {
        let mut memory = Memory::default();
        let changes: Vec<_> = (0..128).map(|key| put(key, 7)).collect();
        let a = stage_state_update(&memory, empty_root(), &changes).unwrap();
        let b = stage_state_update(
            &memory,
            empty_root(),
            &changes.iter().rev().cloned().collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(a.root, b.root);
        let old = persist(&mut memory, a);
        let change = stage_state_update(&memory, old, &[put(1, 9)]).unwrap();
        assert!(change.nodes.len() < 32);
        assert!(change.loaded_nodes < 32);
        let new = persist(&mut memory, change);
        assert_ne!(new, old);
        assert_eq!(
            read_state_value(&memory, old, &1u32.to_be_bytes()).unwrap(),
            Some(vec![7])
        );
        assert_eq!(
            read_state_value(&memory, new, &1u32.to_be_bytes()).unwrap(),
            Some(vec![9])
        );
        for key in 2u32..128 {
            assert_eq!(
                read_state_value(&memory, new, &key.to_be_bytes()).unwrap(),
                Some(vec![7])
            );
        }
        assert_eq!(
            read_state_value(&memory, new, &999u32.to_be_bytes()).unwrap(),
            None
        );
    }

    #[test]
    fn deletion_compresses_paths_and_restores_canonical_empty_root() {
        let mut memory = Memory::default();
        let changes: Vec<_> = (0..32).map(|key| put(key, 3)).collect();
        let staged = stage_state_update(&memory, empty_root(), &changes).unwrap();
        let mut root = persist(&mut memory, staged);
        for index in 0u32..32 {
            let removed = StateChange::Delete {
                key: index.to_be_bytes().to_vec(),
            };
            let update = stage_state_update(&memory, root, &[removed]).unwrap();
            root = persist(&mut memory, update);
            let expected: Vec<_> = (index + 1..32).map(|key| put(key, 3)).collect();
            assert_eq!(
                root,
                stage_state_update(&memory, empty_root(), &expected)
                    .unwrap()
                    .root
            );
        }
        assert_eq!(root, empty_root());
    }

    #[test]
    fn corrupt_missing_and_noncanonical_nodes_fail_closed() {
        let mut memory = Memory::default();
        let update = stage_state_update(&memory, empty_root(), &[put(1, 7)]).unwrap();
        let root = persist(&mut memory, update);
        memory.0.get_mut(&root).unwrap()[35] ^= 1;
        assert!(read_state_value(&memory, root, &1u32.to_be_bytes()).is_err());
        memory.0.clear();
        assert!(read_state_value(&memory, root, &1u32.to_be_bytes()).is_err());
        let node = Node::Branch {
            bit: 256,
            prefix: [0; 32],
            left: [1; 32],
            right: [2; 32],
        };
        assert!(Node::decode(&node.encode()).is_err());
        assert!(Node::decode(&[1, 2, 3]).is_err());
    }

    #[test]
    fn updates_are_bounded_and_never_write_to_reader() {
        let memory = Memory::default();
        let changes = [
            put(1, 7),
            put(1, 8),
            StateChange::Delete {
                key: 1u32.to_be_bytes().to_vec(),
            },
        ];
        let update = stage_state_update(&memory, empty_root(), &changes).unwrap();
        assert_eq!(update.root, empty_root());
        assert!(update.nodes.is_empty());
        assert!(memory.0.is_empty());
        let oversized = StateChange::Put {
            key: vec![1],
            value: vec![0; MAX_VALUE_BYTES + 1],
        };
        assert!(stage_state_update(&memory, empty_root(), &[oversized]).is_err());
        assert!(read_state_value(&memory, empty_root(), &[]).is_err());
        assert!(stage_state_update(
            &memory,
            empty_root(),
            &vec![put(1, 7); MAX_BATCH_CHANGES + 1]
        )
        .is_err());
    }

    #[test]
    fn no_op_changes_do_not_restage_ancestor_nodes() {
        let mut memory = Memory::default();
        let initial: Vec<_> = (0..128).map(|key| put(key, 7)).collect();
        let update = stage_state_update(&memory, empty_root(), &initial).unwrap();
        let root = persist(&mut memory, update);
        let changes = [
            put(1, 7),
            StateChange::Delete {
                key: 999u32.to_be_bytes().to_vec(),
            },
        ];
        let update = stage_state_update(&memory, root, &changes).unwrap();
        assert_eq!(update.parent_root(), root);
        assert_eq!(update.root(), root);
        assert!(update.nodes().is_empty());
        assert!(update.loaded_nodes() > 0);
    }

    #[test]
    fn collapsing_a_branch_checks_survivor_availability_and_direction() {
        // Exercise collapse on both sides, including a valid hash whose leaf
        // belongs on the wrong side of the original authenticated branch.
        for delete_index in [1u32, 2u32] {
            let mut memory = Memory::default();
            let update =
                stage_state_update(&memory, empty_root(), &[put(1, 7), put(2, 8)]).unwrap();
            let root = persist(&mut memory, update);
            let Node::Branch {
                bit,
                prefix,
                left,
                right,
            } = Node::decode(memory.0.get(&root).unwrap()).unwrap()
            else {
                panic!("two distinct keys require a branch")
            };
            let deleted_key = digest_key(&delete_index.to_be_bytes()).unwrap();
            let deleting_right = bit_at(&deleted_key, bit);
            let survivor = if deleting_right { left } else { right };
            let removed = memory.0.remove(&survivor).unwrap();
            let delete = StateChange::Delete {
                key: delete_index.to_be_bytes().to_vec(),
            };
            let error =
                stage_state_update(&memory, root, std::slice::from_ref(&delete)).unwrap_err();
            assert!(error.to_string().contains("missing"));

            memory.0.insert(survivor, removed);
            let wrong_bytes = Node::Leaf {
                key: deleted_key,
                value: vec![99],
            }
            .encode();
            let wrong_hash = digest_node(&wrong_bytes);
            memory.0.insert(wrong_hash, wrong_bytes);
            let wrong_branch = Node::Branch {
                bit,
                prefix,
                left: if deleting_right { wrong_hash } else { left },
                right: if deleting_right { right } else { wrong_hash },
            }
            .encode();
            let wrong_root = digest_node(&wrong_branch);
            memory.0.insert(wrong_root, wrong_branch);
            let error = stage_state_update(&memory, wrong_root, &[delete]).unwrap_err();
            assert!(error.to_string().contains("authenticated parent path"));
        }
    }

    #[test]
    fn invalid_child_direction_is_not_reported_as_absence() {
        let mut memory = Memory::default();
        let update = stage_state_update(&memory, empty_root(), &[put(1, 7), put(2, 8)]).unwrap();
        let root = persist(&mut memory, update);
        let Node::Branch {
            bit,
            prefix,
            left,
            right,
        } = Node::decode(memory.0.get(&root).unwrap()).unwrap()
        else {
            panic!("two distinct keys require a branch")
        };
        let bytes = Node::Branch {
            bit,
            prefix,
            left: right,
            right: left,
        }
        .encode();
        let bad_root = digest_node(&bytes);
        memory.0.insert(bad_root, bytes);
        for key in [1u32, 2u32] {
            assert!(read_state_value(&memory, bad_root, &key.to_be_bytes()).is_err());
            assert!(stage_state_update(&memory, bad_root, &[put(key, 9)]).is_err());
        }
    }

    #[test]
    fn randomized_changes_match_ordered_map_across_batch_boundaries() {
        // Fixed xorshift input avoids a random dependency or nondeterministic CI.
        let mut seed = 0x37a5_239f_012b_6ed1u64;
        let mut changes = Vec::new();
        for _ in 0..192 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let key = ((seed >> 8) as u32 % 64).to_be_bytes().to_vec();
            changes.push(if seed & 3 == 0 {
                StateChange::Delete { key }
            } else {
                StateChange::Put {
                    key,
                    value: seed.to_be_bytes()[..3].to_vec(),
                }
            });
        }
        let expected_root = stage_state_update(&Memory::default(), empty_root(), &changes)
            .unwrap()
            .root;
        for chunk_size in [1, 7, 48, 192] {
            let mut memory = Memory::default();
            let mut expected = BTreeMap::<Vec<u8>, Vec<u8>>::new();
            let mut root = empty_root();
            for chunk in changes.chunks(chunk_size) {
                let update = stage_state_update(&memory, root, chunk).unwrap();
                root = persist(&mut memory, update);
                for change in chunk {
                    match change {
                        StateChange::Put { key, value } => {
                            expected.insert(key.clone(), value.clone());
                        }
                        StateChange::Delete { key } => {
                            expected.remove(key);
                        }
                    }
                }
                for index in 0u32..64 {
                    let key = index.to_be_bytes();
                    assert_eq!(
                        read_state_value(&memory, root, &key).unwrap(),
                        expected.get(key.as_slice()).cloned()
                    );
                }
                let rebuilt: Vec<_> = expected
                    .iter()
                    .map(|(key, value)| StateChange::Put {
                        key: key.clone(),
                        value: value.clone(),
                    })
                    .collect();
                assert_eq!(
                    root,
                    stage_state_update(&Memory::default(), empty_root(), &rebuilt)
                        .unwrap()
                        .root
                );
            }
            assert_eq!(root, expected_root);
        }
    }

    #[test]
    fn node_validation_and_reader_budget_are_local_and_bounded() {
        let mut memory = Memory::default();
        let update = stage_state_update(&memory, empty_root(), &[put(1, 7)]).unwrap();
        let root = persist(&mut memory, update);
        let bytes = memory.0.get(&root).unwrap();
        validate_state_node_bytes(&root, bytes).unwrap();
        assert!(validate_state_node_bytes(&empty_root(), bytes).is_err());
        let malformed = vec![1, 2, 3];
        assert!(validate_state_node_bytes(&digest_node(&malformed), &malformed).is_err());
        let mut planner = Planner::new(&memory);
        planner.load(&root).unwrap();
        planner.load(&root).unwrap();
        assert_eq!(
            planner.loaded_nodes, 1,
            "repeated immutable reads use the cache"
        );
        let mut exhausted = Planner::new(&memory);
        exhausted.loaded_nodes = MAX_READER_READS;
        assert!(exhausted.load(&root).is_err());
    }

    #[test]
    #[ignore = "large incremental-state capacity fixture; explicitly run"]
    fn current_state_larger_than_eight_mib_has_bounded_single_key_updates() {
        // This exercises the unactivated tree component, not fresh-chain
        // protocol integration or AOEM persistence. Every value is legal;
        // capacity comes from real distinct current-state entries.
        const KEYS: u32 = 32_768;
        const CHUNK: u32 = 512;
        let mut memory = Memory::default();
        let mut root = empty_root();
        for first in (0..KEYS).step_by(CHUNK as usize) {
            let changes: Vec<_> = (first..first + CHUNK)
                .map(|index| StateChange::Put {
                    key: index.to_be_bytes().to_vec(),
                    value: vec![7; MAX_VALUE_BYTES],
                })
                .collect();
            let update = stage_state_update(&memory, root, &changes).unwrap();
            root = persist(&mut memory, update);
        }

        // Count only nodes reachable from the CURRENT root, not the retained
        // obsolete versions, so historical garbage cannot satisfy the test.
        let mut stack = vec![root];
        let mut reachable = BTreeSet::new();
        let mut current_bytes = 0usize;
        let mut leaves = 0usize;
        while let Some(hash) = stack.pop() {
            assert!(
                reachable.insert(hash),
                "canonical tree must not share a child"
            );
            let bytes = memory.0.get(&hash).expect("all current nodes are present");
            current_bytes += bytes.len();
            match decode_state_node(&hash, bytes).unwrap() {
                Node::Leaf { value, .. } => {
                    assert_eq!(value, vec![7; MAX_VALUE_BYTES]);
                    leaves += 1;
                }
                Node::Branch { left, right, .. } => {
                    stack.push(left);
                    stack.push(right);
                }
            }
        }
        assert_eq!(leaves, KEYS as usize);
        assert_eq!(reachable.len(), 2 * KEYS as usize - 1);
        assert_eq!(
            current_bytes,
            KEYS as usize * (35 + MAX_VALUE_BYTES) + (KEYS as usize - 1) * 99
        );
        assert!(current_bytes > 8 * 1024 * 1024);

        let old_root = root;
        let changed_key = KEYS / 2;
        let update = stage_state_update(
            &memory,
            root,
            &[StateChange::Put {
                key: changed_key.to_be_bytes().to_vec(),
                value: vec![9; MAX_VALUE_BYTES],
            }],
        )
        .unwrap();
        assert!(
            update.nodes().len() <= 64,
            "one key must not rewrite the whole state"
        );
        assert!(
            update.loaded_nodes() <= 64,
            "one key must not load the whole state"
        );
        let staged_bytes: usize = update.nodes().values().map(Vec::len).sum();
        assert!(staged_bytes < 32 * 1024);
        assert_ne!(update.root(), old_root);
        eprintln!(
            "state-tree-only scale evidence: keys={KEYS} current_bytes={current_bytes} loaded_nodes={} staged_nodes={} staged_bytes={staged_bytes}",
            update.loaded_nodes(), update.nodes().len(),
        );
        let new_root = persist(&mut memory, update);
        assert_eq!(
            read_state_value(&memory, old_root, &changed_key.to_be_bytes()).unwrap(),
            Some(vec![7; MAX_VALUE_BYTES])
        );
        assert_eq!(
            read_state_value(&memory, new_root, &changed_key.to_be_bytes()).unwrap(),
            Some(vec![9; MAX_VALUE_BYTES])
        );
        for unchanged in [0u32, KEYS - 1] {
            assert_eq!(
                read_state_value(&memory, new_root, &unchanged.to_be_bytes()).unwrap(),
                Some(vec![7; MAX_VALUE_BYTES])
            );
        }
        assert_eq!(
            read_state_value(&memory, new_root, &KEYS.to_be_bytes()).unwrap(),
            None
        );
    }
}
