// Current consensus downgrade counterexamples. The existing CI
// "round_bft_candidate" filter remains meaningful without a historical adapter.
// Uses the consensus crate's explicitly non-production fixtures, not ML-DSA
// runtime evidence; actual AOEM process coverage remains in native_pipeline.
mod round_bft_candidate_tests {
    use novovm_consensus::round_bft::{
        test_vectors::{sign_proposal, sign_vote, FixtureSigningKey},
        wire::{self, Context, Phase, Quorum, Validator, ValidatorSet},
    };

    fn fixture() -> (FixtureSigningKey, ValidatorSet, Context) {
        let key = FixtureSigningKey::from_bytes(&[7; 32]);
        let member = Validator::new(key.verifying_key().as_bytes(), 1).unwrap();
        let set = ValidatorSet::new_for_test(98_919_729, 1, 1, vec![member]).unwrap();
        let context = Context {
            chain_id: set.chain_id(),
            genesis_config_commitment: [1; 32],
            protocol_commitment: [2; 32],
            epoch: set.epoch(),
            validator_set_hash: set.hash(),
            height: 1,
            parent_block_hash: [0; 32],
            parent_decision_hash: [0; 32],
        };
        (key, set, context)
    }

    #[test]
    fn round_bft_candidate_mldsa65_rejects_ed25519_shapes_and_missing_verifier() {
        let (key, set, context) = fixture();
        // The shared fixture exposes a component-only sized key. Its Ed25519
        // prefix must never be accepted as a current validator public key.
        let public = key.verifying_key();
        assert!(Validator::new(&public.as_bytes()[..32], 1).is_err());
        let vote = sign_vote(context, 0, Phase::Precommit, Some([3; 32]), &set, &key).unwrap();
        vote.verify(&set).unwrap();
        let mut short_signature = vote.clone();
        short_signature.signature.truncate(64);
        assert!(short_signature.verify(&set).is_err());
        assert!(wire::encode_vote(&short_signature).is_err());

        let metadata = ValidatorSet::new(
            set.chain_id(),
            set.epoch(),
            set.activation_height(),
            set.members().to_vec(),
        )
        .unwrap();
        assert_eq!(metadata.hash(), set.hash());
        assert!(!metadata.has_verifier());
        assert!(vote.verify(&metadata).is_err());
        assert!(sign_vote(context, 0, Phase::Precommit, Some([3; 32]), &metadata, &key).is_err());
    }

    fn assert_current_codec_rejects_downgrades(bytes: Vec<u8>, rejects: impl Fn(&[u8]) -> bool) {
        assert!(!rejects(&bytes), "current fixture must first decode");
        let mut old_magic = bytes.clone();
        old_magic[..8].copy_from_slice(b"NOVRBFT1");
        assert!(rejects(&old_magic));
        let mut old_version = bytes.clone();
        old_version[8..10].copy_from_slice(&1u16.to_be_bytes());
        assert!(rejects(&old_version));
        for suite in [0u16, 44, 87] {
            let mut other_suite = bytes.clone();
            other_suite[10..12].copy_from_slice(&suite.to_be_bytes());
            assert!(rejects(&other_suite));
        }
    }

    #[test]
    fn round_bft_candidate_mldsa65_rejects_old_wire_and_alternate_suites() {
        let (key, set, context) = fixture();
        let value = [3; 32];
        let vote = sign_vote(context, 0, Phase::Precommit, Some(value), &set, &key).unwrap();
        let proposal = sign_proposal(context, 0, value, None, &set, &key).unwrap();
        let quorum = Quorum::from_votes(&set, vec![vote.clone()]).unwrap();
        proposal.verify(&set).unwrap();
        quorum.verify(&set).unwrap();
        assert_current_codec_rejects_downgrades(wire::encode_vote(&vote).unwrap(), |bytes| {
            wire::decode_vote(bytes).is_err()
        });
        assert_current_codec_rejects_downgrades(
            wire::encode_proposal(&proposal).unwrap(),
            |bytes| wire::decode_proposal(bytes).is_err(),
        );
        assert_current_codec_rejects_downgrades(wire::encode_quorum(&quorum).unwrap(), |bytes| {
            wire::decode_quorum(bytes).is_err()
        });
    }
}
