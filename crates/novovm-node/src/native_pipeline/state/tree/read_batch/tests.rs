use super::*;
use std::cell::{Cell, RefCell};

#[derive(Default, Clone)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

struct Observed<'a> {
    memory: &'a Memory,
    calls: Cell<usize>,
    hashes: RefCell<BTreeSet<NodeHash>>,
}

impl<'a> Observed<'a> {
    fn new(memory: &'a Memory) -> Self {
        Self {
            memory,
            calls: Cell::new(0),
            hashes: RefCell::default(),
        }
    }
}

impl StateNodeReader for Observed<'_> {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.calls.set(self.calls.get() + 1);
        self.hashes.borrow_mut().insert(*hash);
        self.memory.read_node(hash)
    }
}

fn fixture(count: u32) -> (Memory, NodeHash) {
    let changes: Vec<_> = (0..count)
        .map(|key| StateChange::Put {
            key: key.to_be_bytes().to_vec(),
            value: key.to_be_bytes().to_vec(),
        })
        .collect();
    let update = stage_state_update(&Memory::default(), empty_root(), &changes).unwrap();
    (Memory(update.nodes), update.root)
}

fn ordered(
    reader: &dyn StateNodeReader,
    root: NodeHash,
    keys: &[Vec<u8>],
) -> Result<Vec<Option<Vec<u8>>>> {
    keys.iter()
        .map(|key| read_state_value(reader, root, key))
        .collect()
}

fn synthetic(leaves: &[(NodeHash, Vec<u8>)]) -> (Memory, NodeHash) {
    let memory = Memory::default();
    let mut planner = Planner::new(&memory);
    let mut root = empty_root();
    for (key, value) in leaves {
        root = planner.change(root, *key, Some(value), 0).unwrap();
    }
    planner.retain_reachable(root).unwrap();
    (Memory(planner.nodes), root)
}

fn read_synthetic(
    reader: &dyn StateNodeReader,
    root: NodeHash,
    keys: &[NodeHash],
) -> Result<Vec<Option<Vec<u8>>>> {
    let mut queries: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(position, key)| Query {
            key: *key,
            position,
        })
        .collect();
    queries.sort_unstable_by_key(|query| query.key);
    let mut results = vec![None; keys.len()];
    let mut planner = Planner::new(reader);
    planner.read_queries(root, &queries, 0, &mut results)?;
    assert_eq!(planner.staged_calls, 0);
    assert!(planner.nodes.is_empty());
    Ok(results)
}

fn first_byte(byte: u8) -> NodeHash {
    let mut key = [0; 32];
    key[0] = byte;
    key
}

#[test]
fn unordered_duplicates_present_and_absent_match_independent_point_reads() {
    let (memory, root) = fixture(512);
    let before = memory.0.clone();
    let mut keys: Vec<_> = (0..256u32)
        .rev()
        .map(|index| ((index * 37) % 769).to_be_bytes().to_vec())
        .collect();
    keys.extend([keys[0].clone(), keys[7].clone(), keys[0].clone()]);
    let expected = ordered(&memory, root, &keys).unwrap();
    assert_eq!(read_state_values(&memory, root, &keys).unwrap(), expected);
    keys.reverse();
    assert_eq!(
        read_state_values(&memory, root, &keys).unwrap(),
        ordered(&memory, root, &keys).unwrap()
    );
    assert_eq!(
        memory.0, before,
        "reads must not stage or mutate the provider"
    );
}

#[test]
fn absent_endpoints_do_not_discard_matching_middle_or_duplicate_outputs() {
    let leaves = [(first_byte(0x50), vec![5]), (first_byte(0x58), vec![8])];
    let (memory, root) = synthetic(&leaves);
    let keys = [0x60, 0x50, 0x54, 0x40, 0x58, 0x50, 0x5f].map(first_byte);
    assert_eq!(
        read_synthetic(&memory, root, &keys).unwrap(),
        vec![
            None,
            Some(vec![5]),
            None,
            None,
            Some(vec![8]),
            Some(vec![5]),
            None
        ],
    );
    let observed = Observed::new(&memory);
    assert_eq!(
        read_synthetic(&observed, root, &[first_byte(0x40), first_byte(0x60)]).unwrap(),
        vec![None, None]
    );
    assert_eq!(
        observed.calls.get(),
        1,
        "authenticated root alone proves both absences"
    );
}

