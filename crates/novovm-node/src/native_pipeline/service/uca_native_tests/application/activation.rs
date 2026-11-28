//! Existing-store activation via the original four validator and application paths.
use super::*;
use crate::native_pipeline::{
    consensus::{business_activation::review_activation, ArchiveRead},
    service::client::operator::receipt_proof::Download,
};
use novovm_protocol::{
    native_query::{ApplicationCommandStatusV1, IdentityStateV1, RegistrationStatusV1},
    uca_registration::{SignedUcaRegistrationV1, UcaRegistrationIntentV1, REGISTRATION_CONTEXT_V1},
};
#[cfg(feature = "iroh-transport")]
mod authority_proof;
mod concurrent;

fn application_command_status(
    nodes: &mut [rpc::RpcLifecycle],
    client: &mut client::NativeApplicationClient,
    command: &SignedApplicationGrantV1,
) -> Result<ApplicationCommandStatusV1> {
    let selector = Query::UcaApplicationCommand {
        command_id: command.intent.command_id()?,
    };
    let deadline = Instant::now() + TIMEOUT;
    loop {
        ensure!(
            Instant::now() < deadline,
            "application command status deadline"
        );
        match query(nodes, client, selector.clone())? {
            Answer::UcaApplicationCommand(ApplicationCommandStatusV1::NotRecent) => continue,
            Answer::UcaApplicationCommand(status) => return Ok(status),
            answer => anyhow::bail!("unexpected application command answer: {answer:?}"),
        }
    }
}

