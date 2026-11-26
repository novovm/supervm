//! Real native input and published queries only; no pool, vote or tree injection.
use super::*;
mod activation;
use novovm_protocol::{
    native_query::{
        ApplicationGrantStateV1, ApplicationGrantViewV1, IdentityStatusV1, UcaIdentityIdV1,
    },
    uca_application::{
        ApplicationGrantActionV1, ApplicationGrantIntentV1, ApplicationGrantSelectorV1,
        SignedApplicationGrantV1,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublishedHead {
    height: u64,
    block_hash: [u8; 32],
    state_root: [u8; 32],
    receipt_batch_commitment: [u8; 32],
    state_version: u64,
}

struct Observations {
    selectors: [ApplicationGrantSelectorV1; 2],
    alice: AssetSpendIntentV3,
    bob: AssetSpendIntentV3,
}

#[derive(Debug, PartialEq, Eq)]
struct PublishedSnapshot {
    heads: Vec<PublishedHead>,
    grants: Vec<Vec<ApplicationGrantViewV1>>,
    assets: Vec<(AssetSpendStateViewV2, AssetSpendStateViewV2)>,
}

fn send_application(
    nodes: &mut [rpc::RpcLifecycle],
    client: &mut client::NativeApplicationClient,
    signed: &SignedApplicationGrantV1,
) -> Result<()> {
    let prepared = client.prepare_application_grants(vec![signed.encode()?])?;
    send_input(nodes, client, &prepared)
}

fn send_input(
    nodes: &mut [rpc::RpcLifecycle],
    client: &mut client::NativeApplicationClient,
    prepared: &client::PreparedApplicationInput,
) -> Result<()> {
    let deadline = Instant::now() + TIMEOUT;
    let mut sent = false;
    loop {
        ensure!(
            Instant::now() < deadline,
            "native application delivery timeout"
        );
        tick(nodes)?;
        let now = Instant::now();
        for completion in client.poll(wall_ms()?, now, 64)?.completed {
            if completion.body.id() == prepared.id() {
                ensure!(
                    completion.state == ObjectSendState::Delivered,
                    "application input expired"
                );
                return Ok(()); // Custody only; published queries below establish test evidence.
            }
        }
        if !sent {
            sent = client.try_send(prepared, now)?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn heads(
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
    height: u64,
    pending: u64,
) -> Result<Vec<PublishedHead>> {
    let mut heads = Vec::new();
    for (client_index, client) in clients.iter_mut().enumerate() {
        let deadline = Instant::now() + TIMEOUT;
        let mut last_status = None;
        loop {
            ensure!(
                Instant::now() < deadline,
                "four-service application publication timeout at {height}, client={client_index}, expected_pending={pending}, last_native_status={last_status:?}, nodes={:?}",
                publication_diagnostics(nodes)
            );
            let answer = query(nodes, client, Query::Status).with_context(|| format!(
                "application publication status request failed at {height}, client={client_index}, last_native_status={last_status:?}, nodes={:?}",
                publication_diagnostics(nodes)
            ))?;
            let Answer::Status(status) = answer else {
                anyhow::bail!("application published status unavailable");
            };
            last_status = Some(status);
            ensure!(
                status.height <= height,
                "unexpected application head advancement"
            );
            ensure!(!status.projection_failed, "application projection failed");
            if status.height == height
                && status.indexed_height == height
                && !status.recovering
                && status.pending == pending
            {
                heads.push(PublishedHead {
                    height,
                    block_hash: status.block_hash,
                    state_root: status.state_root,
                    receipt_batch_commitment: status.receipt_batch_commitment,
                    state_version: status.state_version,
                });
                break;
            }
        }
    }
    ensure!(
        heads.len() == 4 && heads.iter().all(|head| head == &heads[0]),
        "four published application heads differ: {heads:?}"
    );
    Ok(heads)
}

fn publication_diagnostics(nodes: &[rpc::RpcLifecycle]) -> Vec<serde_json::Value> {
    nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let status = node.status();
            let mut evidence = serde_json::Map::new();
            evidence.insert("node".into(), serde_json::json!(index));
            // Fixed fields and four nodes only; no per-poll log or profile history.
            for field in [
                "head",
                "current_height",
                "round",
                "scheduled_proposer",
                "local_proposer",
                "recovery_in_progress",
                "pending",
                "rpc_indexed_height",
                "projection_error",
                "executed_batches",
                "durable_decisions",
                "execution_failures",
                "requested_bodies",
                "served_bodies",
                "native_pending_deliveries",
                "native_rejected_messages",
                "native_expired_sends",
                "gossip_repair",
                "gossip_verified_inputs",
                "gossip_rejected_inputs",
                "rpc_authentication",
            ] {
                evidence.insert(field.into(), status[field].clone());
            }
            serde_json::Value::Object(evidence)
        })
        .collect()
}

fn grant_views(
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
    selector: &ApplicationGrantSelectorV1,
    height: u64,
) -> Result<Vec<ApplicationGrantViewV1>> {
    let mut views = Vec::new();
    for client in clients {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            ensure!(
                Instant::now() < deadline,
                "published application query timeout"
            );
            match query(
                nodes,
                client,
                Query::UcaApplicationGrant {
                    selector: selector.clone(),
                },
            )? {
                Answer::UcaApplicationGrant(ApplicationGrantStateV1::Pending) => {}
                Answer::UcaApplicationGrant(ApplicationGrantStateV1::Present(view))
                    if view.height < height => {}
                Answer::UcaApplicationGrant(ApplicationGrantStateV1::Missing {
                    height: old,
                    ..
                }) if old < height => {}
                Answer::UcaApplicationGrant(ApplicationGrantStateV1::Present(view)) => {
                    ensure!(
                        view.height == height,
                        "application query did not use current published head"
                    );
                    views.push(view);
                    break;
                }
                other => anyhow::bail!("unexpected published application query {other:?}"),
            }
        }
    }
    ensure!(
        views.len() == 4 && views.iter().all(|view| view == &views[0]),
        "four-node application grant views differ"
    );
    Ok(views)
}

fn missing_grant(
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
    selector: &ApplicationGrantSelectorV1,
    head: &PublishedHead,
) -> Result<()> {
    for client in clients {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            ensure!(
                Instant::now() < deadline,
                "scoped application query timeout"
            );
            match query(
                nodes,
                client,
                Query::UcaApplicationGrant {
                    selector: selector.clone(),
                },
            )? {
                Answer::UcaApplicationGrant(ApplicationGrantStateV1::Pending) => {}
                Answer::UcaApplicationGrant(ApplicationGrantStateV1::Missing {
                    state_root,
                    height,
                }) => {
                    assert_eq!((state_root, height), (head.state_root, head.height));
                    break;
                }
                other => anyhow::bail!("application permission crossed a scope: {other:?}"),
            }
        }
    }
    Ok(())
}

fn snapshot(
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
    observations: &Observations,
    height: u64,
    pending: u64,
) -> Result<PublishedSnapshot> {
    let heads = heads(nodes, clients, height, pending)?;
    let grants = observations
        .selectors
        .iter()
        .map(|selector| grant_views(nodes, clients, selector, height))
        .collect::<Result<Vec<_>>>()?;
    let assets = account_views(nodes, clients, &observations.alice, &observations.bob)?;
    for views in &grants {
        ensure!(
            views
                .iter()
                .all(|view| view.state_root == heads[0].state_root),
            "application query root differs from published head"
        );
    }
    ensure!(
        assets.iter().all(|(alice, bob)| alice.height == height
            && bob.height == height
            && alice.state_root == heads[0].state_root
            && bob.state_root == heads[0].state_root),
        "V4 asset/application queries used different heads"
    );
    Ok(PublishedSnapshot {
        heads,
        grants,
        assets,
    })
}

fn check_grant(view: &ApplicationGrantViewV1, issuer: [u8; 32], revision: u64, revoked: bool) {
    assert_eq!(view.revision, revision);
    assert_eq!(view.revoked, revoked);
    assert_eq!(view.issuer_key_ref, issuer);
    assert_eq!(view.identity_key_ref, issuer);
    assert_eq!(view.identity_status, IdentityStatusV1::Active);
    assert_eq!((view.not_before_height, view.expires_at_height), (1, 100));
    assert!(view.not_before_height <= view.height && view.height < view.expires_at_height);
}

fn assets_unchanged(
    before: &[(AssetSpendStateViewV2, AssetSpendStateViewV2)],
    after: &[(AssetSpendStateViewV2, AssetSpendStateViewV2)],
) {
    let mut expected = before.to_vec();
    for ((alice, bob), (actual_alice, actual_bob)) in expected.iter_mut().zip(after) {
        for (old, actual) in [(&mut *alice, actual_alice), (&mut *bob, actual_bob)] {
            // This fixture's original asset grants activate at height 1.
            // The query exposes effective permission, not the stored raw flag.
            if old.height == 0 {
                assert!(!old.permits_nov_transfer);
                assert_eq!(old.not_before_height, 1);
                assert!(actual.height >= 1 && actual.height < old.expires_at_height);
                old.permits_nov_transfer = true;
            }
        }
        alice.height = actual_alice.height;
        alice.state_root = actual_alice.state_root;
        bob.height = actual_bob.height;
        bob.state_root = actual_bob.state_root;
    }
    assert_eq!(
        expected, after,
        "application command changed asset authority/nonce/balance/budget"
    );
}

fn queued_asset(
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
    signed: &SignedAssetSpendV3,
) -> Result<()> {
    let hash = Sha256::digest(signed.intent.signing_bytes()?).into();
    for (client_index, client) in clients.iter_mut().enumerate() {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            ensure!(
                Instant::now() < deadline,
                "future-nonce asset was not retained at client={client_index}, nodes={:?}",
                publication_diagnostics(nodes)
            );
            match query(nodes, client, Query::Receipt { hash })? {
                Answer::Receipt {
                    state: ReceiptState::Received,
                    ..
                } => break,
                Answer::Receipt {
                    state: ReceiptState::NotRecent,
                    ..
                } => {}
                other => anyhow::bail!(
                    "future-nonce asset unexpectedly left the pending pool: {other:?}"
                ),
            }
        }
    }
    Ok(())
}

