//! Source: `runtime/novovm-host/src/consensus/wire/tests.rs` at `2da3583c`.
//! Migrated rules/tests; no production signing or publication is activated here.
//!
use super::*;

fn fixture() -> (Vec<SigningKey>, ValidatorSet) {
    let keys: Vec<_> = (1u8..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect();
    let members = keys
        .iter()
        .map(|key| Validator::new(key.verifying_key().to_bytes(), 1).unwrap())
        .collect();
    (keys, ValidatorSet::new(91, 1, 1, members).unwrap())
}

fn context(set: &ValidatorSet) -> Context {
    Context {
        chain_id: set.chain_id(),
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        epoch: set.epoch(),
        validator_set_hash: set.hash(),
        height: 2,
        parent_block_hash: [3; 32],
        parent_decision_hash: [4; 32],
    }
}

fn leader<'a>(
    keys: &'a [SigningKey],
    set: &ValidatorSet,
    height: u64,
    round: u64,
) -> &'a SigningKey {
    let id = set.leader(height, round).unwrap();
    keys.iter()
        .find(|key| {
            Validator::new(key.verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == id
        })
        .unwrap()
}

fn votes(keys: &[SigningKey], set: &ValidatorSet, phase: Phase, value: Option<Hash>) -> Vec<Vote> {
    keys.iter()
        .take(3)
        .map(|key| Vote::sign(context(set), 2, phase, value, set, key).unwrap())
        .collect()
}

#[test]
fn validator_sets_are_canonical_weighted_and_domain_bound() {
    let (_, set) = fixture();
    assert_eq!(set.total_weight(), 4);
    assert_eq!(set.quorum_weight(), 3);
    assert_eq!(set.members().len(), 4);
    assert!(set
        .members()
        .windows(2)
        .all(|pair| pair[0].id() < pair[1].id()));
    let mut reverse = set.members().to_vec();
    reverse.reverse();
    assert_eq!(ValidatorSet::new(91, 1, 1, reverse).unwrap(), set);
    for (chain, epoch, activation) in [(92, 1, 1), (91, 2, 1), (91, 1, 2)] {
        assert_ne!(
            ValidatorSet::new(chain, epoch, activation, set.members().to_vec())
                .unwrap()
                .hash(),
            set.hash()
        );
    }
    assert_ne!(
        ValidatorSet::new(
            91,
            1,
            1,
            set.members()
                .iter()
                .map(|member| Validator::new(*member.public_key(), member.weight() + 1).unwrap())
                .collect()
        )
        .unwrap()
        .hash(),
        set.hash()
    );
    for member in set.members() {
        assert_eq!(set.member(&member.id()), Some(member));
    }
    assert!(set.member(&[0xff; 32]).is_none());
}

#[test]
fn invalid_keys_duplicates_counts_and_weight_overflow_are_rejected() {
    let (_, set) = fixture();
    let pk = *set.members()[0].public_key();
    assert!(Validator::new(pk, 0).is_err());
    let mut identity = [0; 32];
    identity[0] = 1;
    assert!(
        Validator::new(identity, 1).is_err(),
        "small-order identity must not be a validator"
    );
    assert!(
        Validator::new([0; 32], 1).is_err(),
        "small-order point must not be a validator"
    );
    assert!(ValidatorSet::new(91, 1, 1, vec![]).is_err());
    assert!(
        ValidatorSet::new(91, 1, 1, vec![set.members()[0].clone(); MAX_VALIDATORS + 1]).is_err()
    );
    assert!(ValidatorSet::new(91, 1, 1, vec![set.members()[0].clone(); 2]).is_err());
    for (chain, epoch, activation) in [(0, 1, 1), (91, 0, 1), (91, 1, 0)] {
        assert!(ValidatorSet::new(chain, epoch, activation, set.members().to_vec()).is_err());
    }
    assert!(ValidatorSet::new(
        91,
        1,
        1,
        vec![
            Validator::new(pk, u64::MAX).unwrap(),
            Validator::new(*set.members()[1].public_key(), 1).unwrap()
        ]
    )
    .is_err());
    let huge = ValidatorSet::new(91, 1, 1, vec![Validator::new(pk, u64::MAX).unwrap()]).unwrap();
    assert_eq!(
        huge.quorum_weight(),
        ((u128::from(u64::MAX) * 2) / 3 + 1) as u64
    );
}

