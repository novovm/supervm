//! Pure frontier/Patricia checks, not a zkVM proof or AOEM execution claim.
use super::*;
use crate::state::tree::empty_root;
use sha2::{Digest, Sha256};

// The existing frontier test module keeps its Memory private. Use the same
// small real-tree reader here; no mocked path validation or prebuilt authority.
#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn budget() -> CaptureBudget {
    CaptureBudget {
        keys: 32,
        nodes: 128,
        bytes: 32 * 1024,
    }
}
fn access(key: &[u8], put: bool, delete: bool) -> DeclaredAccess {
    DeclaredAccess {
        key: key.to_vec(),
        may_put: put,
        may_delete: delete,
    }
}
fn put(key: &[u8], value: &[u8]) -> StateChange {
    StateChange::Put {
        key: key.to_vec(),
        value: value.to_vec(),
    }
}
fn parent() -> (Memory, NodeHash) {
    let update = stage_state_update(
        &Memory::default(),
        empty_root(),
        &[
            put(b"a", b"first"),
            put(b"b", b"second"),
            put(b"c", b"untouched"),
        ],
    )
    .unwrap();
    (Memory(update.nodes().clone()), update.root())
}

// Deliberately unchecked test builder permits malformed ordering/duplicates;
// production decoding must reject them rather than silently collecting a map.
fn wire_parts(
    root: NodeHash,
    declarations: &[DeclaredAccess],
    nodes: &[(NodeHash, Vec<u8>)],
) -> Vec<u8> {
    let mut wire = MAGIC.to_vec();
    wire.extend_from_slice(&root);
    wire.extend_from_slice(&u32::try_from(declarations.len()).unwrap().to_be_bytes());
    for declaration in declarations {
        wire.extend_from_slice(&u16::try_from(declaration.key.len()).unwrap().to_be_bytes());
        wire.extend_from_slice(&declaration.key);
        wire.push(u8::from(declaration.may_put) | (u8::from(declaration.may_delete) << 1));
    }
    wire.extend_from_slice(&u32::try_from(nodes.len()).unwrap().to_be_bytes());
    for (hash, bytes) in nodes {
        wire.extend_from_slice(hash);
        wire.extend_from_slice(&u16::try_from(bytes.len()).unwrap().to_be_bytes());
        wire.extend_from_slice(bytes);
    }
    wire
}
fn entries(input: &OwnedStateInput) -> Vec<(NodeHash, Vec<u8>)> {
    input
        .nodes
        .iter()
        .map(|(hash, bytes)| (*hash, bytes.clone()))
        .collect()
}

#[test]
fn canonical_roundtrip_retains_exact_access_and_real_update_after_source_drop() {
    let (memory, root) = parent();
    let declared = [
        access(b"a", true, true),
        access(b"b", false, false),
        access(b"new", true, true),
    ];
    let original = OwnedStateInput::capture(&memory, root, &declared, budget()).unwrap();
    let wire = original.encode_witness(budget()).unwrap();
    let reversed = declared.iter().rev().cloned().collect::<Vec<_>>();
    let reordered = OwnedStateInput::capture(&memory, root, &reversed, budget()).unwrap();
    assert_eq!(reordered.encode_witness(budget()).unwrap(), wire);
    let imported = OwnedStateInput::from_witness(root, &reversed, &wire, budget()).unwrap();
    assert_eq!(imported.encode_witness(budget()).unwrap(), wire);
    assert_eq!(imported.nodes, original.nodes);
    assert_eq!(imported.access, original.access);
    let changes = [
        StateChange::Delete { key: b"a".to_vec() },
        put(b"new", b"created"),
        put(b"a", b"replacement"),
    ];
    let expected = stage_state_update(&memory, root, &changes).unwrap();
    drop(memory);
    assert_eq!(imported.read(b"a").unwrap(), Some(b"first".to_vec()));
    assert_eq!(imported.read(b"new").unwrap(), None);
    assert!(imported.read(b"c").is_err());
    assert!(imported.stage(&[put(b"b", b"denied")]).is_err());
    assert!(imported.stage(&[put(b"c", b"denied")]).is_err());
    let actual = imported.stage(&changes).unwrap();
    assert_eq!(actual.parent_root(), root);
    assert_eq!(actual.root(), expected.root());
    assert_eq!(actual.nodes(), expected.nodes());
}

#[test]
fn every_truncation_and_changed_byte_and_trailing_payload_fail_closed() {
    let (memory, root) = parent();
    let declared = [access(b"a", true, true)];
    let input = OwnedStateInput::capture(&memory, root, &declared, budget()).unwrap();
    let wire = input.encode_witness(budget()).unwrap();
    for length in 0..wire.len() {
        assert!(OwnedStateInput::from_witness(root, &declared, &wire[..length], budget()).is_err());
    }
    for index in 0..wire.len() {
        let mut changed = wire.clone();
        changed[index] ^= 1;
        assert!(
            OwnedStateInput::from_witness(root, &declared, &changed, budget()).is_err(),
            "changed offset {index} accepted"
        );
    }
    let mut trailing = wire.clone();
    trailing.push(0);
    assert!(OwnedStateInput::from_witness(root, &declared, &trailing, budget()).is_err());
    assert!(OwnedStateInput::from_witness(empty_root(), &declared, &wire, budget()).is_err());
}

