use super::*;

fn replay(
    revision: u64,
    message: Option<DurableMessage>,
    evidence: ReplayEvidence,
) -> ReplayRecord {
    ReplayRecord {
        revision,
        message,
        evidence,
    }
}

fn reference(record: &ReplayRecord) -> ReplayRef {
    ReplayRef {
        revision: record.revision,
        digest: replay_digest(record).unwrap(),
    }
}

fn locator(value: Hash) -> CandidateLocator {
    CandidateLocator {
        value,
        candidate_id: [0x90; 32],
        document_digest: [0x91; 32],
    }
}

fn proposal_evidence(
    identity: &Identity,
    proposal: &Proposal,
    justification: Option<VerifiedQuorum>,
) -> ReplayEvidence {
    ReplayEvidence::Proposal {
        proposal: proposal.verify(&identity.set).unwrap(),
        justification,
        candidate: locator(proposal.value),
    }
}

fn identity() -> Identity {
    let key = SigningKey::from_bytes(&[7; 32]);
    let member = Validator::new(key.verifying_key().to_bytes(), 1).unwrap();
    let validator = member.id();
    let set = Arc::new(ValidatorSet::new(71, 1, 1, vec![member]).unwrap());
    let context = ConsensusContext {
        chain_id: 71,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        epoch: 1,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    Identity {
        owner: Arc::new(()),
        context,
        parent: ParentPoint {
            height: 0,
            block_hash: [0; 32],
            state_root: [3; 32],
            receipt_batch_commitment: [4; 32],
            state_version: 0,
            decision_hash: [0; 32],
        },
        set,
        key,
        validator,
    }
}

fn nil_snapshot(identity: &Identity) -> (Snapshot, DurableMessage) {
    let next = State::new(identity.context)
        .unwrap()
        .prepare_timeout(LocalTimeout {
            context: identity.context,
            round: 0,
            step: Step::Propose,
        })
        .unwrap();
    let (state, intent, _) = next.into_parts();
    let intent = intent.unwrap();
    let vote = Vote::sign(
        intent.context,
        intent.round,
        intent.phase,
        intent.value,
        &identity.set,
        &identity.key,
    )
    .unwrap();
    let message = DurableMessage::Vote(vote);
    let record = replay(1, Some(message.clone()), ReplayEvidence::None);
    (
        Snapshot {
            state,
            revision: 1,
            proposed: None,
            witness: None,
            replay: ReplayIndex {
                prevote: Some(reference(&record)),
                ..ReplayIndex::default()
            },
        },
        message,
    )
}

#[test]
fn journal_snapshot_and_outbox_are_exactly_bound_and_validate_real_signature() {
    let identity = identity();
    let (snapshot, message) = nil_snapshot(&identity);
    let bytes = encode_snapshot(&identity, &snapshot).unwrap();
    let record = replay(
        snapshot.revision,
        Some(message.clone()),
        ReplayEvidence::None,
    );
    let outbox = encode_outbox(&snapshot, &bytes, &record).unwrap();
    let recovered = decode_snapshot(&identity, &bytes).unwrap();
    assert_eq!(recovered.state, snapshot.state);
    assert!(matches!(
        decode_outbox(&identity, &recovered, &bytes, &outbox)
            .unwrap()
            .message,
        Some(DurableMessage::Vote(_))
    ));
    let mut changed_snapshot = snapshot.clone();
    changed_snapshot.revision = 2;
    let changed = encode_snapshot(&identity, &changed_snapshot).unwrap();
    assert!(decode_outbox(&identity, &changed_snapshot, &changed, &outbox).is_err());
    let DurableMessage::Vote(mut bad) = message else {
        unreachable!()
    };
    bad.signature[0] ^= 1;
    let bad_outbox = encode_outbox(
        &snapshot,
        &bytes,
        &replay(
            snapshot.revision,
            Some(DurableMessage::Vote(bad)),
            ReplayEvidence::None,
        ),
    )
    .unwrap();
    assert!(decode_outbox(&identity, &snapshot, &bytes, &bad_outbox).is_err());

    // Even freshly recomputed record pins cannot relabel a signed prevote step
    // as state-only or a leader-proposal transition.
    let missing_vote = encode_outbox(
        &snapshot,
        &bytes,
        &replay(snapshot.revision, None, ReplayEvidence::None),
    )
    .unwrap();
    assert!(decode_outbox(&identity, &snapshot, &bytes, &missing_vote).is_err());
    let value = [0x44; 32];
    let proposal = Proposal::sign(
        identity.context,
        0,
        value,
        None,
        &identity.set,
        &identity.key,
    )
    .unwrap();
    let mut proposed = snapshot.clone();
    proposed.proposed = Some((0, value, None));
    let saved = encode_snapshot(&identity, &proposed).unwrap();
    let message = DurableMessage::Proposal(proposal.clone());
    let proposal_record = replay(
        proposed.revision,
        Some(message.clone()),
        proposal_evidence(&identity, &proposal, None),
    );
    let wrong_step = encode_outbox(&proposed, &saved, &proposal_record).unwrap();
    assert!(decode_outbox(&identity, &proposed, &saved, &wrong_step).is_err());

    let precommit = Vote::sign(
        identity.context,
        0,
        wire::Phase::Precommit,
        Some(value),
        &identity.set,
        &identity.key,
    )
    .unwrap();
    let qc = Quorum::from_votes(&identity.set, vec![precommit])
        .unwrap()
        .verify(&identity.set)
        .unwrap();
    proposed.state = State::new(identity.context)
        .unwrap()
        .prepare_decision(&proposal.verify(&identity.set).unwrap(), value, &qc)
        .unwrap()
        .into_parts()
        .0;
    let saved = encode_snapshot(&identity, &proposed).unwrap();
    let missing_decision = encode_outbox(&proposed, &saved, &proposal_record).unwrap();
    assert!(decode_outbox(&identity, &proposed, &saved, &missing_decision).is_err());
}

#[test]
fn journal_corrupt_truncated_noncanonical_parent_and_declared_lengths_fail() {
    let mut identity = identity();
    let (snapshot, _) = nil_snapshot(&identity);
    let bytes = encode_snapshot(&identity, &snapshot).unwrap();
    for length in 0..bytes.len() {
        assert!(decode_snapshot(&identity, &bytes[..length]).is_err());
    }
    for offset in 0..bytes.len() {
        let mut damaged = bytes.clone();
        damaged[offset] ^= 1;
        assert!(decode_snapshot(&identity, &damaged).is_err());
    }
    let mut trailing = bytes[..bytes.len() - 32].to_vec();
    trailing.push(0);
    assert!(decode_snapshot(&identity, &seal(trailing).unwrap()).is_err());
    // No proposal: state length immediately follows magic, ID, parent, revision, tag.
    let mut oversized = bytes[..bytes.len() - 32].to_vec();
    oversized[81..85].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode_snapshot(&identity, &seal(oversized).unwrap()).is_err());
    identity.parent.state_version += 1;
    assert!(decode_snapshot(&identity, &bytes).is_err());
}

