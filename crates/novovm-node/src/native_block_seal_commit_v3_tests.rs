use super::*;
use crate::native_block_seal::commit_v3::{
    decision_target_v3, NovNativeSealDecisionCertificateV3 as Certificate,
    NovNativeSealDecisionVoteV3 as Vote,
};

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
