use super::*;
use crate::native_block_seal::commit_v3::{
    decision_target_v3, NovNativeSealDecisionCertificateV3 as Certificate,
    NovNativeSealDecisionVoteV3 as Vote,
};

fn durable_v3_fixture(
    label: &str,
    chain: u64,
) -> (
    TestNodeV1,
    Vec<SigningKey>,
    NovNativeSealValidatorSetV1,
    NovNativeSealQuorumCertificateV1,
) {
    let (node, block, keys, set) = genesis_fixture_v1(label, chain);
    let (proposal, raw) = proposal_and_votes_v1(&node, &block, &keys, &set, 0, 4);
    let qc = NovNativeSealQuorumCertificateV1::from_votes(proposal.subject, &set, raw).unwrap();
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    (node, keys, set, qc)
}

#[test]
fn commit_v3_durable_decision_replays_across_round_and_restart_without_new_signature() {
    use super::native_block_seal_newview::{advance_v1, authority_v1, local_qc_v1};
    let (mut node, block, keys, set) = genesis_fixture_v1("v3-round-replay", 85_410);
    let authority = authority_v1(&node, &set);
    let (first, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
    node.store()
        .persist_local_verified_qc(node.ledger(), &first.qc, &set)
        .unwrap();
    let vote = node
        .store()
        .sign_local_decision_vote_v3(node.ledger(), &first.qc, &set, &keys[0])
        .unwrap();
    advance_v1(&node, &authority, &keys, 0);
    let (next, _) = local_qc_v1(&node, &block, &authority, &keys, 1);
    node.store()
        .persist_local_verified_qc(node.ledger(), &next.qc, &set)
        .unwrap();
    node.reopen_store();
    let before = node
        .store()
        .db
        .iterator(IteratorMode::Start)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        node.store()
            .sign_local_decision_vote_v3(node.ledger(), &next.qc, &set, &keys[0])
            .unwrap(),
        vote
    );
    assert_eq!(
        node.store()
            .db
            .iterator(IteratorMode::Start)
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        before
    );
    let mut votes = vec![vote];
    for key in &keys[1..3] {
        votes.push(
            node.store()
                .sign_local_decision_vote_v3(node.ledger(), &next.qc, &set, key)
                .unwrap(),
        );
    }
    let original = Certificate::from_votes(first.qc, &set, votes.clone()).unwrap();
    let equivalent = Certificate::from_votes(next.qc, &set, votes).unwrap();
    assert!(node
        .store()
        .persist_local_verified_decision_certificate_v3(node.ledger(), &original, &set)
        .unwrap());
    node.reopen_store();
    assert!(!node
        .store()
        .persist_local_verified_decision_certificate_v3(node.ledger(), &equivalent, &set)
        .unwrap());
    assert_eq!(
        node.store()
            .load_decision_certificate_by_height_v3(set.chain_id, set.epoch, 1)
            .unwrap(),
        Some(original)
    );
    assert!(
        !node
            .ledger()
            .load_candidate_record(set.chain_id, block.header.block_hash)
            .unwrap()
            .unwrap()
            .finalized
    );
}

#[test]
fn commit_v3_archive_detects_loss_tampering_and_wrong_height() {
    for fault in 0..6 {
        let (mut node, keys, set, qc) = durable_v3_fixture("v3-archive-loss", 85_440 + fault);
        assert!(node
            .store()
            .load_decision_certificate_by_height_v3(set.chain_id, set.epoch, 1)
            .unwrap()
            .is_none());
        let votes = keys[..3]
            .iter()
            .map(|key| {
                node.store()
                    .sign_local_decision_vote_v3(node.ledger(), &qc, &set, key)
                    .unwrap()
            })
            .collect();
        let cert = Certificate::from_votes(qc.clone(), &set, votes).unwrap();
        let head = node.ledger().load_head(set.chain_id).unwrap();
        let archived = std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                node.store()
                    .persist_local_verified_decision_certificate_v3(node.ledger(), &cert, &set)
                    .unwrap()
            });
            let second = scope.spawn(|| {
                node.store()
                    .persist_local_verified_decision_certificate_v3(node.ledger(), &cert, &set)
                    .unwrap()
            });
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_ne!(archived.0, archived.1);
        node.reopen_store();
        assert_eq!(
            node.store()
                .load_decision_certificate_by_height_v3(set.chain_id, set.epoch, 1)
                .unwrap(),
            Some(cert.clone())
        );
        assert_eq!(node.ledger().load_head(set.chain_id).unwrap(), head);
        assert!(node
            .store()
            .load_commit_certificate_by_height(set.chain_id, set.epoch, 1)
            .is_err());
        assert!(node
            .store()
            .load_commit_certificate_by_height_v2(set.chain_id, set.epoch, 1)
            .is_err());
        let slot =
            crate::native_block_seal::commit::certificate_height_key(set.chain_id, set.epoch, 1);
        let marker = format!("{slot}/decision-v3-certificate-hash");
        match fault {
            0 => node.store().db.delete(slot.as_bytes()).unwrap(),
            1 => node.store().db.delete(marker.as_bytes()).unwrap(),
            2 => node
                .store()
                .db
                .delete(qc_object_key_v1(&qc.qc_hash))
                .unwrap(),
            3 => node
                .store()
                .db
                .put(marker.as_bytes(), serde_json::to_vec(&[0u8; 32]).unwrap())
                .unwrap(),
            4 => {
                let mut bad = cert.clone();
                bad.votes[0].signature[0] ^= 1;
                node.store()
                    .db
                    .put(slot.as_bytes(), serde_json::to_vec(&bad).unwrap())
                    .unwrap();
            }
            _ => {
                let wrong = crate::native_block_seal::commit::certificate_height_key(
                    set.chain_id,
                    set.epoch,
                    2,
                );
                node.store()
                    .db
                    .put(wrong.as_bytes(), serde_json::to_vec(&cert).unwrap())
                    .unwrap();
                assert!(node
                    .store()
                    .load_decision_certificate_by_height_v3(set.chain_id, set.epoch, 2)
                    .is_err());
                continue;
            }
        }
        node.reopen_store();
        assert!(node
            .store()
            .load_decision_certificate_by_height_v3(set.chain_id, set.epoch, 1)
            .is_err());
        assert!(node
            .store()
            .persist_local_verified_decision_certificate_v3(node.ledger(), &cert, &set)
            .is_err());
        assert!(node
            .store()
            .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &keys[3])
            .is_err());
    }
}