#[test]
fn context_requires_full_domain_and_exact_first_block_parent_convention() {
    let (_, set) = fixture();
    let ctx = context(&set);
    ctx.validate(&set).unwrap();
    let mut first = ctx;
    first.height = 1;
    first.parent_block_hash = [0; 32];
    first.parent_decision_hash = [0; 32];
    first.validate(&set).unwrap();
    first.parent_block_hash = [3; 32];
    assert!(first.validate(&set).is_err());
    first.parent_block_hash = [0; 32];
    first.parent_decision_hash = [4; 32];
    assert!(first.validate(&set).is_err());
    for field in 0..8 {
        let mut bad = ctx;
        match field {
            0 => bad.chain_id = 0,
            1 => bad.genesis_config_commitment = [0; 32],
            2 => bad.protocol_commitment = [0; 32],
            3 => bad.epoch = 0,
            4 => bad.validator_set_hash = [0; 32],
            5 => bad.height = 0,
            6 => bad.parent_block_hash = [0; 32],
            _ => bad.parent_decision_hash = [0; 32],
        }
        assert!(bad.validate(&set).is_err(), "zero field {field}");
    }
    let future_set = ValidatorSet::new(91, 1, 3, set.members().to_vec()).unwrap();
    let mut before_activation = ctx;
    before_activation.validator_set_hash = future_set.hash();
    assert!(before_activation.validate(&future_set).is_err());
}

#[test]
fn leader_schedule_is_sorted_round_robin_and_checked() {
    let (_, set) = fixture();
    for round in 0..12 {
        assert_eq!(
            set.leader(1, round).unwrap(),
            set.members()[(round % 4) as usize].id()
        );
        assert_eq!(
            set.leader(2, round).unwrap(),
            set.members()[((round + 1) % 4) as usize].id()
        );
    }
    assert!(set.leader(0, 0).is_err());
    assert!(set.leader(2, u64::MAX).is_err());
    assert!(set.leader(u64::MAX, 2).is_err());
}

#[test]
fn real_votes_nil_and_value_roundtrip_but_do_not_grant_other_phase() {
    let (keys, set) = fixture();
    for phase in [Phase::Prevote, Phase::Precommit] {
        for value in [None, Some([8; 32])] {
            let vote = Vote::sign(context(&set), 2, phase, value, &set, &keys[0]).unwrap();
            assert_eq!(vote.verify(&set).unwrap().vote(), &vote);
            let wire = encode_vote(&vote).unwrap();
            assert_eq!(wire.len(), PREFIX_BYTES + VOTE_BYTES);
            assert_eq!(decode_vote(&wire).unwrap(), vote);
            let mut another_phase = vote.clone();
            another_phase.phase = if phase == Phase::Prevote {
                Phase::Precommit
            } else {
                Phase::Prevote
            };
            assert!(another_phase.verify(&set).is_err());
        }
    }
    assert!(Vote::sign(
        context(&set),
        2,
        Phase::Prevote,
        Some([0; 32]),
        &set,
        &keys[0]
    )
    .is_err());
    let outsider = SigningKey::from_bytes(&[99; 32]);
    assert!(Vote::sign(context(&set), 2, Phase::Prevote, None, &set, &outsider).is_err());
}

#[test]
fn vote_signature_binds_every_context_field_round_phase_value_and_signer() {
    let (keys, set) = fixture();
    let vote = Vote::sign(
        context(&set),
        2,
        Phase::Prevote,
        Some([8; 32]),
        &set,
        &keys[0],
    )
    .unwrap();
    for field in 0..13 {
        let mut changed = vote.clone();
        match field {
            0 => changed.context.chain_id += 1,
            1 => changed.context.genesis_config_commitment[0] ^= 1,
            2 => changed.context.protocol_commitment[0] ^= 1,
            3 => changed.context.epoch += 1,
            4 => changed.context.validator_set_hash[0] ^= 1,
            5 => changed.context.height += 1,
            6 => changed.context.parent_block_hash[0] ^= 1,
            7 => changed.context.parent_decision_hash[0] ^= 1,
            8 => changed.round += 1,
            9 => changed.phase = Phase::Precommit,
            10 => changed.value = None,
            11 => changed.value = Some([9; 32]),
            _ => {
                changed.validator_id = Validator::new(keys[1].verifying_key().to_bytes(), 1)
                    .unwrap()
                    .id()
            }
        }
        assert_ne!(
            changed.message(),
            vote.message(),
            "omitted signed field {field}"
        );
        assert!(
            changed.verify(&set).is_err(),
            "accepted changed signed field {field}"
        );
    }
    let mut corrupt = vote.clone();
    corrupt.signature[0] ^= 1;
    assert!(corrupt.verify(&set).is_err());
    for length in [0, 63, 65, 4096] {
        let mut bad = vote.clone();
        bad.signature.resize(length, 0);
        assert!(bad.verify(&set).is_err());
        assert!(encode_vote(&bad).is_err());
    }
}