#[test]
fn full_256_branch_depth_and_last_bit_absence_remain_bounded() {
    // Synthetic digests reach the real depth limit without changing SHA-256 or
    // attempting an infeasible search for 256-bit public-key prefix collisions.
    let mut leaves = vec![([0; 32], vec![0])];
    for bit in 0..256usize {
        let mut key = [0; 32];
        key[bit / 8] = 0x80 >> (bit % 8);
        leaves.push((key, (bit as u16).to_be_bytes().to_vec()));
    }
    let (memory, root) = synthetic(&leaves);
    let mut keys: Vec<_> = leaves.iter().rev().map(|(key, _)| *key).collect();
    keys.extend([[0; 32], first_byte(0xc0)]);
    let expected: Vec<_> = keys
        .iter()
        .map(|key| {
            leaves
                .iter()
                .find(|(stored, _)| stored == key)
                .map(|(_, value)| value.clone())
        })
        .collect();
    let observed = Observed::new(&memory);
    assert_eq!(read_synthetic(&observed, root, &keys).unwrap(), expected);
    assert_eq!(observed.calls.get(), 513);
}

#[test]
fn every_node_required_by_point_reads_remains_required_and_hash_checked() {
    let (memory, root) = fixture(128);
    let keys: Vec<_> = (0..16u32)
        .map(|index| (index * 11).to_be_bytes().to_vec())
        .collect();
    let observed = Observed::new(&memory);
    let expected = ordered(&observed, root, &keys).unwrap();
    assert_eq!(read_state_values(&memory, root, &keys).unwrap(), expected);
    for hash in observed.hashes.into_inner() {
        let mut missing = memory.clone();
        missing.0.remove(&hash);
        assert!(ordered(&missing, root, &keys).is_err());
        assert!(read_state_values(&missing, root, &keys).is_err());
        let mut corrupt = memory.clone();
        corrupt.0.get_mut(&hash).unwrap()[0] ^= 0x80;
        assert!(ordered(&corrupt, root, &keys).is_err());
        assert!(read_state_values(&corrupt, root, &keys).is_err());
    }
}

#[test]
fn cached_wrong_edge_is_rejected_before_compressed_prefix_absence() {
    let memory = Memory::default();
    let mut builder = Planner::new(&memory);
    let wrong_leaf = builder
        .stage(Node::Leaf {
            key: first_byte(0x90),
            value: vec![9],
        })
        .unwrap();
    let right_leaf = builder
        .stage(Node::Leaf {
            key: first_byte(0x58),
            value: vec![8],
        })
        .unwrap();
    let root = builder
        .stage(Node::Branch {
            bit: 4,
            prefix: first_byte(0x50),
            left: wrong_leaf,
            right: right_leaf,
        })
        .unwrap();
    let memory = Memory(builder.nodes);
    let mut planner = Planner::new(&memory);
    planner.load(&wrong_leaf).unwrap();
    // 0x54 would be absent from a valid 0x50 leaf, but this leaf was attached
    // to the wrong parent side; valid bytes/hash and a cache hit cannot hide it.
    let queries = [Query {
        key: first_byte(0x54),
        position: 0,
    }];
    let error = planner
        .read_queries(root, &queries, 0, &mut [None])
        .unwrap_err();
    assert!(error.to_string().contains("authenticated parent path"));
}

#[test]
fn wrong_branch_placement_is_checked_before_filtering_all_queries_as_absent() {
    let (mut memory, wrong) =
        synthetic(&[(first_byte(0x50), vec![5]), (first_byte(0x58), vec![8])]);
    let sibling = Node::Leaf {
        key: first_byte(0x10),
        value: vec![1],
    }
    .encode();
    let sibling_hash = digest_node(&sibling);
    memory.0.insert(sibling_hash, sibling);
    let bytes = Node::Branch {
        bit: 0,
        prefix: [0; 32],
        left: sibling_hash,
        right: wrong,
    }
    .encode();
    let root = digest_node(&bytes);
    memory.0.insert(root, bytes);
    let error = read_synthetic(&memory, root, &[first_byte(0x80), first_byte(0xa0)]).unwrap_err();
    assert!(error.to_string().contains("authenticated parent path"));
}

