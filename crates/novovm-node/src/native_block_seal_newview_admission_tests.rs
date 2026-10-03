// Local-only synthetic-key admission tests, included beside the new-view fixtures.

fn admission_request_v1(
    block: &NovNativeDurableBlockV1,
    round: u64,
) -> NovNativeSealLocalProposalRequestV1 {
    NovNativeSealLocalProposalRequestV1 {
        chain_id: block.header.chain_id,
        block_hash: block.header.block_hash,
        round,
        justify_qc_hash: None,
    }
}

fn admission_certificate_v1(
    node: &TestNodeV1,
    authority: &NovNativeSealEpochAuthorityV1,
    keys: &[SigningKey],
    round: u64,
) -> NovNativeSealNewViewCertificateV1 {
    let state = node
        .store()
        .load_round_tracking(node.ledger(), &authority.validator_set, 1)
        .unwrap()
        .unwrap();
    assert_eq!(state.current.round, round);
    certificate_v1(
        authority,
        state.current,
        state.previous_timeout.unwrap(),
        observations_v1(node, authority, &keys[..3], round),
    )
}

fn admission_key_v1(chain_id: u64, round: u64) -> String {
    format!("native_block_seal/v1/new-view-admission/{chain_id}/1/1/{round}")
}

fn seal_database_snapshot_v1(node: &TestNodeV1) -> Vec<(Vec<u8>, Vec<u8>)> {
    node.store()
        .db
        .iterator(IteratorMode::Start)
        .map(|item| {
            let (key, value) = item.unwrap();
            (key.to_vec(), value.to_vec())
        })
        .collect()
}

// Explicit corruption fixture only: never expose a bypass in production APIs.
fn inject_test_qc_v1(node: &TestNodeV1, evidence: &NovNativeSealNewViewQcV1) {
    let qc = &evidence.qc;
    let mut batch = RocksDbWriteBatch::default();
    put_json_v1(
        &mut batch,
        proposal_object_key_v1(&evidence.proposal.proposal_hash).as_bytes(),
        &evidence.proposal,
        "test proposal injection",
    )
    .unwrap();
    put_json_v1(
        &mut batch,
        qc_object_key_v1(&qc.qc_hash).as_bytes(),
        qc,
        "test QC injection",
    )
    .unwrap();
    for (key, kind, binding) in [
        (
            qc_subject_index_key_v1(&qc.subject_hash),
            "subject",
            qc.subject_hash,
        ),
        (
            qc_block_index_key_v1(qc.subject.chain_id, &qc.subject.block_hash),
            "block",
            qc.subject.block_hash,
        ),
        (
            qc_height_index_key_v1(qc.subject.chain_id, qc.subject.epoch, qc.subject.height),
            "height",
            [0; 32],
        ),
    ] {
        node.store()
            .stage_qc_index_v1(
                &mut batch,
                key,
                kind,
                qc.subject.chain_id,
                qc.subject.epoch,
                qc.subject.height,
                binding,
                qc.qc_hash,
            )
            .unwrap();
    }
    write_sync_v1(&node.store().db, batch).unwrap();
}

#[test]
fn native_seal_new_view_admission_nonzero_entry_points_fail_without_admission() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-required", 84_001);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let request = admission_request_v1(&block, 1);
    let subject = node
        .store()
        .prepare_local_subject(
            node.ledger(),
            set.chain_id,
            request.block_hash,
            &set,
            1,
            None,
        )
        .unwrap();
    let evidence = raw_qc_v1(
        subject,
        &authority,
        &keys,
        leader_key_v1(&authority, 1, 1, &keys),
    );
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .sign_local_proposal(
            node.ledger(),
            &request,
            &set,
            leader_key_v1(&authority, 1, 1, &keys),
        )
        .is_err());
    assert!(node
        .store()
        .sign_local_vote(node.ledger(), &evidence.proposal, &set, &keys[0])
        .is_err());
    assert!(node
        .store()
        .persist_locally_matched_remote_proposal(node.ledger(), &evidence.proposal, &set)
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
    // A pre-existing, signed proposal object must not bypass the QC admission gate.
    node.store()
        .db
        .put(
            proposal_object_key_v1(&evidence.proposal.proposal_hash).as_bytes(),
            serde_json::to_vec(&evidence.proposal).unwrap(),
        )
        .unwrap();
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
}

