//! Pool/capability ownership tests: the real strict verifier creates tokens;
//! no AOEM worker, synthetic ingress permit, or persistence ACK is fabricated.
use super::*;
use crate::native_pipeline::execution::plan::BatchContext;
use crate::native_pipeline::ingress::batch::{AuthenticationBudget, SignatureCheckedBatch};
use crate::native_pipeline::pipeline::admitted_apfl_for_test;

fn raw(count: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|nonce| super::super::tests::signed_raw(19, nonce as u64))
        .collect()
}

fn body(batch: Arc<ApflTransferBatch>) -> Message {
    Message::ApflBody {
        context: BatchContext {
            chain_id: 71,
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
            business_program: [3; 32],
            semantic_version: 1,
            effect_contract: [4; 32],
            parent_block_hash: [0; 32],
            parent_height: 0,
            parent_state_root: [5; 32],
            parent_receipt_root: [6; 32],
            parent_state_version: 0,
            receipt_codec: [7; 32],
            height: 1,
            slot: 1,
            timestamp_unix_ms: 123,
        },
        batch,
    }
}

fn pool(
    source: &Arc<ApflTransferBatch>,
) -> Result<(VecDeque<Hash>, BTreeMap<Hash, Pending>, PoolRetention)> {
    let owner = Arc::new(());
    let mut order = VecDeque::new();
    let mut pending = BTreeMap::new();
    let mut retained = PoolRetention::default();
    for index in 0..source.len() {
        let admitted = admitted_apfl_for_test(owner.clone(), 71, source.clone(), index)?;
        let checked = admitted.checked();
        let hash = checked.tx_hash();
        let signer = checked.nonce_identity();
        let nonce = checked.transfer().nonce;
        let canonical_len = checked.transfer().encoded_len()?;
        let input = AdmissionInput::Apfl {
            batch: source.clone(),
            index,
        };
        retained.reserve(&input, admitted.retained_metadata_bytes(), MAX_POOL_BYTES)?;
        order.push_back(hash);
        pending.insert(
            hash,
            Pending {
                input,
                admitted: Some(admitted),
                canonical_len,
                signer,
                nonce,
                gossip_missing: 0,
            },
        );
    }
    Ok((order, pending, retained))
}

#[test]
fn admitted_metadata_shares_original_pool_byte_budget_and_releases_per_row() -> Result<()> {
    let source = apfl_batch(&raw(2), 8)?;
    let owner_bytes = source.retained_bytes()?;
    let row0 = AdmissionInput::Apfl {
        batch: source.clone(),
        index: 0,
    };
    let row1 = AdmissionInput::Apfl {
        batch: source.clone(),
        index: 1,
    };
    let owner = Arc::new(());
    let first = admitted_apfl_for_test(owner.clone(), 71, source.clone(), 0)?;
    let second = admitted_apfl_for_test(owner, 71, source.clone(), 1)?;
    let metadata = first.retained_metadata_bytes();
    assert!(metadata > 0);
    assert_eq!(second.retained_metadata_bytes(), metadata);
    let mut retained = PoolRetention::default();
    assert!(retained.reserve(&row0, metadata, owner_bytes).is_err());
    assert_eq!(
        (
            retained.bytes,
            retained.metadata_bytes,
            retained.owners.len()
        ),
        (0, 0, 0)
    );
    retained.reserve(&row0, metadata, owner_bytes + metadata)?;
    assert!(retained
        .reserve(&row1, metadata, owner_bytes + metadata)
        .is_err());
    assert_eq!(retained.bytes, owner_bytes + metadata);
    assert_eq!(retained.metadata_bytes, metadata);
    assert_eq!(retained.owners.len(), 1);
    retained.reserve(&row1, metadata, owner_bytes + 2 * metadata)?;
    assert_eq!(retained.bytes, owner_bytes + 2 * metadata);
    retained.release(&row0, metadata);
    assert_eq!(retained.bytes, owner_bytes + metadata);
    assert_eq!(
        retained.owners.len(),
        1,
        "last row still pins the dictionary"
    );
    retained.release(&row1, metadata);
    assert_eq!(
        (
            retained.bytes,
            retained.metadata_bytes,
            retained.owners.len()
        ),
        (0, 0, 0)
    );
    assert!(retained.reserve(&row0, usize::MAX, usize::MAX).is_err());
    assert_eq!(
        (
            retained.bytes,
            retained.metadata_bytes,
            retained.owners.len()
        ),
        (0, 0, 0)
    );
    Ok(())
}

#[test]
fn raw_pool_discards_decoded_capability_instead_of_pinning_an_extra_source() -> Result<()> {
    let raw = raw(1);
    let source = apfl_batch(&raw, 8)?;
    let weak = Arc::downgrade(&source);
    let admitted = admitted_apfl_for_test(Arc::new(()), 71, source.clone(), 0)?;
    assert!(admitted
        .checked()
        .transfer()
        .matches_canonical_bytes(&raw[0])?);
    drop(source);
    assert!(weak.upgrade().is_some());
    let input = AdmissionInput::Raw(raw[0].clone());
    assert!(retain_admitted_for_pool(&input, admitted)?.is_none());
    assert!(
        weak.upgrade().is_none(),
        "Raw does not retain the decoded capability"
    );
    let mut retained = PoolRetention::default();
    retained.reserve(&input, 0, MAX_POOL_BYTES)?;
    assert_eq!(retained.metadata_bytes, 0);
    retained.release(&input, 0);
    assert_eq!(retained.bytes, 0);
    Ok(())
}

