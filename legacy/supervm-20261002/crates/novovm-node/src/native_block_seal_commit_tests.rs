use super::*;
use crate::native_block_seal::commit::{
    NovNativeSealCommitCertificateV1 as Certificate, NovNativeSealCommitVoteV1,
};

fn prepare(
    node: &TestNodeV1,
    block: &NovNativeDurableBlockV1,
    keys: &[SigningKey],
    set: &NovNativeSealValidatorSetV1,
) -> NovNativeSealQuorumCertificateV1 {
    let (proposal, votes) = proposal_and_votes_v1(node, block, keys, set, 0, keys.len());
    NovNativeSealQuorumCertificateV1::from_votes(proposal.subject, set, votes).unwrap()
}

fn commit_votes(
    node: &TestNodeV1,
    qc: &NovNativeSealQuorumCertificateV1,
    keys: &[SigningKey],
    set: &NovNativeSealValidatorSetV1,
) -> Vec<NovNativeSealCommitVoteV1> {
    keys.iter()
        .map(|key| {
            node.store()
                .sign_local_commit_vote(node.ledger(), qc, set, key)
                .unwrap()
        })
        .collect()
}

#[test]
fn commit_quorum_is_distinct_durable_and_does_not_finalize() {
    let (mut node, block, keys, set) = genesis_fixture_v1("commit-restart", 85_001);
    let qc = prepare(&node, &block, &keys[..3], &set);
    let before = node.ledger().load_head(set.chain_id).unwrap();
    let outbox = node
        .store()
        .load_pending_outbox(set.chain_id, set.validators[0].validator_id, 16)
        .unwrap();
    assert!(node
        .store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
        .is_err());
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    let votes = commit_votes(&node, &qc, &keys[..3], &set);
    assert!(Certificate::from_votes(qc.clone(), &set, votes[..2].to_vec()).is_err());
    let certificate = Certificate::from_votes(qc.clone(), &set, votes.clone()).unwrap();
    certificate.verify(&set).unwrap();
    assert_eq!(certificate.signed_weight, 3);
    assert!(node
        .store()
        .persist_local_verified_commit_certificate(node.ledger(), &certificate, &set)
        .unwrap());
    assert!(!node
        .store()
        .persist_local_verified_commit_certificate(node.ledger(), &certificate, &set)
        .unwrap());
    let mut reversed = votes.clone();
    reversed.reverse();
    assert_eq!(
        Certificate::from_votes(qc.clone(), &set, reversed).unwrap(),
        certificate
    );
    node.reopen_store();
    assert_eq!(
        node.store()
            .load_commit_certificate_by_height(set.chain_id, set.epoch, qc.subject.height)
            .unwrap(),
        Some(certificate.clone())
    );
    assert_eq!(commit_votes(&node, &qc, &keys[..3], &set), votes);
    let decoded: Certificate =
        serde_json::from_slice(&serde_json::to_vec(&certificate).unwrap()).unwrap();
    decoded.verify(&set).unwrap();
    assert_eq!(node.ledger().load_head(set.chain_id).unwrap(), before);
    let record = node
        .ledger()
        .load_candidate_record(set.chain_id, block.header.block_hash)
        .unwrap()
        .unwrap();
    assert!(!record.finalized && !record.safe && !record.proof_sealed && !record.chain_canonical);
    assert_eq!(
        node.store()
            .load_pending_outbox(set.chain_id, set.validators[0].validator_id, 16)
            .unwrap(),
        outbox
    );
}