#[test]
fn native_seal_new_view_admission_quorum_persists_without_signing_or_finality() {
    let (mut node, block, keys, set) = genesis_fixture_v1("admission-quorum", 84_002);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    let before_head = node.ledger().load_head(set.chain_id).unwrap();
    let before_candidate = node
        .ledger()
        .load_candidate_record(set.chain_id, block.header.block_hash)
        .unwrap();
    assert!(node
        .store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .unwrap());
    let record = node
        .store()
        .load_local_new_view_admission(set.chain_id, set.epoch, 1, 1)
        .unwrap()
        .unwrap();
    assert_eq!(record.subject.block_hash, block.header.block_hash);
    assert_eq!(record.subject.round, 1);
    assert_eq!(record.certificate, certificate);
    assert_ne!(record.admission_hash, [0; 32]);
    for key in &keys {
        let signer = validator_id_v1(key.verifying_key().as_bytes());
        assert!(node
            .store()
            .load_pending_outbox(set.chain_id, signer, 16)
            .unwrap()
            .is_empty());
        assert!(node
            .store()
            .db
            .get(height_lock_key_v1(&record.subject, signer).as_bytes())
            .unwrap()
            .is_none());
    }
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
    node.reopen_store();
    assert!(!node
        .store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .unwrap());
    assert_eq!(
        node.store()
            .load_local_new_view_admission(set.chain_id, set.epoch, 1, 1)
            .unwrap(),
        Some(record)
    );
}

#[test]
fn native_seal_new_view_admission_rejects_quorum_domain_and_request_mismatch() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-domain", 84_003);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    let before = seal_database_snapshot_v1(&node);
    for variant in [
        "quorum",
        "timeout",
        "signature",
        "authority",
        "height",
        "genesis",
        "round",
    ] {
        let mut bad = certificate.clone();
        match variant {
            "quorum" => {
                bad.observations.pop();
            }
            "timeout" => bad.previous_timeout.votes.truncate(2),
            "signature" => bad.observations[0].signature[0] ^= 1,
            "authority" => bad.authority_commitment[0] ^= 1,
            "height" => bad.context.height += 1,
            "genesis" => bad.context.genesis_block_hash[0] ^= 1,
            "round" => bad.context.round += 1,
            _ => unreachable!(),
        }
        assert!(
            node.store()
                .admit_local_new_view_candidate(node.ledger(), &authority, &bad, &request)
                .is_err(),
            "{variant}"
        );
        assert_eq!(seal_database_snapshot_v1(&node), before, "{variant}");
    }
    for variant in ["chain", "block", "round", "justify"] {
        let mut bad = request.clone();
        match variant {
            "chain" => bad.chain_id += 1,
            "block" => bad.block_hash[0] ^= 1,
            "round" => bad.round = 0,
            "justify" => bad.justify_qc_hash = Some([0x91; 32]),
            _ => unreachable!(),
        }
        assert!(
            node.store()
                .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &bad)
                .is_err(),
            "{variant}"
        );
        assert_eq!(seal_database_snapshot_v1(&node), before, "{variant}");
    }
}

#[test]
fn native_seal_new_view_admission_only_scheduled_leader_can_propose() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-leader", 84_004);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    node.store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .unwrap();
    let leader = leader_key_v1(&authority, 1, 1, &keys);
    let nonleader = keys
        .iter()
        .find(|key| key.verifying_key() != leader.verifying_key())
        .unwrap();
    let subject = node
        .store()
        .prepare_local_subject(
            node.ledger(),
            set.chain_id,
            request.block_hash,
            &set,
            1,
            None,
        )
        .unwrap();
    let bad = raw_qc_v1(subject, &authority, &keys, nonleader);
    bad.proposal.verify(&set).unwrap();
    bad.qc.verify(&set).unwrap();
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .sign_local_proposal(node.ledger(), &request, &set, nonleader)
        .is_err());
    assert!(node
        .store()
        .sign_local_vote(node.ledger(), &bad.proposal, &set, &keys[0])
        .is_err());
    assert!(node
        .store()
        .persist_locally_matched_remote_proposal(node.ledger(), &bad.proposal, &set)
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
    node.store()
        .db
        .put(
            proposal_object_key_v1(&bad.proposal.proposal_hash).as_bytes(),
            serde_json::to_vec(&bad.proposal).unwrap(),
        )
        .unwrap();
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .persist_local_verified_qc(node.ledger(), &bad.qc, &set)
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
    assert!(node
        .store()
        .sign_local_proposal(node.ledger(), &request, &set, leader)
        .is_ok());
}

