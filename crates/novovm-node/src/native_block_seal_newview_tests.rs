// Synthetic-key tests; no running validator, network, or AOEM engine is used.
use super::*;
use crate::native_block_seal::newview::{
    NovNativeSealNewViewCertificateV1, NovNativeSealNewViewObservationV1, NovNativeSealNewViewQcV1,
    CERTIFICATE_SCHEMA_V1, OBSERVATION_SCHEMA_V1,
};
use crate::native_block_seal::timeout::{
    NovNativeSealTimeoutCertificateV1, NovNativeSealTimeoutContextV1,
};
use crate::native_block_seal_overlay::{
    NovNativeSealEpochAuthorityV1, NovNativeSealValidatorTransportBindingV1,
    NOV_NATIVE_SEAL_OVERLAY_MAX_ROUND_V1,
};

fn authority_v1(
    node: &TestNodeV1,
    set: &NovNativeSealValidatorSetV1,
) -> NovNativeSealEpochAuthorityV1 {
    let bindings = set
        .validators
        .iter()
        .enumerate()
        .map(
            |(index, validator)| NovNativeSealValidatorTransportBindingV1 {
                validator_id: validator.validator_id,
                transport_peer_id: format!("{:064x}", index + 1),
            },
        )
        .collect();
    NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(
        node.ledger(),
        set.clone(),
        bindings,
    )
    .unwrap()
}

fn leader_key_v1<'a>(
    authority: &NovNativeSealEpochAuthorityV1,
    height: u64,
    round: u64,
    keys: &'a [SigningKey],
) -> &'a SigningKey {
    let index = ((height - authority.activation_height + round)
        % authority.validator_set.validators.len() as u64) as usize;
    let leader = authority.validator_set.validators[index].validator_id;
    keys.iter()
        .find(|key| validator_id_v1(key.verifying_key().as_bytes()) == leader)
        .unwrap()
}