#[test]
fn untouched_child_is_not_loaded_and_empty_queries_do_not_validate_root() {
    let (mut memory, root) = synthetic(&[(first_byte(0), vec![0]), (first_byte(0x80), vec![8])]);
    let Node::Branch { right, .. } = Node::decode(&memory.0[&root]).unwrap() else {
        panic!("expected branch")
    };
    memory.0.remove(&right);
    let observed = Observed::new(&memory);
    assert_eq!(
        read_synthetic(&observed, root, &[first_byte(0)]).unwrap(),
        vec![Some(vec![0])]
    );
    assert_eq!(observed.calls.get(), 2);
    assert!(read_synthetic(&observed, root, &[first_byte(0x80)]).is_err());
    let empty = Memory::default();
    let observed = Observed::new(&empty);
    assert!(read_state_values(&observed, [9; 32], &[])
        .unwrap()
        .is_empty());
    assert_eq!(observed.calls.get(), 0);
}

#[test]
fn query_key_node_and_cumulative_reader_limits_are_preserved() {
    let (memory, root) = fixture(1);
    let key = 0u32.to_be_bytes().to_vec();
    let keys = vec![key; MAX_BATCH_READ_KEYS];
    assert_eq!(
        read_state_values(&memory, root, &keys).unwrap().len(),
        MAX_BATCH_READ_KEYS
    );
    let mut too_many = keys;
    too_many.push(vec![1]);
    assert!(read_state_values(&memory, root, &too_many).is_err());
    assert_eq!(
        read_state_values(&memory, empty_root(), &[vec![1; MAX_KEY_BYTES]]).unwrap(),
        vec![None]
    );
    for invalid in [Vec::new(), vec![1; MAX_KEY_BYTES + 1]] {
        let observed = Observed::new(&memory);
        assert!(read_state_values(&observed, root, &[vec![1], invalid]).is_err());
        assert_eq!(observed.calls.get(), 0);
    }
    let mut planner = Planner::new(&memory);
    planner.loaded_nodes = MAX_READER_READS;
    let queries = [Query {
        key: [0; 32],
        position: 0,
    }];
    assert!(planner
        .read_queries(root, &queries, 0, &mut [None])
        .unwrap_err()
        .to_string()
        .contains("reader budget"));
    let bytes = Node::Leaf {
        key: [0; 32],
        value: vec![1; MAX_VALUE_BYTES + 1],
    }
    .encode();
    let hash = digest_node(&bytes);
    let malformed = Memory(BTreeMap::from([(hash, bytes)]));
    assert!(read_synthetic(&malformed, hash, &[[0; 32]]).is_err());
}

#[test]
fn shared_512_key_traversal_reads_each_reachable_node_once() {
    let (memory, root) = fixture(512);
    let keys: Vec<_> = (0..512u32).map(|key| key.to_be_bytes().to_vec()).collect();
    let original = Observed::new(&memory);
    let expected = ordered(&original, root, &keys).unwrap();
    let batch = Observed::new(&memory);
    assert_eq!(read_state_values(&batch, root, &keys).unwrap(), expected);
    eprintln!(
        "state-tree-only 512 reads: batch={} original={}",
        batch.calls.get(),
        original.calls.get()
    );
    assert_eq!(batch.calls.get(), 1023);
    assert_eq!(batch.calls.get(), batch.hashes.borrow().len());
    assert!(batch.calls.get() < original.calls.get());
}

#[test]
fn thread_local_counters_record_successful_calls_and_original_query_count_only() {
    let (memory, root) = fixture(1);
    let before = read_batch_stats_for_test();
    let keys = vec![0u32.to_be_bytes().to_vec(); 3];
    read_state_values(&memory, root, &keys).unwrap();
    assert_eq!(read_batch_stats_for_test(), (before.0 + 1, before.1 + 3));
    assert!(read_state_values(&memory, [9; 32], &keys).is_err());
    assert!(read_state_values(&memory, root, &[Vec::new()]).is_err());
    assert_eq!(read_batch_stats_for_test(), (before.0 + 1, before.1 + 3));
    read_state_values(&memory, [9; 32], &[]).unwrap();
    assert_eq!(read_batch_stats_for_test(), (before.0 + 2, before.1 + 3));
}