#[test]
fn native_seal_new_view_admission_current_qc_does_not_break_replay_or_remaining_vote() {
    let (mut node, block, keys, set) = genesis_fixture_v1("admission-current-qc", 84_005);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    node.store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .unwrap();
    let proposal = node
        .store()
        .sign_local_proposal(
            node.ledger(),
            &request,
            &set,
            leader_key_v1(&authority, 1, 1, &keys),
        )
        .unwrap();
    let votes = keys[..3]
        .iter()
        .map(|key| {
            node.store()
                .sign_local_vote(node.ledger(), &proposal, &set, key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let qc =
        NovNativeSealQuorumCertificateV1::from_votes(proposal.subject.clone(), &set, votes.clone())
            .unwrap();
    assert!(node
        .store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap());
    node.reopen_store();
    assert_eq!(
        node.store()
            .sign_local_proposal(
                node.ledger(),
                &request,
                &set,
                leader_key_v1(&authority, 1, 1, &keys)
            )
            .unwrap(),
        proposal
    );
    assert_eq!(
        node.store()
            .sign_local_vote(node.ledger(), &proposal, &set, &keys[0])
            .unwrap(),
        votes[0]
    );
    assert!(node
        .store()
        .sign_local_vote(node.ledger(), &proposal, &set, &keys[3])
        .is_ok());
    assert!(!node
        .store()
        .persist_local_verified_qc(node.ledger(), &qc, &set)
        .unwrap());
    assert!(!node
        .store()
        .persist_locally_matched_remote_proposal(node.ledger(), &proposal, &set)
        .unwrap());
}

#[test]
fn native_seal_new_view_admission_historical_outbox_survives_later_round_and_restart() {
    let (mut node, block, keys, set) = genesis_fixture_v1("admission-history", 84_006);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let (evidence, _) = local_qc_v1(&node, &block, &authority, &keys, 1);
    node.store()
        .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
        .unwrap();
    let signer = evidence.proposal.proposer_id;
    let outbox = node
        .store()
        .load_pending_outbox(set.chain_id, signer, 16)
        .unwrap();
    assert_eq!(outbox.len(), 2);
    advance_v1(&node, &authority, &keys, 1);
    node.reopen_store();
    assert_eq!(
        node.store()
            .load_pending_outbox(set.chain_id, signer, 16)
            .unwrap(),
        outbox
    );
    assert!(node
        .store()
        .sign_local_proposal(
            node.ledger(),
            &admission_request_v1(&block, 1),
            &set,
            leader_key_v1(&authority, 1, 1, &keys)
        )
        .is_err());
    assert!(node
        .store()
        .sign_local_vote(node.ledger(), &evidence.proposal, &set, &keys[0])
        .is_err());
    assert!(node
        .store()
        .load_local_new_view_admission(set.chain_id, set.epoch, 1, 1)
        .unwrap()
        .is_some());
}

#[test]
fn native_seal_new_view_admission_missing_or_tampered_evidence_blocks_signing_and_outbox() {
    for variant in [
        "missing",
        "hash",
        "subject",
        "quorum",
        "authority",
        "schema",
    ] {
        let (mut node, block, keys, set) = genesis_fixture_v1("admission-corruption", 84_007);
        let authority = authority_v1(&node, &set);
        advance_v1(&node, &authority, &keys, 0);
        let (evidence, _) = local_qc_v1(&node, &block, &authority, &keys, 1);
        let key = admission_key_v1(set.chain_id, 1);
        if variant == "missing" {
            node.store().db.delete(key.as_bytes()).unwrap();
        } else {
            let bytes = node.store().db.get(key.as_bytes()).unwrap().unwrap();
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            match variant {
                "hash" => {
                    value["admission_hash"][0] =
                        serde_json::json!(value["admission_hash"][0].as_u64().unwrap() ^ 1)
                }
                "subject" => value["subject"]["post_state_root"][0] = serde_json::json!(255),
                "quorum" => value["certificate"]["observations"] = serde_json::json!([]),
                "authority" => {
                    value["authority"]["authority_commitment"][0] = serde_json::json!(255)
                }
                "schema" => value["schema"] = serde_json::json!("unsupported"),
                _ => unreachable!(),
            }
            node.store()
                .db
                .put(key.as_bytes(), serde_json::to_vec(&value).unwrap())
                .unwrap();
        }
        node.reopen_store();
        assert!(
            node.store()
                .sign_local_proposal(
                    node.ledger(),
                    &admission_request_v1(&block, 1),
                    &set,
                    leader_key_v1(&authority, 1, 1, &keys)
                )
                .is_err(),
            "{variant}"
        );
        assert!(
            node.store()
                .sign_local_vote(node.ledger(), &evidence.proposal, &set, &keys[0])
                .is_err(),
            "{variant}"
        );
        assert!(
            node.store()
                .persist_locally_matched_remote_proposal(node.ledger(), &evidence.proposal, &set)
                .is_err(),
            "{variant}"
        );
        assert!(
            node.store()
                .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
                .is_err(),
            "{variant}"
        );
        assert!(
            node.store()
                .load_pending_outbox(set.chain_id, evidence.proposal.proposer_id, 16)
                .is_err(),
            "{variant}"
        );
    }
}

#[test]
fn native_seal_new_view_admission_highest_qc_must_match_local_immutable_candidate() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-root", 84_008);
    let authority = authority_v1(&node, &set);
    let mut subject = node
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
    subject.post_state_root[0] ^= 1;
    subject.subject_hash = subject_hash_v1(&subject);
    let misleading = raw_qc_v1(
        subject,
        &authority,
        &keys,
        leader_key_v1(&authority, 1, 0, &keys),
    );
    let (context, tc) = advance_v1(&node, &authority, &keys, 0);
    let observations = keys[..3]
        .iter()
        .map(|key| raw_observation_v1(&context, &authority, Some(misleading.clone()), key))
        .collect();
    let certificate = certificate_v1(&authority, context.clone(), tc, observations);
    certificate.verify(&context, &authority).unwrap();
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .admit_local_new_view_candidate(
            node.ledger(),
            &authority,
            &certificate,
            &admission_request_v1(&block, 1)
        )
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
}

#[test]
fn native_seal_new_view_admission_certificate_cannot_omit_known_prior_qc() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-omit", 84_009);
    let authority = authority_v1(&node, &set);
    let (evidence, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
    node.store()
        .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
        .unwrap();
    let (context, tc) = advance_v1(&node, &authority, &keys, 0);
    let observations = keys[..3]
        .iter()
        .map(|key| raw_observation_v1(&context, &authority, None, key))
        .collect();
    let certificate = certificate_v1(&authority, context.clone(), tc, observations);
    assert!(certificate.verify(&context, &authority).unwrap().is_none());
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .admit_local_new_view_candidate(
            node.ledger(),
            &authority,
            &certificate,
            &admission_request_v1(&block, 1)
        )
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
}

#[test]
fn native_seal_new_view_admission_late_qc_blocks_stale_empty_certificate() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-late-qc", 84_010);
    let authority = authority_v1(&node, &set);
    let (evidence, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    assert!(certificate
        .verify(&certificate.context, &authority)
        .unwrap()
        .is_none());
    node.store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .unwrap();
    node.store()
        .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
        .unwrap();
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .is_err());
    assert!(node
        .store()
        .sign_local_proposal(
            node.ledger(),
            &request,
            &set,
            leader_key_v1(&authority, 1, 1, &keys)
        )
        .is_err());
    let mut subject = evidence.proposal.subject.clone();
    subject.round = 1;
    subject.subject_hash = subject_hash_v1(&subject);
    let proposal = sign_proposal_v1(subject, &set, leader_key_v1(&authority, 1, 1, &keys)).unwrap();
    assert!(node
        .store()
        .sign_local_vote(node.ledger(), &proposal, &set, &keys[0])
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
}

#[test]
fn native_seal_new_view_admission_empty_certificate_does_not_release_height_lock() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-retain-lock", 84_011);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    let subject = node
        .store()
        .prepare_local_subject(
            node.ledger(),
            set.chain_id,
            request.block_hash,
            &set,
            1,
            None,
        )
        .unwrap();
    let leader = leader_key_v1(&authority, 1, 1, &keys);
    let signer = validator_id_v1(leader.verifying_key().as_bytes());
    let (_, mut old_lock) = node
        .store()
        .prepare_safety_locks_v1(&subject, signer)
        .unwrap();
    old_lock.block_hash[0] ^= 1;
    old_lock.first_round = 0;
    old_lock.highest_round = 0;
    let lock_key = height_lock_key_v1(&subject, signer);
    let before_lock = serde_json::to_vec(&old_lock).unwrap();
    node.store()
        .db
        .put(lock_key.as_bytes(), &before_lock)
        .unwrap();
    node.store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .unwrap();
    assert_eq!(
        node.store().db.get(lock_key.as_bytes()).unwrap().unwrap(),
        before_lock
    );
    assert!(node
        .store()
        .sign_local_proposal(node.ledger(), &request, &set, leader)
        .is_err());
    let proposal = sign_proposal_v1(subject, &set, leader).unwrap();
    assert!(node
        .store()
        .sign_local_vote(node.ledger(), &proposal, &set, leader)
        .is_err());
    assert_eq!(
        node.store().db.get(lock_key.as_bytes()).unwrap().unwrap(),
        before_lock
    );
}

#[test]
fn native_seal_new_view_admission_requires_active_round_and_read_write_store() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-current-round", 84_012);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    let readonly = NovNativeBlockSealStoreV1::open_existing_read_only(node.store().path())
        .unwrap()
        .unwrap();
    assert!(readonly
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .is_err());
    let round_key = format!(
        "native_block_seal/v1/round-state/{}/{}/1",
        set.chain_id, set.epoch
    );
    let round_bytes = node.store().db.get(round_key.as_bytes()).unwrap().unwrap();
    node.store().db.delete(round_key.as_bytes()).unwrap();
    assert!(node
        .store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .is_err());
    node.store()
        .db
        .put(round_key.as_bytes(), round_bytes)
        .unwrap();
    advance_v1(&node, &authority, &keys, 1);
    assert!(node
        .store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .is_err());
    assert!(node
        .store()
        .load_local_new_view_admission(set.chain_id, set.epoch, 1, 1)
        .unwrap()
        .is_none());
}