fn reject_application_unchanged(
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
    ingress: usize,
    signed: &SignedApplicationGrantV1,
    observations: &Observations,
    expected: &PublishedSnapshot,
) -> Result<()> {
    let Answer::Status(before) = query(nodes, &mut clients[ingress], Query::Status)? else {
        anyhow::bail!("application rejection status unavailable");
    };
    send_application(nodes, &mut clients[ingress], signed)?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        ensure!(
            Instant::now() < deadline,
            "invalid application command not rejected: {:?}",
            signed.intent
        );
        if let Answer::Status(status) = query(nodes, &mut clients[ingress], Query::Status)? {
            ensure!(
                status.height == before.height && status.pending == 0,
                "invalid application command created state/work"
            );
            if status.rejected_inputs > before.rejected_inputs {
                break;
            }
        }
    }
    assert_eq!(
        &snapshot(nodes, clients, observations, before.height, 0)?,
        expected,
        "rejected application input changed a published head, grant or asset"
    );
    Ok(())
}

pub(super) fn run(
    runtime: &AoemExecFacade,
    configs: &[ResidentConfig],
    keys: &[(Vec<u8>, zeroize::Zeroizing<Vec<u8>>)],
    node_keys: &[config::test_custody::TestNodeKeys],
    mut nodes: Vec<rpc::RpcLifecycle>,
    mut clients: Vec<client::NativeApplicationClient>,
    ingress: usize,
    client_identity: novovm_network::product_overlay::TransportIdentity,
) -> Result<()> {
    ensure!(
        nodes.len() == 4 && clients.len() == 4,
        "four real services/clients required"
    );
    ensure!(
        nodes.iter().all(rpc::RpcLifecycle::supports_application),
        "application fixture did not select the V4 service template"
    );
    let validators = nodes[0].node.validators.clone();
    let chain_id = configs[0].genesis.chain_id;
    let domain = configs[0].genesis.genesis_config_commitment;
    let selector = ApplicationGrantSelectorV1 {
        chain_id,
        genesis: domain,
        identity: UcaIdentityIdV1::new(IDS[0]).map_err(anyhow::Error::msg)?,
        application: [7; 32],
        service: [8; 32],
        device: [9; 32],
    };
    let mut other = selector.clone();
    other.application = [17; 32];
    other.service = [18; 32];
    other.device = [19; 32];
    let intent = |payer: usize, recipient: usize, amount| -> Result<AssetSpendIntentV3> {
        Ok(AssetSpendIntentV3 {
            authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
            asset: 1,
            authority_update: None,
            recipient_creation: None,
            chain_id,
            genesis: domain,
            payer: ADDRESSES[payer].to_vec(),
            authorization_epoch: 1,
            spend_key_ref: spend_key_ref_v1(&keys[payer + 2].0)?,
            recipient: ADDRESSES[recipient].to_vec(),
            amount,
            max_fee: 50,
            nonce: 0,
            not_before_height: 1,
            expires_at_height: 100,
        })
    };
    let observations = Observations {
        selectors: [selector.clone(), other.clone()],
        alice: intent(0, 1, 100)?,
        bob: intent(1, 0, 1)?,
    };
    let initial_assets = account_views(
        &mut nodes,
        &mut clients,
        &observations.alice,
        &observations.bob,
    )?;
    let initial_heads = heads(&mut nodes, &mut clients, 0, 0)?;
    missing_grant(&mut nodes, &mut clients, &selector, &initial_heads[0])?;
    let issuer: [u8; 32] =
        novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1(
            novovm_adapter_api::UcaKeyAlgo::Mldsa65,
            &keys[4].0,
        )
        .try_into()
        .map_err(|_| anyhow::anyhow!("application issuer key reference must be 32 bytes"))?;
    let signer = runtime.application_grant_signer_v1()?;
    let authorize = |selector| ApplicationGrantIntentV1 {
        selector,
        issuer_key_ref: issuer,
        expected_revision: 0,
        valid_from_height: 1,
        valid_until_height: 100,
        action: ApplicationGrantActionV1::Authorize {
            not_before_height: 1,
            expires_at_height: 100,
        },
    };
    let grant = signer.sign(authorize(selector.clone()), &keys[4].0, &keys[4].1)?;
    send_application(&mut nodes, &mut clients[ingress], &grant)?;
    let first_heads = heads(&mut nodes, &mut clients, 1, 0)?;
    for view in grant_views(&mut nodes, &mut clients, &selector, 1)? {
        check_grant(&view, issuer, 1, false);
        assert_eq!(view.state_root, first_heads[0].state_root);
    }
    // Each individual scope dimension must select a different, absent grant.
    for dimension in 0..3 {
        let mut absent = selector.clone();
        match dimension {
            0 => absent.application = other.application,
            1 => absent.service = other.service,
            _ => absent.device = other.device,
        }
        missing_grant(&mut nodes, &mut clients, &absent, &first_heads[0])?;
    }
    let second_grant = signer.sign(authorize(other), &keys[4].0, &keys[4].1)?;
    send_application(&mut nodes, &mut clients[ingress], &second_grant)?;
    let active = snapshot(&mut nodes, &mut clients, &observations, 2, 0)?;
    for views in &active.grants {
        check_grant(&views[0], issuer, 1, false);
    }
    assets_unchanged(&initial_assets, &active.assets);

    let spend = runtime.asset_spend_signer_v3()?.sign(
        observations.alice.clone(),
        &keys[2].0,
        &keys[2].1,
    )?;
    let spend_receipt = publish(&mut nodes, &mut clients, ingress, &spend)?;
    let charged = fee(spend_receipt, 3, 1)?;
    let spent = snapshot(&mut nodes, &mut clients, &observations, 3, 0)?;
    assert_eq!(spent.assets[0].0.payer_balance, 900 - charged);
    assert_eq!(spent.assets[0].0.remaining_debit_budget, 900 - charged);
    assert_eq!(spent.assets[0].0.next_nonce, 1);
    assert_eq!(spent.assets[0].1.payer_balance, 100);
    assert_eq!(spent.assets[0].1.next_nonce, 0);
    assert_eq!(spent.assets[0].1.remaining_debit_budget, 1000);
    for views in &spent.grants {
        check_grant(&views[0], issuer, 1, false);
    }

    // Alice's next nonce is 1. Keep a real signed nonce-2 row ahead of the
    // valid application command in every original pool; never inject/reorder it.
    let mut future_intent = observations.alice.clone();
    future_intent.amount = 1;
    future_intent.nonce = 2;
    let future = runtime
        .asset_spend_signer_v3()?
        .sign(future_intent, &keys[2].0, &keys[2].1)?;
    send(&mut nodes, &mut clients[ingress], &future, 0, 1)?;
    queued_asset(&mut nodes, &mut clients, &future)?;
    assert_eq!(
        snapshot(&mut nodes, &mut clients, &observations, 3, 1)?,
        spent,
        "future-nonce input advanced the published head or changed state"
    );

    let mut revoke_intent = grant.intent.clone();
    revoke_intent.expected_revision = 1;
    revoke_intent.action = ApplicationGrantActionV1::Revoke;
    let revoke = signer.sign(revoke_intent, &keys[4].0, &keys[4].1)?;
    send_application(&mut nodes, &mut clients[ingress], &revoke)?;
    let revoked = snapshot(&mut nodes, &mut clients, &observations, 4, 1)?;
    check_grant(&revoked.grants[0][0], issuer, 2, true);
    check_grant(&revoked.grants[1][0], issuer, 1, false);
    assets_unchanged(&spent.assets, &revoked.assets);
    queued_asset(&mut nodes, &mut clients, &future)?;

    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    let mut nodes = start(configs, StartMode::Existing)?;
    let mut clients = configs
        .iter()
        .map(|config| connect(config, validators.clone(), client_identity.clone()))
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(
        snapshot(&mut nodes, &mut clients, &observations, 4, 0)?,
        revoked,
        "cold restart changed published V4 heads/grants/assets"
    );
    for client in &mut clients {
        assert_eq!(receipt(&mut nodes, client, &spend)?, spend_receipt);
    }

    // The recovered tombstone must reject the exact old grant, stale revision,
    // repeated revoke, inactive/expired command windows and another purpose key.
    let mut rejected = vec![grant.clone(), revoke.clone()];
    let mut repeated_revoke = revoke.intent.clone();
    repeated_revoke.expected_revision = 2;
    rejected.push(signer.sign(repeated_revoke, &keys[4].0, &keys[4].1)?);
    let mut inactive = authorize(selector.clone());
    inactive.expected_revision = 2;
    inactive.valid_from_height = 50;
    inactive.action = ApplicationGrantActionV1::Authorize {
        not_before_height: 50,
        expires_at_height: 100,
    };
    rejected.push(signer.sign(inactive, &keys[4].0, &keys[4].1)?);
    let mut expired = authorize(selector.clone());
    expired.expected_revision = 2;
    expired.valid_until_height = 4;
    rejected.push(signer.sign(expired, &keys[4].0, &keys[4].1)?);
    let mut foreign = authorize(selector.clone());
    foreign.expected_revision = 2;
    foreign.issuer_key_ref =
        novovm_adapter_api::uca_key_binding::derive_primary_key_ref_from_binding_v1(
            novovm_adapter_api::UcaKeyAlgo::Mldsa65,
            &keys[0].0,
        )
        .try_into()
        .map_err(|_| {
            anyhow::anyhow!("foreign application issuer key reference must be 32 bytes")
        })?;
    rejected.push(signer.sign(foreign, &keys[0].0, &keys[0].1)?);
    let mut root_device = selector;
    root_device.device = issuer;
    rejected.push(signer.sign(authorize(root_device), &keys[4].0, &keys[4].1)?);
    let mut bad_signature = grant;
    bad_signature.signature[0] ^= 1;
    rejected.push(bad_signature);
    for command in rejected {
        reject_application_unchanged(
            &mut nodes,
            &mut clients,
            ingress,
            &command,
            &observations,
            &revoked,
        )?;
    }

    // Reading/refusing inputs alone cannot prove that the reopened original
    // journal, projection and signer can publish another valid candidate.
    let mut reauthorize = authorize(observations.selectors[0].clone());
    reauthorize.expected_revision = 2;
    let reauthorized = signer.sign(reauthorize, &keys[4].0, &keys[4].1)?;
    send_application(&mut nodes, &mut clients[ingress], &reauthorized)?;
    let resumed = snapshot(&mut nodes, &mut clients, &observations, 5, 0)?;
    check_grant(&resumed.grants[0][0], issuer, 3, false);
    check_grant(&resumed.grants[1][0], issuer, 1, false);
    assets_unchanged(&revoked.assets, &resumed.assets);
    assert_ne!(resumed.heads[0].block_hash, revoked.heads[0].block_hash);
    assert_ne!(resumed.heads[0].state_root, revoked.heads[0].state_root);
    drop(clients);
    activation::run(
        runtime,
        configs,
        node_keys,
        nodes,
        client_identity,
        &resumed.heads,
    )?;
    println!("V4 application: four real services/DBs, native UDP grants, asset spend, future-nonce non-starvation, scoped revocation, cold recovery, unchanged-head refusals and post-restart reauthorization; no deployment or TPS claim");
    Ok(())
}
