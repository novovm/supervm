//! Selection accounting only: shaped signatures here confer no admission or
//! spending authority. Real ML-DSA/parent authorization tests run separately.
use super::*;
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV2, SignedAssetSpendV2, MAX_SIGNED_SPEND_BYTES,
    MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
};
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};

fn pending_uca(rows: usize) -> Result<(VecDeque<Hash>, BTreeMap<Hash, Pending>)> {
    let public = vec![9; MLDSA65_PUBLIC_KEY_BYTES];
    let mut order = VecDeque::new();
    let mut pending = BTreeMap::new();
    for first in (0..rows).step_by(64) {
        let raws = (first..(first + 64).min(rows))
            .map(|nonce| {
                SignedAssetSpendV2 {
                    intent: AssetSpendIntentV2 {
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
        let batch = Arc::new(AssetSpendBatchV2::from_raw(
            &raws,
            AssetSpendBatchLimitsV2 {
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
        select_prepared_nonce_window(&old, &pending, &known, successor.clone(), 1024).unwrap();
    assert_eq!(partial.len(), 512);
    let window = asset_nonce_selection_window(&order, &pending, &known, successor.clone(), 0, 1024);
    assert_eq!(
        window,
        order.iter().skip(512).copied().collect::<VecDeque<_>>()
    );
    let (selected, full) =
        select_prepared_nonce_window(&window, &pending, &known, successor, 1024).unwrap();
    assert_eq!(selected.len(), 1024);
    assert!(full);
    assert_eq!(order.len(), 1536);
    assert_eq!(pending.len(), 1536);

    let empty = BTreeMap::new();
    let unknown = asset_nonce_selection_window(&order, &pending, &empty, empty.clone(), 0, 1024);
    assert!(
        select_prepared_nonce_window(&unknown, &pending, &empty, empty.clone(), 1024).is_none()
    );
    Ok(())
}

#[test]
fn asset_full_known_stream_does_not_skip_an_older_unknown_nonce_subject() -> Result<()> {
    let (order, mut pending) = pending_uca(1025)?;
    // Selection-only fixture: distinguish the older subject without granting
    // it a read result or passing this shaped row to execution.
    pending.get_mut(&order[0]).unwrap().signer = [4; 32];
    let known = BTreeMap::from([([3; 32], 1)]);
    let (available, _) =
        select_pending_batch_with_budget(&order, &pending, &known, BTreeMap::new(), 1024);
    assert_eq!(available.len(), 1024);
    let window = asset_nonce_selection_window(&order, &pending, &known, BTreeMap::new(), 0, 1024);
    assert_eq!(window, nonce_selection_window(&order, 0, 1024));
    assert_eq!(window.front(), order.front());
    assert!(
        select_prepared_nonce_window(&window, &pending, &known, BTreeMap::new(), 1024).is_none()
    );
    Ok(())
}

#[test]
fn asset_nonce_window_keeps_low_load_and_missing_predecessor_behavior() -> Result<()> {
    let (order, mut pending) = pending_uca(64)?;
    let known = BTreeMap::from([([3; 32], 0)]);
    let window = asset_nonce_selection_window(&order, &pending, &known, BTreeMap::new(), 0, 1024);
    let (selected, full) =
        select_prepared_nonce_window(&window, &pending, &known, BTreeMap::new(), 1024).unwrap();
    assert_eq!(selected.len(), 64);
    assert!(!full);
    pending.remove(&order[0]);
    let window = asset_nonce_selection_window(&order, &pending, &known, BTreeMap::new(), 0, 1024);
    let (selected, _) =
        select_prepared_nonce_window(&window, &pending, &known, BTreeMap::new(), 1024).unwrap();
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
            SignedAssetSpendV2 {
                intent: AssetSpendIntentV2 {
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
        let batch = Arc::new(AssetSpendBatchV2::from_raw(
            chunk,
            AssetSpendBatchLimitsV2 {
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
    let (selected, full) =
        select_pending_batch_with_budget(&order, &pending, &known, BTreeMap::new(), 1024);
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
        select_prepared_nonce_window(&first_quantum, &pending, &known, BTreeMap::new(), 64)
            .unwrap();
    assert_eq!(partial.len(), 32);
    assert!(!full, "32 verified rows do not fill a 64-row source budget");

    let full_source = nonce_selection_window(&order, 0, 64);
    let (selected, full) =
        select_prepared_nonce_window(&full_source, &pending, &known, BTreeMap::new(), 64).unwrap();
    assert_eq!(selected.len(), 64);
    assert!(
        full,
        "later admission must not block a full ready candidate"
    );

    let partial_source = nonce_selection_window(&order, 0, 128);
    let (selected, full) =
        select_prepared_nonce_window(&partial_source, &pending, &known, BTreeMap::new(), 1024)
            .unwrap();
    assert_eq!(selected.len(), 128);
    assert!(
        !full,
        "128 rows no longer exhaust the candidate byte budget"
    );

    let (selected, full) =
        select_prepared_nonce_window(&order, &pending, &known, BTreeMap::new(), 1024).unwrap();
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
        select_prepared_nonce_window(&order, &pending, &known, BTreeMap::new(), 1024).unwrap();
    assert!(selected.is_empty() && !full);
    assert!(select_prepared_nonce_window(
        &order,
        &pending,
        &BTreeMap::new(),
        BTreeMap::new(),
        1024,
    )
    .is_none());
    Ok(())
}
