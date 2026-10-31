//! Selection accounting only: shaped signatures here confer no admission or
//! spending authority. Real ML-DSA/parent authorization tests run separately.
use super::*;
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV3, SignedAssetSpendV3, MAX_SIGNED_SPEND_BYTES,
    MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
};
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3};

fn pending_uca(rows: usize) -> Result<(VecDeque<Hash>, BTreeMap<Hash, Pending>)> {
    let public = vec![9; MLDSA65_PUBLIC_KEY_BYTES];
    let mut order = VecDeque::new();
    let mut pending = BTreeMap::new();
    for first in (0..rows).step_by(64) {
        let raws = (first..(first + 64).min(rows))
            .map(|nonce| {
                SignedAssetSpendV3 {
                    intent: AssetSpendIntentV3 {
                        authority:
                            novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
                        asset: 1,
                        authority_update: None,
                        recipient_creation: None,
                        chain_id: 71,
                        genesis: [1; 32],
                        payer: vec![1; 20],
                        authorization_epoch: 1,
                        spend_key_ref: spend_key_ref_v1(&public)?,
                        recipient: vec![2; 20],
                        amount: 1,
                        max_fee: 1,
                        nonce: nonce as u64,
                        not_before_height: 1,
                        expires_at_height: 50,
                    },
                    public_key: public.clone(),
                    signature: vec![7; MLDSA65_SIGNATURE_BYTES],
                }
                .encode()
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let batch = Arc::new(AssetSpendBatchV3::from_raw(
            &raws,
            AssetSpendBatchLimitsV3 {
                transactions: 1024,
                transaction_bytes: MAX_SIGNED_SPEND_BYTES,
                body_bytes: super::super::uca_body_byte_limit(1024),
            },
        )?);
        for (index, raw) in raws.iter().enumerate() {
            let nonce = first + index;
            let mut hash = [4; 32];
            hash[..8].copy_from_slice(&(nonce as u64).to_le_bytes());
            order.push_back(hash);
            pending.insert(
                hash,
                Pending {
                    input: AdmissionInput::Uca {
                        batch: batch.clone(),
                        index,
                    },
                    admitted: None,
                    canonical_len: raw.len(),
                    signer: [3; 32],
                    nonce: nonce as u64,
                    gossip_missing: 0,
                },
            );
        }
    }
    Ok((order, pending))
}

#[test]
fn asset_successor_window_fills_past_queued_predecessor_rows_without_nonce_credit() -> Result<()> {
    let (order, pending) = pending_uca(1536)?;
    let known = BTreeMap::from([([3; 32], 0)]);
    let successor = BTreeMap::from([([3; 32], 512)]);
    let old = nonce_selection_window(&order, 0, 1024);
    let (partial, _) =
        select_prepared_nonce_window(&old, &pending, &known, &successor, 1024).unwrap();
    assert_eq!(partial.len(), 512);
    let (window, no_ready) =
        asset_nonce_selection_window(&order, &pending, &known, &successor, 0, 1024);
    assert!(!no_ready);
    assert_eq!(
        window,
        order.iter().skip(512).copied().collect::<VecDeque<_>>()
    );
    let (selected, full) =
        select_prepared_nonce_window(&window, &pending, &known, &successor, 1024).unwrap();
    assert_eq!(selected.len(), 1024);
    assert!(full);
    assert_eq!(order.len(), 1536);
    assert_eq!(pending.len(), 1536);

    let empty = BTreeMap::new();
    let (unknown, no_ready) =
        asset_nonce_selection_window(&order, &pending, &empty, &empty, 0, 1024);
    assert!(!no_ready);
    assert!(select_prepared_nonce_window(&unknown, &pending, &empty, &empty, 1024).is_none());
    Ok(())
}

#[test]
fn asset_full_known_stream_does_not_skip_an_older_unknown_nonce_subject() -> Result<()> {
    let (order, mut pending) = pending_uca(1025)?;
    // Selection-only fixture: distinguish the older subject without granting
    // it a read result or passing this shaped row to execution.
    pending.get_mut(&order[0]).unwrap().signer = [4; 32];
    let known = BTreeMap::from([([3; 32], 1)]);
    let (available, _, no_ready) =
        select_pending_batch_with_budget(&order, &pending, &known, BTreeMap::new(), 1024);
    assert!(!no_ready);
    assert_eq!(available.len(), 1024);
    let (window, no_ready) =
        asset_nonce_selection_window(&order, &pending, &known, &BTreeMap::new(), 0, 1024);
    assert!(!no_ready);
    assert_eq!(window, nonce_selection_window(&order, 0, 1024));
    assert_eq!(window.front(), order.front());
    assert!(
        select_prepared_nonce_window(&window, &pending, &known, &BTreeMap::new(), 1024).is_none()
    );
    Ok(())
}

#[test]
fn asset_nonce_window_keeps_low_load_and_missing_predecessor_behavior() -> Result<()> {
    let (order, mut pending) = pending_uca(64)?;
    let known = BTreeMap::from([([3; 32], 0)]);
    let (window, no_ready) =
        asset_nonce_selection_window(&order, &pending, &known, &BTreeMap::new(), 0, 1024);
    assert!(!no_ready);
    let (selected, full) =
        select_prepared_nonce_window(&window, &pending, &known, &BTreeMap::new(), 1024).unwrap();
    assert_eq!(selected.len(), 64);
    assert!(!full);
    pending.remove(&order[0]);
    let (window, no_ready) =
        asset_nonce_selection_window(&order, &pending, &known, &BTreeMap::new(), 0, 1024);
    assert!(no_ready);
    let (selected, _) =
        select_prepared_nonce_window(&window, &pending, &known, &BTreeMap::new(), 1024).unwrap();
    assert!(selected.is_empty(), "a known nonce gap cannot be skipped");
    Ok(())
}

#[test]
fn asset_selection_1024_rows_at_8m_preserves_queued_nonce_suffix() -> Result<()> {
    let (order, pending) = pending_uca(1024 + 64)?;
    let limit = super::super::uca_body_byte_limit(1024);
    assert_eq!(limit, 8 * 1024 * 1024);
    let row = &pending[&order[0]];
    assert_eq!(row.selection_input_bytes(), Some(row.canonical_len));
    let expected_count = 1024;
    let known = BTreeMap::from([([3; 32], 0)]);
    let selected = select_pending_batch(&order, &pending, &known, BTreeMap::new(), 1024);
    assert_eq!(selected.len(), expected_count);
    assert_eq!(
        selected,
        order
            .iter()
            .take(expected_count)
            .copied()
            .collect::<Vec<_>>()
    );
    let bytes = selected
        .iter()
        .map(|hash| pending[hash].selection_input_bytes().unwrap())
        .sum::<usize>();
    assert!(bytes <= limit);
    assert!(
        bytes > 384 * 1024,
        "the old byte ceiling cannot hold this prefix"
    );
    assert_eq!(
        pending.len(),
        1088,
        "selection cannot discard the queued nonce suffix"
    );
    assert_eq!(
        known[&[3; 32]], 0,
        "selection cannot grant an external nonce credit"
    );
    let advanced = BTreeMap::from([([3; 32], expected_count as u64)]);
    let later = select_pending_batch(&order, &pending, &advanced, BTreeMap::new(), 1024);
    assert_eq!(
        later,
        order
            .iter()
            .skip(expected_count)
            .copied()
            .collect::<Vec<_>>()
    );
    assert_eq!(later.len(), 64);
    // Lower configured row limits still win; low load need not wait for 1024.
    for rows in [1, 32, 64, 1024] {
        assert_eq!(
            select_pending_batch(&order, &pending, &known, BTreeMap::new(), rows).len(),
            rows
        );
    }
    Ok(())
}

#[test]
fn asset_selection_refuses_invalid_row_and_length_before_nonce_credit() -> Result<()> {
    let (order, mut pending) = pending_uca(128)?;
    let first = order[0];
    pending.get_mut(&first).unwrap().canonical_len = usize::MAX;
    assert!(pending[&first].selection_input_bytes().is_none());
    let known = BTreeMap::from([([3; 32], 0)]);
    assert!(select_pending_batch(&order, &pending, &known, BTreeMap::new(), 1024).is_empty());
    let entry = pending.get_mut(&first).unwrap();
    entry.canonical_len = 1;
    let AdmissionInput::Uca { index, .. } = &mut entry.input else {
        unreachable!()
    };
    *index = usize::MAX;
    assert!(pending[&first].selection_input_bytes().is_none());
    assert!(select_pending_batch(&order, &pending, &known, BTreeMap::new(), 1024).is_empty());
    assert_eq!(known[&[3; 32]], 0);
    Ok(())
}

#[test]
fn eight_grant_openings_respect_original_access_budget_and_preserve_suffix() -> Result<()> {
    use novovm_protocol::uca_spend::{AssetAccountCreationV2, AssetGrantConfigV2};
    let public = vec![9; MLDSA65_PUBLIC_KEY_BYTES];
    let key_ref = spend_key_ref_v1(&public)?;
    let rows = 1056;
    let mut raws = Vec::new();
    for index in 0..rows {
        let mut management = [0xf0; 32];
        management[..8].copy_from_slice(&(index as u64).to_be_bytes());
        let creation = AssetAccountCreationV2 {
            management_key_ref: management,
            recovery_key_ref: None,
            grants: (1..=8)
                .map(|key| AssetGrantConfigV2 {
                    key_ref: [key; 32],
                    permits_nov_transfer: true,
                    not_before_height: 1,
                    expires_at_height: 100,
                    max_debit_per_intent: 1000,
                    remaining_debit_budget: 1000,
                })
                .collect(),
        };
        let mut payer = [1; 20];
        payer[12..].copy_from_slice(&(index as u64).to_be_bytes());
        raws.push(
            SignedAssetSpendV3 {
                intent: AssetSpendIntentV3 {
                    authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
                    chain_id: 71,
                    genesis: [1; 32],
                    payer: payer.to_vec(),
                    authorization_epoch: 1,
                    spend_key_ref: key_ref,
                    recipient: creation.account(71, &[1; 32])?.to_vec(),
                    asset: 1,
                    amount: 1,
                    max_fee: 1,
                    nonce: 0,
                    not_before_height: 1,
                    expires_at_height: 50,
                    authority_update: None,
                    recipient_creation: Some(creation),
                },
                public_key: public.clone(),
                signature: vec![7; MLDSA65_SIGNATURE_BYTES],
            }
            .encode()?,
        );
    }
    let mut order = VecDeque::new();
    let mut pending = BTreeMap::new();
    let mut known = BTreeMap::new();
    for (batch_index, chunk) in raws.chunks(64).enumerate() {
        let batch = Arc::new(AssetSpendBatchV3::from_raw(
            chunk,
            AssetSpendBatchLimitsV3 {
                transactions: 1024,
                transaction_bytes: MAX_SIGNED_SPEND_BYTES,
                body_bytes: super::super::uca_body_byte_limit(1024),
            },
        )?);
        for (index, raw) in chunk.iter().enumerate() {
            let row = batch_index * 64 + index;
            let mut id = [5; 32];
            id[..8].copy_from_slice(&(row as u64).to_be_bytes());
            known.insert(id, 0);
            order.push_back(id);
            pending.insert(
                id,
                Pending {
                    input: AdmissionInput::Uca {
                        batch: batch.clone(),
                        index,
                    },
                    admitted: None,
                    canonical_len: raw.len(),
                    signer: id,
                    nonce: 0,
                    gossip_missing: 0,
                },
            );
        }
    }
    let (selected, full, no_ready) =
        select_pending_batch_with_budget(&order, &pending, &known, BTreeMap::new(), 1024);
    assert!(!no_ready);
    assert!(full && !selected.is_empty() && selected.len() < 1024);
    let keys = 42
        + selected
            .iter()
            .map(|id| pending[id].selection_access_keys().unwrap())
            .sum::<usize>();
    assert!(keys <= ASSET_PLAN_ACCESS_KEYS);
    assert!(
        keys + pending[&order[selected.len()]]
            .selection_access_keys()
            .unwrap()
            > ASSET_PLAN_ACCESS_KEYS
    );
    assert!(
        selected
            .iter()
            .map(|id| pending[id].canonical_len)
            .sum::<usize>()
            < super::super::uca_body_byte_limit(1024),
        "access, not bytes, ends this prefix"
    );
    assert_eq!(
        selected,
        order
            .iter()
            .take(selected.len())
            .copied()
            .collect::<Vec<_>>()
    );
    assert_eq!(pending.len(), rows);
    let later_order = order.iter().skip(selected.len()).copied().collect();
    let later = select_pending_batch(&later_order, &pending, &known, BTreeMap::new(), 1024);
    assert_eq!(
        later,
        order
            .iter()
            .skip(selected.len())
            .copied()
            .collect::<Vec<_>>()
    );
    assert!(
        known.values().all(|nonce| *nonce == 0),
        "selection cannot mutate parent nonce state"
    );
    Ok(())
}

#[test]
fn asset_ready_selection_reports_actual_budget_not_pool_count_or_partial_quantum() -> Result<()> {
    let (order, mut pending) = pending_uca(1024 + 64)?;
    let known = BTreeMap::from([([3; 32], 0)]);
    let first_quantum = order.iter().take(32).copied().collect();
    let (partial, full) =
        select_prepared_nonce_window(&first_quantum, &pending, &known, &BTreeMap::new(), 64)
            .unwrap();
    assert_eq!(partial.len(), 32);
    assert!(!full, "32 verified rows do not fill a 64-row source budget");

    let full_source = nonce_selection_window(&order, 0, 64);
    let (selected, full) =
        select_prepared_nonce_window(&full_source, &pending, &known, &BTreeMap::new(), 64).unwrap();
    assert_eq!(selected.len(), 64);
    assert!(
        full,
        "later admission must not block a full ready candidate"
    );

    let partial_source = nonce_selection_window(&order, 0, 128);
    let (selected, full) =
        select_prepared_nonce_window(&partial_source, &pending, &known, &BTreeMap::new(), 1024)
            .unwrap();
    assert_eq!(selected.len(), 128);
    assert!(
        !full,
        "128 rows no longer exhaust the candidate byte budget"
    );

    let (selected, full) =
        select_prepared_nonce_window(&order, &pending, &known, &BTreeMap::new(), 1024).unwrap();
    assert_eq!(selected.len(), 1024);
    assert!(full, "the configured row limit makes a full candidate");
    let bytes = selected
        .iter()
        .map(|hash| pending[hash].selection_input_bytes().unwrap())
        .sum::<usize>();
    assert!(bytes <= super::super::uca_body_byte_limit(1024));
    assert_eq!(known[&[3; 32]], 0);
    assert_eq!(pending.len(), 1088);

    // Many retained rows do not imply an executable prefix. Neither a real
    // nonce gap nor an unresolved read grants the full-candidate exception.
    pending.get_mut(&order[0]).unwrap().nonce = 7;
    let (selected, full) =
        select_prepared_nonce_window(&order, &pending, &known, &BTreeMap::new(), 1024).unwrap();
    assert!(selected.is_empty() && !full);
    assert!(select_prepared_nonce_window(
        &order,
        &pending,
        &BTreeMap::new(),
        &BTreeMap::new(),
        1024,
    )
    .is_none());
    Ok(())
}

fn selection_position() -> Position {
    Position {
        height: 4,
        round: 0,
        parent: ParentPoint {
            height: 3,
            block_hash: [1; 32],
            state_root: [2; 32],
            receipt_batch_commitment: [3; 32],
            state_version: 12,
            decision_hash: [4; 32],
        },
        source: Some((
            VoteContext {
                chain_id: 71,
                genesis_config_commitment: [5; 32],
                protocol_commitment: [6; 32],
                epoch: 1,
                validator_set_hash: [7; 32],
                height: 3,
                parent_block_hash: [8; 32],
                parent_decision_hash: [9; 32],
            },
            2,
        )),
    }
}

#[test]
fn asset_source_wait_reuses_only_unchanged_partial_selection() -> Result<()> {
    let (order, pending) = pending_uca(32)?;
    let known = BTreeMap::from([([3; 32], 0)]);
    let (selected, full) =
        select_prepared_nonce_window(&order, &pending, &known, &BTreeMap::new(), 64).unwrap();
    assert_eq!(selected.len(), 32);
    assert!(!full);
    let position = selection_position();
    let stamp = SelectionStamp {
        pool_revision: 32,
        nonce_revision: 1,
        boundary: false,
        awaiting_admission: true,
    };
    let wait = SelectionWait {
        position,
        stamp,
        reason: SelectionWaitReason::SourceBoundary,
        selected: selected.len(),
    };
    for _ in 0..128 {
        assert!(wait.matches(position, stamp));
    }
    for changed in [
        SelectionStamp {
            boundary: true,
            ..stamp
        },
        SelectionStamp {
            awaiting_admission: false,
            ..stamp
        },
        SelectionStamp {
            pool_revision: 33,
            ..stamp
        },
        SelectionStamp {
            nonce_revision: 2,
            ..stamp
        },
    ] {
        assert!(!wait.matches(position, changed));
    }
    assert_eq!(known[&[3; 32]], 0);
    Ok(())
}

#[test]
fn asset_whole_pool_gap_wait_wakes_for_same_size_nonce_bridge() -> Result<()> {
    let (mut order, mut pending) = pending_uca(33)?;
    let bridge_hash = order.pop_front().unwrap();
    let bridge = pending.remove(&bridge_hash).unwrap();
    let known = BTreeMap::from([([3; 32], 0)]);
    let (window, no_ready) =
        asset_nonce_selection_window(&order, &pending, &known, &BTreeMap::new(), 0, 16);
    assert_eq!(window.len(), 16);
    assert!(
        no_ready,
        "the original full-order pass proves every row is future"
    );
    let position = selection_position();
    let stamp = SelectionStamp {
        pool_revision: 33,
        nonce_revision: 1,
        boundary: false,
        awaiting_admission: true,
    };
    let wait = SelectionWait {
        position,
        stamp,
        reason: SelectionWaitReason::NoReady,
        selected: 0,
    };
    let count = pending.len();
    let bytes: usize = pending.values().map(|row| row.canonical_len).sum();
    let last = order.pop_back().unwrap();
    pending.remove(&last);
    pending.insert(bridge_hash, bridge);
    order.push_front(bridge_hash);
    assert_eq!(pending.len(), count);
    assert_eq!(
        pending.values().map(|row| row.canonical_len).sum::<usize>(),
        bytes
    );
    let changed = SelectionStamp {
        pool_revision: stamp.pool_revision + 2,
        ..stamp
    };
    assert!(
        !wait.matches(position, changed),
        "pool count/bytes cannot identify a mutation"
    );
    let (selected, full, no_ready) =
        select_pending_batch_with_budget(&order, &pending, &known, BTreeMap::new(), 32);
    assert_eq!(selected.len(), 32);
    assert_eq!(selected.first(), Some(&bridge_hash));
    assert!(full && !no_ready);
    assert_eq!(known[&[3; 32]], 0);
    Ok(())
}

#[test]
fn asset_local_gap_never_parks_over_ready_or_unknown_tail() -> Result<()> {
    let (order, mut pending) = pending_uca(3)?;
    pending.get_mut(&order[0]).unwrap().nonce = 1;
    pending.get_mut(&order[1]).unwrap().nonce = 2;
    let tail = pending.get_mut(&order[2]).unwrap();
    tail.signer = [4; 32];
    tail.nonce = 0;
    let known = BTreeMap::from([([3; 32], 0), ([4; 32], 0)]);
    let (window, no_ready) =
        asset_nonce_selection_window(&order, &pending, &known, &BTreeMap::new(), 0, 2);
    assert!(
        !no_ready,
        "a ready tail invalidates the whole-pool empty proof"
    );
    assert!(
        select_prepared_nonce_window(&window, &pending, &known, &BTreeMap::new(), 2,)
            .unwrap()
            .0
            .is_empty()
    );
    let next = advance_nonce_window(0, window.len(), order.len());
    let (window, no_ready) =
        asset_nonce_selection_window(&order, &pending, &known, &BTreeMap::new(), next, 2);
    assert!(!no_ready);
    assert_eq!(
        select_prepared_nonce_window(&window, &pending, &known, &BTreeMap::new(), 2,)
            .unwrap()
            .0,
        vec![order[2]]
    );

    let unknown_tail = BTreeMap::from([([3; 32], 0)]);
    let (_, no_ready) =
        asset_nonce_selection_window(&order, &pending, &unknown_tail, &BTreeMap::new(), 0, 2);
    assert!(
        !no_ready,
        "an unresolved tail must retain ordinary nonce rotation"
    );
    let (window, no_ready) =
        asset_nonce_selection_window(&order, &pending, &unknown_tail, &BTreeMap::new(), next, 2);
    assert!(!no_ready);
    assert!(
        select_prepared_nonce_window(&window, &pending, &unknown_tail, &BTreeMap::new(), 2,)
            .is_none()
    );
    Ok(())
}

#[test]
fn asset_metadata_or_capacity_exit_is_not_known_empty() -> Result<()> {
    let (order, mut pending) = pending_uca(2)?;
    let known = BTreeMap::from([([3; 32], 0)]);
    pending.get_mut(&order[0]).unwrap().canonical_len += 1;
    let (selected, full, no_ready) =
        select_pending_batch_with_budget(&order, &pending, &known, BTreeMap::new(), 1024);
    assert!(selected.is_empty() && !full && !no_ready);
    let first = pending.get_mut(&order[0]).unwrap();
    first.canonical_len = super::super::body_byte_limit(1) + 1;
    first.input = AdmissionInput::Raw(vec![0; 8]);
    let (selected, full, no_ready) =
        select_pending_batch_with_budget(&order, &pending, &known, BTreeMap::new(), 1);
    assert!(selected.is_empty() && full && !no_ready);
    Ok(())
}

#[test]
fn asset_selection_wait_binds_full_parent_round_and_successor_source() {
    let position = selection_position();
    let stamp = SelectionStamp {
        pool_revision: 1,
        nonce_revision: 1,
        boundary: false,
        awaiting_admission: true,
    };
    let wait = SelectionWait {
        position,
        stamp,
        reason: SelectionWaitReason::NoReady,
        selected: 0,
    };
    let (source, source_round) = position.source.unwrap();
    for changed in [
        Position {
            height: position.height + 1,
            ..position
        },
        Position {
            round: 1,
            ..position
        },
        Position {
            source: None,
            ..position
        },
        Position {
            source: Some((source, source_round + 1)),
            ..position
        },
        Position {
            source: Some((
                VoteContext {
                    parent_decision_hash: [10; 32],
                    ..source
                },
                source_round,
            )),
            ..position
        },
    ] {
        assert!(!wait.matches(changed, stamp));
    }
    let parent = position.parent;
    for changed in [
        ParentPoint {
            height: parent.height + 1,
            ..parent
        },
        ParentPoint {
            block_hash: [10; 32],
            ..parent
        },
        ParentPoint {
            state_root: [10; 32],
            ..parent
        },
        ParentPoint {
            receipt_batch_commitment: [10; 32],
            ..parent
        },
        ParentPoint {
            state_version: parent.state_version + 1,
            ..parent
        },
        ParentPoint {
            decision_hash: [10; 32],
            ..parent
        },
    ] {
        assert!(!wait.matches(
            Position {
                parent: changed,
                ..position
            },
            stamp
        ));
    }
}

#[test]
#[ignore = "real trusted AOEM and original native pool/admission owners; run alone"]
fn actual_asset_no_ready_wait_preserves_progress_until_nonce_bridge_arrives() -> Result<()> {
    super::super::uca_native_tests::with_uca_gossip_service_fixture(|nodes, signed| {
        let rpc = &mut nodes[0];
        rpc.batch_size = 2;
        let source = |rows: &[SignedAssetSpendV3]| -> Result<Arc<AssetSpendBatchV3>> {
            Ok(Arc::new(AssetSpendBatchV3::from_raw(
                &rows
                    .iter()
                    .map(SignedAssetSpendV3::encode)
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                AssetSpendBatchLimitsV3 {
                    transactions: 1024,
                    transaction_bytes: MAX_SIGNED_SPEND_BYTES,
                    body_bytes: super::super::uca_body_byte_limit(1024),
                },
            )?))
        };
        let admit = |rpc: &mut RpcLifecycle, batch: Arc<AssetSpendBatchV3>| -> Result<()> {
            rpc.incoming_from_client = false;
            rpc.incoming = Some((rpc.node.controller.uca_transactions_message(batch)?, 0));
            let deadline = Instant::now() + Duration::from_secs(15);
            while rpc.incoming.is_some() || rpc.admission.has_pending_native_admission() {
                ensure!(
                    Instant::now() < deadline,
                    "original selection-wait admission timeout"
                );
                rpc.poll_admission()?;
                std::thread::yield_now();
            }
            Ok(())
        };
        assert_eq!(signed[0].intent.nonce, 0);
        assert_eq!(signed[1].intent.nonce, 1);
        assert_eq!(signed[3].intent.nonce, 3);
        admit(rpc, source(&signed[1..4])?)?;
        assert_eq!(rpc.pending.len(), 3);
        let now = Instant::now();
        rpc.poll(now)?;
        let wait = rpc
            .selection_wait
            .context("whole-pool gap did not retain its wait")?;
        assert!(wait.reason == SelectionWaitReason::NoReady);
        let window = rpc
            .nonce_window
            .as_ref()
            .context("waiting window missing")?
            .1
            .clone();
        let scan = rpc.nonce_scan;
        let revision = rpc.pool_revision;
        for _ in 0..64 {
            rpc.poll(now)?;
            assert_eq!(rpc.nonce_scan, scan);
            assert!(rpc.selection_wait.is_some());
            assert_eq!(rpc.nonce_window.as_ref().unwrap().1, window);
            assert!(rpc.offer.is_none());
        }
        // Original duplicate admission does not change selectable pool state.
        admit(rpc, source(&signed[1..4])?)?;
        assert_eq!(rpc.pool_revision, revision);
        admit(rpc, source(&signed[..1])?)?;
        assert_ne!(rpc.pool_revision, revision);
        for _ in 0..=rpc.order.len() {
            rpc.poll(now)?;
            if rpc.offer.is_some() {
                break;
            }
        }
        assert!(rpc.selection_wait.is_none());
        let (_, body) = rpc
            .offer
            .as_ref()
            .context("nonce bridge did not resume body selection")?;
        let Message::AssetBody { batch, .. } = body.as_ref() else {
            anyhow::bail!("unexpected selected body contract");
        };
        assert_eq!(
            batch.len(),
            1,
            "the original finite window does not reorder older future rows"
        );
        assert!(batch.view(0)?.matches_signed(&signed[0])?);
        assert_eq!(
            rpc.pending.len(),
            4,
            "selection is not publication or nonce consumption"
        );
        Ok(())
    })
}
