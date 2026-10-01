use super::*;
use crate::native_block_seal::commit_v2::{
    commit_target_v2, NovNativeSealCommitCertificateV2 as Certificate,
};

fn subsets(
    node: &TestNodeV1,
    block: &NovNativeDurableBlockV1,
    keys: &[SigningKey],
    set: &NovNativeSealValidatorSetV1,
) -> Vec<NovNativeSealQuorumCertificateV1> {
    let (proposal, votes) = proposal_and_votes_v1(node, block, keys, set, 0, 4);
    (0..4)
        .map(|omit| {
            NovNativeSealQuorumCertificateV1::from_votes(
                proposal.subject.clone(),
                set,
                votes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != omit)
                    .map(|(_, v)| v.clone())
                    .collect(),
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn commit_v2_four_stores_with_different_qc_subsets_form_one_quorum() {
    let mut nodes = Vec::new();
    let mut witnesses = Vec::new();
    let mut votes = Vec::new();
    let (_, set) = validator_fixture_v1(85_101);
    for index in 0..4 {
        let (node, block, keys, _) = genesis_fixture_v1("commit-v2-four", set.chain_id);
        let qc = subsets(&node, &block, &keys, &set)[index].clone();
        assert!(node
            .store()
            .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[index])
            .is_err());
        node.store()
            .persist_local_verified_qc(node.ledger(), &qc, &set)
            .unwrap();
        votes.push(
            node.store()
                .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[index])
                .unwrap(),
        );
        witnesses.push(qc);
        nodes.push((node, block, keys));
    }
    for pair in witnesses.windows(2) {
        assert_ne!(pair[0].qc_hash, pair[1].qc_hash);
        assert_eq!(
            commit_target_v2(&pair[0], &set).unwrap(),
            commit_target_v2(&pair[1], &set).unwrap()
        );
    }
    assert!(Certificate::from_votes(witnesses[0].clone(), &set, votes[..2].to_vec()).is_err());
    for witness in &witnesses {
        let cert = Certificate::from_votes(witness.clone(), &set, votes[..3].to_vec()).unwrap();
        cert.verify(&set).unwrap();
        let decoded: Certificate =
            serde_json::from_slice(&serde_json::to_vec(&cert).unwrap()).unwrap();
        decoded.verify(&set).unwrap();
        for vote in &votes {
            vote.verify(witness, &set).unwrap();
        }
    }
    for (index, (node, block, keys)) in nodes.iter_mut().enumerate() {
        let before = node.ledger().load_head(set.chain_id).unwrap();
        let archived =
            Certificate::from_votes(witnesses[index].clone(), &set, votes[..3].to_vec()).unwrap();
        assert!(node
            .store()
            .persist_local_verified_commit_certificate_v2(node.ledger(), &archived, &set)
            .unwrap());
        let equivalent = &witnesses[(index + 1) % 4];
        node.store()
            .persist_local_verified_qc(node.ledger(), equivalent, &set)
            .unwrap();
        node.store()
            .sign_local_timeout(node.ledger(), &set, 1, 0, &keys[index])
            .unwrap();
        node.reopen_store();
        let equivalent_certificate =
            Certificate::from_votes(equivalent.clone(), &set, votes[1..].to_vec()).unwrap();
        assert!(!node
            .store()
            .persist_local_verified_commit_certificate_v2(
                node.ledger(),
                &equivalent_certificate,
                &set
            )
            .unwrap());
        assert_eq!(
            node.store()
                .load_commit_certificate_by_height_v2(set.chain_id, set.epoch, 1)
                .unwrap(),
            Some(archived)
        );
        assert!(node
            .store()
            .load_commit_certificate_by_height(set.chain_id, set.epoch, 1)
            .is_err());
        assert_eq!(
            node.store()
                .sign_local_commit_vote_v2(node.ledger(), equivalent, &set, &keys[index])
                .unwrap(),
            votes[index]
        );
        assert_eq!(node.ledger().load_head(set.chain_id).unwrap(), before);
        let record = node
            .ledger()
            .load_candidate_record(set.chain_id, block.header.block_hash)
            .unwrap()
            .unwrap();
        assert!(
            !record.finalized && !record.safe && !record.proof_sealed && !record.chain_canonical
        );
        // A surviving equivalent QC cannot conceal loss of the original witness.
        node.store()
            .db
            .delete(qc_object_key_v1(&witnesses[index].qc_hash))
            .unwrap();
        assert!(node
            .store()
            .load_commit_certificate_by_height_v2(set.chain_id, set.epoch, 1)
            .is_err());
        assert!(node
            .store()
            .persist_local_verified_commit_certificate_v2(
                node.ledger(),
                &equivalent_certificate,
                &set
            )
            .is_err());
        assert!(node
            .store()
            .sign_local_commit_vote_v2(node.ledger(), equivalent, &set, &keys[index])
            .is_err());
    }
}

#[test]
fn commit_v2_weighted_quorum_and_corrupt_archive_are_rejected() {
    let (mut node, block, keys, _) = genesis_fixture_v1("commit-v2-weight", 85_105);
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
    let (proposal, prepare_votes) = proposal_and_votes_v1(&node, &block, &keys, &set, 0, 4);
    let qc = NovNativeSealQuorumCertificateV1::from_votes(proposal.subject, &set, prepare_votes)
        .unwrap();
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    let votes = keys
        .iter()
        .map(|key| {
            node.store()
                .sign_local_commit_vote_v2(node.ledger(), &qc, &set, key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(Certificate::from_votes(qc.clone(), &set, votes[1..].to_vec()).is_err());
    let certificate = Certificate::from_votes(qc, &set, votes[..2].to_vec()).unwrap();
    assert_eq!(certificate.signed_weight, 7);
    node.store()
        .persist_local_verified_commit_certificate_v2(node.ledger(), &certificate, &set)
        .unwrap();
    let key = crate::native_block_seal::commit::certificate_height_key(set.chain_id, set.epoch, 1);
    let wrong_height =
        crate::native_block_seal::commit::certificate_height_key(set.chain_id, set.epoch, 2);
    node.store()
        .db
        .put(
            wrong_height.as_bytes(),
            serde_json::to_vec(&certificate).unwrap(),
        )
        .unwrap();
    assert!(node
        .store()
        .load_commit_certificate_by_height_v2(set.chain_id, set.epoch, 2)
        .is_err());
    let mut bad = certificate.clone();
    bad.votes[0].signature[0] ^= 1;
    node.store()
        .db
        .put(key.as_bytes(), serde_json::to_vec(&bad).unwrap())
        .unwrap();
    node.reopen_store();
    assert!(node
        .store()
        .load_commit_certificate_by_height_v2(set.chain_id, set.epoch, 1)
        .is_err());
    assert!(node
        .store()
        .persist_local_verified_commit_certificate_v2(node.ledger(), &certificate, &set)
        .is_err());
}

#[test]
fn commit_v2_never_reinterprets_v1_locks_or_signatures() {
    for v2_first in [false, true] {
        let (mut node, block, keys, set) = genesis_fixture_v1("commit-v2-version", 85_102);
        let qc = subsets(&node, &block, &keys, &set)[0].clone();
        node.store()
            .persist_local_verified_qc(node.ledger(), &qc, &set)
            .unwrap();
        if v2_first {
            node.store()
                .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[0])
                .unwrap();
            node.reopen_store();
            assert!(node
                .store()
                .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
                .is_err());
        } else {
            let old = node
                .store()
                .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
                .unwrap();
            node.reopen_store();
            assert!(node
                .store()
                .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[0])
                .is_err());
            let mut vote = node
                .store()
                .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[1])
                .unwrap();
            vote.signature = old.signature;
            vote.validator_id = old.validator_id;
            assert!(vote.verify(&qc, &set).is_err());
        }
    }
}

#[test]
fn commit_v2_rejects_bad_witnesses_targets_weights_and_signatures() {
    let (node, block, keys, set) = genesis_fixture_v1("commit-v2-negative", 85_103);
    let qc = subsets(&node, &block, &keys, &set)[0].clone();
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    let votes = keys[..3]
        .iter()
        .map(|key| {
            node.store()
                .sign_local_commit_vote_v2(node.ledger(), &qc, &set, key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let good = Certificate::from_votes(qc.clone(), &set, votes.clone()).unwrap();
    for index in 0..7 {
        let mut bad = good.clone();
        match index {
            0 => bad.votes[0].signature[0] ^= 1,
            1 => bad.votes[0].target_hash[0] ^= 1,
            2 => bad.signed_weight += 1,
            3 => bad.certificate_hash[0] ^= 1,
            4 => bad.prepare.votes[0].signature[0] ^= 1,
            5 => bad.votes.reverse(),
            _ => bad.schema = "novovm-native-seal-commit-certificate/v1".into(),
        }
        assert!(bad.verify(&set).is_err());
    }
    assert!(Certificate::from_votes(qc.clone(), &set, vec![votes[0].clone(); 3]).is_err());
    let mut insufficient = qc.clone();
    insufficient.votes.pop();
    assert!(commit_target_v2(&insufficient, &set).is_err());
    // Even valid prepare signatures over another round or state cannot reuse
    // these confirmations. No production signing guards are bypassed here.
    for change_round in [false, true] {
        let mut subject = qc.subject.clone();
        if change_round {
            subject.round += 1;
        } else {
            subject.post_state_root[0] ^= 1;
        }
        subject.subject_hash = subject_hash_v1(&subject);
        let proposal = sign_proposal_v1(subject.clone(), &set, &keys[0]).unwrap();
        let raw = keys[..3]
            .iter()
            .map(|key| sign_vote_v1(&proposal, &set, key).unwrap())
            .collect();
        let other = NovNativeSealQuorumCertificateV1::from_votes(subject, &set, raw).unwrap();
        assert_ne!(
            commit_target_v2(&other, &set).unwrap(),
            votes[0].target_hash
        );
        assert!(Certificate::from_votes(other, &set, votes.clone()).is_err());
    }
}

#[test]
fn commit_v2_timeout_and_missing_safety_locks_fail_closed() {
    let (mut node, block, keys, set) = genesis_fixture_v1("commit-v2-locks", 85_104);
    let qc = subsets(&node, &block, &keys, &set)[0].clone();
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    node.store()
        .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[0])
        .unwrap();
    node.store()
        .sign_local_timeout(node.ledger(), &set, 1, 0, &keys[1])
        .unwrap();
    assert!(node
        .store()
        .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[1])
        .is_err());
    let id = validator_id_v1(keys[0].verifying_key().as_bytes());
    node.store()
        .db
        .delete(height_lock_key_v1(&qc.subject, id))
        .unwrap();
    node.reopen_store();
    assert!(node
        .store()
        .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[0])
        .is_err());
    let slot = crate::native_block_seal::commit::lock_key(&qc.subject, id);
    node.store().db.put(slot.as_bytes(), b"broken").unwrap();
    assert!(node
        .store()
        .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[0])
        .is_err());
}