#[test]
fn equal_signed_row_from_another_dictionary_is_not_retained_under_input_budget() -> Result<()> {
    let raws = raw(3);
    let source = apfl_batch(&raws, 8)?;
    let small = apfl_batch(&raws[..1], 8)?;
    let admitted = admitted_apfl_for_test(Arc::new(()), 71, source.clone(), 0)?;
    assert_eq!(admitted.checked().transfer(), small.row(0)?);
    let old = Arc::downgrade(&source);
    drop(source);
    let input = AdmissionInput::Apfl {
        batch: small.clone(),
        index: 0,
    };
    assert!(retain_admitted_for_pool(&input, admitted).is_err());
    assert!(old.upgrade().is_none(), "rejected source must be released");
    let same = admitted_apfl_for_test(Arc::new(()), 71, small, 0)?;
    assert!(retain_admitted_for_pool(&input, same)?.is_some());
    Ok(())
}

#[test]
fn body_capability_reuse_requires_complete_signed_rows_not_canonical_hashes() -> Result<()> {
    let raws = raw(2);
    let source = apfl_batch(&raws, 8)?;
    let (order, mut pending, _) = pool(&source)?;
    let selected = selected_apfl_batch(&order.iter().copied().collect::<Vec<_>>(), &pending, 8)?;
    assert!(!Arc::ptr_eq(&source, &selected));
    assert_eq!(
        admitted_for_body(&body(selected), &pending)?.unwrap().len(),
        2
    );

    let mut changed = raws.clone();
    *changed[0].last_mut().unwrap() ^= 1;
    let changed = apfl_batch(&changed, 8)?;
    assert_eq!(
        changed.row(0)?.canonical_tx_hash()?,
        source.row(0)?.canonical_tx_hash()?
    );
    assert!(admitted_for_body(&body(changed), &pending)?.is_none());

    // Even a correctly retained raw row must take the strict route: retaining
    // a second decoded source merely to make reuse possible is forbidden.
    let first = *order.front().unwrap();
    let entry = pending.get_mut(&first).unwrap();
    entry.input = AdmissionInput::Raw(raws[0].clone());
    assert!(admitted_for_body(&body(apfl_batch(&raws, 8)?), &pending)?.is_none());
    entry_restore(&mut pending, first, source.clone(), 0);
    pending.get_mut(&first).unwrap().admitted = None;
    assert!(admitted_for_body(&body(apfl_batch(&raws, 8)?), &pending)?.is_none());
    Ok(())
}

fn entry_restore(
    pending: &mut BTreeMap<Hash, Pending>,
    hash: Hash,
    batch: Arc<ApflTransferBatch>,
    index: usize,
) {
    pending.get_mut(&hash).unwrap().input = AdmissionInput::Apfl { batch, index };
}

#[test]
fn regrouped_candidate_does_not_pin_retired_pool_dictionaries() -> Result<()> {
    let source = apfl_batch(&raw(3), 8)?;
    let weak = Arc::downgrade(&source);
    let (order, mut pending, mut retained) = pool(&source)?;
    let selected = vec![order[0], order[2]];
    let batch = selected_apfl_batch(&selected, &pending, 8)?;
    let message = body(batch.clone());
    let admitted = admitted_for_body(&message, &pending)?.unwrap();
    // Exercise the same non-cryptographic dictionary rebind used by the
    // pipeline's branded API; owner rejection is tested in that pipeline.
    let checked = admitted.iter().map(|row| row.checked()).collect::<Vec<_>>();
    let candidate = SignatureCheckedBatch::rebind_apfl(
        71,
        batch,
        &checked,
        AuthenticationBudget {
            transactions: 8,
            transaction_bytes: 1024,
            body_bytes: super::super::body_byte_limit(8),
        },
    )?;
    drop(checked);
    drop(admitted);
    drop(source);
    assert!(
        weak.upgrade().is_some(),
        "the live pool still owns its source"
    );
    for hash in order {
        let entry = pending.remove(&hash).unwrap();
        retained.release(&entry.input, entry.retained_metadata_bytes());
        drop(entry);
    }
    assert!(
        weak.upgrade().is_none(),
        "candidate tokens must use only the new dictionary"
    );
    assert_eq!(
        (
            retained.bytes,
            retained.metadata_bytes,
            retained.owners.len()
        ),
        (0, 0, 0)
    );
    assert_eq!(candidate.transactions().len(), 2);
    assert_eq!(candidate.transactions()[1].transfer().nonce, 2);
    assert!(matches!(message, Message::ApflBody { .. }));
    Ok(())
}