fn local_qc_v1(
    node: &TestNodeV1,
    block: &NovNativeDurableBlockV1,
    authority: &NovNativeSealEpochAuthorityV1,
    keys: &[SigningKey],
    round: u64,
) -> (NovNativeSealNewViewQcV1, Vec<NovNativeSealVoteV1>) {
    let proposal = node
        .store()
        .sign_local_proposal(
            node.ledger(),
            &NovNativeSealLocalProposalRequestV1 {
                chain_id: authority.chain_id,
                block_hash: block.header.block_hash,
                round,
                justify_qc_hash: None,
            },
            &authority.validator_set,
            leader_key_v1(authority, block.header.height, round, keys),
        )
        .unwrap();
    let votes = keys
        .iter()
        .map(|key| {
            node.store()
                .sign_local_vote(node.ledger(), &proposal, &authority.validator_set, key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let qc = NovNativeSealQuorumCertificateV1::from_votes(
        proposal.subject.clone(),
        &authority.validator_set,
        votes[..3].to_vec(),
    )
    .unwrap();
    (NovNativeSealNewViewQcV1 { proposal, qc }, votes)
}

fn advance_v1(
    node: &TestNodeV1,
    authority: &NovNativeSealEpochAuthorityV1,
    keys: &[SigningKey],
    round: u64,
) -> (
    NovNativeSealTimeoutContextV1,
    NovNativeSealTimeoutCertificateV1,
) {
    node.store()
        .start_round_tracking(node.ledger(), &authority.validator_set, 1)
        .unwrap();
    let votes = keys
        .iter()
        .map(|key| {
            node.store()
                .sign_local_timeout(node.ledger(), &authority.validator_set, 1, round, key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let tc = NovNativeSealTimeoutCertificateV1 {
        context: votes[0].context.clone(),
        votes,
    };
    let state = node
        .store()
        .advance_round_tracking(node.ledger(), &authority.validator_set, &tc)
        .unwrap();
    (state.current, tc)
}

fn observations_v1(
    node: &TestNodeV1,
    authority: &NovNativeSealEpochAuthorityV1,
    keys: &[SigningKey],
    round: u64,
) -> Vec<NovNativeSealNewViewObservationV1> {
    let mut observations = keys
        .iter()
        .map(|key| {
            node.store()
                .sign_local_new_view(node.ledger(), authority, 1, round, key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    observations.sort_by_key(|observation| observation.validator_id);
    observations
}

fn certificate_v1(
    authority: &NovNativeSealEpochAuthorityV1,
    context: NovNativeSealTimeoutContextV1,
    previous_timeout: NovNativeSealTimeoutCertificateV1,
    observations: Vec<NovNativeSealNewViewObservationV1>,
) -> NovNativeSealNewViewCertificateV1 {
    NovNativeSealNewViewCertificateV1 {
        schema: CERTIFICATE_SCHEMA_V1.into(),
        authority_commitment: authority.authority_commitment,
        context,
        previous_timeout,
        observations,
    }
}

// These raw signing helpers intentionally bypass durable signing to construct
// Byzantine messages whose signatures are valid but whose evidence is unsafe.
fn raw_qc_v1(
    subject: NovNativeSealSubjectV1,
    authority: &NovNativeSealEpochAuthorityV1,
    keys: &[SigningKey],
    proposer: &SigningKey,
) -> NovNativeSealNewViewQcV1 {
    let proposal = sign_proposal_v1(subject, &authority.validator_set, proposer).unwrap();
    let votes = keys[..3]
        .iter()
        .map(|key| sign_vote_v1(&proposal, &authority.validator_set, key).unwrap())
        .collect();
    let qc = NovNativeSealQuorumCertificateV1::from_votes(
        proposal.subject.clone(),
        &authority.validator_set,
        votes,
    )
    .unwrap();
    NovNativeSealNewViewQcV1 { proposal, qc }
}

fn raw_observation_v1(
    context: &NovNativeSealTimeoutContextV1,
    authority: &NovNativeSealEpochAuthorityV1,
    highest_qc: Option<NovNativeSealNewViewQcV1>,
    key: &SigningKey,
) -> NovNativeSealNewViewObservationV1 {
    let mut observation = NovNativeSealNewViewObservationV1 {
        schema: OBSERVATION_SCHEMA_V1.into(),
        authority_commitment: authority.authority_commitment,
        context: context.clone(),
        highest_qc,
        validator_id: validator_id_v1(key.verifying_key().as_bytes()),
        signature: Vec::new(),
    };
    observation.signature = key.sign(&observation.message()).to_bytes().to_vec();
    observation
}

#[test]
fn native_seal_new_view_requires_durable_current_round_and_previous_tc() {
    let (node, _, keys, set) = genesis_fixture_v1("new-view-prerequisites", 83_001);
    let authority = authority_v1(&node, &set);
    for round in [0, 1, u64::MAX] {
        assert!(node
            .store()
            .sign_local_new_view(node.ledger(), &authority, 1, round, &keys[0])
            .is_err());
    }
    node.store()
        .start_round_tracking(node.ledger(), &set, 1)
        .unwrap();
    assert!(node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
        .is_err());
    let (expected, _) = advance_v1(&node, &authority, &keys, 0);
    let observation = node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
        .unwrap();
    observation.verify(&expected, &authority).unwrap();
    assert!(observation.highest_qc.is_none());
    assert!(node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 2, &keys[0])
        .is_err());
    assert!(node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 2, 1, &keys[0])
        .is_err());
    let outsider = SigningKey::from_bytes(&[0x93; 32]);
    assert!(node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &outsider)
        .is_err());
    let readonly = NovNativeBlockSealStoreV1::open_existing_read_only(node.store().path())
        .unwrap()
        .unwrap();
    assert!(readonly
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[1])
        .is_err());
}

#[test]
fn native_seal_new_view_quorum_without_qc_rejects_tampering_and_domain_replay() {
    let (node, _, keys, set) = genesis_fixture_v1("new-view-quorum", 83_002);
    let authority = authority_v1(&node, &set);
    let (expected, tc) = advance_v1(&node, &authority, &keys, 0);
    let observations = observations_v1(&node, &authority, &keys[..3], 1);
    let certificate = certificate_v1(&authority, expected.clone(), tc, observations);
    assert!(certificate.verify(&expected, &authority).unwrap().is_none());
    let mut bad = certificate.clone();
    bad.observations.pop();
    assert!(bad.verify(&expected, &authority).is_err());
    bad = certificate.clone();
    bad.observations[1] = bad.observations[0].clone();
    assert!(bad.verify(&expected, &authority).is_err());
    bad = certificate.clone();
    bad.observations[0].signature[0] ^= 1;
    assert!(bad.verify(&expected, &authority).is_err());
    bad = certificate.clone();
    bad.previous_timeout.votes.truncate(2);
    assert!(bad.verify(&expected, &authority).is_err());
    bad = certificate.clone();
    bad.previous_timeout.context.round += 1;
    assert!(bad.verify(&expected, &authority).is_err());
    bad = certificate.clone();
    bad.authority_commitment[0] ^= 1;
    assert!(bad.verify(&expected, &authority).is_err());
    for field in ["chain_id", "epoch", "height", "round"] {
        let mut wrong = serde_json::to_value(&expected).unwrap();
        wrong[field] = serde_json::json!(wrong[field].as_u64().unwrap() + 1);
        let wrong: NovNativeSealTimeoutContextV1 = serde_json::from_value(wrong).unwrap();
        assert!(certificate.verify(&wrong, &authority).is_err(), "{field}");
    }
    for field in [
        "genesis_block_hash",
        "protocol_config_commitment",
        "validator_set_hash",
    ] {
        let mut wrong = serde_json::to_value(&expected).unwrap();
        wrong[field][0] = serde_json::json!(wrong[field][0].as_u64().unwrap() ^ 1);
        let wrong: NovNativeSealTimeoutContextV1 = serde_json::from_value(wrong).unwrap();
        assert!(certificate.verify(&wrong, &authority).is_err(), "{field}");
    }
    let mut unknown = serde_json::to_value(&certificate).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<NovNativeSealNewViewCertificateV1>(unknown).is_err());
    let mut unknown = serde_json::to_value(&certificate.observations[0]).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<NovNativeSealNewViewObservationV1>(unknown).is_err());
}

#[test]
fn native_seal_new_view_counts_weight_not_observation_count() {
    let (node, _, keys, _) = genesis_fixture_v1("new-view-weighted", 83_003);
    let validators = keys
        .iter()
        .zip([3, 1, 1, 1])
        .map(|(key, weight)| {
            NovNativeSealValidatorV1::new(*key.verifying_key().as_bytes(), weight).unwrap()
        })
        .collect();
    let set = NovNativeSealValidatorSetV1::new(83_003, 1, 1, validators).unwrap();
    assert_eq!(set.quorum_weight, 5);
    let authority = authority_v1(&node, &set);
    let (expected, tc) = advance_v1(&node, &authority, &keys, 0);
    let low = observations_v1(&node, &authority, &keys[1..], 1);
    let mut certificate = certificate_v1(&authority, expected.clone(), tc, low);
    assert!(certificate.verify(&expected, &authority).is_err());
    certificate.observations = observations_v1(&node, &authority, &keys[..3], 1);
    assert!(certificate.verify(&expected, &authority).unwrap().is_none());
}

#[test]
fn native_seal_new_view_highest_qc_keeps_height_locks_and_finality_unchanged() {
    let (node, block, keys, set) = genesis_fixture_v1("new-view-highest", 83_004);
    let authority = authority_v1(&node, &set);
    let (first, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
    node.store()
        .persist_local_verified_qc(node.ledger(), &first.qc, &set)
        .unwrap();
    advance_v1(&node, &authority, &keys, 0);
    let (second, _) = local_qc_v1(&node, &block, &authority, &keys, 1);
    node.store()
        .persist_local_verified_qc(node.ledger(), &second.qc, &set)
        .unwrap();
    let before_head = node.ledger().load_head(set.chain_id).unwrap();
    let before_candidate = node
        .ledger()
        .load_candidate_record(set.chain_id, block.header.block_hash)
        .unwrap();
    let signer = validator_id_v1(keys[0].verifying_key().as_bytes());
    let lock_key = height_lock_key_v1(&second.proposal.subject, signer);
    let before_lock = node.store().db.get(lock_key.as_bytes()).unwrap().unwrap();
    let (expected, tc) = advance_v1(&node, &authority, &keys, 1);
    let observations = observations_v1(&node, &authority, &keys[..3], 2);
    assert!(observations
        .iter()
        .all(|v| v.highest_qc.as_ref() == Some(&second)));
    let certificate = certificate_v1(&authority, expected.clone(), tc, observations);
    assert_eq!(
        certificate.verify(&expected, &authority).unwrap(),
        Some(second.clone())
    );
    assert_eq!(
        node.store().db.get(lock_key.as_bytes()).unwrap().unwrap(),
        before_lock
    );
    assert_eq!(node.ledger().load_head(set.chain_id).unwrap(), before_head);
    assert_eq!(
        node.ledger()
            .load_candidate_record(set.chain_id, block.header.block_hash)
            .unwrap(),
        before_candidate
    );
    let candidate = before_candidate.unwrap();
    assert!(
        !candidate.proof_sealed
            && !candidate.chain_canonical
            && !candidate.safe
            && !candidate.finalized
    );
    let mut competing = second.proposal.subject.clone();
    competing.round = 2;
    competing.block_hash[0] ^= 1;
    assert!(node
        .store()
        .prepare_safety_locks_v1(&competing, signer)
        .is_err());
    assert_eq!(NOV_NATIVE_SEAL_OVERLAY_MAX_ROUND_V1, 0);
    assert!(authority.expected_leader(1, 1).is_err());
}

#[test]
fn native_seal_new_view_equivalent_qc_subsets_are_not_conflicting_candidates() {
    let (node, block, keys, set) = genesis_fixture_v1("new-view-subsets", 83_005);
    let authority = authority_v1(&node, &set);
    let (first, votes) = local_qc_v1(&node, &block, &authority, &keys, 0);
    let second = NovNativeSealQuorumCertificateV1::from_votes(
        first.proposal.subject.clone(),
        &set,
        votes[1..].to_vec(),
    )
    .unwrap();
    assert_ne!(first.qc.qc_hash, second.qc_hash);
    node.store()
        .persist_local_verified_qc(node.ledger(), &first.qc, &set)
        .unwrap();
    node.store()
        .persist_local_verified_qc(node.ledger(), &second, &set)
        .unwrap();
    let (expected, tc) = advance_v1(&node, &authority, &keys, 0);
    let observations = observations_v1(&node, &authority, &keys[..3], 1);
    let certificate = certificate_v1(&authority, expected.clone(), tc, observations);
    let selected = certificate.verify(&expected, &authority).unwrap().unwrap();
    assert_eq!(selected.qc.subject_hash, first.qc.subject_hash);
    assert!(selected.qc.qc_hash == first.qc.qc_hash || selected.qc.qc_hash == second.qc_hash);
}

#[test]
fn native_seal_new_view_late_qc_does_not_resign_an_existing_snapshot() {
    let (mut node, block, keys, set) = genesis_fixture_v1("new-view-late-qc", 83_006);
    let authority = authority_v1(&node, &set);
    let (evidence, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
    let (expected, tc) = advance_v1(&node, &authority, &keys, 0);
    let first = node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
        .unwrap();
    assert!(first.highest_qc.is_none());
    node.store()
        .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
        .unwrap();
    assert_eq!(
        node.store()
            .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
            .unwrap(),
        first
    );
    let mut observations = observations_v1(&node, &authority, &keys[1..3], 1);
    assert!(observations
        .iter()
        .all(|v| v.highest_qc.as_ref() == Some(&evidence)));
    observations.push(first.clone());
    observations.sort_by_key(|v| v.validator_id);
    let certificate = certificate_v1(&authority, expected.clone(), tc, observations);
    assert_eq!(
        certificate.verify(&expected, &authority).unwrap(),
        Some(evidence)
    );
    node.reopen_store();
    assert_eq!(
        node.store()
            .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
            .unwrap(),
        first
    );
    assert_eq!(
        node.store()
            .load_local_new_view(node.ledger(), &authority, 1, 1, first.validator_id)
            .unwrap(),
        Some(first)
    );
}

#[test]
fn native_seal_new_view_concurrent_replay_and_restart_are_idempotent() {
    let (mut node, _, keys, set) = genesis_fixture_v1("new-view-concurrent", 83_007);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let second = NovNativeBlockSealStoreV1::open(node.store().path()).unwrap();
    let (first, duplicate) = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            node.store()
                .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
                .unwrap()
        });
        let duplicate = scope.spawn(|| {
            second
                .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
                .unwrap()
        });
        (first.join().unwrap(), duplicate.join().unwrap())
    });
    assert_eq!(first, duplicate);
    drop(second);
    node.reopen_store();
    assert_eq!(
        node.store()
            .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
            .unwrap(),
        first
    );
    advance_v1(&node, &authority, &keys, 1);
    node.store()
        .sign_local_new_view(node.ledger(), &authority, 1, 2, &keys[0])
        .unwrap();
    assert!(node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
        .is_err());
    assert_eq!(
        node.store()
            .load_local_new_view(node.ledger(), &authority, 1, 1, first.validator_id)
            .unwrap(),
        Some(first)
    );
}

#[test]
fn native_seal_new_view_missing_referenced_qc_or_index_fails_closed() {
    for delete_index in [false, true] {
        let (mut node, block, keys, set) = genesis_fixture_v1("new-view-missing-qc", 83_008);
        let authority = authority_v1(&node, &set);
        let (evidence, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
        node.store()
            .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
            .unwrap();
        advance_v1(&node, &authority, &keys, 0);
        let first = node
            .store()
            .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
            .unwrap();
        let key = if delete_index {
            qc_height_index_key_v1(set.chain_id, set.epoch, 1)
        } else {
            qc_object_key_v1(&evidence.qc.qc_hash)
        };
        node.store().db.delete(key.as_bytes()).unwrap();
        assert!(node
            .store()
            .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
            .is_err());
        assert!(node
            .store()
            .load_local_new_view(node.ledger(), &authority, 1, 1, first.validator_id)
            .is_err());
        node.reopen_store();
        assert!(node
            .store()
            .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
            .is_err());
    }
}

#[test]
fn native_seal_new_view_valid_signatures_cannot_authorize_wrong_leader_or_future_qc() {
    let (node, block, keys, set) = genesis_fixture_v1("new-view-resigned", 83_009);
    let authority = authority_v1(&node, &set);
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
    let (expected, _) = advance_v1(&node, &authority, &keys, 0);
    let leader = leader_key_v1(&authority, 1, 0, &keys);
    let nonleader = keys
        .iter()
        .find(|key| key.verifying_key() != leader.verifying_key())
        .unwrap();
    let wrong_leader = raw_qc_v1(subject.clone(), &authority, &keys, nonleader);
    wrong_leader.qc.verify(&set).unwrap();
    let observation = raw_observation_v1(&expected, &authority, Some(wrong_leader), &keys[0]);
    assert!(observation.verify(&expected, &authority).is_err());

    let mut future_subject = subject.clone();
    future_subject.round = expected.round;
    future_subject.subject_hash = subject_hash_v1(&future_subject);
    let future = raw_qc_v1(
        future_subject,
        &authority,
        &keys,
        leader_key_v1(&authority, 1, expected.round, &keys),
    );
    future.qc.verify(&set).unwrap();
    let observation = raw_observation_v1(&expected, &authority, Some(future.clone()), &keys[0]);
    assert!(observation.verify(&expected, &authority).is_err());
    node.store()
        .persist_locally_matched_remote_proposal(node.ledger(), &future.proposal, &set)
        .unwrap();
    node.store()
        .persist_local_verified_qc(node.ledger(), &future.qc, &set)
        .unwrap();
    // A known future QC cannot be silently omitted to issue an empty report.
    assert!(node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
        .is_err());

    let mut wrong_domain = subject;
    wrong_domain.protocol_config_commitment[0] ^= 1;
    wrong_domain.network_domain_commitment = network_domain_commitment_v1(
        wrong_domain.chain_id,
        &wrong_domain.genesis_block_hash,
        &wrong_domain.protocol_config_commitment,
    );
    wrong_domain.subject_hash = subject_hash_v1(&wrong_domain);
    let wrong_domain = raw_qc_v1(wrong_domain, &authority, &keys, leader);
    wrong_domain.qc.verify(&set).unwrap();
    assert!(
        raw_observation_v1(&expected, &authority, Some(wrong_domain), &keys[0])
            .verify(&expected, &authority)
            .is_err()
    );
    let outsider = SigningKey::from_bytes(&[0x94; 32]);
    assert!(raw_observation_v1(&expected, &authority, None, &outsider)
        .verify(&expected, &authority)
        .is_err());
}

#[test]
fn native_seal_new_view_cross_round_competing_qcs_fail_closed_with_valid_signatures() {
    let (node, genesis, keys, set) = genesis_fixture_v1("new-view-conflicting-qcs", 83_010);
    let authority = authority_v1(&node, &set);
    let (parent, _) = local_qc_v1(&node, &genesis, &authority, &keys, 0);
    node.store()
        .persist_local_verified_qc(node.ledger(), &parent.qc, &set)
        .unwrap();
    let block = commit_block_v1(node.ledger(), set.chain_id, 2, Some(&genesis), 0x61);
    let subject = node
        .store()
        .prepare_local_subject(
            node.ledger(),
            set.chain_id,
            block.header.block_hash,
            &set,
            0,
            Some(parent.qc.qc_hash),
        )
        .unwrap();
    let first = raw_qc_v1(
        subject.clone(),
        &authority,
        &keys,
        leader_key_v1(&authority, 2, 0, &keys),
    );
    let votes = keys
        .iter()
        .map(|key| {
            node.store()
                .sign_local_timeout(node.ledger(), &set, 2, 1, key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let tc = NovNativeSealTimeoutCertificateV1 {
        context: votes[0].context.clone(),
        votes,
    };
    let mut expected = tc.context.clone();
    expected.round = 2;
    first.verify(&expected, &authority).unwrap();
    for (round, changed_field) in [
        (0, "block"),
        (1, "block"),
        (1, "post_state_root"),
        (1, "block_receipt_root"),
    ] {
        let mut competing = subject.clone();
        competing.round = round;
        match changed_field {
            "block" => competing.block_hash[0] ^= 1,
            "post_state_root" => competing.post_state_root[0] ^= 1,
            "block_receipt_root" => competing.block_receipt_root[0] ^= 1,
            _ => unreachable!(),
        }
        competing.inline_body_commitment = inline_body_commitment_v1(
            competing.chain_id,
            competing.height,
            &competing.block_hash,
            &competing.ordered_tx_root,
            &competing.body_digest,
            competing.body_bytes,
            competing.tx_count,
        );
        competing.subject_hash = subject_hash_v1(&competing);
        let second = raw_qc_v1(
            competing,
            &authority,
            &keys,
            leader_key_v1(&authority, 2, round, &keys),
        );
        second.verify(&expected, &authority).unwrap();
        let observations = vec![
            raw_observation_v1(&expected, &authority, Some(first.clone()), &keys[0]),
            raw_observation_v1(&expected, &authority, Some(second), &keys[1]),
            raw_observation_v1(&expected, &authority, None, &keys[2]),
        ];
        for observation in &observations {
            observation.verify(&expected, &authority).unwrap();
        }
        let certificate = certificate_v1(&authority, expected.clone(), tc.clone(), observations);
        assert!(
            certificate.verify(&expected, &authority).is_err(),
            "round {round}: {changed_field}"
        );
    }
}

#[test]
fn native_seal_new_view_missing_observation_watermark_or_authority_fails_closed() {
    for missing in ["observation", "watermark", "authority", "timeout"] {
        let (mut node, _, keys, set) = genesis_fixture_v1("new-view-corrupt-record", 83_011);
        let authority = authority_v1(&node, &set);
        advance_v1(&node, &authority, &keys, 0);
        let first = node
            .store()
            .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
            .unwrap();
        let prefix = format!(
            "native_block_seal/v1/new-view/{}/{}/{}/",
            set.chain_id,
            set.epoch,
            hex_v1(&first.validator_id)
        );
        match missing {
            "observation" => node
                .store()
                .db
                .delete(format!("{prefix}observation/1/1").as_bytes())
                .unwrap(),
            "watermark" => node
                .store()
                .db
                .delete(format!("{prefix}watermark").as_bytes())
                .unwrap(),
            "authority" => node
                .store()
                .db
                .delete(
                    format!(
                        "native_block_seal/v1/new-view-authority/{}/{}",
                        set.chain_id, set.epoch
                    )
                    .as_bytes(),
                )
                .unwrap(),
            "timeout" => {
                let key = format!("{prefix}observation/1/1");
                let bytes = node.store().db.get(key.as_bytes()).unwrap().unwrap();
                let mut record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                record["previous_timeout"]["votes"] = serde_json::json!([]);
                node.store()
                    .db
                    .put(key.as_bytes(), serde_json::to_vec(&record).unwrap())
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            node.store()
                .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
                .is_err(),
            "{missing}"
        );
        assert!(
            node.store()
                .load_local_new_view(node.ledger(), &authority, 1, 1, first.validator_id)
                .is_err(),
            "{missing}"
        );
        node.reopen_store();
        assert!(
            node.store()
                .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
                .is_err(),
            "{missing}"
        );
    }
}

#[test]
fn native_seal_new_view_timeout_prevents_new_signatures_but_retains_existing_snapshot() {
    let (node, _, keys, set) = genesis_fixture_v1("new-view-timed-out", 83_012);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let first = node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
        .unwrap();
    node.store()
        .sign_local_timeout(node.ledger(), &set, 1, 1, &keys[0])
        .unwrap();
    node.store()
        .sign_local_timeout(node.ledger(), &set, 1, 1, &keys[1])
        .unwrap();
    assert_eq!(
        node.store()
            .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
            .unwrap(),
        first
    );
    assert!(node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[1])
        .is_err());
}

#[test]
fn native_seal_new_view_cannot_omit_qc_when_height_index_is_missing_or_truncated() {
    for truncate in [false, true] {
        let (node, block, keys, set) = genesis_fixture_v1("new-view-pre-sign-index", 83_013);
        let authority = authority_v1(&node, &set);
        let (first, votes) = local_qc_v1(&node, &block, &authority, &keys, 0);
        let second = NovNativeSealQuorumCertificateV1::from_votes(
            first.proposal.subject.clone(),
            &set,
            votes[1..].to_vec(),
        )
        .unwrap();
        node.store()
            .persist_local_verified_qc(node.ledger(), &first.qc, &set)
            .unwrap();
        node.store()
            .persist_local_verified_qc(node.ledger(), &second, &set)
            .unwrap();
        advance_v1(&node, &authority, &keys, 0);
        let key = qc_height_index_key_v1(set.chain_id, set.epoch, 1);
        if truncate {
            let bytes = node.store().db.get(key.as_bytes()).unwrap().unwrap();
            let mut index: NovNativeSealQcIndexV1 = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(index.qc_hashes.len(), 2);
            index.qc_hashes.pop();
            node.store()
                .db
                .put(key.as_bytes(), serde_json::to_vec(&index).unwrap())
                .unwrap();
        } else {
            node.store().db.delete(key.as_bytes()).unwrap();
        }
        assert!(node
            .store()
            .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
            .is_err());
        assert!(node
            .store()
            .load_local_new_view(
                node.ledger(),
                &authority,
                1,
                1,
                validator_id_v1(keys[0].verifying_key().as_bytes())
            )
            .unwrap()
            .is_none());
    }
}

#[test]
fn native_seal_new_view_accepts_equivalent_tc_subsets_but_rejects_bad_timeout_signatures() {
    let (node, _, keys, set) = genesis_fixture_v1("new-view-tc-subsets", 83_014);
    let authority = authority_v1(&node, &set);
    let (expected, full_tc) = advance_v1(&node, &authority, &keys, 0);
    let observations = observations_v1(&node, &authority, &keys[..3], 1);
    assert_eq!(full_tc.votes.len(), 4);
    let mut first_tc = full_tc.clone();
    first_tc.votes = full_tc.votes[..3].to_vec();
    let mut second_tc = full_tc.clone();
    second_tc.votes = full_tc.votes[1..].to_vec();
    assert_ne!(first_tc, second_tc);
    first_tc.verify(&full_tc.context, &set).unwrap();
    second_tc.verify(&full_tc.context, &set).unwrap();
    let first = certificate_v1(&authority, expected.clone(), first_tc, observations.clone());
    let mut second = certificate_v1(&authority, expected.clone(), second_tc, observations);
    assert!(first.verify(&expected, &authority).unwrap().is_none());
    assert!(second.verify(&expected, &authority).unwrap().is_none());
    assert_eq!(first.observations, second.observations);
    second.previous_timeout.votes[0].signature[0] ^= 1;
    assert!(second.verify(&expected, &authority).is_err());
}

#[test]
fn native_seal_new_view_verifies_qc_inventory_before_filtering_by_height() {
    let (node, block, keys, set) = genesis_fixture_v1("new-view-hidden-qc", 83_015);
    let authority = authority_v1(&node, &set);
    let (evidence, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
    node.store()
        .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
        .unwrap();
    advance_v1(&node, &authority, &keys, 0);
    let mut corrupted_qc = evidence.qc.clone();
    corrupted_qc.subject.height = 2;
    // Keep the object key and signatures unchanged: trusting this unverified
    // height before validating would hide the QC from the requested inventory.
    node.store()
        .db
        .put(
            qc_object_key_v1(&evidence.qc.qc_hash).as_bytes(),
            serde_json::to_vec(&corrupted_qc).unwrap(),
        )
        .unwrap();
    node.store()
        .db
        .delete(qc_height_index_key_v1(set.chain_id, set.epoch, 1).as_bytes())
        .unwrap();
    assert!(node
        .store()
        .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[0])
        .is_err());
    assert!(node
        .store()
        .load_local_new_view(
            node.ledger(),
            &authority,
            1,
            1,
            validator_id_v1(keys[0].verifying_key().as_bytes())
        )
        .unwrap()
        .is_none());
}