#[test]
fn vote_signing_preimage_is_explicit_and_old_signatures_cannot_be_relabelled() {
    let (keys, set) = fixture();
    let ctx = context(&set);
    let vote = Vote::sign(ctx, 2, Phase::Prevote, Some([8; 32]), &set, &keys[0]).unwrap();
    // Independent field-by-field oracle: do not call the production encoder.
    let mut oracle = Sha256::new();
    oracle.update(b"novovm-round-bft/v1/vote\0");
    oracle.update(ctx.chain_id.to_be_bytes());
    oracle.update(ctx.genesis_config_commitment);
    oracle.update(ctx.protocol_commitment);
    oracle.update(ctx.epoch.to_be_bytes());
    oracle.update(ctx.validator_set_hash);
    oracle.update(ctx.height.to_be_bytes());
    oracle.update(ctx.parent_block_hash);
    oracle.update(ctx.parent_decision_hash);
    oracle.update(2u64.to_be_bytes());
    oracle.update([0, 1]); // Prevote + Some value, no implicit enum codec.
    oracle.update([8; 32]);
    oracle.update(vote.validator_id);
    let expected: Hash = oracle.finalize().into();
    assert_eq!(vote.signature, keys[0].sign(&expected).to_bytes());
    let mut legacy = Vec::from(&b"VOTE:"[..]);
    legacy.extend_from_slice(&[8; 32]);
    legacy.extend_from_slice(&ctx.height.to_le_bytes());
    let mut relabelled = vote;
    relabelled.signature = keys[0].sign(&legacy).to_bytes().to_vec();
    assert!(relabelled.verify(&set).is_err());
    let mut foreign_domain = expected;
    foreign_domain[0] ^= 1;
    relabelled.signature = keys[0].sign(&foreign_domain).to_bytes().to_vec();
    assert!(relabelled.verify(&set).is_err());
}

#[test]
fn proposals_require_the_scheduled_leader_and_strict_valid_round() {
    let (keys, set) = fixture();
    let ctx = context(&set);
    let key = leader(&keys, &set, ctx.height, 2);
    for valid_round in [None, Some(0), Some(1)] {
        let proposal = Proposal::sign(ctx, 2, [8; 32], valid_round, &set, key).unwrap();
        assert_eq!(proposal.verify(&set).unwrap().proposal(), &proposal);
        assert_eq!(
            decode_proposal(&encode_proposal(&proposal).unwrap()).unwrap(),
            proposal
        );
    }
    for valid_round in [Some(2), Some(3), Some(u64::MAX)] {
        assert!(Proposal::sign(ctx, 2, [8; 32], valid_round, &set, key).is_err());
    }
    assert!(Proposal::sign(ctx, 2, [0; 32], None, &set, key).is_err());
    let wrong = keys
        .iter()
        .find(|candidate| candidate.verifying_key() != key.verifying_key())
        .unwrap();
    assert!(Proposal::sign(ctx, 2, [8; 32], None, &set, wrong).is_err());
    // A cryptographically valid wrong-leader signature must still be refused.
    let mut proposal = Proposal::sign(ctx, 2, [8; 32], None, &set, key).unwrap();
    proposal.proposer_id = Validator::new(wrong.verifying_key().to_bytes(), 1)
        .unwrap()
        .id();
    proposal.signature = wrong.sign(&proposal.message()).to_bytes().to_vec();
    assert!(proposal.verify(&set).is_err());
    assert!(Proposal::sign(
        ctx,
        0,
        [8; 32],
        Some(0),
        &set,
        leader(&keys, &set, ctx.height, 0)
    )
    .is_err());
}