#[test]
fn independent_access_rejects_missing_extra_duplicate_keys_and_changed_permissions() {
    let (memory, root) = parent();
    let declared = [access(b"a", true, false), access(b"b", false, true)];
    let input = OwnedStateInput::capture(&memory, root, &declared, budget()).unwrap();
    let wire = input.encode_witness(budget()).unwrap();
    for wrong in [
        vec![],
        vec![declared[0].clone()],
        vec![
            declared[0].clone(),
            declared[1].clone(),
            access(b"c", false, false),
        ],
        vec![declared[0].clone(), declared[0].clone()],
        vec![access(b"a", false, false), declared[1].clone()],
        vec![access(b"a", true, true), declared[1].clone()],
        vec![access(b"", true, false), declared[1].clone()],
        vec![access(&[1; 257], true, false), declared[1].clone()],
    ] {
        assert!(OwnedStateInput::from_witness(root, &wrong, &wire, budget()).is_err());
    }
    for malformed in [
        vec![declared[1].clone(), declared[0].clone()],
        vec![declared[0].clone(), declared[0].clone()],
    ] {
        let changed = wire_parts(root, &malformed, &entries(&input));
        assert!(OwnedStateInput::from_witness(root, &declared, &changed, budget()).is_err());
    }
}

#[test]
fn ordered_unique_nodes_reject_detached_extras_and_candidate_output_substitution() {
    let (memory, root) = parent();
    let declared = [access(b"a", true, false)];
    let input = OwnedStateInput::capture(&memory, root, &declared, budget()).unwrap();
    let original = entries(&input);
    assert!(original.len() >= 2);
    let mut reversed = original.clone();
    reversed.reverse();
    let mut duplicate = original.clone();
    duplicate.insert(0, original[0].clone());
    for nodes in [reversed, duplicate] {
        assert!(OwnedStateInput::from_witness(
            root,
            &declared,
            &wire_parts(root, &declared, &nodes),
            budget()
        )
        .is_err());
    }
    let extra = memory
        .0
        .iter()
        .find(|(hash, _)| !input.nodes.contains_key(*hash))
        .unwrap();
    let mut extra_nodes = input.nodes.clone();
    extra_nodes.insert(*extra.0, extra.1.clone());
    let extra_wire = wire_parts(
        root,
        &declared,
        &extra_nodes.into_iter().collect::<Vec<_>>(),
    );
    let error = OwnedStateInput::from_witness(root, &declared, &extra_wire, budget())
        .err()
        .unwrap();
    assert!(error.to_string().contains("extraneous"));
    let update = input.stage(&[put(b"a", b"candidate-output")]).unwrap();
    let output_nodes = update
        .nodes()
        .iter()
        .map(|(hash, bytes)| (*hash, bytes.clone()))
        .collect::<Vec<_>>();
    assert!(OwnedStateInput::from_witness(
        root,
        &declared,
        &wire_parts(root, &declared, &output_nodes),
        budget()
    )
    .is_err());
}

#[test]
fn missing_absence_path_or_delete_sibling_is_not_absence() {
    let (memory, root) = parent();
    let declared = [access(b"a", true, true), access(b"absent", true, false)];
    let input = OwnedStateInput::capture(&memory, root, &declared, budget()).unwrap();
    assert_eq!(input.read(b"absent").unwrap(), None);
    for hash in input.nodes.keys() {
        let mut incomplete = input.nodes.clone();
        incomplete.remove(hash);
        assert!(OwnedStateInput::from_witness(
            root,
            &declared,
            &wire_parts(root, &declared, &incomplete.into_iter().collect::<Vec<_>>()),
            budget()
        )
        .is_err());
    }
    let read_only =
        OwnedStateInput::capture(&memory, root, &[access(b"a", true, false)], budget()).unwrap();
    assert!(read_only.nodes.len() < input.nodes.len());
    assert!(OwnedStateInput::from_witness(
        root,
        &declared,
        &wire_parts(root, &declared, &entries(&read_only)),
        budget()
    )
    .is_err());
}

#[test]
fn rehashed_wrong_child_direction_is_rejected_not_just_individual_hash_checked() {
    let update = stage_state_update(
        &Memory::default(),
        empty_root(),
        &[put(b"a", b"1"), put(b"b", b"2")],
    )
    .unwrap();
    let mut nodes = update.nodes().clone();
    let mut branch = nodes.remove(&update.root()).unwrap();
    assert_eq!(branch.len(), 99);
    let left = branch[35..67].to_vec();
    let right = branch[67..99].to_vec();
    branch[35..67].copy_from_slice(&right);
    branch[67..99].copy_from_slice(&left);
    let mut hash = Sha256::new();
    hash.update(b"novovm-state-patricia-v1:node\0");
    hash.update(&branch);
    let bad_root: NodeHash = hash.finalize().into();
    validate_state_node_bytes(&bad_root, &branch).unwrap();
    nodes.insert(bad_root, branch);
    let declared = [access(b"a", true, true)];
    let wire = wire_parts(bad_root, &declared, &nodes.into_iter().collect::<Vec<_>>());
    let error = OwnedStateInput::from_witness(bad_root, &declared, &wire, budget())
        .err()
        .unwrap();
    assert!(error.to_string().contains("authenticated parent path"));
}

