//! Existing-store activation via the original four validator and application paths.
use super::*;
use crate::native_pipeline::{
    consensus::{business_activation::review_activation, ArchiveRead},
    service::client::operator::receipt_proof::Download,
};
use novovm_protocol::{
    native_query::IdentityStateV1,
    uca_registration::{SignedUcaRegistrationV1, UcaRegistrationIntentV1, REGISTRATION_CONTEXT_V1},
};

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
    let prepared = clients[ingress].prepare_uca_registration(proof.encode()?)?;
    send_input(&mut nodes, &mut clients[ingress], &prepared)?;
    let registered = heads(&mut nodes, &mut clients, boundary.height, 0)?;
    assert_ne!(registered[0].state_root, parent.state_root);
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
    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    println!("Existing-store V5 activation and native UCA/NUAP publication survived a four-validator cold reopen");
    Ok(())
}
