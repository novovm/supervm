//! Test keys and synthetic ledger claims only; no real AOEM execution proof.
use super::*;
use crate::native_block_seal_overlay::{
    NovNativeSealEpochAuthorityV1, NovNativeSealValidatorTransportBindingV1,
};
use crate::tx_ingress::native_nonce_bundle::encode_bundle;
use crate::tx_ingress::native_nonce_checkpoint::{test_fixture_v1, NonceMigrationCheckpointV1};
use crate::tx_ingress::native_nonce_source_qc::{
    verify_nonce_source_qc_json_v1, NonceSourceQcInputsV1,
};

struct Fixture {
    bundle: Vec<u8>,
    checkpoint: NonceMigrationCheckpointV1,
    authority: NovNativeSealEpochAuthorityV1,
    keys: Vec<SigningKey>,
    pairs: Vec<(NovNativeSealProposalV1, NovNativeSealQuorumCertificateV1)>,
}

impl Fixture {
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": "novovm-native-nonce-source-prepare-qc/v1",
            "entries": self.pairs.iter().map(|(proposal,qc)| serde_json::json!({"proposal":proposal,"qc":qc})).collect::<Vec<_>>()
        })
    }

    fn verify(&self) -> Result<crate::tx_ingress::native_nonce_source_qc::NonceSourceQcReportV1> {
        let digest =
            crate::tx_ingress::native_nonce_bundle::checkpoint_bundle_digest_v1(&self.bundle);
        let pin = self
            .authority
            .authority_commitment
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        verify_nonce_source_qc_json_v1(
            &serde_json::to_vec(&self.json())?,
            &NonceSourceQcInputsV1 {
                bundle: &self.bundle,
                bundle_digest: &digest,
                checkpoint: &self.checkpoint,
                authority: &self.authority,
                expected_authority_commitment: &pin,
            },
        )
    }

    fn replace_subject(&mut self, index: usize, subject: NovNativeSealSubjectV1) {
        let leader = self.authority.expected_leader(subject.height, 0).unwrap();
        let key = self
            .keys
            .iter()
            .find(|key| validator_id_v1(key.verifying_key().as_bytes()) == leader)
            .unwrap();
        let proposal = sign_proposal_v1(subject, &self.authority.validator_set, key).unwrap();
        let votes = self
            .keys
            .iter()
            .take(3)
            .map(|key| sign_vote_v1(&proposal, &self.authority.validator_set, key).unwrap())
            .collect();
        let qc = NovNativeSealQuorumCertificateV1::from_votes(
            proposal.subject.clone(),
            &self.authority.validator_set,
            votes,
        )
        .unwrap();
        self.pairs[index] = (proposal, qc);
    }
}

fn fixture() -> Fixture {
    fixture_with_weights(&[1, 1, 1, 1])
}