pub(super) fn run(
    runtime: &AoemExecFacade,
    configs: &[ResidentConfig],
    mut nodes: Vec<rpc::RpcLifecycle>,
    mut clients: Vec<client::NativeApplicationClient>,
    client_identity: novovm_network::product_overlay::TransportIdentity,
    prior: &[PublishedHead],
) -> Result<()> {
    use novovm_adapter_api::{uca_key_binding::derive_primary_key_ref_from_binding_v1, UcaKeyAlgo};

    let parent = nodes[0].node.controller.parent();
    let boundary = nodes[0].node.controller.context();
    ensure!(
        boundary.height == parent.height + 1
            && prior.iter().all(|head| {
                head.state_root == parent.state_root && head.block_hash == parent.block_hash
            }),
        "activation fixture must start on the already published checkpoint"
    );
    let mut archive = ArchiveRead::new(
        parent.height,
        parent,
        boundary,
        nodes[0].node.validators.clone(),
    )?;
    let deadline = Instant::now() + TIMEOUT;
    let preceding = loop {
        ensure!(
            Instant::now() < deadline,
            "activation fixture archive deadline"
        );
        if let Some(block) = archive.poll(&nodes[0].node.pipeline)? {
            break block
                .context("fixture finalized anchor not found")?
                .parent();
        }
        tick(&mut nodes)?;
    };
    let mut download = Download::header(parent.height);
    let header = loop {
        ensure!(
            Instant::now() < deadline,
            "activation fixture anchor download deadline"
        );
        let response = query(&mut nodes, &mut clients[0], download.selector()?)?;
        if let Some(bytes) = download.accept(response)? {
            break bytes;
        }
    };
    let reviewed = review_activation(&configs[0], runtime, &preceding, &header)?;
    assert_eq!(reviewed.subject().context, boundary);
    assert_eq!(reviewed.subject().parent_state_root, parent.state_root);
    assert_eq!(reviewed.subject().next.semantic_version, 6);
    assert!(reviewed.assemble(vec![]).is_err());
    let mut corrupted = header.clone();
    *corrupted.last_mut().context("test archive empty")? ^= 1;
    assert!(review_activation(&configs[0], runtime, &preceding, &corrupted).is_err());
    let approvals = configs
        .iter()
        .map(|config| review_activation(config, runtime, &preceding, &header)?.approve(runtime))
        .collect::<Result<Vec<_>>>()?;
    let bundle = reviewed.assemble(approvals)?;
    let mut configs = configs.to_vec();
    for config in &mut configs {
        config.business_activation = Some(bundle.clone());
        config.native_client_limit = 8;
    }
    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    let reopen = || -> Result<_> {
        let nodes = start(&configs, StartMode::Existing)?;
        let validators = nodes[0].node.validators.clone();
        let clients = configs
            .iter()
            .map(|config| connect(config, validators.clone(), client_identity.clone()))
            .collect::<Result<Vec<_>>>()?;
        Ok((nodes, clients))
    };
    let (mut nodes, mut clients) = reopen()?;
    assert_eq!(heads(&mut nodes, &mut clients, parent.height, 0)?, prior);
    let ingress = (0..4)
        .find(|index| !nodes[*index].node.controller.is_local_leader().unwrap())
        .context("nonleader registration ingress")?;

    let crypto = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let key = crypto.generate_keypair()?;
    let reference = derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &key.public_key);
    let name = format!(
        "uca:{}",
        reference
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let identity = UcaIdentityIdV1::new(&name).map_err(anyhow::Error::msg)?;
    let intent = UcaRegistrationIntentV1 {
        chain_id: boundary.chain_id,
        genesis: boundary.genesis_config_commitment,
        identity: identity.clone(),
        primary_key_ref: reference.as_slice().try_into()?,
        valid_from_height: boundary.height,
        valid_until_height: boundary.height + 8,
    };
    let signature = crypto.sign(
        &key.public_key,
        &key.expanded_secret_key,
        &intent.signing_bytes()?,
        REGISTRATION_CONTEXT_V1,
    )?;
    let proof = SignedUcaRegistrationV1 {
        intent,
        public_key: key.public_key.to_vec(),
        signature,
    };
    let mut competing = proof.intent.clone();
    competing.valid_until_height += 1;
    let competing = runtime.sign_uca_registration_v1(
        competing,
        &key.public_key,
        &key.expanded_secret_key,
        boundary.chain_id,
        boundary.genesis_config_commitment,
        boundary.height,
    )?;
    let raw = [proof.encode()?, competing.encode()?];
    let registration_queries = raw.each_ref().map(|raw| Query::UcaRegistration {
        request_hash: Sha256::digest(raw).into(),
    });
    let solo = std::slice::from_mut(&mut nodes[ingress]);
    for raw in raw {
        let prepared = clients[ingress].prepare_uca_registration(raw)?;
        send_input(solo, &mut clients[ingress], &prepared)?;
    }
    let deadline = Instant::now() + TIMEOUT;
    for selector in &registration_queries {
        loop {
            ensure!(
                Instant::now() < deadline,
                "same-identity concurrent intake deadline"
            );
            match query(solo, &mut clients[ingress], selector.clone())? {
                Answer::UcaRegistration(RegistrationStatusV1::NotRecent) => continue,
                Answer::UcaRegistration(
                    RegistrationStatusV1::Received | RegistrationStatusV1::Verified,
                ) => break,
                answer => anyhow::bail!("same-identity claims must both reach intake: {answer:?}"),
            }
        }
    }
    let registered = heads(&mut nodes, &mut clients, boundary.height, 0)?;
    assert_ne!(registered[0].state_root, parent.state_root);
    let contests = registration_queries
        .iter()
        .map(|selector| query(&mut nodes, &mut clients[ingress], selector.clone()))
        .collect::<Result<Vec<_>>>()?;
    let winner = Answer::UcaRegistration(RegistrationStatusV1::Finalized {
        height: boundary.height,
        block_hash: registered[0].block_hash,
    });
    assert!(contests.contains(&winner));
    assert!(contests.contains(&Answer::UcaRegistration(
        RegistrationStatusV1::IdentityAlreadyRegistered
    )));
    println!("UCA same identity: {registration_queries:?} => {contests:?}");
    #[cfg(feature = "iroh-transport")]
    let authority_verifier = {
        use crate::native_pipeline::{
            proof::uca_authority::UcaAuthorityClaimV1,
            service::client::operator::receipt_proof::FinalityVerifier,
        };
        let domain = MessageDomain {
            chain_id: boundary.chain_id,
            genesis: boundary.genesis_config_commitment,
            protocol: boundary.protocol_commitment,
            validators: nodes[0].node.validators.clone(),
        };
        let verifier = FinalityVerifier::new(&domain, client_identity.clone());
        let intents = [&proof.intent, &competing.intent];
        let selected = usize::from(contests[0] != winner);
        authority_proof::check_certified(
            &mut nodes,
            &mut clients[ingress],
            &verifier,
            parent,
            UcaAuthorityClaimV1::Registration(intents[selected].clone()),
            UcaAuthorityClaimV1::Registration(intents[1 - selected].clone()),
            &format!("registration-{}", boundary.height),
        )?;
        verifier
    };
    let selector = Query::UcaIdentity {
        chain_id: boundary.chain_id,
        genesis: boundary.genesis_config_commitment,
        identity: identity.clone(),
    };
    for client in &mut clients {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            ensure!(
                Instant::now() < deadline,
                "native registration identity query timeout"
            );
            match query(&mut nodes, client, selector.clone())? {
                Answer::UcaIdentity(IdentityStateV1::Present(view)) => {
                    assert_eq!(view.primary_key_ref, proof.intent.primary_key_ref);
                    assert_eq!(view.state_root, registered[0].state_root);
                    assert_eq!(view.status, IdentityStatusV1::Active);
                    break;
                }
                Answer::UcaIdentity(IdentityStateV1::Pending) => (),
                answer => anyhow::bail!("unexpected native registration query result: {answer:?}"),
            }
        }
    }
    let selector = ApplicationGrantSelectorV1 {
        chain_id: boundary.chain_id,
        genesis: boundary.genesis_config_commitment,
        identity,
        application: [71; 32],
        service: [72; 32],
        device: [73; 32],
    };
    let grant_height = boundary.height + 1;
    let application = runtime.application_grant_signer_v1()?.sign(
        ApplicationGrantIntentV1 {
            selector: selector.clone(),
            issuer_key_ref: proof.intent.primary_key_ref,
            expected_revision: 0,
            valid_from_height: grant_height,
            valid_until_height: grant_height + 8,
            action: ApplicationGrantActionV1::Authorize {
                not_before_height: grant_height,
                expires_at_height: grant_height + 20,
            },
        },
        &key.public_key,
        &key.expanded_secret_key,
    )?;
    send_application(&mut nodes, &mut clients[ingress], &application)?;
    let published = heads(&mut nodes, &mut clients, grant_height, 0)?;
    assert_ne!(published[0].state_root, registered[0].state_root);
    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    let (mut nodes, mut clients) = reopen()?;
    assert_eq!(heads(&mut nodes, &mut clients, grant_height, 0)?, published);
    for grant in grant_views(&mut nodes, &mut clients, &selector, grant_height)? {
        assert_eq!(grant.issuer_key_ref, proof.intent.primary_key_ref);
        assert_eq!(grant.revision, 1);
        assert!(!grant.revoked);
        assert_eq!(grant.not_before_height, grant_height);
        assert_eq!(grant.expires_at_height, grant_height + 20);
    }
    #[cfg(feature = "iroh-transport")]
    let grant_parent = nodes[0].node.controller.parent();
    let revoke_height = grant_height + 1;
    let replacement_intent = ApplicationGrantIntentV1 {
        expected_revision: 1,
        valid_from_height: revoke_height,
        action: ApplicationGrantActionV1::Authorize {
            not_before_height: revoke_height,
            expires_at_height: grant_height + 20,
        },
        ..application.intent.clone()
    };
    let signer = runtime.application_grant_signer_v1()?;
    let make_replacement = |device, revision| {
        signer.sign(
            replacement_intent
                .clone()
                .into_device_replacement(device, revision)?,
            &key.public_key,
            &key.expanded_secret_key,
        )
    };
    let mut successor = selector.clone();
    successor.device = [74; 32];
    let mut alternative = selector.clone();
    alternative.device = [75; 32];
    missing_grant(&mut nodes, &mut clients, &successor, &published[0])?;
    missing_grant(&mut nodes, &mut clients, &alternative, &published[0])?;
    let stale = make_replacement(successor.device, 1)?;
    send_application(&mut nodes, &mut clients[ingress], &stale)?;
    assert_eq!(
        application_command_status(&mut nodes, &mut clients[ingress], &stale)?,
        ApplicationCommandStatusV1::Rejected
    );
    assert_eq!(heads(&mut nodes, &mut clients, grant_height, 0)?, published);
    missing_grant(&mut nodes, &mut clients, &successor, &published[0])?;
    for grant in grant_views(&mut nodes, &mut clients, &selector, grant_height)? {
        assert_eq!(grant.revision, 1);
        assert!(!grant.revoked);
    }

    let replacement = make_replacement(successor.device, 0)?;
    let competing = make_replacement(alternative.device, 0)?;
    let solo = std::slice::from_mut(&mut nodes[ingress]);
    send_application(solo, &mut clients[ingress], &replacement)?;
    assert_eq!(
        application_command_status(solo, &mut clients[ingress], &replacement)?,
        ApplicationCommandStatusV1::Received
    );
    send_application(solo, &mut clients[ingress], &competing)?;
    assert_eq!(
        application_command_status(solo, &mut clients[ingress], &competing)?,
        ApplicationCommandStatusV1::Rejected
    );
    let revoked = heads(&mut nodes, &mut clients, revoke_height, 0)?;
    assert_ne!(revoked[0].state_root, published[0].state_root);
    #[cfg(feature = "iroh-transport")]
    {
        use crate::native_pipeline::proof::uca_authority::UcaAuthorityClaimV1;
        authority_proof::check_certified(
            &mut nodes,
            &mut clients[ingress],
            &authority_verifier,
            grant_parent,
            UcaAuthorityClaimV1::DeviceGrant {
                selector: successor.clone(),
                revision: 1,
            },
            UcaAuthorityClaimV1::DeviceGrant {
                selector: selector.clone(),
                revision: 1,
            },
            &format!("replacement-{}", revoke_height),
        )?;
    }
    for grant in grant_views(&mut nodes, &mut clients, &selector, revoke_height)? {
        assert_eq!(grant.revision, 2);
        assert!(grant.revoked);
        assert_eq!(grant.state_root, revoked[0].state_root);
    }
    for grant in grant_views(&mut nodes, &mut clients, &successor, revoke_height)? {
        assert_eq!(grant.revision, 1);
        assert!(!grant.revoked);
        assert_eq!(grant.state_root, revoked[0].state_root);
    }
    missing_grant(&mut nodes, &mut clients, &alternative, &revoked[0])?;
    let replacement_status =
        application_command_status(&mut nodes, &mut clients[ingress], &replacement)?;
    let ApplicationCommandStatusV1::Finalized {
        ref receipt,
        height,
        block_hash,
        state_root,
    } = replacement_status
    else {
        anyhow::bail!("replacement did not finalize: {replacement_status:?}");
    };
    assert_eq!(
        (height, block_hash, state_root),
        (revoke_height, revoked[0].block_hash, revoked[0].state_root)
    );
    assert!(receipt.revoked);
    assert_eq!((receipt.previous_revision, receipt.revision), (1, 2));
    println!("UCA atomic device replacement on four stores: {replacement_status:?}; stale successor and competing replacement rejected");

    // The original authorization is still within its signed height window,
    // so rejection here depends on the published revision rather than expiry.
    let Answer::Status(before) = query(&mut nodes, &mut clients[ingress], Query::Status)? else {
        anyhow::bail!("native stale-authorization baseline unavailable");
    };
    assert!(before.current_height < application.intent.valid_until_height);
    send_application(&mut nodes, &mut clients[ingress], &application)?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        ensure!(
            Instant::now() < deadline,
            "stale native authorization was not rejected"
        );
        if let Answer::Status(status) = query(&mut nodes, &mut clients[ingress], Query::Status)? {
            assert_eq!(
                (status.height, status.state_root),
                (revoke_height, revoked[0].state_root)
            );
            if status.rejected_inputs > before.rejected_inputs {
                break;
            }
        }
    }
    assert_eq!(heads(&mut nodes, &mut clients, revoke_height, 0)?, revoked);
    let (request, expected, final_height) =
        concurrent::run(runtime, &configs, &mut nodes, &mut clients)?;
    let before_reopen = heads(&mut nodes, &mut clients, final_height, 0)?;
    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    let (mut nodes, mut clients) = reopen()?;
    assert_eq!(
        heads(&mut nodes, &mut clients, final_height, 0)?,
        before_reopen
    );
    for client in &mut clients {
        assert_eq!(
            query(&mut nodes, client, request.clone())?,
            Answer::UcaRegistration(expected)
        );
        assert_eq!(
            application_command_status(&mut nodes, client, &replacement)?,
            replacement_status
        );
    }
    for grant in grant_views(&mut nodes, &mut clients, &successor, final_height)? {
        assert_eq!(grant.revision, 1);
        assert!(!grant.revoked);
        assert_eq!(grant.state_root, before_reopen[0].state_root);
    }
    for grant in grant_views(&mut nodes, &mut clients, &selector, final_height)? {
        assert_eq!(grant.revision, 2);
        assert!(grant.revoked);
        assert_eq!(grant.state_root, before_reopen[0].state_root);
    }
    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    println!("Existing-store V6 activation, atomic device replacement, cold reopen and stale grant rejection verified on four validators");
    Ok(())
}
