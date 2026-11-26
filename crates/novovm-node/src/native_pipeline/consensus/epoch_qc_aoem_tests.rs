//! Offline cryptographic evidence; no activation, storage, or network owner.
use super::crypto::AoemConsensusCrypto;
use anyhow::{Context as _, Result};
use novovm_consensus::round_bft::{
    crypto::ConsensusSigner,
    test_vectors::sign_vote,
    wire::{decode_quorum, encode_quorum, Context, Phase, Quorum, Validator, ValidatorSet},
};
use novovm_exec::AoemExecFacade;
use std::{path::PathBuf, sync::Arc};

#[test]
#[ignore = "requires explicit trusted NOVOVM_AOEM_TEST_LIBRARY and real ML-DSA-65"]
fn aoem_qc_binds_epoch_set_and_weight() -> Result<()> {
    let library = PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY").context("explicit AOEM library required")?,
    )
    .canonicalize()?;
    let facade = AoemExecFacade::open(&library, Default::default())?;
    let crypto = Arc::new(AoemConsensusCrypto::new(&facade)?);
    // These keys exist only in memory and never join a deployed validator set.
    let keys = (0..3)
        .map(|_| crypto.signer(crypto.generate_test_key()?))
        .collect::<Result<Vec<_>>>()?;
    let make_set = |epoch, weights: [u64; 3]| -> Result<ValidatorSet> {
        let validators = keys
            .iter()
            .zip(weights)
            .map(|(key, weight)| Validator::new(key.public_key(), weight))
            .collect::<Result<Vec<_>>>()?;
        ValidatorSet::new_with_verifier(921_177, epoch, 1, validators, crypto.clone())
    };
    let old = make_set(7, [2, 1, 1])?;
    let next = make_set(8, [2, 1, 1])?;
    // Same epoch, keys, total, and participating weight; different commitment.
    let changed_set = make_set(7, [1, 2, 1])?;
    let context = |set: &ValidatorSet| Context {
        chain_id: set.chain_id(),
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        epoch: set.epoch(),
        validator_set_hash: set.hash(),
        height: 2,
        parent_block_hash: [3; 32],
        parent_decision_hash: [4; 32],
    };
    let votes = |set: &ValidatorSet| -> Result<Vec<_>> {
        keys[..2]
            .iter()
            .map(|key| {
                sign_vote(
                    context(set),
                    4,
                    Phase::Precommit,
                    Some([5; 32]),
                    set,
                    key.as_ref(),
                )
            })
            .collect()
    };
    let historical = Quorum::from_votes(&old, votes(&old)?)?;
    let bytes = encode_quorum(&historical)?;
    let certificate = decode_quorum(&bytes)?;
    assert_eq!(certificate.verify(&old)?.signed_weight(), 3);

    for other in [&next, &changed_set] {
        let error = certificate.verify(other).unwrap_err();
        assert_eq!(
            error.to_string(),
            "consensus context differs from validator set"
        );

        // Changing public fields to match the target does not create fresh signatures.
        let mut rewritten = certificate.clone();
        for vote in &mut rewritten.votes {
            vote.context = context(other);
        }
        assert!(format!("{:#}", rewritten.verify(other).unwrap_err())
            .contains("invalid round-bft vote signature"));
    }

    // Both individual signatures are valid; dropping the weight-1 signer
    // leaves a real, authenticated weight-2 certificate candidate.
    let heavy = Validator::new(keys[0].public_key(), 2)?;
    let single = certificate
        .votes
        .iter()
        .find(|vote| vote.validator_id == heavy.id())
        .context("weight-2 signer missing")?
        .clone();
    single.verify(&old)?;
    assert_eq!(old.quorum_weight(), 3);
    let below = Quorum {
        votes: vec![single],
    };
    assert_eq!(
        below.verify(&old).unwrap_err().to_string(),
        "insufficient quorum weight"
    );

    let successor = Quorum::from_votes(&next, votes(&next)?)?;
    assert_eq!(successor.verify(&next)?.signed_weight(), 3);
    assert_eq!(decode_quorum(&bytes)?.verify(&old)?.signed_weight(), 3);
    Ok(())
}
