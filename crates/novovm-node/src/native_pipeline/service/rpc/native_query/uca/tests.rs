use super::*;
use crate::native_pipeline::business::nov_transfer_batch::{balance_key, nonce_key};
use crate::native_pipeline::business::uca_spend_state::{
    account_record_changes, grant_record_changes, AssetAccountAuthorityV2,
};
use crate::native_pipeline::state::tree::StateChange;
use novovm_adapter_api::uca_spend_policy::SuppliedAssetSpendGrantV2;
use novovm_protocol::native_query::AssetReadIntentV2;

#[test]
fn uca_local_turn_caps_work_and_preserves_progress_without_a_real_clock() {
    let mut calls = 0;
    let (result, progress, steps) = poll_local_turn(
        || {
            calls += 1;
            (Ok(None::<()>), true, false)
        },
        || Duration::ZERO,
    );
    assert!(result.unwrap().is_none());
    assert!(progress);
    assert_eq!(calls, LOCAL_STEPS_PER_TURN);
    assert_eq!(steps, LOCAL_STEPS_PER_TURN);

    let mut calls = 0;
    let mut clock_calls = 0;
    let (_, progress, steps) = poll_local_turn(
        || {
            calls += 1;
            (Ok(None::<()>), true, false)
        },
        || {
            clock_calls += 1;
            if clock_calls == 1 {
                Duration::ZERO
            } else {
                LOCAL_TURN_BUDGET
            }
        },
    );
    assert_eq!(calls, 2);
    assert!(progress);
    assert_eq!(steps, 2);
}

#[test]
fn uca_local_turn_yields_at_pending_admitted_and_completed_io_boundaries() {
    for boundary_progress in [false, true] {
        let mut calls = 0;
        let (_, progress, steps) = poll_local_turn(
            || {
                calls += 1;
                (Ok(None::<()>), boundary_progress, true)
            },
            || panic!("I/O boundaries do not spend another local clock/step"),
        );
        assert_eq!(calls, 1);
        assert_eq!(progress, boundary_progress);
        assert_eq!(steps, 0);
    }
    let mut calls = 0;
    let (_, progress, steps) = poll_local_turn(
        || {
            calls += 1;
            (Ok(None::<()>), calls == 1, false)
        },
        || Duration::ZERO,
    );
    assert_eq!(calls, 2);
    assert!(
        progress,
        "a later not-ready step cannot erase earlier progress"
    );
    assert_eq!(steps, 1);
    let (_, progress, steps) = poll_local_turn(
        || (Ok(None::<()>), false, false),
        || panic!("no-progress step must yield immediately"),
    );
    assert!(!progress);
    assert_eq!(steps, 0);
}

#[test]
fn uca_local_turn_preserves_terminal_result_and_error_after_local_progress() {
    let mut calls = 0;
    let (result, progress, steps) = poll_local_turn(
        || {
            calls += 1;
            (Ok((calls == 3).then_some(19)), true, false)
        },
        || Duration::ZERO,
    );
    assert_eq!(result.unwrap(), Some(19));
    assert!(progress);
    assert_eq!((calls, steps), (3, 3));
    let mut calls = 0;
    let (result, progress, steps) = poll_local_turn(
        || {
            calls += 1;
            if calls == 2 {
                (
                    Err::<Option<()>, _>(anyhow::anyhow!("original capture error")),
                    false,
                    false,
                )
            } else {
                (Ok(None), true, false)
            }
        },
        || Duration::ZERO,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("original capture error"));
    assert!(progress);
    assert_eq!((calls, steps), (2, 1));
}

fn intent() -> AssetSpendIntentV2 {
    AssetSpendIntentV2 {
        asset: 1,
        authority_update: None,
        chain_id: 7,
        genesis: [2; 32],
        payer: vec![1; 20],
        authorization_epoch: 3,
        spend_key_ref: [4; 32],
        recipient: vec![2; 20],
        amount: 10,
        max_fee: 2,
        nonce: 9,
        not_before_height: 1,
        expires_at_height: 10,
    }
}
fn account(byte: u8) -> Account {
    Account::try_from(vec![byte; 20]).unwrap()
}
fn fixture() -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
    let intent = intent();
    let mut changes = Vec::new();
    for address in [account(1), account(2)] {
        changes.extend(
            account_record_changes(
                7,
                &intent.genesis,
                &AssetAccountAuthorityV2 {
                    account: address.as_bytes().to_vec(),
                    asset: 1,
                    active: true,
                    management_key_ref: [6; 32],
                    recovery_key_ref: None,
                    current_spend_epoch: 3,
                },
            )
            .unwrap(),
        );
        changes.push(StateChange::Put {
            key: balance_key(&address),
            value: u128::MAX.to_le_bytes().to_vec(),
        });
    }
    changes.extend(
        grant_record_changes(
            &intent,
            &SuppliedAssetSpendGrantV2 {
                asset: 1,
                account: intent.payer.clone(),
                key_ref: intent.spend_key_ref,
                authorization_epoch: 3,
                permits_nov_transfer: true,
                revoked: true,
                not_before_height: 1,
                expires_at_height: 100,
                max_debit_per_intent: 200,
                remaining_debit_budget: 123,
            },
        )
        .unwrap(),
    );
    changes.push(StateChange::Put {
        key: nonce_key(&intent.nonce_identity().unwrap()),
        value: 9u64.to_le_bytes().to_vec(),
    });
    changes
        .into_iter()
        .map(|change| match change {
            StateChange::Put { key, value } => (key, Some(value)),
            StateChange::Delete { key } => (key, None),
        })
        .collect()
}

