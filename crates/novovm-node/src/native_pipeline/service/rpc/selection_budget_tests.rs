//! Selection accounting only: shaped signatures here confer no admission or
//! spending authority. Real ML-DSA/parent authorization tests run separately.
use super::*;
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, AssetSpendIntentV2, SignedAssetSpendV2, MAX_SIGNED_SPEND_BYTES,
    MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
};
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};

fn pending_uca() -> Result<(VecDeque<Hash>, BTreeMap<Hash, Pending>)> {
    let public = vec![9; MLDSA65_PUBLIC_KEY_BYTES];
    let mut order = VecDeque::new();
    let mut pending = BTreeMap::new();
    for first in [0usize, 64] {
        let raws = (first..first + 64)
            .map(|nonce| {
                SignedAssetSpendV2 {
                    intent: AssetSpendIntentV2 {
                        asset: 1,
                        authority_update: None,
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
fn asset_selection_charges_actual_signed_bytes_at_384k_without_losing_nonce_prefix() -> Result<()> {
    let (order, pending) = pending_uca()?;
    let limit = super::super::uca_body_byte_limit(1024);
    assert_eq!(limit, 384 * 1024);
    let row = &pending[&order[0]];
    assert_eq!(row.selection_input_bytes(), Some(row.canonical_len));
    let expected_count = limit / row.canonical_len;
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
        bytes
            + pending[&order[expected_count]]
                .selection_input_bytes()
                .unwrap()
            > limit
    );
    assert_eq!(
        pending.len(),
        128,
        "byte limit cannot discard queued transactions"
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
    // A stricter row-count budget still wins; byte accounting is not capacity expansion.
    assert_eq!(
        select_pending_batch(&order, &pending, &known, BTreeMap::new(), 32).len(),
        32
    );
    Ok(())
}

#[test]
fn asset_selection_refuses_invalid_row_and_length_before_nonce_credit() -> Result<()> {
    let (order, mut pending) = pending_uca()?;
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
