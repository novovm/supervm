//! Cryptographic decision evidence only: no execution, journal, signing
//! permission, publication, or finalized-state capability is created here.

use super::{wire, VerifiedDecision};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

const VALUE: wire::Hash = [0x81; 32];

struct Fixture {
    keys: Vec<SigningKey>,
    set: wire::ValidatorSet,
    context: wire::Context,
}

impl Fixture {
    fn new() -> Self {
        let keys: Vec<_> = (41..45)
            .map(|seed| SigningKey::from_bytes(&[seed; 32]))
            .collect();
        let set = wire::ValidatorSet::new(
            52,
            1,
            1,
            keys.iter()
                .map(|key| wire::Validator::new(key.verifying_key().to_bytes(), 1).unwrap())
                .collect(),
        )
        .unwrap();
        let context = wire::Context {
            chain_id: 52,
            genesis_config_commitment: [0x11; 32],
            protocol_commitment: [0x12; 32],
            epoch: 1,
            validator_set_hash: set.hash(),
            height: 1,
            parent_block_hash: [0; 32],
            parent_decision_hash: [0; 32],
        };
        Self { keys, set, context }
    }

    fn leader(&self, context: wire::Context, round: u64) -> &SigningKey {
        let id = self.set.leader(context.height, round).unwrap();
        let public = self.set.member(&id).unwrap().public_key();
        self.keys
            .iter()
            .find(|key| key.verifying_key().as_bytes() == public)
            .unwrap()
    }

    fn proposal(&self, context: wire::Context, round: u64, value: wire::Hash) -> wire::Proposal {
        wire::Proposal::sign(
            context,
            round,
            value,
            None,
            &self.set,
            self.leader(context, round),
        )
        .unwrap()
    }

    fn quorum(
        &self,
        context: wire::Context,
        round: u64,
        phase: wire::Phase,
        value: Option<wire::Hash>,
        count: usize,
    ) -> wire::Quorum {
        let mut votes: Vec<_> = self
            .keys
            .iter()
            .take(count)
            .map(|key| wire::Vote::sign(context, round, phase, value, &self.set, key).unwrap())
            .collect();
        votes.sort_by_key(|vote| vote.validator_id);
        // Deliberately permit an insufficient fixture: verification, not this
        // builder, must reject 0/4, 1/4 and 2/4.
        wire::Quorum { votes }
    }

    fn precommits(&self, round: u64) -> wire::Quorum {
        self.quorum(self.context, round, wire::Phase::Precommit, Some(VALUE), 3)
    }

    fn verify(
        &self,
        proposal: &wire::Proposal,
        certificate: &wire::Quorum,
    ) -> anyhow::Result<VerifiedDecision> {
        VerifiedDecision::verify(proposal, certificate, &self.set, self.context, VALUE)
    }
}

#[test]
fn verified_decision_accepts_real_same_round_precommits_as_read_only_evidence() {
    let fixture = Fixture::new();
    for round in [0, 7] {
        let proposal = fixture.proposal(fixture.context, round, VALUE);
        let certificate = fixture.precommits(round);
        proposal.verify(&fixture.set).unwrap();
        certificate.verify(&fixture.set).unwrap();
        let proof = fixture.verify(&proposal, &certificate).unwrap();
        assert_eq!(proof.context(), &fixture.context);
        assert_eq!(proof.value(), VALUE);
        assert_eq!(proof.proposal(), &proposal);
        assert_eq!(proof.certificate(), &certificate);
        assert_eq!(
            wire::encode_quorum(proof.certificate()).unwrap(),
            wire::encode_quorum(&certificate).unwrap()
        );
    }
}

#[test]
fn verified_decision_rejects_insufficient_precommit_weight() {
    let fixture = Fixture::new();
    let proposal = fixture.proposal(fixture.context, 0, VALUE);
    for count in 0..3 {
        let certificate = fixture.quorum(
            fixture.context,
            0,
            wire::Phase::Precommit,
            Some(VALUE),
            count,
        );
        assert!(
            fixture.verify(&proposal, &certificate).is_err(),
            "{count}/4"
        );
    }
}

#[test]
fn verified_decision_rejects_other_phase_round_value_and_nil_even_with_valid_quorum() {
    let fixture = Fixture::new();
    let proposal = fixture.proposal(fixture.context, 2, VALUE);
    for (round, phase, value) in [
        (2, wire::Phase::Prevote, Some(VALUE)),
        (1, wire::Phase::Precommit, Some(VALUE)),
        (3, wire::Phase::Precommit, Some(VALUE)),
        (2, wire::Phase::Precommit, Some([0x82; 32])),
        (2, wire::Phase::Precommit, None),
    ] {
        let certificate = fixture.quorum(fixture.context, round, phase, value, 3);
        certificate.verify(&fixture.set).unwrap();
        assert!(fixture.verify(&proposal, &certificate).is_err());
    }
}