#[test]
fn observation_uses_same_record_codec_and_never_turns_revoked_grant_into_permission() {
    let records = fixture();
    let access = AssetSpendStateAccessV2::new(intent(), account(1), account(2)).unwrap();
    let read = |key: &[u8]| Ok(records.get(key).cloned().flatten());
    let state = access.read_optional_with([9; 32], read).unwrap().unwrap();
    let snapshot = view(&state, 5);
    assert!(state.matches_intent(&intent()));
    let mut other_intent = intent();
    other_intent.nonce += 1;
    assert!(!state.matches_intent(&other_intent));
    assert!(snapshot.revoked);
    assert_eq!(snapshot.next_nonce, 9);
    assert_eq!(snapshot.remaining_debit_budget, 123);
    assert_eq!(snapshot.payer_balance, u128::MAX);
    assert_eq!(snapshot.recipient_balance, u128::MAX);
    assert_eq!(snapshot.state_root, [9; 32]);
    assert_eq!(
        state.static_failure(5),
        Some(novovm_adapter_api::uca_spend_policy::SpendPolicyError::Revoked)
    );
    assert!(AssetSpendStateAccessV2::new(intent(), account(3), account(2)).is_err());
    assert!(access.read_optional_with([0; 32], read).is_err());
}

#[test]
fn absent_records_are_not_malformed_or_unavailable_reads() {
    let access = AssetSpendStateAccessV2::new(intent(), account(1), account(2)).unwrap();
    assert!(access
        .read_optional_with([9; 32], |_| Ok(None))
        .unwrap()
        .is_none());
    assert!(access
        .read_optional_with([9; 32], |_| anyhow::bail!("IO unavailable"))
        .is_err());
    let mut records = fixture();
    let header = records
        .keys()
        .find(|key| key.ends_with(b"/len"))
        .unwrap()
        .clone();
    records.insert(header, None);
    assert!(access
        .read_optional_with([9; 32], |key| Ok(records.get(key).cloned().flatten()))
        .is_err());
}

#[test]
fn read_queue_bounds_deduplicates_and_reports_pending_not_absence() {
    let mut queries = UcaQueries::default();
    let now = Instant::now();
    for n in 0..8 {
        let mut intent = intent();
        intent.nonce = n;
        let query = Query::AssetSpendState {
            intent: AssetReadIntentV2::new(&intent).unwrap(),
        };
        assert_eq!(
            queries.read(query.clone(), [9; 32], 5, now),
            Answer::AssetSpendState(AssetState::Pending)
        );
        assert_eq!(
            queries.read(query, [8; 32], 6, now),
            Answer::AssetSpendState(AssetState::Pending)
        );
        assert_eq!(queries.0.len(), n as usize + 1);
    }
    let query = Query::AssetSpendState {
        intent: AssetReadIntentV2::new(&intent()).unwrap(),
    };
    assert_eq!(
        queries.read(query, [9; 32], 5, now),
        Answer::Error(QueryError::Busy)
    );
    assert!(queries
        .0
        .iter()
        .all(|entry| entry.root == [9; 32] && entry.height == 5));
}