#[test]
fn proposal_signature_binds_all_fields_and_cannot_be_used_as_vote() {
    let (keys, set) = fixture();
    let ctx = context(&set);
    let key = leader(&keys, &set, ctx.height, 2);
    let proposal = Proposal::sign(ctx, 2, [8; 32], None, &set, key).unwrap();
    for field in 0..12 {
        let mut changed = proposal.clone();
        match field {
            0 => changed.context.chain_id += 1,
            1 => changed.context.genesis_config_commitment[0] ^= 1,
            2 => changed.context.protocol_commitment[0] ^= 1,
            3 => changed.context.epoch += 1,
            4 => changed.context.validator_set_hash[0] ^= 1,
            5 => changed.context.height += 1,
            6 => changed.context.parent_block_hash[0] ^= 1,
            7 => changed.context.parent_decision_hash[0] ^= 1,
            8 => changed.round += 1,
            9 => changed.value = [9; 32],
            10 => changed.valid_round = Some(0),
            _ => changed.proposer_id[0] ^= 1,
        }
        assert_ne!(
            changed.message(),
            proposal.message(),
            "omitted proposal field {field}"
        );
        assert!(changed.verify(&set).is_err());
    }
    let mut vote = Vote::sign(ctx, 2, Phase::Prevote, Some([8; 32]), &set, key).unwrap();
    vote.signature = proposal.signature;
    assert!(vote.verify(&set).is_err());
}

#[test]
fn quorum_is_three_of_four_only_for_equal_weight_and_exact_single_phase() {
    let (keys, set) = fixture();
    for phase in [Phase::Prevote, Phase::Precommit] {
        for value in [None, Some([8; 32])] {
            let mut input = votes(&keys, &set, phase, value);
            assert!(Quorum::from_votes(&set, input[..2].to_vec()).is_err());
            input.reverse();
            let quorum = Quorum::from_votes(&set, input).unwrap();
            let verified = quorum.verify(&set).unwrap();
            assert_eq!(verified.context(), &context(&set));
            assert_eq!(verified.round(), 2);
            assert_eq!(verified.phase(), phase);
            assert_eq!(verified.value(), value);
            assert_eq!(verified.signed_weight(), 3);
            assert_eq!(verified.quorum(), &quorum);
            assert_eq!(
                decode_quorum(&encode_quorum(&quorum).unwrap()).unwrap(),
                quorum
            );
        }
    }
}

#[test]
fn quorum_rejects_duplicate_unsorted_tampered_and_mixed_signed_evidence() {
    let (keys, set) = fixture();
    let good = votes(&keys, &set, Phase::Prevote, Some([8; 32]));
    for field in 0..7 {
        let mut mixed = good.clone();
        let mut changed = mixed[2].clone();
        match field {
            0 => changed.round += 1,
            1 => changed.phase = Phase::Precommit,
            2 => changed.value = None,
            3 => changed.value = Some([9; 32]),
            4 => changed.context.genesis_config_commitment[0] ^= 1,
            5 => changed.context.protocol_commitment[0] ^= 1,
            _ => changed.context.parent_decision_hash[0] ^= 1,
        }
        changed.signature = keys[2].sign(&changed.message()).to_bytes().to_vec();
        changed.verify(&set).unwrap(); // Every individual signature is real.
        mixed[2] = changed;
        assert!(
            Quorum::from_votes(&set, mixed).is_err(),
            "mixed field {field}"
        );
    }
    assert!(Quorum::from_votes(
        &set,
        vec![good[0].clone(), good[0].clone(), good[1].clone()]
    )
    .is_err());
    let mut quorum = Quorum::from_votes(&set, good.clone()).unwrap();
    quorum.votes.reverse();
    assert!(quorum.verify(&set).is_err());
    assert!(encode_quorum(&quorum).is_err());
    let mut bad_signature = good;
    bad_signature[1].signature[0] ^= 1;
    assert!(Quorum::from_votes(&set, bad_signature).is_err());
    assert!(Quorum::from_votes(&set, vec![]).is_err());
}

#[test]
fn weighted_quorum_does_not_confuse_signature_count_with_weight() {
    let (keys, _) = fixture();
    let set = ValidatorSet::new(
        91,
        1,
        1,
        keys.iter()
            .zip([5, 3, 2, 1])
            .map(|(key, weight)| Validator::new(key.verifying_key().to_bytes(), weight).unwrap())
            .collect(),
    )
    .unwrap();
    assert_eq!(set.total_weight(), 11);
    assert_eq!(set.quorum_weight(), 8);
    let all: Vec<_> = keys
        .iter()
        .map(|key| {
            Vote::sign(context(&set), 0, Phase::Precommit, Some([8; 32]), &set, key).unwrap()
        })
        .collect();
    assert!(
        Quorum::from_votes(&set, all[1..].to_vec()).is_err(),
        "three signatures weigh only six"
    );
    assert_eq!(
        Quorum::from_votes(&set, all[..2].to_vec())
            .unwrap()
            .verify(&set)
            .unwrap()
            .signed_weight(),
        8
    );
}