#[test]
fn commit_certificate_storage_rejects_replacement_and_corruption() {
    let (mut node, block, keys, set) = genesis_fixture_v1("commit-archive", 85_020);
    let qc = prepare(&node, &block, &keys, &set);
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    let votes = commit_votes(&node, &qc, &keys, &set);
    let certificate = Certificate::from_votes(qc.clone(), &set, votes[..3].to_vec()).unwrap();
    let alternate = Certificate::from_votes(qc.clone(), &set, votes).unwrap();
    let slot = format!(
        "{KEY_PREFIX_V1}commit/certificate/{:020}/{:020}/{:020}",
        set.chain_id, set.epoch, qc.subject.height
    );
    let mut bad = certificate.clone();
    bad.votes[0].signature[0] ^= 1;
    assert!(node
        .store()
        .persist_local_verified_commit_certificate(node.ledger(), &bad, &set)
        .is_err());
    assert!(node.store().db.get(slot.as_bytes()).unwrap().is_none());
    node.store()
        .persist_local_verified_commit_certificate(node.ledger(), &certificate, &set)
        .unwrap();
    assert!(node
        .store()
        .persist_local_verified_commit_certificate(node.ledger(), &alternate, &set)
        .is_err());
    node.reopen_store();
    assert_eq!(
        node.store()
            .load_commit_certificate_by_height(set.chain_id, set.epoch, qc.subject.height)
            .unwrap(),
        Some(certificate.clone())
    );
    // Valid bytes under a different height key cannot masquerade as that height.
    let other_slot = format!(
        "{KEY_PREFIX_V1}commit/certificate/{:020}/{:020}/{:020}",
        set.chain_id,
        set.epoch,
        qc.subject.height + 1
    );
    node.store()
        .db
        .put(
            other_slot.as_bytes(),
            serde_json::to_vec(&certificate).unwrap(),
        )
        .unwrap();
    assert!(node
        .store()
        .load_commit_certificate_by_height(set.chain_id, set.epoch, qc.subject.height + 1)
        .is_err());
    node.store()
        .db
        .put(slot.as_bytes(), serde_json::to_vec(&bad).unwrap())
        .unwrap();
    node.reopen_store();
    assert!(node
        .store()
        .load_commit_certificate_by_height(set.chain_id, set.epoch, qc.subject.height)
        .is_err());
    assert!(node
        .store()
        .persist_local_verified_commit_certificate(node.ledger(), &certificate, &set)
        .is_err());
    // Restoring certificate bytes does not excuse a missing durable dependency.
    node.store()
        .db
        .put(slot.as_bytes(), serde_json::to_vec(&certificate).unwrap())
        .unwrap();
    node.store()
        .db
        .delete(qc_object_key_v1(&qc.qc_hash).as_bytes())
        .unwrap();
    node.reopen_store();
    assert!(node
        .store()
        .load_commit_certificate_by_height(set.chain_id, set.epoch, qc.subject.height)
        .is_err());
}

#[test]
fn commit_rejects_signature_reuse_tampering_and_duplicate_signers() {
    let (node, block, keys, set) = genesis_fixture_v1("commit-negative", 85_002);
    let qc = prepare(&node, &block, &keys[..3], &set);
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    let votes = commit_votes(&node, &qc, &keys[..3], &set);
    let good = Certificate::from_votes(qc.clone(), &set, votes.clone()).unwrap();
    let mut bad = votes[0].clone();
    bad.signature = qc
        .votes
        .iter()
        .find(|v| v.validator_id == bad.validator_id)
        .unwrap()
        .signature
        .clone();
    assert!(bad.verify(&qc, &set).is_err());
    for index in 0..5 {
        let mut bad = good.clone();
        match index {
            0 => bad.votes[0].signature[0] ^= 1,
            1 => bad.votes[0].prepare_qc_hash[0] ^= 1,
            2 => bad.prepare.subject.post_state_root[0] ^= 1,
            3 => bad.signed_weight += 1,
            _ => bad.certificate_hash[0] ^= 1,
        }
        assert!(bad.verify(&set).is_err());
    }
    assert!(Certificate::from_votes(qc.clone(), &set, vec![votes[0].clone(); 3]).is_err());
    let mut bad = good;
    bad.votes.swap(0, 1);
    assert!(bad.verify(&set).is_err());
    let (_, wrong_set) = validator_fixture_v1(set.chain_id + 1);
    assert!(votes[0].verify(&qc, &wrong_set).is_err());
    assert!(node
        .store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &SigningKey::from_bytes(&[99; 32]))
        .is_err());
}