#[derive(Default)]
struct Memory(BTreeMap<Hash, Vec<u8>>);
impl crate::native_pipeline::state::tree::StateNodeReader for Memory {
    fn read_node(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn capture_budget() -> CaptureBudget {
    CaptureBudget {
        keys: 4096,
        nodes: 4096,
        bytes: 2 * 1024 * 1024,
    }
}

fn tree_fixture(records: &BTreeMap<Vec<u8>, Option<Vec<u8>>>) -> (Memory, Hash) {
    use crate::native_pipeline::state::tree::{empty_root, stage_state_update};
    let changes = records
        .iter()
        .filter_map(|(key, value)| {
            value.as_ref().map(|value| StateChange::Put {
                key: key.clone(),
                value: value.clone(),
            })
        })
        .collect::<Vec<_>>();
    let state = stage_state_update(&Memory::default(), empty_root(), &changes).unwrap();
    (Memory(state.nodes().clone()), state.root())
}

// Pure channel-independent frontier exercise; request counts here are logical
// 64-node batches, not physical disk reads. Product I/O is exercised separately.
fn finish_memory_capture(
    read: &mut UcaBatchStateRead,
    memory: &Memory,
) -> Vec<Result<UcaReadResult>> {
    if let Some(capture) = read.capture.as_mut() {
        loop {
            match capture.advance(CAPTURE_QUANTUM).unwrap() {
                CaptureStep::More => {}
                CaptureStep::NeedRead => {
                    let request = capture.next_request().unwrap().unwrap();
                    assert!((1..=64).contains(&request.len()));
                    read.stats.io_requests += 1;
                    read.stats.io_nodes += request.len();
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
        read.input = Some(read.capture.take().unwrap().finish().unwrap());
    }
    for _ in 0..1000 {
        if let Some(rows) = read.poll_captured(None).unwrap() {
            return rows;
        }
    }
    panic!("bounded captured-row driver did not finish")
}

#[test]
fn asset_batch_capture_deduplicates_all_10_keys_and_preserves_order_and_revocation() {
    let (memory, root) = tree_fixture(&fixture());
    let inputs = (0..32)
        .map(|nonce| {
            let mut intent = intent();
            intent.nonce = nonce;
            (intent, account(1), account(2))
        })
        .collect();
    let mut read = UcaBatchStateRead::new(root, inputs, capture_budget()).unwrap();
    assert_eq!(read.stats().declaration_keys, 32 * 10);
    assert_eq!(read.stats().unique_keys, 10);
    let rows = finish_memory_capture(&mut read, &memory);
    assert_eq!(rows.len(), 32);
    for (nonce, row) in rows.into_iter().enumerate() {
        let UcaReadResult::Present(state) = row.unwrap() else {
            panic!("known state reported missing")
        };
        let mut expected = intent();
        expected.nonce = nonce as u64;
        assert!(state.matches_intent(&expected));
        assert_eq!(state.parent_root(), root);
        assert_eq!(state.next_nonce(), 9);
        assert!(state.grant().unwrap().revoked);
    }
    assert!(read.stats().io_requests > 0);
    assert!(read.stats().io_requests < read.stats().declaration_keys);
    assert!(read.input.is_none());
    assert!(read.completed);
    assert!(read.made_progress());
}

#[test]
fn uca_local_turn_captured_rows_match_single_steps_including_corrupt_pages() {
    let good = fixture();
    let mut missing = good.clone();
    let page = missing
        .iter()
        .find(|(key, value)| {
            value.is_some() && key.windows(b"/page/".len()).any(|part| part == b"/page/")
        })
        .map(|(key, _)| key.clone())
        .unwrap();
    missing.insert(page, None);
    for records in [good, missing] {
        let (memory, root) = tree_fixture(&records);
        let inputs = (0..32)
            .map(|nonce| {
                let mut row = intent();
                row.nonce = nonce;
                (row, account(1), account(2))
            })
            .collect::<Vec<_>>();
        let mut serial = UcaBatchStateRead::new(root, inputs.clone(), capture_budget()).unwrap();
        let expected = finish_memory_capture(&mut serial, &memory);
        let mut combined = UcaBatchStateRead::new(root, inputs, capture_budget()).unwrap();
        let declaration = AssetSpendStateAccessV2::new(intent(), account(1), account(2)).unwrap();
        combined.capture = None;
        combined.input = Some(
            OwnedStateInput::capture(&memory, root, declaration.declarations(), capture_budget())
                .unwrap(),
        );
        let mut turns = 0;
        let actual = loop {
            turns += 1;
            assert!(turns < 20, "finite local turn fixture must complete");
            let (result, progressed, steps) = poll_local_turn(
                || {
                    combined.made_progress = false;
                    let result = combined.poll_captured(None);
                    (result, combined.made_progress, false)
                },
                || Duration::ZERO,
            );
            assert!(progressed);
            assert!(steps <= LOCAL_STEPS_PER_TURN);
            if let Some(rows) = result.unwrap() {
                break rows;
            }
        };
        assert_eq!(
            turns, 1,
            "two leaf passes and four decode passes fit one turn"
        );
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
            match (actual, expected) {
                (Ok(UcaReadResult::Present(actual)), Ok(UcaReadResult::Present(expected))) => {
                    let mut row = intent();
                    row.nonce = index as u64;
                    assert!(actual.matches_intent(&row));
                    assert_eq!(actual.parent_root(), expected.parent_root());
                    assert_eq!(actual.next_nonce(), expected.next_nonce());
                    assert_eq!(actual.balances(), expected.balances());
                    assert_eq!(
                        serde_json::to_value(actual.grant()).unwrap(),
                        serde_json::to_value(expected.grant()).unwrap()
                    );
                    assert_eq!(actual.static_failure(5), expected.static_failure(5));
                }
                (Err(actual), Err(expected)) => {
                    assert_eq!(actual.to_string(), expected.to_string())
                }
                _ => panic!("cooperative capture changed row result or order"),
            }
        }
        assert!(combined.completed);
    }
}

#[test]
fn batch_capture_bad_alias_is_row_local_and_all_invalid_needs_no_io() {
    let (memory, root) = tree_fixture(&fixture());
    let mut read = UcaBatchStateRead::new(
        root,
        vec![
            (intent(), account(1), account(2)),
            (intent(), account(1), account(1)),
            (intent(), account(3), account(2)),
        ],
        capture_budget(),
    )
    .unwrap();
    let mut rows = finish_memory_capture(&mut read, &memory).into_iter();
    assert!(matches!(
        rows.next().unwrap(),
        Ok(UcaReadResult::Present(_))
    ));
    assert!(rows.next().unwrap().is_err());
    assert!(rows.next().unwrap().is_err());
    let mut invalid = UcaBatchStateRead::new(
        root,
        vec![(intent(), account(1), account(1))],
        capture_budget(),
    )
    .unwrap();
    assert!(invalid.capture.is_none());
    assert!(finish_memory_capture(&mut invalid, &Memory::default())[0].is_err());
    assert_eq!(invalid.stats().io_requests, 0);
    assert_eq!(invalid.stats().unique_keys, 0);
}

#[test]
fn batch_capture_keeps_tail_checks_missing_and_budget_failures_distinct() {
    let access = AssetSpendStateAccessV2::new(intent(), account(1), account(2)).unwrap();
    let tail = access
        .declarations()
        .iter()
        .find(|d| {
            d.key.ends_with(&1u16.to_be_bytes())
                && d.key.windows(7).any(|part| part == b"/grant/")
                && d.key.windows(6).any(|part| part == b"/page/")
        })
        .unwrap()
        .key
        .clone();
    let mut records = fixture();
    records.insert(tail, Some(vec![1]));
    let (memory, root) = tree_fixture(&records);
    let mut read = UcaBatchStateRead::new(
        root,
        vec![(intent(), account(1), account(2))],
        capture_budget(),
    )
    .unwrap();
    assert!(finish_memory_capture(&mut read, &memory)[0].is_err());
    let mut missing = UcaBatchStateRead::new(
        crate::native_pipeline::state::tree::empty_root(),
        vec![(intent(), account(1), account(2))],
        capture_budget(),
    )
    .unwrap();
    assert!(matches!(
        finish_memory_capture(&mut missing, &Memory::default())[0],
        Ok(UcaReadResult::Missing)
    ));
    assert!(UcaBatchStateRead::new(
        [0; 32],
        vec![(intent(), account(1), account(2))],
        capture_budget()
    )
    .is_err());
    let mut small = capture_budget();
    small.keys = access.declarations().len() - 1;
    assert!(UcaBatchStateRead::new(root, vec![(intent(), account(1), account(2))], small).is_err());
}

#[test]
fn batch_capture_does_not_mix_roots_or_accept_reordered_node_replies() {
    let (memory, root) = tree_fixture(&fixture());
    let mut changed = fixture();
    changed.insert(
        balance_key(&account(1)),
        Some(42u128.to_le_bytes().to_vec()),
    );
    let (other_memory, other_root) = tree_fixture(&changed);
    assert_ne!(root, other_root);
    for (memory, root, expected) in [(&memory, root, u128::MAX), (&other_memory, other_root, 42)] {
        let mut read = UcaBatchStateRead::new(
            root,
            vec![(intent(), account(1), account(2))],
            capture_budget(),
        )
        .unwrap();
        let UcaReadResult::Present(state) =
            finish_memory_capture(&mut read, memory).remove(0).unwrap()
        else {
            panic!("known state missing")
        };
        assert_eq!(state.balances().0, expected);
        assert_eq!(state.parent_root(), root);
    }
    let mut read = UcaBatchStateRead::new(
        root,
        vec![(intent(), account(1), account(2))],
        capture_budget(),
    )
    .unwrap();
    let capture = read.capture.as_mut().unwrap();
    loop {
        match capture.advance(CAPTURE_QUANTUM).unwrap() {
            CaptureStep::More => {}
            CaptureStep::NeedRead => {
                let request = capture.next_request().unwrap().unwrap();
                let mut values: Vec<_> = request
                    .iter()
                    .map(|hash| memory.0.get(hash).cloned())
                    .collect();
                if values.len() > 1 {
                    values.reverse();
                    assert!(capture.accept(values).is_err());
                    assert!(capture.advance(CAPTURE_QUANTUM).is_err());
                    break;
                }
                capture.accept(values).unwrap();
            }
            CaptureStep::Complete => panic!("fixture should require a multi-node frontier"),
        }
    }
}