fn fixture_with_weights(weights: &[u64; 4]) -> Fixture {
    let (snapshot, head, blocks, checkpoint, path) = test_fixture_v1();
    let bundle = encode_bundle(&snapshot, &head, &blocks).unwrap();
    let ledger = NovNativeBlockLedgerV1::open_existing_read_only(&path)
        .unwrap()
        .unwrap();
    let keys = (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    let validators = keys
        .iter()
        .zip(weights)
        .map(|(key, weight)| {
            NovNativeSealValidatorV1::new(key.verifying_key().to_bytes(), *weight).unwrap()
        })
        .collect::<Vec<_>>();
    let bindings = validators
        .iter()
        .enumerate()
        .map(
            |(index, validator)| NovNativeSealValidatorTransportBindingV1 {
                validator_id: validator.validator_id,
                transport_peer_id: format!("{:02x}", index + 17).repeat(32),
            },
        )
        .collect();
    let set = NovNativeSealValidatorSetV1::new(checkpoint.chain_id, 1, 1, validators).unwrap();
    let authority =
        NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(&ledger, set, bindings)
            .unwrap();
    let stores = (0..4)
        .map(|index| {
            NovNativeBlockSealStoreV1::open(
                &path.with_extension(format!("source-qc-signer-{index}")),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let mut pairs = Vec::new();
    let mut previous = None;
    for block in &blocks {
        let leader = authority.expected_leader(block.header.height, 0).unwrap();
        let index = keys
            .iter()
            .position(|key| validator_id_v1(key.verifying_key().as_bytes()) == leader)
            .unwrap();
        let proposal = stores[index]
            .sign_local_proposal(
                &ledger,
                &NovNativeSealLocalProposalRequestV1 {
                    chain_id: checkpoint.chain_id,
                    block_hash: block.header.block_hash,
                    round: 0,
                    justify_qc_hash: previous,
                },
                &authority.validator_set,
                &keys[index],
            )
            .unwrap();
        let mut votes = Vec::new();
        for (store, key) in stores.iter().zip(&keys).take(3) {
            votes.push(
                store
                    .sign_local_vote(&ledger, &proposal, &authority.validator_set, key)
                    .unwrap(),
            );
        }
        let qc = NovNativeSealQuorumCertificateV1::from_votes(
            proposal.subject.clone(),
            &authority.validator_set,
            votes,
        )
        .unwrap();
        for store in &stores {
            store
                .persist_locally_matched_remote_proposal(
                    &ledger,
                    &proposal,
                    &authority.validator_set,
                )
                .unwrap();
            store
                .persist_local_verified_qc(&ledger, &qc, &authority.validator_set)
                .unwrap();
        }
        previous = Some(qc.qc_hash);
        pairs.push((proposal, qc));
    }
    Fixture {
        bundle,
        checkpoint,
        authority,
        keys,
        pairs,
    }
}

#[test]
fn native_nonce_source_qc_fixture_matches_durable_signers() {
    let fixture = fixture();
    assert!(fixture.verify().unwrap().prepare_qc_chain_verified);
    let json = serde_json::json!({
        "source_qc":fixture.json(),
        "expected_authority_commitment":fixture.authority.authority_commitment.iter().map(|b|format!("{b:02x}")).collect::<String>(),
        "authority":fixture.authority,
    });
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../novovmctl/tests/fixtures/native_nonce_source_qc_v1.json"
    ))
    .unwrap();
    assert_eq!(
        json, expected,
        "portable fixture must match durable test-key signing"
    );
}

#[test]
fn native_nonce_source_qc_valid_signatures_cannot_reanchor_state_or_parent_qc() {
    let mut fixture = fixture();
    let original = fixture.pairs.clone();
    let mut subject = fixture.pairs[1].0.subject.clone();
    subject.post_state_root[0] ^= 1;
    subject.subject_hash = subject_hash_v1(&subject);
    fixture.replace_subject(1, subject);
    fixture.pairs[1]
        .1
        .verify(&fixture.authority.validator_set)
        .unwrap();
    assert!(fixture.verify().is_err());
    fixture.pairs = original.clone();
    // Another valid 3-of-4 QC for the same parent block is not the child's
    // exact justify object. Checking only parent block hash would miss this.
    let proposal = &fixture.pairs[0].0;
    let votes = fixture
        .keys
        .iter()
        .skip(1)
        .map(|key| sign_vote_v1(proposal, &fixture.authority.validator_set, key).unwrap())
        .collect();
    fixture.pairs[0].1 = NovNativeSealQuorumCertificateV1::from_votes(
        proposal.subject.clone(),
        &fixture.authority.validator_set,
        votes,
    )
    .unwrap();
    assert_ne!(fixture.pairs[0].1.qc_hash, original[0].1.qc_hash);
    assert!(fixture.verify().is_err());
}

#[test]
fn native_nonce_source_qc_valid_signatures_cannot_bypass_leader_or_round_zero() {
    let mut fixture = fixture();
    let original = fixture.pairs.clone();
    let subject = fixture.pairs[1].0.subject.clone();
    let wrong_key = fixture
        .keys
        .iter()
        .find(|key| {
            validator_id_v1(key.verifying_key().as_bytes())
                != fixture.authority.expected_leader(2, 0).unwrap()
        })
        .unwrap();
    let proposal = sign_proposal_v1(subject, &fixture.authority.validator_set, wrong_key).unwrap();
    let votes = fixture
        .keys
        .iter()
        .take(3)
        .map(|key| sign_vote_v1(&proposal, &fixture.authority.validator_set, key).unwrap())
        .collect();
    let qc = NovNativeSealQuorumCertificateV1::from_votes(
        proposal.subject.clone(),
        &fixture.authority.validator_set,
        votes,
    )
    .unwrap();
    proposal.verify(&fixture.authority.validator_set).unwrap();
    qc.verify(&fixture.authority.validator_set).unwrap();
    fixture.pairs[1] = (proposal, qc);
    assert!(fixture.verify().is_err());
    fixture.pairs = original;
    let mut subject = fixture.pairs[1].0.subject.clone();
    subject.round = 1;
    subject.subject_hash = subject_hash_v1(&subject);
    fixture.replace_subject(1, subject);
    fixture.pairs[1]
        .1
        .verify(&fixture.authority.validator_set)
        .unwrap();
    assert!(fixture.verify().is_err());
}

#[test]
fn native_nonce_source_qc_weighted_votes_not_signature_count_determine_quorum() {
    let mut fixture = fixture_with_weights(&[3, 1, 1, 1]);
    assert!(fixture.verify().unwrap().prepare_qc_chain_verified);
    let proposal = &fixture.pairs[1].0;
    // Three genuine signatures whose weight is only half the configured set.
    let mut votes: Vec<_> = fixture
        .keys
        .iter()
        .skip(1)
        .map(|key| sign_vote_v1(proposal, &fixture.authority.validator_set, key).unwrap())
        .collect();
    votes.sort_by_key(|vote| vote.validator_id);
    let qc = &mut fixture.pairs[1].1;
    qc.votes = votes;
    qc.signed_weight = 3;
    qc.qc_hash = qc_hash_v1(qc);
    assert_eq!(qc.signature_count, 3);
    assert_eq!(qc.quorum_weight, 5);
    assert!(fixture.verify().is_err());
}
