// Separate signer databases, shared real AOEM authority. Not a network test.
fn exercise_fresh_successor_signing(
    path: &Path,
    params: &serde_json::Value,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    parent: [u8; 32],
    id: [u8; 32],
    competing: [u8; 32],
) {
    use crate::native_block_seal::commit_v3::NovNativeSealDecisionCertificateV3 as Certificate;
    use crate::native_block_seal::{
        NovNativeBlockSealStoreV1 as Seal, NovNativeSealLocalProposalRequestV1 as Request,
        NovNativeSealQuorumCertificateV1 as Qc,
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
    // Reuse the first-height databases: do not erase safety history to sign height two.
    let paths = (0..4)
        .map(|i| path.with_extension(format!("genesis-seal-{i}")))
        .collect::<Vec<_>>();
    let stores = paths
        .iter()
        .map(|p| Seal::open(p).unwrap())
        .collect::<Vec<_>>();
    let hash = workspace::load_block_artifact_v1(chain, id, params)
        .unwrap()
        .unwrap()
        .block()
        .header
        .block_hash;
    let other = workspace::load_block_artifact_v1(chain, competing, params)
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
    let proposal =
        workspace::with_verified_finalized_successor_v1(chain, parent, id, pin, params, |view| {
            assert!(view
                .load_seal_eligible_local_candidate_v1(chain, other)
                .is_err());
            assert!(view
                .abort_unselected_candidate_branch(chain, hash, "forbidden")
                .is_err());
            let wrong = Request {
                justify_qc_hash: Some([9; 32]),
                ..request.clone()
            };
            assert!(stores[1]
                .sign_local_proposal(view, &wrong, set, &keys[1])
                .is_err());
            let unadmitted_round = Request {
                round: 1,
                ..request.clone()
            };
            assert!(stores[2]
                .sign_local_proposal(view, &unadmitted_round, set, &keys[2])
                .is_err());
            stores[1].sign_local_proposal(view, &request, set, &keys[1])
        })
        .unwrap();
    assert_eq!(proposal.subject.height, 2);
    assert_eq!(
        proposal.subject.proof_version,
        crate::native_block_seal::NOV_NATIVE_BLOCK_SEAL_FRESH_SUCCESSOR_PROOF_V1
    );
    let votes = (0..3)
        .map(|i| {
            workspace::with_verified_finalized_successor_v1(
                chain,
                parent,
                id,
                pin,
                params,
                |view| stores[i].sign_local_vote(view, &proposal, set, &keys[i]),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(Qc::from_votes(proposal.subject.clone(), set, votes[..2].to_vec()).is_err());
    let qc = Qc::from_votes(proposal.subject.clone(), set, votes).unwrap();
    let decisions = (0..3)
        .map(|i| {
            workspace::with_verified_finalized_successor_v1(
                chain,
                parent,
                id,
                pin,
                params,
                |view| {
                    stores[i].persist_local_verified_qc(view, &qc, set)?;
                    stores[i].sign_local_decision_vote_v3(view, &qc, set, &keys[i])
                },
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(Certificate::from_votes(qc.clone(), set, decisions[..2].to_vec()).is_err());
    let certificate = Certificate::from_votes(qc.clone(), set, decisions.clone()).unwrap();
    let other_request = Request {
        block_hash: other,
        ..request.clone()
    };
    let other_proposal = workspace::with_verified_finalized_successor_v1(
        chain,
        parent,
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
    assert!(workspace::with_verified_finalized_successor_v1(
        chain,
        parent,
        competing,
        pin,
        params,
        |view| stores[0].sign_local_vote(view, &other_proposal, set, &keys[0])
    )
    .is_err());
    drop(stores);
    let reopened = Seal::open(&paths[0]).unwrap();
    assert_eq!(
        decisions[0],
        workspace::with_verified_finalized_successor_v1(chain, parent, id, pin, params, |view| {
            reopened.sign_local_decision_vote_v3(view, &qc, set, &keys[0])
        })
        .unwrap()
    );
    assert!(workspace::with_verified_finalized_successor_v1(
        chain,
        parent,
        id,
        pin,
        params,
        |view| reopened.persist_local_verified_decision_certificate_v3(view, &certificate, set)
    )
    .unwrap());
    drop(reopened);
    let reopened = Seal::open(&paths[0]).unwrap();
    assert_eq!(
        reopened
            .load_decision_certificate_by_height_v3(chain, 1, 2)
            .unwrap(),
        Some(certificate.clone())
    );
    assert!(!workspace::with_verified_finalized_successor_v1(
        chain,
        parent,
        id,
        pin,
        params,
        |view| reopened.persist_local_verified_decision_certificate_v3(view, &certificate, set)
    )
    .unwrap());
    assert!(
        !workspace::load_block_artifact_v1(chain, id, params)
            .unwrap()
            .unwrap()
            .block()
            .header
            .finalized
    );
}