#[test]
fn commit_v3_archive_rejects_old_version_and_foreign_execution() {
    let (node, keys, set, qc) = durable_v3_fixture("v3-archive-version", 85_450);
    let raw = keys[..3]
        .iter()
        .map(|k| {
            node.store()
                .sign_local_commit_vote_v2(node.ledger(), &qc, &set, k)
                .unwrap()
        })
        .collect();
    let old = crate::native_block_seal::commit_v2::NovNativeSealCommitCertificateV2::from_votes(
        qc.clone(),
        &set,
        raw,
    )
    .unwrap();
    node.store()
        .persist_local_verified_commit_certificate_v2(node.ledger(), &old, &set)
        .unwrap();
    // Test-only signatures: prove a valid V3 aggregate cannot overwrite V2.
    let raw = keys[..3]
        .iter()
        .map(|k| decision_vote(&qc, &set, k))
        .collect();
    let cert = Certificate::from_votes(qc.clone(), &set, raw).unwrap();
    assert!(node
        .store()
        .load_decision_certificate_by_height_v3(set.chain_id, set.epoch, 1)
        .is_err());
    assert!(node
        .store()
        .persist_local_verified_decision_certificate_v3(node.ledger(), &cert, &set)
        .is_err());
    let mut subject = qc.subject.clone();
    subject.post_state_root[0] ^= 1;
    let other = witness(subject, &set, &keys, 0);
    let raw = keys[..3]
        .iter()
        .map(|k| decision_vote(&other, &set, k))
        .collect();
    let foreign = Certificate::from_votes(other, &set, raw).unwrap();
    assert!(node
        .store()
        .persist_local_verified_decision_certificate_v3(node.ledger(), &foreign, &set)
        .is_err());
    assert_eq!(
        node.store()
            .load_commit_certificate_by_height_v2(set.chain_id, set.epoch, 1)
            .unwrap(),
        Some(old)
    );
}

#[test]
fn commit_v3_durable_lock_versions_are_mutually_exclusive() {
    for old_version in [1, 2, 3] {
        let (mut node, keys, set, qc) = durable_v3_fixture("v3-version-lock", 85_411 + old_version);
        match old_version {
            1 => {
                node.store()
                    .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
                    .unwrap();
            }
            2 => {
                node.store()
                    .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[0])
                    .unwrap();
            }
            _ => {
                node.store()
                    .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &keys[0])
                    .unwrap();
            }
        }
        node.reopen_store();
        if old_version == 3 {
            assert!(node
                .store()
                .sign_local_commit_vote(node.ledger(), &qc, &set, &keys[0])
                .is_err());
            assert!(node
                .store()
                .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[0])
                .is_err());
        } else {
            assert!(node
                .store()
                .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &keys[0])
                .is_err());
        }
    }
}