#[test]
fn codec_rejects_unknown_old_formats_trailing_and_all_truncations() {
    let (keys, set) = fixture();
    let vote = votes(&keys, &set, Phase::Prevote, Some([8; 32]))[0].clone();
    let wire = encode_vote(&vote).unwrap();
    for length in 0..wire.len() {
        assert!(decode_vote(&wire[..length]).is_err());
    }
    let mut tail = wire.clone();
    tail.push(0);
    assert!(decode_vote(&tail).is_err());
    for offset in [0, 8, 9, 10] {
        let mut bad = wire.clone();
        bad[offset] ^= 0xff;
        assert!(decode_vote(&bad).is_err());
    }
    let mut old = wire.clone();
    old[..8].copy_from_slice(b"NOVSRW01");
    assert!(decode_vote(&old).is_err());
    let proposal = Proposal::sign(
        context(&set),
        2,
        [8; 32],
        None,
        &set,
        leader(&keys, &set, 2, 2),
    )
    .unwrap();
    let proposal_wire = encode_proposal(&proposal).unwrap();
    for length in 0..proposal_wire.len() {
        assert!(decode_proposal(&proposal_wire[..length]).is_err());
    }
    let mut tail = proposal_wire;
    tail.push(0);
    assert!(decode_proposal(&tail).is_err());
    let quorum =
        Quorum::from_votes(&set, votes(&keys, &set, Phase::Prevote, Some([8; 32]))).unwrap();
    let quorum_wire = encode_quorum(&quorum).unwrap();
    for length in 0..quorum_wire.len() {
        assert!(decode_quorum(&quorum_wire[..length]).is_err());
    }
    let mut tail = quorum_wire;
    tail.push(0);
    assert!(decode_quorum(&tail).is_err());
}

#[test]
fn codec_bounds_counts_before_allocation_and_requires_canonical_tags() {
    let (keys, set) = fixture();
    let nil = Vote::sign(context(&set), 2, Phase::Prevote, None, &set, &keys[0]).unwrap();
    let wire = encode_vote(&nil).unwrap();
    let phase_offset = PREFIX_BYTES + CONTEXT_BYTES + 8;
    let value_tag = phase_offset + 1;
    for (offset, byte) in [
        (phase_offset, 2),
        (value_tag, 2),
        (value_tag, 1),
        (value_tag + 1, 1),
    ] {
        let mut bad = wire.clone();
        bad[offset] = byte;
        assert!(decode_vote(&bad).is_err());
    }
    let proposal = Proposal::sign(
        context(&set),
        2,
        [8; 32],
        None,
        &set,
        leader(&keys, &set, 2, 2),
    )
    .unwrap();
    let wire = encode_proposal(&proposal).unwrap();
    let valid_tag = PREFIX_BYTES + CONTEXT_BYTES + 8 + 32;
    for (offset, byte) in [(valid_tag, 2), (valid_tag + 8, 1)] {
        let mut bad = wire.clone();
        bad[offset] = byte;
        assert!(decode_proposal(&bad).is_err());
    }
    for count in [0, MAX_VALIDATORS as u32 + 1, u32::MAX, 1] {
        let mut malicious = prefix(3, PREFIX_BYTES + 4);
        malicious.extend_from_slice(&count.to_be_bytes());
        assert!(decode_quorum(&malicious).is_err());
    }
    let mut oversized = vec![0u8; MAX_WIRE_BYTES + 1];
    oversized[..8].copy_from_slice(MAGIC);
    oversized[8..10].copy_from_slice(&VERSION.to_be_bytes());
    oversized[10] = 3;
    assert!(decode_quorum(&oversized).is_err());
}

#[test]
fn independently_signed_foreign_set_epoch_and_chain_are_not_replayed() {
    let (keys, set) = fixture();
    let vote = Vote::sign(
        context(&set),
        2,
        Phase::Prevote,
        Some([8; 32]),
        &set,
        &keys[0],
    )
    .unwrap();
    for (chain, epoch) in [(92, 1), (91, 2)] {
        let other = ValidatorSet::new(chain, epoch, 1, set.members().to_vec()).unwrap();
        assert!(vote.verify(&other).is_err());
        let mut relabelled = vote.clone();
        relabelled.context.chain_id = chain;
        relabelled.context.epoch = epoch;
        relabelled.context.validator_set_hash = other.hash();
        assert!(
            relabelled.verify(&other).is_err(),
            "domain relabelling retained valid signature"
        );
    }
}
