//! Pure incremental-capture tests; no live database, AOEM or chain claims.

use super::*;
use crate::native_pipeline::execution::plan::{BatchContext, BatchPlan, PlanBudget};
use crate::native_pipeline::state::tree::{self, StateChange, StateNodeReader};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::collections::BTreeSet;

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn budget() -> CaptureBudget {
    CaptureBudget {
        keys: 512,
        nodes: 4096,
        bytes: 1024 * 1024,
    }
}
fn key(n: u32) -> Vec<u8> {
    n.to_be_bytes().to_vec()
}
fn put(n: u32, v: u8) -> StateChange {
    StateChange::Put {
        key: key(n),
        value: vec![v],
    }
}
fn delete(n: u32) -> StateChange {
    StateChange::Delete { key: key(n) }
}
fn declarations(count: u32, delete: bool) -> Vec<DeclaredAccess> {
    (0..count)
        .map(|n| DeclaredAccess {
            key: key(n),
            may_put: true,
            may_delete: delete,
        })
        .collect()
}
fn parent(count: u32) -> (Memory, NodeHash) {
    let staged = tree::stage_state_update(
        &Memory::default(),
        tree::empty_root(),
        &(0..count).map(|n| put(n, 7)).collect::<Vec<_>>(),
    )
    .unwrap();
    (Memory(staged.nodes().clone()), staged.root())
}

fn drive(capture: &mut BulkCapture, memory: &Memory, steps: usize) -> Result<Vec<Vec<NodeHash>>> {
    let mut batches = Vec::new();
    for _ in 0..100_000 {
        match capture.advance(steps)? {
            CaptureStep::More => {}
            CaptureStep::NeedRead => {
                let request = capture
                    .next_request()?
                    .expect("blocked capture has a request");
                assert!(!request.is_empty() && request.len() <= 64);
                assert_eq!(capture.next_request()?.as_ref(), Some(&request));
                assert_eq!(capture.advance(steps)?, CaptureStep::NeedRead);
                let values = request
                    .iter()
                    .map(|hash| memory.0.get(hash).cloned())
                    .collect();
                batches.push(request);
                capture.accept(values)?;
            }
            CaptureStep::Complete => return Ok(batches),
        }
    }
    anyhow::bail!("bounded test driver did not terminate")
}

#[test]
fn incremental_reads_are_deduplicated_bounded_and_never_replay_prior_edges() {
    let (memory, root) = parent(160);
    let declarations = declarations(180, true);
    struct EdgeCounter<'a> {
        source: &'a Memory,
        calls: Cell<usize>,
    }
    impl StateNodeReader for EdgeCounter<'_> {
        fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
            self.calls.set(self.calls.get() + 1);
            self.source.read_node(hash)
        }
    }
    let counter = EdgeCounter {
        source: &memory,
        calls: Cell::new(0),
    };
    for declaration in &declarations {
        tree::capture_update_path_v1(&counter, root, &declaration.key, declaration.may_delete)
            .unwrap();
    }
    let expected = OwnedStateInput::capture(&memory, root, &declarations, budget()).unwrap();
    for steps in [1, 7, 256] {
        let mut capture = BulkCapture::new(root, &declarations, budget()).unwrap();
        let batches = drive(&mut capture, &memory, steps).unwrap();
        assert!(batches.iter().any(|batch| batch.len() == 64));
        let all: Vec<_> = batches.into_iter().flatten().collect();
        assert_eq!(
            all.len(),
            all.iter().copied().collect::<BTreeSet<_>>().len()
        );
        assert_eq!(capture.edge_steps, counter.calls.get());
        let input = capture.finish().unwrap();
        assert_eq!(input.nodes, expected.nodes);
        assert_eq!(input.bytes, expected.bytes);
        assert_eq!(input.access, expected.access);
        for declaration in &declarations {
            assert_eq!(
                input.read(&declaration.key).unwrap(),
                expected.read(&declaration.key).unwrap()
            );
        }
    }
}

#[test]
fn source_drop_and_threaded_delete_insert_repeated_changes_match_full_reader() {
    fn owned<T: Send + Sync + 'static>() {}
    owned::<BulkCapture>();
    owned::<OwnedStateInput>();
    let (memory, root) = parent(96);
    let declared = declarations(112, true);
    let changes = [
        delete(0),
        delete(1),
        put(98, 9),
        put(2, 4),
        delete(98),
        put(98, 11),
        delete(2),
        put(2, 12),
    ];
    let expected = tree::stage_state_update(&memory, root, &changes).unwrap();
    let mut capture = BulkCapture::new(root, &declared, budget()).unwrap();
    drive(&mut capture, &memory, 3).unwrap();
    let input = capture.finish().unwrap();
    assert_eq!(input.read(&key(100)).unwrap(), None);
    assert!(input.read(b"unknown").is_err());
    drop(memory);
    let actual = std::thread::spawn(move || input.stage(&changes))
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(actual.parent_root(), expected.parent_root());
    assert_eq!(actual.root(), expected.root());
    assert_eq!(actual.nodes(), expected.nodes());
    assert_eq!(actual.loaded_nodes(), expected.loaded_nodes());
}