#[test]
fn native_seal_new_view_admission_concurrent_handles_are_idempotent() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-concurrent", 84_013);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    let second = NovNativeBlockSealStoreV1::open(node.store().path()).unwrap();
    let results = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            node.store()
                .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
                .unwrap()
        });
        let duplicate = scope.spawn(|| {
            second
                .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
                .unwrap()
        });
        (first.join().unwrap(), duplicate.join().unwrap())
    });
    assert_ne!(results.0, results.1);
    assert!(node
        .store()
        .load_local_new_view_admission(set.chain_id, set.epoch, 1, 1)
        .unwrap()
        .is_some());
}

#[test]
fn native_seal_new_view_admission_corrupt_qc_inventory_blocks_new_signatures() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-inventory", 84_014);
    let authority = authority_v1(&node, &set);
    let (evidence, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
    node.store()
        .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
        .unwrap();
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    node.store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .unwrap();
    node.store()
        .db
        .delete(qc_height_index_key_v1(set.chain_id, set.epoch, 1).as_bytes())
        .unwrap();
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .sign_local_proposal(
            node.ledger(),
            &request,
            &set,
            leader_key_v1(&authority, 1, 1, &keys)
        )
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
}

#[test]
fn native_seal_new_view_admission_omitted_higher_qc_blocks_stale_selected_qc() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-higher-qc", 84_015);
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
    let (context, tc) = advance_v1(&node, &authority, &keys, 1);
    let observations = keys[..3]
        .iter()
        .map(|key| raw_observation_v1(&context, &authority, Some(first.clone()), key))
        .collect();
    let certificate = certificate_v1(&authority, context.clone(), tc, observations);
    assert_eq!(
        certificate.verify(&context, &authority).unwrap(),
        Some(first)
    );
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .admit_local_new_view_candidate(
            node.ledger(),
            &authority,
            &certificate,
            &admission_request_v1(&block, 2)
        )
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
}