#[test]
fn commit_v3_durable_signer_rejects_another_execution_and_serializes_replay() {
    let (node, keys, set, qc) = durable_v3_fixture("v3-conflict", 85_430);
    let votes = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            node.store()
                .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &keys[0])
                .unwrap()
        });
        let second = scope.spawn(|| {
            node.store()
                .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &keys[0])
                .unwrap()
        });
        (first.join().unwrap(), second.join().unwrap())
    });
    assert_eq!(votes.0, votes.1);
    let before = node
        .store()
        .db
        .iterator(IteratorMode::Start)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut subject = qc.subject.clone();
    subject.post_state_root[0] ^= 1;
    let other = witness(subject, &set, &keys, 0);
    assert!(node
        .store()
        .sign_local_decision_vote_v3(node.ledger(), &other, &set, &keys[0])
        .is_err());
    assert!(node
        .store()
        .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &SigningKey::from_bytes(&[99; 32]))
        .is_err());
    assert_eq!(
        node.store()
            .db
            .iterator(IteratorMode::Start)
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        before
    );
}

#[test]
fn commit_v3_durable_timeout_and_missing_evidence_fail_closed() {
    for removed in 0..5 {
        let (mut node, keys, set, qc) = durable_v3_fixture("v3-durable-loss", 85_420 + removed);
        let vote = node
            .store()
            .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &keys[0])
            .unwrap();
        for key in &keys[..2] {
            node.store()
                .sign_local_timeout(node.ledger(), &set, 1, 0, key)
                .unwrap();
        }
        assert_eq!(
            node.store()
                .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &keys[0])
                .unwrap(),
            vote
        );
        assert!(node
            .store()
            .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &keys[1])
            .is_err());
        let slot = crate::native_block_seal::commit::lock_key(&qc.subject, vote.validator_id);
        let key = match removed {
            0 => slot,
            1 => format!("{slot}/decision-v3-vote-hash"),
            2 => qc_object_key_v1(&qc.qc_hash),
            3 => round_lock_key_v1(&qc.subject, vote.validator_id),
            _ => height_lock_key_v1(&qc.subject, vote.validator_id),
        };
        node.store().db.delete(key.as_bytes()).unwrap();
        node.reopen_store();
        assert!(node
            .store()
            .sign_local_decision_vote_v3(node.ledger(), &qc, &set, &keys[0])
            .is_err());
    }
}

// Test-only raw signing deliberately supplies cryptographic witnesses, not
// production new-view admission or local durable signing permission.
fn witness(
    mut subject: NovNativeSealSubjectV1,
    set: &NovNativeSealValidatorSetV1,
    keys: &[SigningKey],
    round: u64,
) -> NovNativeSealQuorumCertificateV1 {
    subject.round = round;
    subject.subject_hash = subject_hash_v1(&subject);
    let proposal =
        sign_proposal_v1(subject.clone(), set, &keys[(round as usize) % keys.len()]).unwrap();
    let votes = keys[..3]
        .iter()
        .map(|k| sign_vote_v1(&proposal, set, k).unwrap())
        .collect();
    NovNativeSealQuorumCertificateV1::from_votes(subject, set, votes).unwrap()
}

fn decision_vote(
    qc: &NovNativeSealQuorumCertificateV1,
    set: &NovNativeSealValidatorSetV1,
    key: &SigningKey,
) -> Vote {
    let target_hash = decision_target_v3(qc, set).unwrap();
    let validator_id = validator_id_v1(key.verifying_key().as_bytes());
    let message = [
        b"novovm-native-seal-decision-signing-v3\0".as_slice(),
        &target_hash,
        &validator_id,
    ]
    .concat();
    let signature = key.sign(&message).to_bytes().to_vec();
    let mut hash = Sha256::new();
    hash.update(b"novovm-native-seal-decision-vote-v3\0");
    hash.update(target_hash);
    hash.update(validator_id);
    hash.update(&signature);
    Vote {
        schema: "novovm-native-seal-decision-vote/v3".into(),
        target_hash,
        validator_id,
        signature,
        vote_hash: hash.finalize().into(),
    }
}

#[test]
fn commit_v3_same_decision_across_rounds_forms_quorum_without_changing_v2() {
    let (node, block, keys, set) = genesis_fixture_v1("decision-v3-rounds", 85_401);
    let subject = node
        .store()
        .prepare_local_subject(
            node.ledger(),
            set.chain_id,
            block.header.block_hash,
            &set,
            0,
            None,
        )
        .unwrap();
    let witnesses = (0..3)
        .map(|r| witness(subject.clone(), &set, &keys, r))
        .collect::<Vec<_>>();
    let votes = witnesses
        .iter()
        .zip(&keys)
        .map(|(qc, k)| decision_vote(qc, &set, k))
        .collect::<Vec<_>>();
    for qc in &witnesses {
        assert_eq!(decision_target_v3(qc, &set).unwrap(), votes[0].target_hash);
        assert!(Certificate::from_votes(qc.clone(), &set, votes[..2].to_vec()).is_err());
        assert!(Certificate::from_votes(qc.clone(), &set, vec![votes[0].clone(); 3]).is_err());
        let cert = Certificate::from_votes(qc.clone(), &set, votes.clone()).unwrap();
        cert.verify(&set).unwrap();
        let decoded: Certificate =
            serde_json::from_slice(&serde_json::to_vec(&cert).unwrap()).unwrap();
        decoded.verify(&set).unwrap();
    }
    assert_ne!(
        super::super::commit_v2::commit_target_v2(&witnesses[0], &set).unwrap(),
        super::super::commit_v2::commit_target_v2(&witnesses[1], &set).unwrap()
    );
    let mut invalid = witnesses[0].clone();
    invalid.votes.pop();
    assert!(decision_target_v3(&invalid, &set).is_err());
    let mut bad = Certificate::from_votes(witnesses[0].clone(), &set, votes).unwrap();
    bad.votes[0].signature[0] ^= 1;
    assert!(bad.verify(&set).is_err());
}