#[test]
fn restored_locked_state_requires_exact_valid_quorum_not_a_claimed_round() {
    let identity = identity();
    let value = [0x55; 32];
    let proposal = Proposal::sign(
        identity.context,
        0,
        value,
        None,
        &identity.set,
        &identity.key,
    )
    .unwrap()
    .verify(&identity.set)
    .unwrap();
    let state = State::new(identity.context)
        .unwrap()
        .prepare_proposal(&proposal, value, None)
        .unwrap()
        .into_parts()
        .0;
    let vote = Vote::sign(
        identity.context,
        0,
        wire::Phase::Prevote,
        Some(value),
        &identity.set,
        &identity.key,
    )
    .unwrap();
    let qc = Quorum::from_votes(&identity.set, vec![vote]).unwrap();
    let verified = qc.verify(&identity.set).unwrap();
    let locked = state
        .prepare_prevote_quorum(&verified, Some((&proposal, value)))
        .unwrap()
        .into_parts()
        .0;
    let mut snapshot = Snapshot {
        state: locked,
        revision: 3,
        proposed: None,
        witness: Some(qc),
        replay: ReplayIndex::default(),
    };
    let saved = encode_snapshot(&identity, &snapshot).unwrap();
    decode_snapshot(&identity, &saved).unwrap();
    snapshot.witness = None;
    let missing = encode_snapshot(&identity, &snapshot).unwrap();
    assert!(decode_snapshot(&identity, &missing).is_err());
    let vote = Vote::sign(
        identity.context,
        0,
        wire::Phase::Precommit,
        Some(value),
        &identity.set,
        &identity.key,
    )
    .unwrap();
    snapshot.witness = Some(Quorum::from_votes(&identity.set, vec![vote]).unwrap());
    let wrong_phase = encode_snapshot(&identity, &snapshot).unwrap();
    assert!(decode_snapshot(&identity, &wrong_phase).is_err());
}

fn signed_proposal(identity: &Identity, round: u64, value: Hash, valid: Option<u64>) -> Proposal {
    Proposal::sign(
        identity.context,
        round,
        value,
        valid,
        &identity.set,
        &identity.key,
    )
    .unwrap()
}

fn quorum(identity: &Identity, round: u64, value: Hash) -> VerifiedQuorum {
    Quorum::from_votes(
        &identity.set,
        vec![Vote::sign(
            identity.context,
            round,
            wire::Phase::Prevote,
            Some(value),
            &identity.set,
            &identity.key,
        )
        .unwrap()],
    )
    .unwrap()
    .verify(&identity.set)
    .unwrap()
}

fn vote_record(
    identity: &Identity,
    revision: u64,
    round: u64,
    phase: wire::Phase,
    value: Option<Hash>,
    evidence: ReplayEvidence,
) -> ReplayRecord {
    replay(
        revision,
        Some(DurableMessage::Vote(
            Vote::sign(
                identity.context,
                round,
                phase,
                value,
                &identity.set,
                &identity.key,
            )
            .unwrap(),
        )),
        evidence,
    )
}

fn certified(
    identity: &Identity,
    proposal: &Proposal,
    certificate: VerifiedQuorum,
) -> ReplayEvidence {
    ReplayEvidence::Certified {
        proposal: proposal.verify(&identity.set).unwrap(),
        certificate,
        candidate: locator(proposal.value),
    }
}

