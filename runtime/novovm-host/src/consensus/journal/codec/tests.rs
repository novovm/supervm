use super::*;

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
    (
        Snapshot {
            state,
            revision: 1,
            proposed: None,
            witness: None,
        },
        DurableMessage::Vote(vote),
    )
}

#[test]
fn journal_snapshot_and_outbox_are_exactly_bound_and_validate_real_signature() {
    let identity = identity();
    let (snapshot, message) = nil_snapshot(&identity);
    let bytes = encode_snapshot(&identity, &snapshot).unwrap();
    let outbox = encode_outbox(&snapshot, &bytes, Some(&message)).unwrap();
    let recovered = decode_snapshot(&identity, &bytes).unwrap();
    assert_eq!(recovered.state, snapshot.state);
    assert!(matches!(
        decode_outbox(&identity, &recovered, &bytes, &outbox).unwrap(),
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
    let bad_outbox = encode_outbox(&snapshot, &bytes, Some(&DurableMessage::Vote(bad))).unwrap();
    assert!(decode_outbox(&identity, &snapshot, &bytes, &bad_outbox).is_err());

    // Even freshly recomputed record pins cannot relabel a signed prevote step
    // as state-only or a leader-proposal transition.
    let missing_vote = encode_outbox(&snapshot, &bytes, None).unwrap();
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
    let wrong_step = encode_outbox(&proposed, &saved, Some(&message)).unwrap();
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
    let missing_decision = encode_outbox(&proposed, &saved, Some(&message)).unwrap();
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