#[test]
fn commit_pins_exact_qc_even_for_same_block_after_restart() {
    let (mut node, block, keys, set) = genesis_fixture_v1("commit-pin", 85_003);
    let all = prepare(&node, &block, &keys, &set);
    let other = NovNativeSealQuorumCertificateV1::from_votes(
        all.subject.clone(),
        &set,
        all.votes[..3].to_vec(),
    )
    .unwrap();
    node.store()
        .persist_local_verified_qc(node.ledger(), &all, &set)
        .unwrap();
    node.store()
        .persist_local_verified_qc(node.ledger(), &other, &set)
        .unwrap();
    node.store()
        .sign_local_commit_vote(node.ledger(), &all, &set, &keys[0])
        .unwrap();
    node.reopen_store();
    assert!(node
        .store()
        .sign_local_commit_vote(node.ledger(), &other, &set, &keys[0])
        .is_err());
    // Other signatures over another valid QC cannot be pooled into one quorum.
    let mut mixed = commit_votes(&node, &all, &keys[..2], &set);
    mixed.push(
        node.store()
            .sign_local_commit_vote(node.ledger(), &other, &set, &keys[2])
            .unwrap(),
    );
    assert!(Certificate::from_votes(all, &set, mixed).is_err());
}

#[test]
fn commit_timeout_fences_new_signatures_but_preserves_exact_replay() {
    let (mut node, block, keys, set) = genesis_fixture_v1("commit-timeout", 85_004);
    let qc = prepare(&node, &block, &keys[..3], &set);
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    let vote = node
        .store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
        .unwrap();
    for key in &keys[..2] {
        node.store()
            .sign_local_timeout(node.ledger(), &set, 1, 0, key)
            .unwrap();
    }
    node.reopen_store();
    assert_eq!(
        node.store()
            .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
            .unwrap(),
        vote
    );
    assert!(node
        .store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[1])
        .is_err());
}

#[test]
fn commit_corrupt_lock_or_missing_qc_stops_replay() {
    let (mut node, block, keys, set) = genesis_fixture_v1("commit-corruption", 85_005);
    let qc = prepare(&node, &block, &keys[..3], &set);
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    node.store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
        .unwrap();
    let key = format!(
        "{KEY_PREFIX_V1}commit/local/{:020}/{:020}/{:020}/{}",
        set.chain_id,
        set.epoch,
        1,
        hex_v1(&validator_id_v1(keys[0].verifying_key().as_bytes()))
    );
    node.store().db.put(key.as_bytes(), b"broken").unwrap();
    node.reopen_store();
    assert!(node
        .store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
        .is_err());
    node.store()
        .db
        .delete(qc_object_key_v1(&qc.qc_hash))
        .unwrap();
    assert!(node
        .store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[1])
        .is_err());
}

#[test]
fn commit_quorum_counts_weight_not_number_of_signatures() {
    let (node, block, keys, _) = genesis_fixture_v1("commit-weight", 85_006);
    let set = NovNativeSealValidatorSetV1::new(
        block.header.chain_id,
        1,
        1,
        keys.iter()
            .zip([4, 3, 2, 1])
            .map(|(key, weight)| {
                NovNativeSealValidatorV1::new(*key.verifying_key().as_bytes(), weight).unwrap()
            })
            .collect(),
    )
    .unwrap();
    let qc = prepare(&node, &block, &keys, &set);
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    let votes = commit_votes(&node, &qc, &keys, &set);
    assert!(Certificate::from_votes(qc.clone(), &set, votes[1..].to_vec()).is_err());
    assert_eq!(
        Certificate::from_votes(qc, &set, votes[..2].to_vec())
            .unwrap()
            .signed_weight,
        7
    );
}