#[test]
fn precise_limits_are_inclusive_and_smaller_limits_fail_closed() {
    let (memory, root) = parent(40);
    let declared = declarations(24, true);
    let expected = OwnedStateInput::capture(&memory, root, &declared, budget()).unwrap();
    let exact = CaptureBudget {
        keys: declared.len(),
        nodes: expected.captured_nodes(),
        bytes: expected.captured_bytes(),
    };
    let mut capture = BulkCapture::new(root, &declared, exact).unwrap();
    drive(&mut capture, &memory, 2).unwrap();
    capture.finish().unwrap();
    assert!(BulkCapture::new(
        root,
        &declared,
        CaptureBudget {
            keys: exact.keys - 1,
            ..exact
        }
    )
    .is_err());
    for limits in [
        CaptureBudget {
            nodes: exact.nodes - 1,
            ..exact
        },
        CaptureBudget {
            bytes: exact.bytes - 1,
            ..exact
        },
    ] {
        let mut capture = BulkCapture::new(root, &declared, limits).unwrap();
        assert!(drive(&mut capture, &memory, 2).is_err());
        assert!(capture.advance(1).is_err());
        assert!(capture.next_request().is_err());
        assert!(capture.finish().is_err());
    }
}

#[test]
fn bad_reply_is_atomic_terminal_and_is_never_an_absent_application_key() {
    let (memory, root) = parent(32);
    for mode in 0..5 {
        let mut capture = BulkCapture::new(root, &declarations(32, true), budget()).unwrap();
        loop {
            match capture.advance(128).unwrap() {
                CaptureStep::More => {}
                CaptureStep::Complete => panic!("expected a multi-key frontier"),
                CaptureStep::NeedRead => {
                    let request = capture.next_request().unwrap().unwrap();
                    let mut reply: Vec<_> = request
                        .iter()
                        .map(|hash| memory.0.get(hash).cloned())
                        .collect();
                    if request.len() < 2 {
                        capture.accept(reply).unwrap();
                        continue;
                    }
                    let original_nodes = capture.state.nodes.clone();
                    let original_bytes = capture.state.bytes;
                    match mode {
                        0 => {
                            reply.pop();
                        }
                        1 => reply.push(None),
                        2 => reply[1] = None,
                        3 => reply[1].as_mut().unwrap()[0] ^= 1,
                        4 => reply.swap(0, 1),
                        _ => unreachable!(),
                    }
                    assert!(capture.accept(reply).is_err());
                    assert_eq!(capture.state.nodes, original_nodes);
                    assert_eq!(capture.state.bytes, original_bytes);
                    assert!(capture
                        .accept(
                            request
                                .iter()
                                .map(|hash| memory.0.get(hash).cloned())
                                .collect()
                        )
                        .is_err());
                    assert!(capture.finish().is_err());
                    break;
                }
            }
        }
    }
}

#[test]
fn empty_parent_finishes_without_io_but_keeps_exact_access_permissions() {
    let declared = [
        DeclaredAccess {
            key: key(0),
            may_put: true,
            may_delete: false,
        },
        DeclaredAccess {
            key: key(1),
            may_put: false,
            may_delete: true,
        },
    ];
    let mut capture = BulkCapture::new(
        tree::empty_root(),
        &declared,
        CaptureBudget {
            keys: 2,
            nodes: 0,
            bytes: 0,
        },
    )
    .unwrap();
    assert!(capture.next_request().unwrap().is_none());
    assert_eq!(capture.advance(1).unwrap(), CaptureStep::Complete);
    let input = capture.finish().unwrap();
    assert_eq!(input.read(&key(0)).unwrap(), None);
    assert!(input.stage(&[put(0, 3), delete(1)]).is_ok());
    assert!(input.stage(&[delete(0)]).is_err());
    assert!(input.stage(&[put(1, 4)]).is_err());
    assert!(input.stage(&[put(2, 4)]).is_err());
}

#[test]
fn incomplete_unsolicited_duplicate_and_invalid_declarations_are_rejected() {
    let (_, root) = parent(1);
    assert!(BulkCapture::new(root, &[], budget()).is_err());
    assert!(BulkCapture::new(
        root,
        &[DeclaredAccess {
            key: vec![],
            may_put: true,
            may_delete: true
        }],
        budget()
    )
    .is_err());
    let mut duplicates = declarations(1, false);
    duplicates.extend(declarations(1, true));
    assert!(BulkCapture::new(root, &duplicates, budget()).is_err());
    assert!(BulkCapture::new(root, &declarations(1, false), budget())
        .unwrap()
        .finish()
        .is_err());
    let mut capture = BulkCapture::new(root, &declarations(1, false), budget()).unwrap();
    assert!(capture.accept(vec![]).is_err());
    assert!(capture.finish().is_err());
    let mut capture = BulkCapture::new(root, &declarations(1, false), budget()).unwrap();
    assert!(capture.advance(0).is_err());
    assert!(capture.finish().is_err());
}