fn timeout(state: State) -> State {
    state
        .prepare_timeout(LocalTimeout {
            context: *state.context(),
            round: state.round(),
            step: state.step(),
        })
        .unwrap()
        .into_parts()
        .0
}

fn late_fixture(identity: &Identity) -> (Snapshot, Vec<ReplayRecord>) {
    // Codec/kernel coverage with genuine signatures; these synthetic values
    // and contradictory single-validator votes are not an execution fixture.
    let a = [0x31; 32];
    let b = [0x32; 32];
    let p0 = signed_proposal(identity, 0, a, None);
    let qc0 = quorum(identity, 0, a);
    let state = State::new(identity.context)
        .unwrap()
        .prepare_proposal(&p0.verify(&identity.set).unwrap(), a, None)
        .unwrap()
        .into_parts()
        .0
        .prepare_prevote_quorum(&qc0, Some((&p0.verify(&identity.set).unwrap(), a)))
        .unwrap()
        .into_parts()
        .0;
    let locked = vote_record(
        identity,
        3,
        0,
        wire::Phase::Precommit,
        Some(a),
        certified(identity, &p0, qc0),
    );
    let state = state.prepare_round_change(1).unwrap().into_parts().0;
    let prevote = vote_record(
        identity,
        5,
        1,
        wire::Phase::Prevote,
        None,
        ReplayEvidence::None,
    );
    let precommit = vote_record(
        identity,
        6,
        1,
        wire::Phase::Precommit,
        None,
        ReplayEvidence::None,
    );
    let state = timeout(timeout(state));
    let p1 = signed_proposal(identity, 1, b, None);
    let qc1 = quorum(identity, 1, b);
    let step = state
        .prepare_prevote_quorum(&qc1, Some((&p1.verify(&identity.set).unwrap(), b)))
        .unwrap();
    assert!(step.intent().is_none());
    let late = replay(7, None, certified(identity, &p1, qc1.clone()));
    let snapshot = Snapshot {
        state: step.into_parts().0,
        revision: 7,
        proposed: None,
        witness: Some(qc1.into_quorum()),
        replay: ReplayIndex {
            proposal: None,
            prevote: Some(reference(&prevote)),
            precommit: Some(reference(&precommit)),
            locked: Some(reference(&locked)),
            valid: Some(reference(&late)),
        },
    };
    (snapshot, vec![locked, prevote, precommit, late])
}

fn historical_bytes(identity: &Identity, record: &ReplayRecord) -> Vec<u8> {
    // Historical lookup intentionally does not reload its old snapshot; the
    // immutable payload is authenticated by the latest snapshot's reference.
    let snapshot = Snapshot {
        state: State::new(identity.context).unwrap(),
        revision: record.revision,
        proposed: None,
        witness: None,
        replay: ReplayIndex::default(),
    };
    encode_outbox(&snapshot, b"historical snapshot pin", record).unwrap()
}

#[test]
fn late_quorum_replay_preserves_nil_votes_and_distinct_old_lock() {
    let identity = identity();
    let (snapshot, records) = late_fixture(&identity);
    let saved = encode_snapshot(&identity, &snapshot).unwrap();
    let restored = decode_snapshot(&identity, &saved).unwrap();
    let mut recovered = Vec::new();
    for record in &records {
        recovered.push(
            decode_replay(
                &identity,
                &reference(record),
                &historical_bytes(&identity, record),
            )
            .unwrap(),
        );
    }
    validate_replay_index(&identity, &restored, &recovered).unwrap();
    let latest = records.last().unwrap();
    let latest_bytes = encode_outbox(&snapshot, &saved, latest).unwrap();
    let latest = decode_outbox(&identity, &snapshot, &saved, &latest_bytes).unwrap();
    assert!(latest.message.is_none());
    assert_eq!(restored.state.locked(), Some((0, [0x31; 32])));
    assert_eq!(restored.state.valid(), Some((1, [0x32; 32])));
    for record in &recovered[1..3] {
        let Some(DurableMessage::Vote(vote)) = &record.message else {
            panic!("lost nil vote")
        };
        assert_eq!(vote.value, None);
    }
    let mut advanced = snapshot.clone();
    advanced.state = advanced
        .state
        .prepare_round_change(4)
        .unwrap()
        .into_parts()
        .0;
    advanced.revision = 8;
    advanced.replay.clear_current();
    validate_replay_index(
        &identity,
        &advanced,
        &[records[0].clone(), records[3].clone()],
    )
    .unwrap();
    assert_eq!(advanced.state.locked(), snapshot.state.locked());
    assert_eq!(advanced.state.valid(), snapshot.state.valid());
}

