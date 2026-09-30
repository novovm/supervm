// Bounded wire/quarantine/collector coverage. Sources are fixture identities,
// not socket-authenticated peers: this is not real-network acceptance.
fn exercise_fresh_successor_overlay(
    path: &Path,
    params: &serde_json::Value,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    parent: [u8; 32],
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
        .map(|(i, v)| Binding {
            validator_id: v.validator_id,
            transport_peer_id: format!("fresh-peer-{i}"),
        })
        .collect::<Vec<_>>();
    let authority =
        workspace::with_verified_finalized_successor_v1(chain, parent, id, pin, params, |view| {
            let (config, _) = view.fresh_genesis_seal_config_v1(chain)?.unwrap();
            let authority = Authority::derive_operator_pinned_fresh_genesis_epoch(
                config,
                pin,
                bindings.clone(),
            )?;
            authority.validate_against_ledger(view)?;
            Ok(authority)
        })
        .unwrap();
    let parent_image =
        workspace::load_finalized_genesis_parent_v1(chain, parent, pin, params).unwrap();
    assert_eq!(
        authority.authority_commitment,
        Authority::derive_operator_pinned_fresh_genesis_epoch(
            parent_image.genesis_config(),
            pin,
            bindings.clone()
        )
        .unwrap()
        .authority_commitment
    );
    let Message::DecisionCertificateV3 {
        proposal: first_proposal,
        ..
    } = &parent_image.finality_proof().witness
    else {
        unreachable!()
    };
    let quarantine = Quarantine::open(&path.with_extension("fresh-quarantine")).unwrap();
    assert_eq!(
        quarantine.load_epoch_authority(chain, 1).unwrap(),
        Some(authority.clone())
    );
    let seal = crate::native_block_seal::NovNativeBlockSealStoreV1::open(
        &path.with_extension("fresh-imported-seal"),
    )
    .unwrap();
    workspace::with_verified_finalized_successor_v1(chain, parent, id, pin, params, |view| {
        assert!(!quarantine.bind_epoch_authority(
            view,
            &authority,
            authority.authority_commitment
        )?);
        assert!(quarantine
            .reconcile_proposal_with_local_execution(
                view,
                &seal,
                &authority,
                first_proposal.proposal_hash
            )
            .is_err());
        Ok(())
    })
    .unwrap();
    let peer = authority.transport_peer_id(proposal.proposer_id).unwrap();
    let artifact = Artifact::Proposal(Box::new(proposal.clone()));
    let wire = encode(&artifact, &authority).unwrap();
    assert_eq!(decode(&wire, &authority).unwrap().artifact, artifact);
    assert!(artifact
        .validate_authenticated_source(&authority, "unknown-peer")
        .is_err());
    let ingress = Ingress {
        local_execution_height: 1,
        local_validator_id: None,
        received_at_unix_ms: proposal.subject.timestamp_unix_ms,
    };
    quarantine
        .ingest_authenticated_wire(&authority, peer, &wire, &ingress)
        .unwrap();
    let mut collector =
        workspace::with_verified_finalized_successor_v1(chain, parent, id, pin, params, |view| {
            quarantine.reconcile_proposal_with_local_execution(
                view,
                &seal,
                &authority,
                proposal.proposal_hash,
            )?;
            Collector::new(view, &seal, authority.clone(), &certificate.prepare)
        })
        .unwrap();
    for (i, vote) in certificate.votes.iter().enumerate() {
        let message = Message::DecisionVoteV3 {
            proposal: Box::new(proposal.clone()),
            qc: Box::new(certificate.prepare.clone()),
            vote: Box::new(vote.clone()),
            certificate: None,
        };
        let source = authority.transport_peer_id(vote.validator_id).unwrap();
        let bytes = encode_round(&message, &authority, 2, source).unwrap();
        assert!(decode_round(&bytes, &authority, 1, source).is_err());
        assert!(collector.ingest_wire("unknown-peer", &bytes).is_err());
        assert_eq!(
            decode_round(&bytes, &authority, 2, source).unwrap(),
            message
        );
        collector.ingest_wire(source, &bytes).unwrap();
        let weight = collector.signed_weight();
        assert!(!collector.ingest_wire(source, &bytes).unwrap());
        assert_eq!(collector.signed_weight(), weight);
        assert_eq!(collector.certificate_message().is_some(), i == 2);
    }
    assert_eq!(collector.signed_weight(), 3);
    let decision = Message::DecisionCertificateV3 {
        proposal: Box::new(proposal.clone()),
        decision: Box::new(certificate.clone()),
        certificate: None,
    };
    let bytes = encode_round(&decision, &authority, 2, peer).unwrap();
    assert_eq!(decode_round(&bytes, &authority, 2, peer).unwrap(), decision);
    let relay = authority
        .transport_bindings
        .iter()
        .find(|binding| binding.transport_peer_id != peer)
        .unwrap()
        .transport_peer_id
        .clone();
    let raw_txs = workspace::load_block_artifact_v1(chain, id, params)
        .unwrap()
        .unwrap()
        .block()
        .body
        .raw_txs
        .clone();
    let relay_wire = encode_round(&decision, &authority, 2, &relay).unwrap();
    let mut assembler = crate::native_candidate_body::CandidateBodyAssemblerV1::new(
        &relay_wire, &authority, 2, &relay,
    )
    .unwrap();
    let chunks = assembler.encode_chunks(&raw_txs).unwrap();
    let mut completed = None;
    for chunk in chunks {
        completed = assembler.push(&relay, &chunk).unwrap();
    }
    assert_eq!(completed.unwrap().raw_txs, raw_txs);
    assert!(crate::native_candidate_body::CandidateBodyAssemblerV1::new(
        &relay_wire, &authority, 3, &relay,
    )
    .is_err());
    assert!(crate::native_candidate_body::CandidateBodyAssemblerV1::new(
        &relay_wire, &authority, 2, "unbound",
    )
    .is_err());
    drop(quarantine);
    let reopened = Quarantine::open(&path.with_extension("fresh-quarantine")).unwrap();
    assert_eq!(
        reopened.load_epoch_authority(chain, 1).unwrap(),
        Some(authority)
    );
}