fn hash_node(bytes: &[u8]) -> NodeHash {
    let mut digest = Sha256::new();
    digest.update(b"novovm-state-patricia-v1:node\0");
    digest.update(bytes);
    digest.finalize().into()
}

#[test]
fn shared_cached_node_is_revalidated_on_every_incoming_edge() {
    let (mut memory, root) = parent(64);
    let original = memory.0[&root].clone();
    assert_eq!(original[0], 2);
    let left: NodeHash = original[35..67].try_into().unwrap();
    let right: NodeHash = original[67..99].try_into().unwrap();
    let mut wrong_left = memory.0[&left].clone();
    assert_eq!(wrong_left[0], 2);
    // This node is valid at the root's RIGHT edge, but invalid below LEFT.
    // It is already loaded for its valid occurrence before the deeper edge.
    wrong_left[35..67].copy_from_slice(&right);
    let wrong_left_hash = hash_node(&wrong_left);
    memory.0.insert(wrong_left_hash, wrong_left);
    let mut wrong_root = original;
    wrong_root[35..67].copy_from_slice(&wrong_left_hash);
    let wrong_root_hash = hash_node(&wrong_root);
    memory.0.insert(wrong_root_hash, wrong_root);
    let mut capture =
        BulkCapture::new(wrong_root_hash, &declarations(64, false), budget()).unwrap();
    assert!(drive(&mut capture, &memory, 512).is_err());
    assert!(capture.state.nodes.contains_key(&right));
    assert!(capture.finish().is_err());
}

#[test]
fn deletion_sibling_placement_is_checked_without_reading_its_descendants() {
    let (mut memory, root) = parent(64);
    let original = memory.0[&root].clone();
    let left = original[35..67].to_vec();
    let right = original[67..99].to_vec();
    let mut swapped = original;
    swapped[35..67].copy_from_slice(&right);
    swapped[67..99].copy_from_slice(&left);
    let wrong = hash_node(&swapped);
    memory.0.insert(wrong, swapped);
    let mut capture = BulkCapture::new(wrong, &declarations(1, true), budget()).unwrap();
    assert!(drive(&mut capture, &memory, 1).is_err());
    assert!(capture.finish().is_err());
    let mut good = BulkCapture::new(root, &declarations(1, true), budget()).unwrap();
    drive(&mut good, &memory, 1).unwrap();
    let expected =
        OwnedStateInput::capture(&memory, root, &declarations(1, true), budget()).unwrap();
    let actual = good.finish().unwrap();
    assert_eq!(actual.nodes, expected.nodes);
    assert!(actual.nodes.len() < memory.0.len() / 2);
    assert_eq!(
        actual.stage(&[delete(0)]).unwrap().root(),
        tree::stage_state_update(&memory, root, &[delete(0)])
            .unwrap()
            .root()
    );
}

#[test]
fn bounded_plan_capture_retains_original_body_root_and_permissions() {
    let (memory, root) = parent(4);
    let context = BatchContext {
        chain_id: 7,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: [3; 32],
        semantic_version: 1,
        effect_contract: [4; 32],
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: root,
        parent_receipt_root: [5; 32],
        parent_state_version: 0,
        receipt_codec: [6; 32],
        height: 1,
        slot: 0,
        timestamp_unix_ms: 9,
    };
    let raw = vec![vec![1, 2, 3]];
    let plan = BatchPlan::new(
        context,
        raw.clone(),
        declarations(1, false),
        PlanBudget {
            transactions: 1,
            transaction_bytes: 8,
            body_bytes: 8,
            access_keys: 1,
        },
    )
    .unwrap();
    let commitment = plan.commitment();
    let mut capture = plan.begin_capture(budget()).unwrap();
    loop {
        match capture.advance(1).unwrap() {
            CaptureStep::More => {}
            CaptureStep::NeedRead => {
                let request = capture.next_request().unwrap().unwrap();
                capture
                    .accept(
                        request
                            .iter()
                            .map(|hash| memory.0.get(hash).cloned())
                            .collect(),
                    )
                    .unwrap();
            }
            CaptureStep::Complete => break,
        }
    }
    let input = capture.finish().unwrap();
    assert_eq!(input.plan().context(), &context);
    assert_eq!(input.plan().raw_transactions(), raw);
    assert_eq!(input.plan().commitment(), commitment);
    assert!(input.read(&key(1)).is_err());
    let output = input.stage(&[put(0, 3)]).unwrap();
    assert_eq!(output.plan_commitment(), commitment);
    assert_eq!(output.update().parent_root(), root);
}