#[test]
fn exact_node_byte_budget_excludes_bounded_framing_and_limits_are_inclusive() {
    let (memory, root) = parent();
    let declared = [access(b"a", true, true), access(b"b", true, true)];
    let input = OwnedStateInput::capture(&memory, root, &declared, budget()).unwrap();
    let exact = CaptureBudget {
        keys: declared.len(),
        nodes: input.captured_nodes(),
        bytes: input.captured_bytes(),
    };
    let wire = input.encode_witness(exact).unwrap();
    assert!(wire.len() > exact.bytes);
    OwnedStateInput::from_witness(root, &declared, &wire, exact).unwrap();
    for smaller in [
        CaptureBudget {
            keys: exact.keys - 1,
            ..exact
        },
        CaptureBudget {
            nodes: exact.nodes - 1,
            ..exact
        },
        CaptureBudget {
            bytes: exact.bytes - 1,
            ..exact
        },
    ] {
        assert!(input.encode_witness(smaller).is_err());
        assert!(OwnedStateInput::from_witness(root, &declared, &wire, smaller).is_err());
    }
    let overflow = CaptureBudget {
        keys: usize::MAX,
        nodes: usize::MAX,
        bytes: usize::MAX,
    };
    assert!(input.encode_witness(overflow).is_err());
    assert!(OwnedStateInput::from_witness(root, &declared, &wire, overflow).is_err());
    let mut huge_count = wire.clone();
    huge_count[40..44].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(OwnedStateInput::from_witness(root, &declared, &huge_count, budget()).is_err());
    let mut huge_length = wire;
    huge_length[44..46].copy_from_slice(&u16::MAX.to_be_bytes());
    assert!(OwnedStateInput::from_witness(root, &declared, &huge_length, budget()).is_err());
}

#[test]
fn trusted_empty_parent_has_empty_frontier_but_still_exact_permissions() {
    let declared = [access(b"new", true, false)];
    let exact = CaptureBudget {
        keys: 1,
        nodes: 0,
        bytes: 0,
    };
    let input =
        OwnedStateInput::capture(&Memory::default(), empty_root(), &declared, exact).unwrap();
    let wire = input.encode_witness(exact).unwrap();
    let restored = OwnedStateInput::from_witness(empty_root(), &declared, &wire, exact).unwrap();
    assert_eq!(restored.read(b"new").unwrap(), None);
    assert!(restored.read(b"unknown").is_err());
    assert!(restored
        .stage(&[StateChange::Delete {
            key: b"new".to_vec()
        }])
        .is_err());
    assert_ne!(
        restored.stage(&[put(b"new", b"value")]).unwrap().root(),
        empty_root()
    );
    let (memory, _) = parent();
    let extra = memory.0.into_iter().take(1).collect::<Vec<_>>();
    assert!(OwnedStateInput::from_witness(
        empty_root(),
        &declared,
        &wire_parts(empty_root(), &declared, &extra),
        budget()
    )
    .is_err());
}

#[test]
fn leaf_minimum_maximum_and_declared_length_limits_match_patricia_v1() {
    let key = vec![0x42; MAX_KEY_BYTES];
    let declared = [access(&key, true, true)];
    for length in [0, 256] {
        let value = vec![0x91; length];
        let update =
            stage_state_update(&Memory::default(), empty_root(), &[put(&key, &value)]).unwrap();
        let root = update.root();
        let memory = Memory(update.nodes().clone());
        assert_eq!(memory.0[&root].len(), 35 + length);
        let exact = CaptureBudget {
            keys: 1,
            nodes: 1,
            bytes: 35 + length,
        };
        let captured = OwnedStateInput::capture(&memory, root, &declared, exact).unwrap();
        let wire = captured.encode_witness(exact).unwrap();
        let imported = OwnedStateInput::from_witness(root, &declared, &wire, exact).unwrap();
        assert_eq!(imported.read(&key).unwrap(), Some(value));
        let node_count_offset = 44 + ACCESS_OVERHEAD + key.len();
        let node_length_offset = node_count_offset + 4 + 32;
        let mut excessive_count = wire.clone();
        excessive_count[node_count_offset..node_count_offset + 4]
            .copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(
            OwnedStateInput::from_witness(root, &declared, &excessive_count, budget()).is_err()
        );
        for invalid_length in [34u16, 292, u16::MAX] {
            let mut malformed = wire.clone();
            malformed[node_length_offset..node_length_offset + 2]
                .copy_from_slice(&invalid_length.to_be_bytes());
            assert!(OwnedStateInput::from_witness(root, &declared, &malformed, budget()).is_err());
        }
        let mut reserved_flags = wire;
        reserved_flags[node_count_offset - 1] |= 4;
        assert!(OwnedStateInput::from_witness(root, &declared, &reserved_flags, budget()).is_err());
    }
}