#[test]
fn nil_precommit_rejects_a_resealed_current_round_lock() {
    let identity = identity();
    let (mut snapshot, records) = late_fixture(&identity);
    let value = [0x32; 32];
    let proposal = signed_proposal(&identity, 1, value, None);
    let verified = proposal.verify(&identity.set).unwrap();
    let certificate = quorum(&identity, 1, value);
    // Both states and each signature are individually well formed. Mixing the
    // nil precommit from one path with the current lock from another is not.
    snapshot.state = State::new(identity.context)
        .unwrap()
        .prepare_round_change(1)
        .unwrap()
        .into_parts()
        .0
        .prepare_proposal(&verified, value, None)
        .unwrap()
        .into_parts()
        .0
        .prepare_prevote_quorum(&certificate, Some((&verified, value)))
        .unwrap()
        .into_parts()
        .0;
    let locked = vote_record(
        &identity,
        8,
        1,
        wire::Phase::Precommit,
        Some(value),
        certified(&identity, &proposal, certificate),
    );
    snapshot.revision = 8;
    snapshot.replay.locked = Some(reference(&locked));
    snapshot.replay.valid = Some(reference(&locked));
    let saved = encode_snapshot(&identity, &snapshot).unwrap();
    let restored = decode_snapshot(&identity, &saved).unwrap();
    let locked = decode_replay(
        &identity,
        &reference(&locked),
        &historical_bytes(&identity, &locked),
    )
    .unwrap();
    let error = validate_replay_index(
        &identity,
        &restored,
        &[records[1].clone(), records[2].clone(), locked],
    )
    .unwrap_err();
    assert!(error.to_string().contains("nil precommit"));
}

#[test]
fn state_only_certified_record_cannot_replace_lock_origin() {
    let identity = identity();
    let (mut snapshot, mut records) = late_fixture(&identity);
    // Recompute both the payload pin and enclosing seal: a valid state-only
    // certificate may update valid, but cannot attest that this signer locked.
    records[0].message = None;
    snapshot.replay.locked = Some(reference(&records[0]));
    let saved = encode_snapshot(&identity, &snapshot).unwrap();
    let restored = decode_snapshot(&identity, &saved).unwrap();
    records[0] = decode_replay(
        &identity,
        &reference(&records[0]),
        &historical_bytes(&identity, &records[0]),
    )
    .unwrap();
    let error = validate_replay_index(&identity, &restored, &records).unwrap_err();
    assert!(error.to_string().contains("local non-nil precommit"));
}

#[test]
fn retained_old_lock_rejects_resealed_unjustified_other_prevote() {
    let identity = identity();
    for valid_round in [None, Some(0)] {
        let (mut snapshot, locked) = retained_lock_fixture(&identity);
        let value = [0x62; 32];
        let proposal = signed_proposal(&identity, 3, value, valid_round);
        let proof = valid_round.map(|round| quorum(&identity, round, value));
        let step = snapshot
            .state
            .prepare_proposal(
                &proposal.verify(&identity.set).unwrap(),
                value,
                proof.as_ref(),
            )
            .unwrap();
        assert_eq!(step.intent().unwrap().value, None);
        snapshot.state = step.into_parts().0;
        snapshot.revision = 5;
        let evidence = proposal_evidence(&identity, &proposal, proof);
        let nil = vote_record(
            &identity,
            5,
            3,
            wire::Phase::Prevote,
            None,
            evidence.clone(),
        );
        snapshot.replay.prevote = Some(reference(&nil));
        validate_replay_index(&identity, &snapshot, &[locked.clone(), nil]).unwrap();

        // The real transition rejected B. Re-signing a non-nil vote and
        // resealing all pins must not turn that unreachable branch into replay.
        let forged = vote_record(&identity, 5, 3, wire::Phase::Prevote, Some(value), evidence);
        snapshot.replay.prevote = Some(reference(&forged));
        let saved = encode_snapshot(&identity, &snapshot).unwrap();
        let restored = decode_snapshot(&identity, &saved).unwrap();
        let forged = decode_replay(
            &identity,
            &reference(&forged),
            &historical_bytes(&identity, &forged),
        )
        .unwrap();
        let error = validate_replay_index(&identity, &restored, &[locked, forged]).unwrap_err();
        assert!(error.to_string().contains("retained lock"));
    }
}

fn retained_lock_fixture(identity: &Identity) -> (Snapshot, ReplayRecord) {
    let value = [0x61; 32];
    let proposal = signed_proposal(identity, 1, value, None);
    let verified = proposal.verify(&identity.set).unwrap();
    let certificate = quorum(identity, 1, value);
    let state = State::new(identity.context)
        .unwrap()
        .prepare_round_change(1)
        .unwrap()
        .into_parts()
        .0
        .prepare_proposal(&verified, value, None)
        .unwrap()
        .into_parts()
        .0
        .prepare_prevote_quorum(&certificate, Some((&verified, value)))
        .unwrap()
        .into_parts()
        .0
        .prepare_round_change(3)
        .unwrap()
        .into_parts()
        .0;
    let locked = vote_record(
        identity,
        3,
        1,
        wire::Phase::Precommit,
        Some(value),
        certified(identity, &proposal, certificate.clone()),
    );
    let snapshot = Snapshot {
        state,
        revision: 4,
        proposed: None,
        witness: Some(certificate.into_quorum()),
        replay: ReplayIndex {
            locked: Some(reference(&locked)),
            valid: Some(reference(&locked)),
            ..ReplayIndex::default()
        },
    };
    (snapshot, locked)
}