#[test]
fn commit_v3_binds_every_subject_field_except_round_and_derived_hash() {
    let (node, block, keys, set) = genesis_fixture_v1("decision-v3-fields", 85_402);
    let subject = node
        .store()
        .prepare_local_subject(
            node.ledger(),
            set.chain_id,
            block.header.block_hash,
            &set,
            0,
            None,
        )
        .unwrap();
    let qc = witness(subject.clone(), &set, &keys, 0);
    let target = decision_target_v3(&qc, &set).unwrap();
    let original = serde_json::to_value(&subject).unwrap();
    for name in original.as_object().unwrap().keys() {
        if name == "round" || name == "subject_hash" {
            continue;
        }
        let mut altered = original.clone();
        match &mut altered[name] {
            serde_json::Value::String(v) => v.push('x'),
            serde_json::Value::Number(v) => *v = (v.as_u64().unwrap() + 1).into(),
            serde_json::Value::Array(v) => v[0] = ((v[0].as_u64().unwrap() + 1) % 256).into(),
            _ => panic!("new subject field needs a mutation test: {name}"),
        }
        let mut changed: NovNativeSealSubjectV1 = serde_json::from_value(altered).unwrap();
        changed.subject_hash = subject_hash_v1(&changed);
        // Invalid domain/codec/dependency changes must reject before targeting.
        if changed.validate(&set).is_err() {
            continue;
        }
        let other = witness(changed, &set, &keys, 0);
        assert_ne!(
            decision_target_v3(&other, &set).unwrap(),
            target,
            "unbound field: {name}"
        );
    }
}

#[test]
fn commit_v3_rejects_relabelled_v2_signatures() {
    let (node, block, keys, set) = genesis_fixture_v1("decision-v3-domain", 85_403);
    let (proposal, votes) = proposal_and_votes_v1(&node, &block, &keys, &set, 0, 3);
    let qc = NovNativeSealQuorumCertificateV1::from_votes(proposal.subject, &set, votes).unwrap();
    node.store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap();
    let old = node
        .store()
        .sign_local_commit_vote_v2(node.ledger(), &qc, &set, &keys[0])
        .unwrap();
    let mut relabelled = decision_vote(&qc, &set, &keys[0]);
    relabelled.signature = old.signature;
    assert!(relabelled.verify(&qc, &set).is_err());
    let mut value = serde_json::to_value(relabelled).unwrap();
    value["unexpected"] = true.into();
    assert!(serde_json::from_value::<Vote>(value).is_err());
}

#[test]
fn commit_v3_uses_weight_and_rejects_certificate_tampering() {
    let (node, block, keys, _) = genesis_fixture_v1("decision-v3-weight", 85_404);
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
    let (proposal, raw) = proposal_and_votes_v1(&node, &block, &keys, &set, 0, 4);
    let qc = NovNativeSealQuorumCertificateV1::from_votes(proposal.subject, &set, raw).unwrap();
    let votes = keys
        .iter()
        .map(|key| decision_vote(&qc, &set, key))
        .collect::<Vec<_>>();
    assert!(Certificate::from_votes(qc.clone(), &set, votes[1..].to_vec()).is_err());
    let cert = Certificate::from_votes(qc, &set, votes[..2].to_vec()).unwrap();
    assert_eq!(cert.signed_weight, 7);
    cert.verify(&set).unwrap();
    for change in 0..7 {
        let mut bad = cert.clone();
        match change {
            0 => bad.signed_weight += 1,
            1 => bad.certificate_hash[0] ^= 1,
            2 => bad.votes.reverse(),
            3 => bad.prepare.votes[0].signature[0] ^= 1,
            4 => bad.votes[0].vote_hash[0] ^= 1,
            5 => bad.votes[0].target_hash[0] ^= 1,
            _ => bad.schema = "novovm-native-seal-commit-certificate/v2".into(),
        }
        assert!(bad.verify(&set).is_err());
    }
}