#[test]
fn native_seal_new_view_admission_inline_highest_qc_requires_durable_import_first() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-inline-qc", 84_016);
    let authority = authority_v1(&node, &set);
    let (evidence, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
    let (context, tc) = advance_v1(&node, &authority, &keys, 0);
    let observations = keys[..3]
        .iter()
        .map(|key| raw_observation_v1(&context, &authority, Some(evidence.clone()), key))
        .collect();
    let certificate = certificate_v1(&authority, context.clone(), tc, observations);
    assert_eq!(
        certificate.verify(&context, &authority).unwrap(),
        Some(evidence.clone())
    );
    let request = admission_request_v1(&block, 1);
    let before = seal_database_snapshot_v1(&node);
    assert!(node
        .store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .is_err());
    assert_eq!(seal_database_snapshot_v1(&node), before);
    assert!(node
        .store()
        .persist_local_verified_qc(node.ledger(), &evidence.qc, &set)
        .unwrap());
    assert!(node
        .store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .unwrap());
}

#[test]
fn native_seal_new_view_admission_missing_durable_highest_qc_fails_recovery() {
    for missing in ["qc", "proposal", "height_index"] {
        let (mut node, block, keys, set) = genesis_fixture_v1("admission-missing-highest", 84_017);
        let authority = authority_v1(&node, &set);
        let (first, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
        node.store()
            .persist_local_verified_qc(node.ledger(), &first.qc, &set)
            .unwrap();
        advance_v1(&node, &authority, &keys, 0);
        let (second, _) = local_qc_v1(&node, &block, &authority, &keys, 1);
        let key = match missing {
            "qc" => qc_object_key_v1(&first.qc.qc_hash),
            "proposal" => proposal_object_key_v1(&first.proposal.proposal_hash),
            "height_index" => qc_height_index_key_v1(set.chain_id, set.epoch, 1),
            _ => unreachable!(),
        };
        node.store().db.delete(key.as_bytes()).unwrap();
        node.reopen_store();
        assert!(
            node.store()
                .load_local_new_view_admission(set.chain_id, set.epoch, 1, 1)
                .is_err(),
            "{missing}"
        );
        assert!(
            node.store()
                .sign_local_proposal(
                    node.ledger(),
                    &admission_request_v1(&block, 1),
                    &set,
                    leader_key_v1(&authority, 1, 1, &keys)
                )
                .is_err(),
            "{missing}"
        );
        assert!(
            node.store()
                .sign_local_vote(node.ledger(), &second.proposal, &set, &keys[0])
                .is_err(),
            "{missing}"
        );
        assert!(
            node.store()
                .load_pending_outbox(set.chain_id, second.proposal.proposer_id, 16)
                .is_err(),
            "{missing}"
        );
    }
}

#[test]
fn native_seal_new_view_admission_cannot_enter_round_with_current_or_future_qc() {
    for qc_round in [1, 2] {
        let (node, block, keys, set) = genesis_fixture_v1("admission-future-qc", 84_018);
        let authority = authority_v1(&node, &set);
        advance_v1(&node, &authority, &keys, 0);
        let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
        let subject = node
            .store()
            .prepare_local_subject(
                node.ledger(),
                set.chain_id,
                block.header.block_hash,
                &set,
                qc_round,
                None,
            )
            .unwrap();
        let evidence = raw_qc_v1(
            subject,
            &authority,
            &keys,
            leader_key_v1(&authority, 1, qc_round, &keys),
        );
        inject_test_qc_v1(&node, &evidence);
        let before = seal_database_snapshot_v1(&node);
        assert!(
            node.store()
                .admit_local_new_view_candidate(
                    node.ledger(),
                    &authority,
                    &certificate,
                    &admission_request_v1(&block, 1)
                )
                .is_err(),
            "QC round {qc_round}"
        );
        assert_eq!(seal_database_snapshot_v1(&node), before);
    }
}

#[test]
fn native_seal_new_view_admission_equivalent_certificate_replay_keeps_original_evidence() {
    let (node, block, keys, set) = genesis_fixture_v1("admission-subsets", 84_019);
    let authority = authority_v1(&node, &set);
    advance_v1(&node, &authority, &keys, 0);
    let certificate = admission_certificate_v1(&node, &authority, &keys, 1);
    let request = admission_request_v1(&block, 1);
    node.store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &certificate, &request)
        .unwrap();
    let record = node
        .store()
        .load_local_new_view_admission(set.chain_id, set.epoch, 1, 1)
        .unwrap()
        .unwrap();
    let mut equivalent = certificate.clone();
    equivalent.previous_timeout.votes.pop();
    equivalent.previous_timeout.votes.reverse();
    equivalent.observations.reverse();
    assert_ne!(equivalent, certificate);
    equivalent.verify(&certificate.context, &authority).unwrap();
    assert!(!node
        .store()
        .admit_local_new_view_candidate(node.ledger(), &authority, &equivalent, &request)
        .unwrap());
    assert_eq!(
        node.store()
            .load_local_new_view_admission(set.chain_id, set.epoch, 1, 1)
            .unwrap(),
        Some(record)
    );
}

