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

#[test]
#[ignore = "explicit packaged AOEM; retained NUAS pool evidence, not asset authorization"]
fn actual_mldsa_uca_body_reuses_only_complete_pool_owned_signed_objects() -> Result<()> {
    use crate::native_pipeline::pipeline::admitted_uca_for_test;
    use novovm_protocol::uca_spend::{
        spend_key_ref_v1, AssetSpendIntentV3, MAX_SIGNED_SPEND_BYTES,
    };
    use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3};
    let library: std::path::PathBuf = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted AOEM library required")?
        .into();
    let facade = novovm_exec::AoemExecFacade::open(&library, Default::default())?;
    let keys = facade.mldsa_keygen_v1(novovm_exec::pq_signature::AoemMldsaParameterSet::MlDsa65)?;
    let signer = facade.asset_spend_signer_v3()?;
    let mut signed = Vec::new();
    for nonce in 0..2 {
        signed.push(signer.sign(
            AssetSpendIntentV3 {
                authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
                asset: 1,
                authority_update: None,
                recipient_creation: None,
                chain_id: 71,
                genesis: [1; 32],
                payer: vec![1; 20],
                authorization_epoch: 1,
                spend_key_ref: spend_key_ref_v1(&keys.public_key)?,
                recipient: vec![2; 20],
                amount: 30,
                max_fee: 50,
                nonce,
                not_before_height: 1,
                expires_at_height: 50,
            },
            &keys.public_key,
            &keys.expanded_secret_key,
        )?);
    }
    let raws = signed
        .iter()
        .map(|s| s.encode())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let limits = AssetSpendBatchLimitsV3 {
        transactions: 16,
        transaction_bytes: MAX_SIGNED_SPEND_BYTES,
        body_bytes: 65536,
    };
    let make = |raws: &[Vec<u8>]| -> Result<_> {
        Ok(Arc::new(AssetSpendBatchV3::from_raw(raws, limits)?))
    };
    let source = make(&raws)?;
    let owner = Arc::new(());
    let mut pending = BTreeMap::new();
    for (index, row) in signed.iter().enumerate() {
        pending.insert(
            uca::hash(row)?,
            Pending {
                input: AdmissionInput::Uca {
                    batch: source.clone(),
                    index,
                },
                admitted: Some(admitted_uca_for_test(
                    owner.clone(),
                    &facade,
                    source.clone(),
                    index,
                )?),
                canonical_len: raws[index].len(),
                signer: row.intent.nonce_identity()?,
                nonce: row.intent.nonce,
                gossip_missing: 0,
            },
        );
    }
    let Message::ApflBody { context, .. } = body(apfl_batch(&raw(1), 8)?) else {
        unreachable!()
    };
    let message = |batch| Message::AssetBody { context, batch };
    let selected = message(make(&raws)?);
    let reused = admitted_for_body(&selected, &pending)?.context("exact UCA rows should reuse")?;
    assert_eq!(reused.len(), 2);
    assert_eq!(reused[0].checked_uca().unwrap().intent().nonce, 0);
    let reversed = message(make(&[raws[1].clone(), raws[0].clone()])?);
    assert_eq!(
        admitted_for_body(&reversed, &pending)?.unwrap()[0]
            .checked_uca()
            .unwrap()
            .intent()
            .nonce,
        1
    );
    // Both rows individually have real valid signature evidence in this pool,
    // but repeating the same intent must decline optional reuse. Otherwise a
    // remote successor reaches rebind's duplicate rejection as a poll error.
    let duplicate = message(make(&[raws[0].clone(), raws[0].clone()])?);
    assert!(
        admitted_for_body(&duplicate, &pending)?.is_none(),
        "duplicate signed rows must remain on the original strict path"
    );
    let mut tampered = signed[0].clone();
    tampered.signature[0] ^= 1;
    assert_eq!(
        uca::hash(&tampered)?,
        uca::hash(&signed[0])?,
        "intent hash is not signature evidence"
    );
    assert!(admitted_for_body(
        &message(make(&[tampered.encode()?, raws[1].clone()])?),
        &pending
    )?
    .is_none());
    let mut changed_account = signed[0].clone();
    changed_account.intent.payer[0] ^= 1;
    assert!(admitted_for_body(
        &message(make(&[changed_account.encode()?, raws[1].clone()])?),
        &pending
    )?
    .is_none());
    let first = uca::hash(&signed[0])?;
    pending.get_mut(&first).unwrap().input = AdmissionInput::Uca {
        batch: make(&raws)?,
        index: 0,
    };
    assert!(
        admitted_for_body(&selected, &pending)?.is_none(),
        "equal bytes cannot pin a different uncharged source"
    );
    pending.get_mut(&first).unwrap().input = AdmissionInput::Uca {
        batch: source,
        index: 0,
    };
    pending.get_mut(&first).unwrap().admitted = None;
    assert!(
        admitted_for_body(&selected, &pending)?.is_none(),
        "missing evidence uses original strict authentication"
    );
    Ok(())
}
