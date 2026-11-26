//! An isolated bootstrap introduces a spontaneously published read service to a cold Client.
use super::*;
use crate::native_pipeline::service::{
    client::operator::ClientConfig,
    network::{Config, Driver},
};
use novovm_network::{
    iroh_transport::IrohAddressV1, product_directory::PeerSignedRelayRecordV1,
    product_service_lease::IrohServiceEndpointV1,
};
use novovm_relay::transport_identity::ephemeral_transport_identity;
use sha2::{Digest, Sha256};
use std::{fs, net::UdpSocket, process::Stdio};

#[test]
#[ignore = "detached original archive, approved relay configuration and Windows Client"]
fn cold_client_uses_published_provider_without_a_provider_pin() -> Result<()> {
    let binary = input("NOVOVM_QUERY_COPY_PROOF_CLIENT")?;
    let request = input("NOVOVM_QUERY_COPY_REQUEST")?;
    let stage = request.parent().context("isolated acceptance directory")?;
    let hash = super::super::hex(&receipt_request_hash(&request)?);
    let original: ClientConfig =
        serde_json::from_slice(&fs::read(input("NOVOVM_QUERY_COPY_FIRST_PROFILE")?)?)?;
    let approved = Config::load(&input("NOVOVM_QUERY_COPY_ROAMING_RELAY_PROFILE")?)?;
    ensure!(
        !approved.relays.is_empty(),
        "an approved public relay is required"
    );
    let mut owner = open_snapshot(true)?;
    owner.rpc.node.public_state_sync = true;
    let head = owner.rpc.node.controller.parent();
    let (context, parent) = super::super::super::native_genesis_anchor(
        &owner.rpc.node.template,
        owner.config.genesis.validator_epoch,
        &owner.rpc.node.validators,
    );
    let make_identity =
        || ephemeral_transport_identity(&original.library, &original.library_sha256);
    let (intro, _) = make_identity()?;
    let (provider, _) = make_identity()?;
    let (offline, _) = make_identity()?;
    let source_key = intro.public_key().to_vec();
    let provider_key = provider.public_key().to_vec();
    ensure!(
        source_key != provider_key
            && offline.public_key() != provider_key
            && owner
                .config
                .genesis
                .validators
                .iter()
                .all(|v| v.public_key != provider_key && v.transport_public_key != provider_key),
        "read service overlaps a preinstalled key"
    );
    let source = format!("{:x}", Sha256::digest(&source_key));
    let serving = format!("{:x}", Sha256::digest(&provider_key));
    let bootstrap_file = stage.join("bootstrap-local.json");
    let provider_file = stage.join("provider-local.json");
    let profile = stage.join("installed-referral.json");
    let device = stage.join("referral-device");
    ensure!(
        [&bootstrap_file, &provider_file, &profile, &device]
            .iter()
            .all(|path| !path.exists()),
        "stale referral fixture"
    );
    let basic = Config {
        probe_control_file: None,
        relays: Vec::new(),
        nameserver: "127.0.0.1".parse()?,
        bind_address: "127.0.0.1:0".parse()?,
        public_record_file: bootstrap_file.clone(),
        carrier_identity_file: None,
        peer_records: Vec::new(),
        discovery: true,
        discovery_seeds: Vec::new(),
        publish_native_query_address: false,
        publish_public_read_provider: false,
        relay_forwarding: false,
        fallback_after_ms: 1000,
        retry_ms: 500,
        udp_recovery_ms: 2000,
    };
    let mut bootstrap =
        Driver::detached_referral_service(basic.clone(), &owner.rpc.node, intro, &bootstrap_file)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while !bootstrap_file.exists() {
        ensure!(Instant::now() < deadline, "bootstrap did not start");
        bootstrap.poll_detached_bootstrap()?;
        thread::sleep(Duration::from_millis(2));
    }
    let record: PeerSignedRelayRecordV1 = serde_json::from_slice(&fs::read(&bootstrap_file)?)?;
    let contact = IrohServiceEndpointV1::from_record(&record)?.address;
    ensure!(
        record.relay_public_key == source_key
            && !contact.direct.is_empty()
            && contact.direct.iter().all(|addr| addr.ip().is_loopback())
            && contact.relays.is_empty(),
        "bootstrap not restricted to loopback"
    );
    owner.novonet = Some(Driver::detached_referral_service(
        Config {
            relays: approved.relays.clone(),
            nameserver: approved.nameserver,
            public_record_file: provider_file.clone(),
            discovery_seeds: vec![bootstrap_file.clone()],
            publish_public_read_provider: true,
            ..basic
        },
        &owner.rpc.node,
        provider,
        &bootstrap_file,
    )?);
    let sink = UdpSocket::bind("127.0.0.1:0")?;
    let offline_contact = IrohAddressV1 {
        endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[67; 32])
            .verifying_key()
            .to_bytes(),
        direct: vec![sink.local_addr()?],
        relays: Vec::new(),
    };
    let bytes = serde_json::to_vec(&serde_json::json!({
        "chain_id":context.chain_id,"genesis":context.genesis_config_commitment,"protocol":context.protocol_commitment,
        "library":original.library,"library_sha256":original.library_sha256,
        "nameserver":approved.nameserver,"relays":approved.relays,
        "seeds":[{"transport_key":source_key,"contact":contact},{"transport_key":offline.public_key(),"contact":offline_contact}],
        "lookup_transport_keys":[], "native_query_peers":[],
        "verification":{"validator_epoch":context.epoch,"validator_set_hash":context.validator_set_hash,
            "validators":owner.config.genesis.validators,"trusted_parent":parent}
    }))?;
    let pin = format!("{:x}", Sha256::digest(&bytes));
    fs::write(&profile, bytes)?;
    let command = |action: &str| {
        let mut command = Command::new(&binary);
        command
            .arg(&profile)
            .arg(action)
            .env("LOCALAPPDATA", &device)
            .env("XDG_STATE_HOME", &device);
        command
    };
    let mut poll = || -> Result<()> {
        bootstrap.poll_detached_bootstrap()?;
        owner.poll()?;
        thread::sleep(Duration::from_millis(2));
        Ok(())
    };
    let state_dir = device
        .join("NOVOVM")
        .join("native-discovery")
        .join(format!(
            "{}-{}-{}",
            context.chain_id,
            super::super::hex(&context.genesis_config_commitment),
            super::super::hex(&context.protocol_commitment)
        ))
        .join(format!(
            "headers-{}-{:x}",
            parent.height,
            Sha256::digest(serde_json::to_vec(&parent)?)
        ))
        .join("state-nodes");
    let node_count = || -> Result<u64> {
        let files = match fs::read_dir(&state_dir) {
            Ok(files) => files,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let mut count = 0;
        for file in files {
            let file = file?;
            if file.file_type()?.is_file()
                && file.path().extension().is_some_and(|ext| ext == "node")
            {
                count += 1;
            }
        }
        Ok(count)
    };
    ensure!(node_count()? == 0, "cold Client already had state nodes");
    let log = stage.join("referral-follow.jsonl");
    let mut follower = IsolatedFollowChild(
        command("follow-state")
            .arg(&pin)
            .stdout(Stdio::from(fs::File::create_new(&log)?))
            .stderr(Stdio::from(fs::File::create_new(
                stage.join("referral-follow.stderr"),
            )?))
            .spawn()?,
    );
    // A cold transfer through a distant relay may span several signed leases.
    let started = Instant::now();
    let deadline = started + Duration::from_secs(900);
    let mut last_progress = started;
    let mut next_measurement = started;
    let mut downloaded = 0;
    let state = loop {
        poll()?;
        if let Some(status) = follower.0.try_wait()? {
            anyhow::bail!("Client exited before syncing: {status}");
        }
        let output = fs::read_to_string(&log)?;
        let events = output
            .split_inclusive('\n')
            .filter(|line| line.ends_with('\n'))
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if let Some(state) = events.iter().find(|e| e["event"] == "verified_state") {
            ensure!(
                events
                    .first()
                    .is_some_and(|e| e["event"] == "starting" && e["reopened_height"] == 0)
                    && events
                        .iter()
                        .any(|e| e["event"] == "verified_locator_accepted"
                            && e["peer_transport_sha256"] == serving
                            && e["locator_sequence"] == state["locator_sequence"]
                            && e["locator_returned_by"] == serde_json::json!([source]))
                    && state["peer_transport_sha256"] == serving
                    && state["transport"] == "iroh"
                    && state["height"] == head.height
                    && state["block_hash"] == super::super::hex(&head.block_hash)
                    && state["state_root"] == super::super::hex(&head.state_root)
                    && state["verified_nodes"].as_u64().is_some_and(|n| n > 0)
                    && state["downloaded_nodes"] == state["verified_nodes"]
                    && state["verified_nodes"].as_u64() == Some(node_count()?)
                    && state_dir
                        .join(format!("{}.node", super::super::hex(&head.state_root)))
                        .is_file(),
                "wrong provider or unverified cold download: {state}"
            );
            break state.clone();
        }
        let now = Instant::now();
        if now >= next_measurement {
            let count = node_count()?;
            if count > downloaded {
                downloaded = count;
                last_progress = now;
            }
            next_measurement = now + Duration::from_secs(5);
        }
        ensure!(
            now < deadline,
            "cold state deadline with {downloaded} persisted nodes"
        );
        ensure!(
            now.duration_since(last_progress) < Duration::from_secs(180),
            "no new persisted state nodes for 180 seconds, {downloaded} present"
        );
    };
    drop(follower);
    for action in ["discover", "discover-verified-receipt"] {
        let file = stage.join(format!("referral-{action}.json"));
        let mut client = command(action);
        if action == "discover-verified-receipt" {
            client.arg(&hash);
        }
        let mut child = IsolatedFollowChild(
            client
                .arg(&pin)
                .stdout(Stdio::from(fs::File::create_new(&file)?))
                .stderr(Stdio::from(fs::File::create_new(
                    stage.join(format!("referral-{action}.stderr")),
                )?))
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            poll()?;
            if let Some(done) = child.0.try_wait()? {
                ensure!(done.success(), "{action} failed");
                break;
            }
            ensure!(Instant::now() < deadline, "{action} deadline");
        }
        let report: Value = serde_json::from_slice(&fs::read(file)?)?;
        ensure!(
            report["authenticated_discovery_sources"] == serde_json::json!([source])
                && report["transport_identity_sha256"] == state["transport_identity_sha256"],
            "wrong bootstrap or changed Client custody"
        );
        if action == "discover" {
            let rows = report["signed_locators_by_source"][&source]
                .as_array()
                .context("attributed locators")?;
            ensure!(
                rows.len() == 1
                    && rows[0]["purpose"] == "public_read"
                    && rows[0]["transport_identity_sha256"] == serving
                    && report["signed_locator_candidates"].as_array() == Some(rows),
                "provider was not introduced by the discovery-only bootstrap"
            );
        } else {
            ensure!(
                report["serving_transport_sha256"] == serving
                    && report["independently_verified_finality"] == true
                    && report["success"] == true
                    && report["tx_hash"] == hash
                    && report["height"] == 1
                    && report["block_hash"]
                        == "0x9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926",
                "original authenticated transaction proof differs"
            );
        }
    }
    ensure!(
        owner.rpc.node.controller.parent() == head && owner.active_client().is_err(),
        "archive or query admission changed"
    );
    let ordinary: PeerSignedRelayRecordV1 = serde_json::from_slice(&fs::read(provider_file)?)?;
    let ordinary_route = IrohServiceEndpointV1::from_record(&ordinary)?;
    ensure!(
        ordinary.relay_public_key == provider_key
            && ordinary_route.address.direct.is_empty()
            && !ordinary_route.address.relays.is_empty()
            && ordinary_route.address.relays.iter().all(|relay| {
                novovm_network::iroh_transport::approved_read_relay(&approved.relays, relay)
            }),
        "ordinary lookup would disclose additional coordinates for the public provider"
    );
    bootstrap.close()?;
    owner.novonet.as_mut().unwrap().close()?;
    owner.rpc.node.shutdown()
}