#[test]
fn native_seal_new_view_admission_prevents_erased_qc_from_becoming_empty_observation() {
    for corruption in ["erase_qc_inventory", "hide_admission_height"] {
        let (node, block, keys, set) = genesis_fixture_v1("admission-erased-qc", 84_020);
        let authority = authority_v1(&node, &set);
        let (first, _) = local_qc_v1(&node, &block, &authority, &keys, 0);
        node.store()
            .persist_local_verified_qc(node.ledger(), &first.qc, &set)
            .unwrap();
        advance_v1(&node, &authority, &keys, 0);
        local_qc_v1(&node, &block, &authority, &keys, 1);
        if corruption == "erase_qc_inventory" {
            for key in [
                qc_object_key_v1(&first.qc.qc_hash),
                qc_subject_index_key_v1(&first.qc.subject_hash),
                qc_block_index_key_v1(set.chain_id, &block.header.block_hash),
                qc_height_index_key_v1(set.chain_id, set.epoch, 1),
            ] {
                node.store().db.delete(key.as_bytes()).unwrap();
            }
        } else {
            let key = admission_key_v1(set.chain_id, 1);
            let bytes = node.store().db.get(key.as_bytes()).unwrap().unwrap();
            let mut record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            record["subject"]["height"] = serde_json::json!(2);
            node.store()
                .db
                .put(key.as_bytes(), serde_json::to_vec(&record).unwrap())
                .unwrap();
        }
        assert!(
            node.store()
                .sign_local_proposal(
                    node.ledger(),
                    &admission_request_v1(&block, 1),
                    &set,
                    leader_key_v1(&authority, 1, 1, &keys)
                )
                .is_err(),
            "{corruption}"
        );
        // This validator has no earlier new-view observation/watermark that
        // would independently reveal the lost QC: the admission inventory must.
        assert!(node
            .store()
            .load_local_new_view(
                node.ledger(),
                &authority,
                1,
                1,
                validator_id_v1(keys[3].verifying_key().as_bytes())
            )
            .unwrap()
            .is_none());
        assert!(
            node.store()
                .sign_local_new_view(node.ledger(), &authority, 1, 1, &keys[3])
                .is_err(),
            "{corruption}"
        );
        advance_v1(&node, &authority, &keys, 1);
        assert!(
            node.store()
                .sign_local_new_view(node.ledger(), &authority, 1, 2, &keys[3])
                .is_err(),
            "{corruption}"
        );
    }
}