#[test]
fn commit_rejects_competing_qc_at_another_round() {
    let (mut node, block, keys, set) = genesis_fixture_v1("commit-conflict", 85_007);
    let qc = prepare(&node, &block, &keys[..3], &set);
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    node.store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
        .unwrap();
    let certificate =
        Certificate::from_votes(qc.clone(), &set, commit_votes(&node, &qc, &keys[..3], &set))
            .unwrap();
    // Deliberately Byzantine fixture: bypass local signing locks to manufacture
    // conflicting signed evidence. This is not an executable candidate fixture.
    let mut subject = qc.subject.clone();
    subject.round += 1;
    subject.block_hash[0] ^= 1;
    // A competing height-one block is also a competing genesis. Keep the
    // synthetic subject structurally valid so the conflict guard is exercised.
    subject.genesis_block_hash = subject.block_hash;
    subject.network_domain_commitment = network_domain_commitment_v1(
        subject.chain_id,
        &subject.genesis_block_hash,
        &subject.protocol_config_commitment,
    );
    subject.inline_body_commitment = inline_body_commitment_v1(
        subject.chain_id,
        subject.height,
        &subject.block_hash,
        &subject.ordered_tx_root,
        &subject.body_digest,
        subject.body_bytes,
        subject.tx_count,
    );
    subject.subject_hash = subject_hash_v1(&subject);
    let proposal = sign_proposal_v1(subject.clone(), &set, &keys[0]).unwrap();
    let votes = keys[..3]
        .iter()
        .map(|key| sign_vote_v1(&proposal, &set, key).unwrap())
        .collect();
    let other = NovNativeSealQuorumCertificateV1::from_votes(subject, &set, votes).unwrap();
    // Inject authenticated conflicting evidence in the test store only.
    let mut batch = RocksDbWriteBatch::default();
    put_json_v1(
        &mut batch,
        proposal_object_key_v1(&proposal.proposal_hash).as_bytes(),
        &proposal,
        "test proposal",
    )
    .unwrap();
    put_json_v1(
        &mut batch,
        qc_object_key_v1(&other.qc_hash).as_bytes(),
        &other,
        "test QC",
    )
    .unwrap();
    node.store()
        .stage_qc_index_v1(
            &mut batch,
            qc_height_index_key_v1(set.chain_id, set.epoch, 1),
            "height",
            set.chain_id,
            set.epoch,
            1,
            [0; 32],
            other.qc_hash,
        )
        .unwrap();
    write_sync_v1(&node.store().db, batch).unwrap();
    node.reopen_store();
    let error = node
        .store()
        .persist_local_verified_commit_certificate(node.ledger(), &certificate, &set)
        .unwrap_err();
    assert!(
        error.to_string().contains("competing prepare QC"),
        "{error:#}"
    );
    assert!(node
        .store()
        .load_commit_certificate_by_height(set.chain_id, set.epoch, qc.subject.height)
        .unwrap()
        .is_none());
    for key in &keys[..2] {
        let error = node
            .store()
            .sign_local_commit_vote(node.ledger(), &qc, &set, key)
            .unwrap_err();
        assert!(
            error.to_string().contains("competing prepare QC"),
            "{error:#}"
        );
    }
}

#[test]
fn commit_missing_shared_safety_lock_stops_replay() {
    let (mut node, block, keys, set) = genesis_fixture_v1("commit-missing-lock", 85_008);
    let qc = prepare(&node, &block, &keys[..3], &set);
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    node.store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
        .unwrap();
    let id = validator_id_v1(keys[0].verifying_key().as_bytes());
    node.store()
        .db
        .delete(height_lock_key_v1(&qc.subject, id))
        .unwrap();
    node.reopen_store();
    assert!(node
        .store()
        .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
        .is_err());
}
