// Real AOEM subject and signatures through bounded wire/quarantine/collector.
// Authenticated-source identities are supplied by the fixture, not a live socket.
fn exercise_fresh_genesis_overlay(
    path: &Path,
    params: &serde_json::Value,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    id: [u8; 32],
    proposal: &crate::native_block_seal::NovNativeSealProposalV1,
    certificate: &crate::native_block_seal::commit_v3::NovNativeSealDecisionCertificateV3,
) {
    use crate::native_block_seal::commit_v3::collector::NovNativeSealDecisionCollectorV3 as Collector;
    use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
    use crate::native_block_seal::round_wire::{
        decode_nov_native_seal_round_wire_v1 as decode_round,
        encode_nov_native_seal_round_wire_v1 as encode_round,
    };
    use crate::native_block_seal_overlay::{
        decode_nov_native_seal_overlay_wire_v1 as decode,
        encode_nov_native_seal_overlay_wire_v1 as encode,
        NovNativeSealEpochAuthorityV1 as Authority, NovNativeSealOverlayArtifactV1 as Artifact,
        NovNativeSealOverlayIngressContextV1 as Ingress,
        NovNativeSealOverlayQuarantineV1 as Quarantine,
        NovNativeSealValidatorTransportBindingV1 as Binding,
    };
    let chain = compiled.identity().chain_id();
    let pin = compiled.config_commitment();
    let bindings = compiled
        .validator_set()
        .validators
        .iter()
        .enumerate()
        .map(|(index, v)| Binding {
            validator_id: v.validator_id,
            transport_peer_id: format!("fresh-peer-{index}"),
        })
        .collect::<Vec<_>>();
    let authority =
        workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
            let (config, _) = view.fresh_genesis_seal_config_v1(chain)?.unwrap();
            assert!(Authority::derive_operator_pinned_fresh_genesis_epoch(
                config,
                [9; 32],
                bindings.clone()
            )
            .is_err());
            let authority = Authority::derive_operator_pinned_fresh_genesis_epoch(
                config,
                pin,
                bindings.clone(),
            )?;
            authority.validate_against_ledger(view)?;
            let mut reverse = bindings.clone();
            reverse.reverse();
            assert_eq!(
                authority,
                Authority::derive_operator_pinned_fresh_genesis_epoch(config, pin, reverse)?
            );
            let mut foreign = config.clone();
            foreign.timestamp_unix_ms += 1;
            let foreign = Authority::derive_operator_pinned_fresh_genesis_epoch(
                &foreign,
                foreign.compile()?.config_commitment(),
                bindings.clone(),
            )?;
            assert!(foreign.validate_against_ledger(view).is_err());
            Ok(authority)
        })
        .unwrap();
    let peer = authority.transport_peer_id(proposal.proposer_id).unwrap();
    let artifact = Artifact::Proposal(Box::new(proposal.clone()));
    let wire = encode(&artifact, &authority).unwrap();
    assert_eq!(decode(&wire, &authority).unwrap().artifact, artifact);
    assert!(artifact
        .validate_authenticated_source(&authority, "unknown-peer")
        .is_err());
    assert!(artifact
        .validate_authenticated_source(&authority, &bindings[1].transport_peer_id)
        .is_err());
    let quarantine = Quarantine::open(&path.with_extension("fresh-quarantine")).unwrap();
    let seal = crate::native_block_seal::NovNativeBlockSealStoreV1::open(
        &path.with_extension("fresh-imported-seal"),
    )
    .unwrap();
    workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
        assert!(quarantine.bind_epoch_authority(
            view,
            &authority,
            authority.authority_commitment
        )?);
        Ok(())
    })
    .unwrap();
    let ingress = Ingress {
        local_execution_height: 0,
        local_validator_id: None,
        received_at_unix_ms: proposal.subject.timestamp_unix_ms,
    };
    quarantine
        .ingest_authenticated_wire(&authority, peer, &wire, &ingress)
        .unwrap();
    let mut collector =
        workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, |view| {
            quarantine.reconcile_proposal_with_local_execution(
                view,
                &seal,
                &authority,
                proposal.proposal_hash,
            )?;
            Collector::new(view, &seal, authority.clone(), &certificate.prepare)
        })
        .unwrap();
    for (index, vote) in certificate.votes.iter().enumerate() {
        let message = Message::DecisionVoteV3 {
            proposal: Box::new(proposal.clone()),
            qc: Box::new(certificate.prepare.clone()),
            vote: Box::new(vote.clone()),
            certificate: None,
        };
        let source = authority.transport_peer_id(vote.validator_id).unwrap();
        let bytes = encode_round(&message, &authority, 1, source).unwrap();
        assert!(decode_round(&bytes, &authority, 1, "unknown-peer").is_err());
        assert_eq!(
            decode_round(&bytes, &authority, 1, source).unwrap(),
            message
        );
        collector.ingest_wire(source, &bytes).unwrap();
        assert_eq!(collector.certificate_message().is_some(), index == 2);
    }
    let message = Message::DecisionCertificateV3 {
        proposal: Box::new(proposal.clone()),
        decision: Box::new(certificate.clone()),
        certificate: None,
    };
    let bytes = encode_round(&message, &authority, 1, peer).unwrap();
    assert_eq!(decode_round(&bytes, &authority, 1, peer).unwrap(), message);
    assert_eq!(collector.signed_weight(), 3);
    drop(quarantine);
    let reopened = Quarantine::open(&path.with_extension("fresh-quarantine")).unwrap();
    assert_eq!(
        reopened.load_epoch_authority(chain, 1).unwrap(),
        Some(authority)
    );
}
