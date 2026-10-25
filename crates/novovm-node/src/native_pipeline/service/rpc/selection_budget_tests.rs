//! Selection accounting only: shaped signatures here confer no admission or
//! spending authority. Real ML-DSA/parent authorization tests run separately.
use super::*;
use novovm_protocol::uca_spend::{
    spend_key_ref_v1, SignedUcaSpendV1, UcaSpendIntentV1, MAX_SIGNED_SPEND_BYTES,
    MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
};
use novovm_protocol::uca_spend_batch::{UcaSpendBatch, UcaSpendBatchLimits};

fn pending_uca() -> Result<(VecDeque<Hash>, BTreeMap<Hash, Pending>)> {
    let public = vec![9; MLDSA65_PUBLIC_KEY_BYTES];
    let mut order = VecDeque::new();
    let mut pending = BTreeMap::new();
    for first in [0usize, 64] {
        let raws = (first..first + 64)
            .map(|nonce| {
                SignedUcaSpendV1 {
                    intent: UcaSpendIntentV1 {
                        chain_id: 71,
                        genesis: [1; 32],
                        account_id: "alice".into(),
                        authorization_epoch: 1,
                        spend_key_ref: spend_key_ref_v1(&public)?,
                        recipient_uca: "bobby".into(),
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
        let hints = (0..64)
            .map(|i| {
                if i % 2 == 0 {
                    (vec![1; 20], vec![2; 32])
                } else {
                    (vec![1; 32], vec![2; 20])
                }
            })
            .collect::<Vec<_>>();
        let batch = Arc::new(UcaSpendBatch::from_raw(
            &raws,
            &hints,
            UcaSpendBatchLimits {
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
fn uca_selection_charges_actual_locators_at_384k_boundary_without_losing_nonce_prefix() -> Result<()>
{
    let (order, pending) = pending_uca()?;
    let limit = super::super::uca_body_byte_limit(1024);
    assert_eq!(limit, 384 * 1024);
    let row = &pending[&order[0]];
    assert_eq!(row.canonical_len, 5416);
    assert_eq!(row.selection_input_bytes(), Some(5416 + 52));
    let old_count = limit / row.canonical_len;
    assert_eq!(old_count, 72);
    assert!(old_count * row.canonical_len <= limit);
    assert!(
        old_count * row.selection_input_bytes().unwrap() > limit,
        "regression must exercise the old oversize candidate"
    );
    let known = BTreeMap::from([([3; 32], 0)]);
    let selected = select_pending_batch(&order, &pending, &known, BTreeMap::new(), 1024);
    assert_eq!(selected.len(), 71);
    assert_eq!(selected, order.iter().take(71).copied().collect::<Vec<_>>());
    let bytes = selected
        .iter()
        .map(|hash| pending[hash].selection_input_bytes().unwrap())
        .sum::<usize>();
    assert!(bytes <= limit);
    assert!(bytes + pending[&order[71]].selection_input_bytes().unwrap() > limit);
    assert_eq!(
        pending.len(),
        128,
        "byte limit cannot discard queued transactions"
    );
    assert_eq!(
        known[&[3; 32]], 0,
        "selection cannot grant an external nonce credit"
    );
    let advanced = BTreeMap::from([([3; 32], 71)]);
    let later = select_pending_batch(&order, &pending, &advanced, BTreeMap::new(), 1024);
    assert_eq!(later, order.iter().skip(71).copied().collect::<Vec<_>>());
    // A stricter row-count budget still wins; byte accounting is not capacity expansion.
    assert_eq!(
        select_pending_batch(&order, &pending, &known, BTreeMap::new(), 32).len(),
        32
    );
    Ok(())
}

#[test]
fn uca_selection_refuses_invalid_locator_and_length_overflow_before_nonce_credit() -> Result<()> {
    let (order, mut pending) = pending_uca()?;
    let first = order[0];
    pending.get_mut(&first).unwrap().canonical_len = usize::MAX;
    assert!(pending[&first].selection_input_bytes().is_none());
    let known = BTreeMap::from([([3; 32], 0)]);
    assert!(select_pending_batch(&order, &pending, &known, BTreeMap::new(), 1024).is_empty());
    let entry = pending.get_mut(&first).unwrap();
    entry.canonical_len = 5416;
    let AdmissionInput::Uca { index, .. } = &mut entry.input else {
        unreachable!()
    };
    *index = usize::MAX;
    assert!(pending[&first].selection_input_bytes().is_none());
    assert!(select_pending_batch(&order, &pending, &known, BTreeMap::new(), 1024).is_empty());
    assert_eq!(known[&[3; 32]], 0);
    Ok(())
}