#[test]
fn retained_lock_rejects_resealed_equal_round_conflicting_justification() {
    let identity = identity();
    let (mut snapshot, locked) = retained_lock_fixture(&identity);
    let value = [0x62; 32];
    let proposal = signed_proposal(&identity, 3, value, Some(1));
    let proof = quorum(&identity, 1, value);
    // The kernel rejects B's QC at the exact round of retained lock A. Even
    // genuine signatures cannot make this mixed transcript a reachable vote.
    assert!(snapshot
        .state
        .prepare_proposal(
            &proposal.verify(&identity.set).unwrap(),
            value,
            Some(&proof)
        )
        .is_err());
    snapshot.state = timeout(snapshot.state);
    snapshot.revision = 5;
    let forged = vote_record(
        &identity,
        5,
        3,
        wire::Phase::Prevote,
        Some(value),
        proposal_evidence(&identity, &proposal, Some(proof)),
    );
    snapshot.replay.prevote = Some(reference(&forged));
    let saved = encode_snapshot(&identity, &snapshot).unwrap();
    let restored = decode_snapshot(&identity, &saved).unwrap();
    let forged = decode_replay(
        &identity,
        &reference(&forged),
        &historical_bytes(&identity, &forged),
    )
    .unwrap();
    let error = validate_replay_index(&identity, &restored, &[locked, forged]).unwrap_err();
    assert!(error.to_string().contains("retained lock"));
}

#[test]
fn retained_lock_accepts_same_value_or_newer_proposal_justification() {
    let identity = identity();
    for (value, valid_round) in [([0x61; 32], None), ([0x62; 32], Some(2))] {
        let (mut snapshot, locked) = retained_lock_fixture(&identity);
        let proposal = signed_proposal(&identity, 3, value, valid_round);
        let proof = valid_round.map(|round| quorum(&identity, round, value));
        let step = snapshot
            .state
            .prepare_proposal(
                &proposal.verify(&identity.set).unwrap(),
                value,
                proof.as_ref(),
            )
            .unwrap();
        assert_eq!(step.intent().unwrap().value, Some(value));
        snapshot.state = step.into_parts().0;
        snapshot.revision = 5;
        let prevote = vote_record(
            &identity,
            5,
            3,
            wire::Phase::Prevote,
            Some(value),
            proposal_evidence(&identity, &proposal, proof),
        );
        snapshot.replay.prevote = Some(reference(&prevote));
        validate_replay_index(&identity, &snapshot, &[locked, prevote]).unwrap();
        assert_eq!(snapshot.state.locked(), Some((1, [0x61; 32])));
    }
}