#[test]
fn verified_decision_rejects_exactly_signed_foreign_context() {
    let fixture = Fixture::new();
    let proposal = fixture.proposal(fixture.context, 0, VALUE);
    for foreign in [
        wire::Context {
            genesis_config_commitment: [0x21; 32],
            ..fixture.context
        },
        wire::Context {
            protocol_commitment: [0x22; 32],
            ..fixture.context
        },
        wire::Context {
            height: 2,
            parent_block_hash: [0x23; 32],
            parent_decision_hash: [0x24; 32],
            ..fixture.context
        },
    ] {
        let certificate = fixture.quorum(foreign, 0, wire::Phase::Precommit, Some(VALUE), 3);
        certificate.verify(&fixture.set).unwrap();
        assert!(fixture.verify(&proposal, &certificate).is_err());

        // Even an internally consistent, genuinely signed proposal+QC does
        // not override the caller's independently pinned local context.
        let foreign_proposal = fixture.proposal(foreign, 0, VALUE);
        foreign_proposal.verify(&fixture.set).unwrap();
        assert!(fixture.verify(&foreign_proposal, &certificate).is_err());
    }
}

#[test]
fn verified_decision_requires_the_independent_expected_context_and_value() {
    let fixture = Fixture::new();
    let proposal = fixture.proposal(fixture.context, 0, VALUE);
    let certificate = fixture.precommits(0);
    let base = fixture.context;
    for expected in [
        wire::Context {
            chain_id: 53,
            ..base
        },
        wire::Context { epoch: 2, ..base },
        wire::Context {
            validator_set_hash: [0x31; 32],
            ..base
        },
        wire::Context {
            genesis_config_commitment: [0x32; 32],
            ..base
        },
        wire::Context {
            protocol_commitment: [0x33; 32],
            ..base
        },
        wire::Context {
            height: 2,
            parent_block_hash: [0x34; 32],
            parent_decision_hash: [0x35; 32],
            ..base
        },
    ] {
        assert!(
            VerifiedDecision::verify(&proposal, &certificate, &fixture.set, expected, VALUE)
                .is_err()
        );
    }
    for expected in [[0; 32], [0x82; 32]] {
        assert!(
            VerifiedDecision::verify(&proposal, &certificate, &fixture.set, base, expected)
                .is_err()
        );
    }
}

#[test]
fn verified_decision_rejects_genuinely_signed_unscheduled_leader() {
    let fixture = Fixture::new();
    let mut proposal = fixture.proposal(fixture.context, 0, VALUE);
    let wrong_key = fixture.leader(fixture.context, 1);
    proposal.proposer_id = wire::Validator::new(wrong_key.verifying_key().to_bytes(), 1)
        .unwrap()
        .id();
    // Sign the declared wrong proposer's exact wire preimage. This is a real
    // signature by a set member, not merely a signature corrupted by mutation.
    let bytes = wire::encode_proposal(&proposal).unwrap();
    let mut hash = Sha256::new();
    hash.update(b"novovm-round-bft/v1/proposal\0");
    hash.update(&bytes[11..bytes.len() - 64]);
    let message = hash.finalize();
    proposal.signature = wrong_key.sign(&message).to_bytes().to_vec();
    wrong_key
        .verifying_key()
        .verify_strict(
            &message,
            &ed25519_dalek::Signature::from_slice(&proposal.signature).unwrap(),
        )
        .unwrap();
    let error = fixture
        .verify(&proposal, &fixture.precommits(0))
        .err()
        .unwrap();
    assert!(format!("{error:#}").contains("scheduled leader"));
}

#[test]
fn verified_decision_rejects_bad_proposal_and_vote_signatures() {
    let fixture = Fixture::new();
    let proposal = fixture.proposal(fixture.context, 0, VALUE);
    let certificate = fixture.precommits(0);
    let mut bad_proposal = proposal.clone();
    bad_proposal.signature[0] ^= 1;
    assert!(fixture.verify(&bad_proposal, &certificate).is_err());
    bad_proposal.signature.pop();
    assert!(fixture.verify(&bad_proposal, &certificate).is_err());

    let mut bad_certificate = certificate.clone();
    bad_certificate.votes[0].signature[0] ^= 1;
    assert!(fixture.verify(&proposal, &bad_certificate).is_err());
    bad_certificate.votes[0].signature.pop();
    assert!(fixture.verify(&proposal, &bad_certificate).is_err());
}

#[test]
fn verified_decision_rejects_duplicate_and_mixed_authentic_votes() {
    let fixture = Fixture::new();
    let proposal = fixture.proposal(fixture.context, 0, VALUE);
    let mut duplicate = fixture.precommits(0);
    duplicate.votes[1] = duplicate.votes[0].clone();
    duplicate.votes.sort_by_key(|vote| vote.validator_id);
    assert!(fixture.verify(&proposal, &duplicate).is_err());

    let mut mixed = fixture.precommits(0);
    let old = &mixed.votes[0];
    let signer = fixture
        .keys
        .iter()
        .find(|key| {
            wire::Validator::new(key.verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == old.validator_id
        })
        .unwrap();
    mixed.votes[0] = wire::Vote::sign(
        fixture.context,
        0,
        wire::Phase::Precommit,
        Some([0x82; 32]),
        &fixture.set,
        signer,
    )
    .unwrap();
    for vote in &mixed.votes {
        vote.verify(&fixture.set).unwrap();
    }
    assert!(fixture.verify(&proposal, &mixed).is_err());
}
