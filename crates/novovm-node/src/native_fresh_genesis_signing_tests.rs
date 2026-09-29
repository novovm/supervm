// Four signers with separate seal databases, one real AOEM candidate authority.
// Not an independent-node or network consensus acceptance test.
fn exercise_fresh_genesis_signing(
    path: &Path,
    params: &serde_json::Value,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    id: [u8; 32],
    competing: [u8; 32],
) {
    use crate::native_block_seal::commit_v3::NovNativeSealDecisionCertificateV3 as Certificate;
    use crate::native_block_seal::{
        NovNativeBlockSealStoreV1 as Seal, NovNativeSealLocalProposalRequestV1 as Request,
        NovNativeSealQuorumCertificateV1 as Qc, NOV_NATIVE_BLOCK_SEAL_FRESH_GENESIS_PROOF_V1,
        NOV_NATIVE_BLOCK_SEAL_PROOF_VERSION_V1,
    };
    let set = compiled.validator_set();
    let chain = set.chain_id;
    let pin = compiled.config_commitment();
    let mut keys = (1..=4)
        .map(|seed| ed25519_dalek::SigningKey::from_bytes(&[seed; 32]))
        .collect::<Vec<_>>();
    keys.sort_by_key(|key| {
        crate::native_block_seal::NovNativeSealValidatorV1::new(key.verifying_key().to_bytes(), 1)
            .unwrap()
            .validator_id
    });
    let seal_paths = (0..4)
        .map(|index| path.with_extension(format!("genesis-seal-{index}")))
        .collect::<Vec<_>>();
    let stores = seal_paths
        .iter()
        .map(|p| Seal::open(p).unwrap())
        .collect::<Vec<_>>();
    let hash = workspace::load_block_artifact_v1(chain, id, params)
        .unwrap()
        .unwrap()
        .block()
        .header
        .block_hash;
    let other_hash = workspace::load_block_artifact_v1(chain, competing, params)
        .unwrap()
        .unwrap()
        .block()
        .header
        .block_hash;
    let request = Request {
        chain_id: chain,
        block_hash: hash,
        round: 0,
        justify_qc_hash: None,
    };
    assert!(workspace::with_verified_genesis_block_candidate_v1(
        chain,
        id,
        [9; 32],
        params,
        |_| Ok(())
    )
    .is_err());
    let proposal =
        workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
            assert!(view.load_by_height(chain, 1)?.is_none());
            assert!(view.load_aoem_ownership()?.is_none());
            assert!(view
                .abort_unselected_candidate_branch(chain, hash, "forbidden")
                .is_err());
            assert!(view
                .load_seal_eligible_local_candidate_v1(chain, other_hash)
                .is_err());
            let mut foreign_validators = set.validators.clone();
            foreign_validators[0].weight += 1;
            let wrong_set = crate::native_block_seal::NovNativeSealValidatorSetV1::new(
                chain,
                1,
                1,
                foreign_validators,
            )
            .unwrap();
            assert!(stores[0]
                .sign_local_proposal(view, &request, &wrong_set, &keys[0])
                .is_err());
            stores[0].sign_local_proposal(view, &request, set, &keys[0])
        })
        .unwrap();
    assert_eq!(
        proposal.subject.proof_version,
        NOV_NATIVE_BLOCK_SEAL_FRESH_GENESIS_PROOF_V1
    );
    assert_eq!(
        proposal.subject.genesis_block_hash,
        compiled.identity().anchor()
    );
    assert_ne!(
        proposal.subject.genesis_block_hash,
        proposal.subject.block_hash
    );
    let mut legacy = proposal.clone();
    legacy.subject.proof_version = NOV_NATIVE_BLOCK_SEAL_PROOF_VERSION_V1.into();
    assert!(legacy.subject.validate(set).is_err());
    let votes = (0..3)
        .map(|index| {
            workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
                stores[index].sign_local_vote(view, &proposal, set, &keys[index])
            })
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(Qc::from_votes(proposal.subject.clone(), set, votes[..2].to_vec()).is_err());
    let qc = Qc::from_votes(proposal.subject.clone(), set, votes).unwrap();
    let decisions = (0..3)
        .map(|index| {
            workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
                stores[index].persist_local_verified_qc(view, &qc, set)?;
                stores[index].sign_local_decision_vote_v3(view, &qc, set, &keys[index])
            })
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(Certificate::from_votes(qc.clone(), set, decisions[..2].to_vec()).is_err());
    let certificate = Certificate::from_votes(qc.clone(), set, decisions.clone()).unwrap();
    exercise_fresh_genesis_overlay(path, params, compiled, id, &proposal, &certificate);
    let other_request = Request {
        block_hash: other_hash,
        ..request.clone()
    };
    // An unsigned alternative can be reconstructed, but a signer already locked
    // on this height cannot issue a conflicting proposal or prepare vote.
    let other_proposal = workspace::with_verified_genesis_block_candidate_v1(
        chain,
        competing,
        pin,
        params,
        |view| {
            assert!(stores[0]
                .sign_local_proposal(view, &other_request, set, &keys[0])
                .is_err());
            stores[3].sign_local_proposal(view, &other_request, set, &keys[3])
        },
    )
    .unwrap();
    assert_eq!(
        other_proposal.subject.genesis_block_hash,
        proposal.subject.genesis_block_hash
    );
    assert!(workspace::with_verified_genesis_block_candidate_v1(
        chain,
        competing,
        pin,
        params,
        |view| stores[0].sign_local_vote(view, &other_proposal, set, &keys[0])
    )
    .is_err());
    drop(stores);
    let reopened = Seal::open(&seal_paths[0]).unwrap();
    assert_eq!(
        workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
            reopened.sign_local_decision_vote_v3(view, &qc, set, &keys[0])
        })
        .unwrap(),
        decisions[0]
    );
    assert!(
        workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
            reopened.persist_local_verified_decision_certificate_v3(view, &certificate, set)
        })
        .unwrap()
    );
    drop(reopened);
    let reopened = Seal::open(&seal_paths[0]).unwrap();
    assert_eq!(
        reopened
            .load_decision_certificate_by_height_v3(chain, 1, 1)
            .unwrap(),
        Some(certificate.clone())
    );
    assert!(
        !workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
            reopened.persist_local_verified_decision_certificate_v3(view, &certificate, set)
        })
        .unwrap()
    );
    assert!(
        !workspace::load_block_artifact_v1(chain, id, params)
            .unwrap()
            .unwrap()
            .block()
            .header
            .finalized
    );
}

include!("native_fresh_genesis_overlay_tests.rs");