#[test]
fn current_round_lock_does_not_retroactively_restrict_original_prevote() {
    let mut identity = identity();
    let keys: Vec<_> = (0..4)
        .map(|index| SigningKey::from_bytes(&[0x81 + index; 32]))
        .collect();
    identity.set = Arc::new(
        ValidatorSet::new(
            71,
            1,
            1,
            keys.iter()
                .map(|key| Validator::new(key.verifying_key().to_bytes(), 1).unwrap())
                .collect(),
        )
        .unwrap(),
    );
    identity.context.validator_set_hash = identity.set.hash();
    let leader = identity.set.leader(identity.context.height, 0).unwrap();
    let leader_key = keys
        .iter()
        .find(|key| {
            key.verifying_key().to_bytes() == *identity.set.member(&leader).unwrap().public_key()
        })
        .unwrap();
    identity.key = keys
        .iter()
        .find(|key| key.verifying_key() != leader_key.verifying_key())
        .unwrap()
        .clone();
    identity.validator = Validator::new(identity.key.verifying_key().to_bytes(), 1)
        .unwrap()
        .id();
    let proposal = |value| {
        Proposal::sign(identity.context, 0, value, None, &identity.set, leader_key).unwrap()
    };
    let a = [0x71; 32];
    let b = [0x72; 32];
    let voted = proposal(b);
    let step = State::new(identity.context)
        .unwrap()
        .prepare_proposal(&voted.verify(&identity.set).unwrap(), b, None)
        .unwrap();
    assert_eq!(step.intent().unwrap().value, Some(b));
    let locked = proposal(a);
    let certificate = Quorum::from_votes(
        &identity.set,
        keys.iter()
            .filter(|key| key.verifying_key() != identity.key.verifying_key())
            .map(|key| {
                Vote::sign(
                    identity.context,
                    0,
                    wire::Phase::Prevote,
                    Some(a),
                    &identity.set,
                    key,
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
    .verify(&identity.set)
    .unwrap();
    // A Byzantine proposer may have equivocated. A later genuine QC for A
    // contains only the other three validators' votes: this local signer never
    // double-prevotes, yet locks A after its original prevote for B.
    let state = step
        .into_parts()
        .0
        .prepare_prevote_quorum(
            &certificate,
            Some((&locked.verify(&identity.set).unwrap(), a)),
        )
        .unwrap()
        .into_parts()
        .0;
    let prevote = vote_record(
        &identity,
        1,
        0,
        wire::Phase::Prevote,
        Some(b),
        proposal_evidence(&identity, &voted, None),
    );
    let precommit = vote_record(
        &identity,
        2,
        0,
        wire::Phase::Precommit,
        Some(a),
        certified(&identity, &locked, certificate.clone()),
    );
    let snapshot = Snapshot {
        state,
        revision: 2,
        proposed: None,
        witness: Some(certificate.into_quorum()),
        replay: ReplayIndex {
            prevote: Some(reference(&prevote)),
            precommit: Some(reference(&precommit)),
            locked: Some(reference(&precommit)),
            valid: Some(reference(&precommit)),
            ..ReplayIndex::default()
        },
    };
    validate_replay_index(&identity, &snapshot, &[prevote, precommit]).unwrap();
    assert_eq!(snapshot.state.locked(), Some((0, a)));
}

#[test]
fn original_proposal_justification_survives_newer_valid_witness() {
    let identity = identity();
    let value = [0x42; 32];
    let p0 = signed_proposal(&identity, 0, value, None);
    let qc0 = quorum(&identity, 0, value);
    let state = State::new(identity.context)
        .unwrap()
        .prepare_proposal(&p0.verify(&identity.set).unwrap(), value, None)
        .unwrap()
        .into_parts()
        .0
        .prepare_prevote_quorum(&qc0, Some((&p0.verify(&identity.set).unwrap(), value)))
        .unwrap()
        .into_parts()
        .0
        .prepare_round_change(1)
        .unwrap()
        .into_parts()
        .0;
    let p1 = signed_proposal(&identity, 1, value, Some(0));
    let old_proposal = replay(
        4,
        Some(DurableMessage::Proposal(p1.clone())),
        proposal_evidence(&identity, &p1, Some(qc0.clone())),
    );
    let prevote = vote_record(
        &identity,
        5,
        1,
        wire::Phase::Prevote,
        Some(value),
        proposal_evidence(&identity, &p1, Some(qc0.clone())),
    );
    let state = state
        .prepare_proposal(&p1.verify(&identity.set).unwrap(), value, Some(&qc0))
        .unwrap()
        .into_parts()
        .0;
    let qc1 = quorum(&identity, 1, value);
    let state = state
        .prepare_prevote_quorum(&qc1, Some((&p1.verify(&identity.set).unwrap(), value)))
        .unwrap()
        .into_parts()
        .0;
    let precommit = vote_record(
        &identity,
        6,
        1,
        wire::Phase::Precommit,
        Some(value),
        certified(&identity, &p1, qc1.clone()),
    );
    let snapshot = Snapshot {
        state,
        revision: 6,
        proposed: Some((1, value, Some(0))),
        witness: Some(qc1.clone().into_quorum()),
        replay: ReplayIndex {
            proposal: Some(reference(&old_proposal)),
            prevote: Some(reference(&prevote)),
            precommit: Some(reference(&precommit)),
            locked: Some(reference(&precommit)),
            valid: Some(reference(&precommit)),
        },
    };
    let records = vec![old_proposal.clone(), prevote, precommit];
    validate_replay_index(&identity, &snapshot, &records).unwrap();
    let recovered = decode_replay(
        &identity,
        &reference(&old_proposal),
        &historical_bytes(&identity, &old_proposal),
    )
    .unwrap();
    let ReplayEvidence::Proposal { justification, .. } = recovered.evidence else {
        panic!("lost proposal proof")
    };
    assert_eq!(justification.unwrap().round(), 0);
    assert_eq!(snapshot.witness.as_ref().unwrap().votes[0].round, 1);
    let mut changed = old_proposal;
    changed.evidence = proposal_evidence(&identity, &p1, Some(qc1));
    // Fresh payload digest and outer seal still cannot replace QC0 by QC1.
    assert!(decode_replay(
        &identity,
        &reference(&changed),
        &historical_bytes(&identity, &changed)
    )
    .is_err());
}

#[test]
fn replay_roles_digest_domain_and_legacy_snapshot_are_fail_closed() {
    let identity = identity();
    let (snapshot, records) = late_fixture(&identity);
    let saved = encode_snapshot(&identity, &snapshot).unwrap();
    let mut legacy = saved[..saved.len() - 32].to_vec();
    legacy[..8].copy_from_slice(b"NVSIGN01");
    assert!(decode_snapshot(&identity, &seal(legacy).unwrap())
        .err()
        .unwrap()
        .to_string()
        .contains("legacy"));
    let mut future = snapshot.clone();
    future.replay.valid.as_mut().unwrap().revision = snapshot.revision + 1;
    assert!(decode_snapshot(&identity, &encode_snapshot(&identity, &future).unwrap()).is_err());
    let mut duplicate = records.clone();
    duplicate[1] = duplicate[0].clone();
    assert!(validate_replay_index(&identity, &snapshot, &duplicate).is_err());
    assert!(validate_replay_index(&identity, &snapshot, &records[..3]).is_err());
    let mut wrong_kind = snapshot.clone();
    wrong_kind.replay.locked = wrong_kind.replay.prevote;
    assert!(validate_replay_index(&identity, &wrong_kind, &records[1..]).is_err());
    let mut conflicting = snapshot.clone();
    conflicting.replay.locked = conflicting.replay.valid;
    conflicting.replay.locked.as_mut().unwrap().digest[0] ^= 1;
    assert!(validate_replay_index(&identity, &conflicting, &records).is_err());
    let latest = records.last().unwrap();
    let mut bytes = historical_bytes(&identity, latest);
    // Change the candidate locator and recompute the enclosing checksum only.
    let offset = bytes.len() - 33;
    bytes[offset] ^= 1;
    bytes.truncate(bytes.len() - 32);
    assert!(decode_replay(&identity, &reference(latest), &seal(bytes).unwrap()).is_err());
    let mut other = self::identity();
    other.context.genesis_config_commitment = [0x77; 32];
    assert!(decode_replay(
        &other,
        &reference(latest),
        &historical_bytes(&identity, latest)
    )
    .is_err());
    let mut zero = latest.clone();
    let ReplayEvidence::Certified { candidate, .. } = &mut zero.evidence else {
        unreachable!()
    };
    candidate.candidate_id = [0; 32];
    assert!(decode_replay(
        &identity,
        &reference(&zero),
        &historical_bytes(&identity, &zero)
    )
    .is_err());
}

#[test]
fn maximum_validator_decision_transition_fits_unchanged_metadata_budgets() {
    // Codec/budget coverage only: this synthetic value is not an executed block
    // or finality evidence. Retain every validator, not merely quorum weight.
    assert_eq!(wire::MAX_VALIDATORS, 1024);
    let keys: Vec<_> = (0..wire::MAX_VALIDATORS)
        .map(|index| {
            let mut seed = [0x78; 32];
            seed[..8].copy_from_slice(&(index as u64).to_be_bytes());
            SigningKey::from_bytes(&seed)
        })
        .collect();
    let members = keys
        .iter()
        .map(|key| Validator::new(key.verifying_key().to_bytes(), 1).unwrap())
        .collect();
    let mut identity = identity();
    identity.set = Arc::new(ValidatorSet::new(71, 1, 1, members).unwrap());
    identity.context.validator_set_hash = identity.set.hash();
    identity.validator = identity.set.leader(identity.context.height, 0).unwrap();
    identity.key = keys
        .iter()
        .find(|key| {
            key.verifying_key().to_bytes()
                == *identity
                    .set
                    .member(&identity.validator)
                    .unwrap()
                    .public_key()
        })
        .unwrap()
        .clone();
    let value = [0x79; 32];
    let proposal = Proposal::sign(
        identity.context,
        0,
        value,
        None,
        &identity.set,
        &identity.key,
    )
    .unwrap();
    let verified_proposal = proposal.verify(&identity.set).unwrap();
    let quorum = |phase| {
        let votes = keys
            .iter()
            .map(|key| {
                Vote::sign(identity.context, 0, phase, Some(value), &identity.set, key).unwrap()
            })
            .collect();
        let quorum = Quorum::from_votes(&identity.set, votes).unwrap();
        let encoded = wire::encode_quorum(&quorum).unwrap();
        assert_eq!(quorum.votes.len(), wire::MAX_VALIDATORS);
        assert_eq!(encoded.len(), wire::MAX_WIRE_BYTES);
        assert_eq!(wire::decode_quorum(&encoded).unwrap(), quorum);
        quorum
    };
    let prevote = quorum(wire::Phase::Prevote);
    let precommit = quorum(wire::Phase::Precommit);
    let state = State::new(identity.context)
        .unwrap()
        .prepare_proposal(&verified_proposal, value, None)
        .unwrap()
        .into_parts()
        .0
        .prepare_prevote_quorum(
            &prevote.verify(&identity.set).unwrap(),
            Some((&verified_proposal, value)),
        )
        .unwrap()
        .into_parts()
        .0;
    let old = Snapshot {
        state,
        revision: 3,
        proposed: Some((0, value, None)),
        witness: Some(prevote),
        replay: ReplayIndex::default(),
    };
    let next = Snapshot {
        state: old
            .state
            .prepare_decision(
                &verified_proposal,
                value,
                &precommit.verify(&identity.set).unwrap(),
            )
            .unwrap()
            .into_parts()
            .0,
        revision: 4,
        ..old.clone()
    };
    let old_saved = encode_snapshot(&identity, &old).unwrap();
    let next_saved = encode_snapshot(&identity, &next).unwrap();
    for (snapshot, saved) in [(&old, &old_saved), (&next, &next_saved)] {
        let recovered = decode_snapshot(&identity, saved).unwrap();
        assert_eq!(recovered.state, snapshot.state);
        assert_eq!(recovered.witness, snapshot.witness);
        assert_eq!(recovered.witness.unwrap().votes.len(), wire::MAX_VALIDATORS);
    }
    // A proposal justified by a full old-round QC must also fit old/new
    // snapshots plus its own evidence QC, not merely a minimal quorum subset.
    let proposer_id = identity.set.leader(identity.context.height, 1).unwrap();
    let proposer_key = keys
        .iter()
        .find(|key| {
            key.verifying_key().to_bytes()
                == *identity.set.member(&proposer_id).unwrap().public_key()
        })
        .unwrap()
        .clone();
    let proposer = Identity {
        owner: Arc::new(()),
        context: identity.context,
        parent: identity.parent,
        set: identity.set.clone(),
        key: proposer_key,
        validator: proposer_id,
    };
    let proposed = Proposal::sign(
        proposer.context,
        1,
        value,
        Some(0),
        &proposer.set,
        &proposer.key,
    )
    .unwrap();
    let proposal_record = replay(
        4,
        Some(DurableMessage::Proposal(proposed.clone())),
        proposal_evidence(
            &proposer,
            &proposed,
            Some(old.witness.as_ref().unwrap().verify(&proposer.set).unwrap()),
        ),
    );
    let proposal_snapshot = Snapshot {
        state: old.state.prepare_round_change(1).unwrap().into_parts().0,
        revision: 4,
        proposed: Some((1, value, Some(0))),
        witness: old.witness.clone(),
        replay: ReplayIndex {
            proposal: Some(reference(&proposal_record)),
            ..ReplayIndex::default()
        },
    };
    let proposal_saved = encode_snapshot(&proposer, &proposal_snapshot).unwrap();
    let proposal_outbox =
        encode_outbox(&proposal_snapshot, &proposal_saved, &proposal_record).unwrap();
    decode_outbox(
        &proposer,
        &proposal_snapshot,
        &proposal_saved,
        &proposal_outbox,
    )
    .unwrap();
    let proposal_old = encode_snapshot(&proposer, &old).unwrap();
    let proposal_transition = MetaTransition::new(vec![
        MetaChange {
            key: MetaKey::ConsensusState(proposer.validator),
            expected: Some(proposal_old),
            value: proposal_saved,
        },
        MetaChange {
            key: MetaKey::ConsensusOutbox {
                validator: proposer.validator,
                sequence: 4,
            },
            expected: None,
            value: proposal_outbox,
        },
        MetaChange {
            key: MetaKey::ChainHead,
            expected: Some(vec![0x80; 4096]),
            value: vec![0x81; 4096],
        },
        MetaChange {
            key: MetaKey::ChainBlock {
                height: proposer.context.height,
            },
            expected: None,
            value: vec![0x82; 4096],
        },
    ])
    .unwrap();
    assert!(proposal_transition.retained_bytes() <= 1024 * 1024);
    let decision = DurableMessage::Decision {
        proposal,
        certificate: precommit,
    };
    let record = replay(next.revision, Some(decision), ReplayEvidence::None);
    let outbox = encode_outbox(&next, &next_saved, &record).unwrap();
    let (archived_proposal, archived_qc) =
        decode_archived_decision(&outbox, next.revision).unwrap();
    assert_eq!(archived_qc.votes.len(), wire::MAX_VALIDATORS);
    let mut legacy = outbox[..outbox.len() - 33].to_vec();
    legacy[..8].copy_from_slice(ARCHIVED_OUT);
    let (old_proposal, old_qc) =
        decode_archived_decision(&seal(legacy).unwrap(), next.revision).unwrap();
    assert_eq!(old_proposal, archived_proposal);
    assert_eq!(old_qc, archived_qc);
    let mut extra_qc = record.clone();
    extra_qc.evidence = certified(
        &identity,
        &archived_proposal,
        old.witness.as_ref().unwrap().verify(&identity.set).unwrap(),
    );
    assert!(encode_outbox(&next, &next_saved, &extra_qc).is_err());
    let recovered = decode_outbox(&identity, &next, &next_saved, &outbox).unwrap();
    let Some(DurableMessage::Decision { certificate, .. }) = &recovered.message else {
        panic!("decision outbox lost its certificate");
    };
    assert_eq!(certificate.votes.len(), wire::MAX_VALIDATORS);
    assert_eq!(
        encode_outbox(&next, &next_saved, &recovered).unwrap(),
        outbox
    );

    // Reserve the entire 4 KiB chain-codec envelope for each old/new head and
    // immutable archive record. These opaque reservations deliberately exceed
    // actual small records; the certificate lives only in the exact outbox.
    let chain_record_bytes = 4096;
    let retained_values =
        old_saved.len() + next_saved.len() + outbox.len() + 3 * chain_record_bytes;
    assert_eq!(MAX_RECORD, 512 * 1024);
    assert!([&old_saved, &next_saved, &outbox]
        .into_iter()
        .all(|bytes| bytes.len() <= 512 * 1024));
    assert!(retained_values <= 1024 * 1024);
    let transition = MetaTransition::new(vec![
        MetaChange {
            key: MetaKey::ConsensusState(identity.validator),
            expected: Some(old_saved),
            value: next_saved,
        },
        MetaChange {
            key: MetaKey::ConsensusOutbox {
                validator: identity.validator,
                sequence: next.revision,
            },
            expected: None,
            value: outbox,
        },
        MetaChange {
            key: MetaKey::ChainHead,
            expected: Some(vec![0x80; chain_record_bytes]),
            value: vec![0x81; chain_record_bytes],
        },
        MetaChange {
            key: MetaKey::ChainBlock {
                height: identity.context.height,
            },
            expected: None,
            value: vec![0x82; chain_record_bytes],
        },
    ])
    .unwrap();
    // The owner also retains logical key bytes, not just the checked values.
    assert!(transition.retained_bytes() > retained_values);
    assert!(transition.retained_bytes() <= 1024 * 1024);
}
