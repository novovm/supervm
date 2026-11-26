//! Opt-in existing-store regression. Only the detached node's query and cold-read owners are polled.
use super::{ResidentConfig, ResidentNode, RpcLifecycle, StartMode};
use crate::native_pipeline::consensus::{statement::ReceiptFinalityProof, wire, ArchiveRead};
use anyhow::{ensure, Context, Result};
use novovm_protocol::native_query::{self, Frame, Query};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct QueryOwner {
    rpc: RpcLifecycle,
    config: ResidentConfig,
    novonet: Option<super::super::network::Driver>,
}

impl QueryOwner {
    fn poll(&mut self) -> Result<()> {
        let now = Instant::now();
        if let Some(novonet) = &mut self.novonet {
            novonet.step_product(&mut self.rpc, now)?;
        }
        let wall_ms = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        let node = &mut self.rpc.node;
        if node.controller.progress.is_recovering() {
            node.controller.progress.poll_recovery(
                &node.pipeline,
                &self.config.genesis.policy,
                super::super::native_decode_limits(
                    self.config.batch_size,
                    self.config.uca_initialization.is_some(),
                ),
            )?;
            node.controller
                .progress
                .poll_acknowledgements(&node.pipeline)?;
        }
        node.controller.poll_io(wall_ms, now)?;
        self.rpc.poll_projection()?;
        self.rpc.poll_native_query(now)
    }

    fn until(&mut self, mut ready: impl FnMut(&Self) -> Result<bool>) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            self.poll()?;
            if ready(self)? {
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "query snapshot regression deadline"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn active_client(&self) -> Result<(String, u64)> {
        let diagnostics = self
            .rpc
            .node
            .controller
            .consensus_diagnostic(Instant::now());
        let peers = diagnostics["configured_peers"]
            .as_array()
            .context("peer diagnostics")?;
        let mut clients = peers.iter().filter(|row| {
            row["validator"].is_null() && row["session_established"].as_bool() == Some(true)
        });
        let row = clients.next().context("no established query client")?;
        ensure!(
            clients.next().is_none(),
            "more than one dynamic query client"
        );
        Ok((
            row["current_route"]["endpoint"]
                .as_str()
                .context("query client endpoint")?
                .to_owned(),
            row["last_authenticated_age_us"]
                .as_u64()
                .context("last authenticated age")?,
        ))
    }

    fn query(&mut self, binary: &Path, profile: PathBuf, request: &Path) -> Result<Value> {
        let profile_json: Value = serde_json::from_slice(&std::fs::read(&profile)?)?;
        let server = self.config.udp_addr.to_string();
        ensure!(
            profile_json["node"]["address"].as_str() == Some(server.as_str())
                && profile_json["bind_addr"]
                    .as_str()
                    .is_some_and(|bind| bind.starts_with("127.0.0.1:"))
                && profile_json["timeout_ms"]
                    .as_u64()
                    .is_some_and(|timeout| (1..=30_000).contains(&timeout)),
            "client must use an isolated loopback route"
        );
        let mut command = Command::new(binary);
        command.arg(profile).arg("query").arg(request);
        let worker = thread::spawn(move || command.output());
        self.until(|_| Ok(worker.is_finished()))?;
        let output = worker
            .join()
            .map_err(|_| anyhow::anyhow!("query worker panicked"))??;
        ensure!(
            output.status.success(),
            "query failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let result = &report["result"];
        ensure!(
            result["kind"] == "node_observation",
            "missing authenticated response"
        );
        let receipt = &result["answer"]["Receipt"];
        ensure!(
            receipt["state"]["Finalized"]["success"] == true,
            "original receipt is not finalized successfully"
        );
        Ok(receipt.clone())
    }
}

fn input(name: &str) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(
        std::env::var_os(name).with_context(|| format!("missing {name}"))?,
    )?)
}

#[test]
#[ignore = "requires a detached original RocksDB copy and two purpose-bound query profiles"]
fn original_snapshot_reclaims_idle_slot_for_another_authenticated_receipt() -> Result<()> {
    let mut owner = open_snapshot(false)?;
    let binary = input("NOVOVM_QUERY_COPY_CLIENT")?;
    let request = input("NOVOVM_QUERY_COPY_REQUEST")?;
    let first = input("NOVOVM_QUERY_COPY_FIRST_PROFILE")?;
    let second = input("NOVOVM_QUERY_COPY_SECOND_PROFILE")?;
    let head = owner.rpc.node.controller.parent();
    let position = (
        owner.rpc.node.controller.progress.context(),
        owner.rpc.node.controller.progress.round(),
    );
    let original = owner.query(&binary, first, &request)?;
    let (old_route, _) = owner.active_client()?;
    owner
        .until(|owner| Ok(owner.active_client()?.1 >= 30_000_000))
        .context("wait for the previous authenticated client to become idle")?;
    let (retained_route, idle_us) = owner.active_client()?;
    ensure!(
        retained_route == old_route,
        "old slot was not retained until authenticated demand"
    );
    let result = owner.query(&binary, second, &request)?;
    let (new_route, _) = owner.active_client()?;
    ensure!(
        old_route != new_route && original == result,
        "slot replacement or receipt mismatch"
    );
    ensure!(
        owner.rpc.node.controller.parent() == head,
        "detached query recovery changed the chain head"
    );
    ensure!(
        position
            == (
                owner.rpc.node.controller.progress.context(),
                owner.rpc.node.controller.progress.round(),
            ),
        "detached queries advanced consensus"
    );
    println!("idle_us={idle_us} old={old_route} new={new_route} receipt={result}");
    verify_archived_receipt(&mut owner, &request, &result)?;
    owner.rpc.node.shutdown()
}

fn open_snapshot(proofs_enabled: bool) -> Result<QueryOwner> {
    let mut config = ResidentConfig::load(&input("NOVOVM_QUERY_COPY_CONFIG")?)?;
    ensure!(
        config.database.is_dir()
            && config.database.file_name().and_then(|name| name.to_str())
                == Some("query-reclaim-copy.rocksdb")
            && config.database.join("NOVOVM-CANDIDATE-DOMAIN").is_file()
            && config.udp_addr.ip().is_loopback()
            && config
                .peers
                .iter()
                .all(|peer| peer.address.ip().is_loopback())
            && config.novonet_config_file.is_none()
            && config.rpc_addr.is_none()
            && config.native_client_limit == 1,
        "requires an isolated copied store, loopback-only routes and one query slot"
    );
    config.database = std::fs::canonicalize(&config.database)?;
    ensure!(
        config.database.file_name().and_then(|name| name.to_str())
            == Some("query-reclaim-copy.rocksdb"),
        "resolved database is not the detached copy"
    );
    config.public_asset_receipt_proofs = proofs_enabled;
    config.public_state_sync = false;
    let node = ResidentNode::start(config.clone(), StartMode::Existing)?;
    let mut owner = QueryOwner {
        rpc: RpcLifecycle::new(node, config.batch_size)?,
        config,
        novonet: None,
    };
    owner
        .until(|owner| {
            let core = &owner.rpc.node.controller;
            Ok(!core.is_recovering()
                && owner.rpc.indexed_height > 0
                && owner.rpc.indexed_height == core.parent().height)
        })
        .context("wait for the copied chain's verified receipt projection")?;
    Ok(owner)
}

#[test]
#[ignore = "requires a detached original RocksDB copy and the proof-capable native Client"]
fn detached_archive_serves_original_receipt_to_independent_client_verifier() -> Result<()> {
    use std::io::Write;
    let binary = input("NOVOVM_QUERY_COPY_PROOF_CLIENT")?;
    let request = input("NOVOVM_QUERY_COPY_REQUEST")?;
    let profile = input("NOVOVM_QUERY_COPY_FIRST_PROFILE")?;
    let hash = receipt_request_hash(&request)?;
    let mut owner = open_snapshot(true)?;
    let head = owner.rpc.node.controller.parent();
    let (context, parent) = super::super::native_genesis_anchor(
        &owner.rpc.node.template,
        owner.config.genesis.validator_epoch,
        &owner.rpc.node.validators,
    );
    let pin = request.with_extension("trusted-parent.json");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pin)?
        .write_all(&serde_json::to_vec(&parent)?)?;
    let expected = super::hex(&hash);
    let mut command = Command::new(binary);
    command
        .arg(profile)
        .arg("verified-receipt")
        .arg(pin)
        .arg(&expected);
    let worker = thread::spawn(move || command.output());
    owner.until(|_| Ok(worker.is_finished()))?;
    let output = worker
        .join()
        .map_err(|_| anyhow::anyhow!("proof query worker panicked"))??;
    ensure!(
        output.status.success(),
        "proof Client failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)?;
    ensure!(
        report["independently_verified_finality"] == true
            && report["success"] == true
            && report["tx_hash"] == expected
            && report["nonce_after"] == 1
            && report["charged_fee"] == "4000000"
            && report["height"] == context.height
            && report["block_hash"]
                == "0x9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926"
            && owner.rpc.node.controller.parent() == head,
        "proof Client differs from the original finalized receipt: {report}"
    );
    println!("independent proof Client: {report}");
    owner.rpc.node.shutdown()
}

fn receipt_request_hash(request: &Path) -> Result<[u8; 32]> {
    let Frame::Request(request) =
        native_query::decode(&std::fs::read(request)?).map_err(anyhow::Error::msg)?
    else {
        anyhow::bail!("expected a receipt request")
    };
    let Query::Receipt { hash } = request.query else {
        anyhow::bail!("expected a receipt selector")
    };
    Ok(hash)
}

#[test]
#[ignore = "requires detached original RocksDB and a discovery-capable Windows Client"]
fn clean_device_discovers_and_verifies_original_receipt_over_same_carrier() -> Result<()> {
    use super::super::{
        client::operator::ClientConfig,
        network::{Config, Driver},
    };
    use novovm_network::{
        discovery_control::native_locator_id_v1,
        iroh_transport::IrohAddressV1,
        product_directory::{sign_relay_record_v1, PeerSignedRelayRecordV1},
        product_service_lease::IrohServiceEndpointV1,
    };
    use sha2::{Digest, Sha256};
    use std::{fs, net::UdpSocket};

    let binary = input("NOVOVM_QUERY_COPY_PROOF_CLIENT")?;
    let request = input("NOVOVM_QUERY_COPY_REQUEST")?;
    let stage = request.parent().context("isolated working directory")?;
    let lan = if std::env::var_os("NOVOVM_QUERY_COPY_LAN").is_some() {
        Some(LanAcceptance::load(&input("NOVOVM_QUERY_COPY_LAN")?)?)
    } else {
        None
    };
    let lan_install =
        lan.is_some() && std::env::var("NOVOVM_QUERY_COPY_WINDOWS_INSTALL").as_deref() == Ok("1");
    let roaming_relay = std::env::var_os("NOVOVM_QUERY_COPY_ROAMING_RELAY_PROFILE")
        .map(|path| Config::load(Path::new(&path)))
        .transpose()?;
    let relay_preflight = std::env::var("NOVOVM_QUERY_COPY_RELAY_PREFLIGHT").as_deref() == Ok("1");
    ensure!(
        !relay_preflight || roaming_relay.is_some(),
        "relay preflight requires an approved roaming relay configuration"
    );
    let (relays, nameserver) = if let Some(network) = &roaming_relay {
        ensure!(
            lan.as_ref().is_some_and(|lan| !lan.receipt_only)
                && !lan_install
                && std::env::var_os("NOVOVM_QUERY_COPY_LAN_DISCOVERY").is_none()
                && !network.relays.is_empty(),
            "roaming relay requires the restricted cross-device state-following harness"
        );
        (network.relays.clone(), network.nameserver)
    } else {
        (Vec::new(), "127.0.0.1".parse()?)
    };
    if lan_install {
        let settings = lan.as_ref().unwrap();
        let peer =
            std::env::var("NOVOVM_QUERY_COPY_ISOLATED_IROH_PEER")?.parse::<std::net::Ipv4Addr>()?;
        ensure!(
            peer == settings.client && !settings.receipt_only,
            "cold Windows LAN fixture requires a scoped direct Iroh peer"
        );
    }
    let hash = super::hex(&receipt_request_hash(&request)?);
    let old_profile: ClientConfig =
        serde_json::from_slice(&fs::read(input("NOVOVM_QUERY_COPY_FIRST_PROFILE")?)?)?;
    let (unreachable, _) = novovm_relay::transport_identity::ephemeral_transport_identity(
        &old_profile.library,
        &old_profile.library_sha256,
    )?;
    let mut owner = open_snapshot(!lan_install && roaming_relay.is_none())?;
    let head = owner.rpc.node.controller.parent();
    let (context, parent) = super::super::native_genesis_anchor(
        &owner.rpc.node.template,
        owner.config.genesis.validator_epoch,
        &owner.rpc.node.validators,
    );
    let pins = (
        context.chain_id,
        context.genesis_config_commitment,
        context.protocol_commitment,
    );
    let primary = stage.join("offline-primary.json");
    let secondary = stage.join("local-secondary.json");
    let sink = UdpSocket::bind("127.0.0.1:0")?;
    let millis = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    let first = sign_relay_record_v1(
        &unreachable,
        native_locator_id_v1(pins),
        vec![IrohServiceEndpointV1 {
            instance: [31; 32],
            address: IrohAddressV1 {
                endpoint_key: ed25519_dalek::SigningKey::from_bytes(&[69; 32])
                    .verifying_key()
                    .to_bytes(),
                direct: vec![sink.local_addr()?],
                relays: Vec::new(),
            },
        }
        .endpoint()?],
        millis,
        millis + 120_000,
        1,
    )?;
    fs::write(&primary, serde_json::to_vec(&first)?)?;

    let bind_address = lan
        .as_ref()
        .map_or(std::net::Ipv4Addr::LOCALHOST, |lan| lan.server);
    owner.novonet = Some(Driver::new_with_query_peers(
        Config {
            probe_control_file: None,
            relays: relays.clone(),
            nameserver,
            bind_address: (bind_address, 0).into(),
            public_record_file: secondary.clone(),
            carrier_identity_file: None,
            peer_records: Vec::new(),
            discovery: true,
            discovery_seeds: Vec::new(),
            publish_native_query_address: lan.is_none()
                && std::env::var("NOVOVM_QUERY_COPY_FOLLOW").as_deref() == Ok("2"),
            relay_forwarding: false,
            fallback_after_ms: 1000,
            retry_ms: 500,
            udp_recovery_ms: 2000,
        },
        &mut owner.rpc.node,
        lan.as_ref()
            .filter(|_| !lan_install)
            .map(|lan| vec![lan.client_key.clone()].into()),
    )?);
    owner.until(|_| {
        if !secondary.is_file() {
            return Ok(false);
        }
        if roaming_relay.is_none() {
            return Ok(true);
        }
        let record: PeerSignedRelayRecordV1 = serde_json::from_slice(&fs::read(&secondary)?)?;
        Ok(!IrohServiceEndpointV1::from_record(&record)?
            .address
            .relays
            .is_empty())
    })?;
    let locator: PeerSignedRelayRecordV1 = serde_json::from_slice(&fs::read(&secondary)?)?;
    let endpoint = IrohServiceEndpointV1::from_record(&locator)?.address;
    ensure!(
        endpoint
            .direct
            .iter()
            .any(|entry| entry.ip() == bind_address)
            && (roaming_relay.is_some()
                || endpoint
                    .direct
                    .iter()
                    .all(|entry| entry.ip() == bind_address))
            && endpoint.relays.iter().all(|relay| relays.contains(relay)),
        "detached proof carrier route differs from its configured interface or relays"
    );
    let key = owner.rpc.node.transport_identity().public_key().to_vec();
    ensure!(locator.relay_public_key == key, "locator identity differs");
    if lan.is_none() && std::env::var("NOVOVM_QUERY_COPY_CARRIER_TAKEOVER").as_deref() == Ok("1") {
        use crate::native_pipeline::consensus::native::MessageDomain;
        use crate::native_pipeline::service::client::operator::receipt_proof::ProofTarget;
        let (another, _) = novovm_relay::transport_identity::ephemeral_transport_identity(
            &old_profile.library,
            &old_profile.library_sha256,
        )?;
        let domain = MessageDomain {
            chain_id: pins.0,
            genesis: pins.1,
            protocol: pins.2,
            validators: owner.rpc.node.validators.clone(),
        };
        let receipt = ProofTarget::new(
            &domain,
            unreachable.clone(),
            parent,
            receipt_request_hash(&request)?,
        )?;
        let worker = thread::spawn(move || {
            super::super::network::test_same_identity_reconnect(
                locator,
                domain,
                unreachable,
                another,
                receipt,
            )
        });
        owner.until(|_| Ok(worker.is_finished()))?;
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("carrier reconnect fixture panicked"))??;
        ensure!(
            owner.rpc.node.controller.parent() == head && owner.active_client().is_err(),
            "carrier reconnect affected the detached chain or legacy UDP query slots"
        );
        owner.novonet.as_mut().unwrap().close()?;
        return owner.rpc.node.shutdown();
    }
    let seeds = if lan.is_some()
        || std::env::var("NOVOVM_QUERY_COPY_BOOTSTRAP_CONTACTS").as_deref() == Ok("1")
    {
        let first_contact = IrohServiceEndpointV1::from_record(&first)?.address;
        let mut contact = endpoint;
        if roaming_relay.is_some() {
            contact.direct.clear();
        }
        println!("cold Client uses two pinned contacts without installed locator files");
        serde_json::json!([
            { "contact": first_contact, "transport_key": first.relay_public_key },
            { "contact": contact, "transport_key": key },
        ])
    } else {
        serde_json::json!([
            { "locator": primary, "transport_key": first.relay_public_key },
            { "locator": secondary, "transport_key": key },
        ])
    };
    let profile = stage.join("installed-discovery.json");
    fs::write(
        &profile,
        serde_json::to_vec(&serde_json::json!({
            "chain_id": pins.0, "genesis": pins.1, "protocol": pins.2,
            "library": old_profile.library, "library_sha256": old_profile.library_sha256,
            "nameserver": nameserver, "relays": relays,
            "seeds": seeds,
            "lookup_transport_keys": [key],
            "verification": {
                "validator_epoch": context.epoch,
                "validator_set_hash": context.validator_set_hash,
                "validators": owner.config.genesis.validators,
                "trusted_parent": parent,
            },
        }))?,
    )?;
    if let Some(lan) = lan {
        if lan_install {
            owner.rpc.node.public_state_sync = true;
            lan.serve_installed_state(&mut owner, &profile, &key)?;
        } else if std::env::var("NOVOVM_QUERY_COPY_LAN_DISCOVERY").as_deref() == Ok("1") {
            owner.rpc.node.public_asset_receipt_proofs = false;
            lan.serve_discovery(&mut owner, &profile, &key)?;
        } else if lan.receipt_only {
            lan.serve_receipt(&mut owner, &profile, &key, &hash)?;
        } else {
            owner.rpc.node.public_asset_receipt_proofs = false;
            owner.rpc.node.public_state_sync = true;
            if relay_preflight {
                lan.serve_relay_preflight(&mut owner, &profile, &key)?;
            } else {
                lan.serve(&mut owner, &profile, &key, roaming_relay.is_some())?;
            }
        }
        ensure!(
            owner.rpc.node.controller.parent() == head,
            "LAN acceptance changed the detached chain"
        );
        owner.novonet.as_mut().unwrap().close()?;
        return owner.rpc.node.shutdown();
    }
    if std::env::var("NOVOVM_QUERY_COPY_WINDOWS_INSTALL").as_deref() == Ok("1") {
        owner.rpc.node.public_asset_receipt_proofs = false;
        owner.rpc.node.public_state_sync = true;
        serve_installed_windows_fixture(&mut owner, &profile, &primary, &secondary)?;
        ensure!(
            owner.rpc.node.controller.parent() == head && owner.active_client().is_err(),
            "Windows installer acceptance changed the detached chain or consumed its UDP client slot"
        );
        owner.novonet.as_mut().unwrap().close()?;
        return owner.rpc.node.shutdown();
    }
    if std::env::var("NOVOVM_QUERY_COPY_UCA_REPLAY").as_deref() == Ok("1") {
        owner.rpc.node.public_asset_receipt_proofs = false;
        test_installed_uca_replay(&mut owner, &binary, &profile, parent.height)?;
        ensure!(
            owner.rpc.node.controller.parent() == head,
            "UCA replay altered the detached chain"
        );
        owner.novonet.as_mut().unwrap().close()?;
        return owner.rpc.node.shutdown();
    }
    if std::env::var("NOVOVM_QUERY_COPY_CHECKPOINT_POLICY").as_deref() == Ok("1") {
        owner.rpc.node.public_asset_receipt_proofs = false;
        owner.rpc.node.public_state_sync = true;
        test_installed_checkpoint_policy(&mut owner, &binary, &profile, &key)?;
        ensure!(
            owner.rpc.node.controller.parent() == head,
            "checkpoint testing altered the detached chain"
        );
        owner.novonet.as_mut().unwrap().close()?;
        return owner.rpc.node.shutdown();
    }
    let follow_test = std::env::var("NOVOVM_QUERY_COPY_FOLLOW");
    if matches!(follow_test.as_deref(), Ok("1" | "2")) {
        owner.rpc.node.public_asset_receipt_proofs = false;
        owner.rpc.node.public_state_sync = true;
        test_installed_follow(
            &mut owner,
            &binary,
            &profile,
            &secondary,
            &key,
            follow_test.as_deref() == Ok("2"),
        )?;
        ensure!(
            owner.rpc.node.controller.parent() == head,
            "follow altered the detached chain"
        );
        owner.novonet.as_mut().unwrap().close()?;
        return owner.rpc.node.shutdown();
    }
    let mut command = Command::new(&binary);
    let data = stage.join("clean-proving-device");
    ensure!(
        !data.exists(),
        "proof must start without a local client identity"
    );
    let profile_pin = format!("{:x}", Sha256::digest(fs::read(&profile)?));
    let rejected = Command::new(&binary)
        .arg(&profile)
        .arg("discover-verified-receipt")
        .arg(&hash)
        .arg("0".repeat(64))
        .env("LOCALAPPDATA", &data)
        .env("XDG_STATE_HOME", &data)
        .output()?;
    ensure!(
        !rejected.status.success()
            && !data.exists()
            && String::from_utf8_lossy(&rejected.stderr).contains("differs from its release pin"),
        "wrong receipt profile pin reached custody or discovery"
    );
    command
        .arg(&profile)
        .arg("discover-verified-receipt")
        .arg(&hash)
        .arg(&profile_pin)
        .env("LOCALAPPDATA", &data)
        .env("XDG_STATE_HOME", &data);
    let worker = thread::spawn(move || command.output());
    owner.until(|_| Ok(worker.is_finished()))?;
    let output = worker
        .join()
        .map_err(|_| anyhow::anyhow!("discovery proof Client panicked"))??;
    ensure!(
        output.status.success(),
        "discovery proof Client failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)?;
    let fingerprint = format!("{:x}", Sha256::digest(&key));
    verify_original_discovered_receipt(&report, &hash, &fingerprint)?;
    ensure!(
        report["transport_identity_sha256"]
            .as_str()
            .is_some_and(|identity| identity.len() == 64 && identity != fingerprint)
            && owner.rpc.node.controller.parent() == head
            && owner.active_client().is_err(),
        "detached verification did not preserve the original chain and independent custody"
    );
    println!("clean-device carrier proof verified: {report}");

    // Header synchronization serves signed roots while whole-receipt export is disabled.
    owner.rpc.node.public_asset_receipt_proofs = false;
    ensure!(
        head.height > 1,
        "isolated archive needs multiple finalized blocks"
    );
    let sync = |owner: &mut QueryOwner, action: &str| -> Result<std::process::Output> {
        let mut command = Command::new(&binary);
        command.arg(&profile).arg(action).env("LOCALAPPDATA", &data);
        let worker = thread::spawn(move || command.output());
        owner.until(|_| Ok(worker.is_finished()))?;
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("chain sync Client panicked"))?
            .map_err(Into::into)
    };
    for expected_reopen in [parent.height, head.height] {
        let output = sync(&mut owner, "sync-chain")?;
        ensure!(
            output.status.success(),
            "chain sync Client failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        ensure!(
            report["reopened_height"] == expected_reopen
                && report["verified_height"] == head.height
                && report["verified_block_hash"] == super::hex(&head.block_hash)
                && report["certified_state_root"] == super::hex(&head.state_root)
                && report["verified_state_version"] == head.state_version
                && report["reached_peer_advertised_height"] == true,
            "chain sync did not match the verified archive: {report}"
        );
        println!("certified chain sync: {report}");
    }
    let header = data
        .join("NOVOVM")
        .join("native-discovery")
        .join(format!(
            "{}-{}-{}",
            pins.0,
            super::hex(&pins.1),
            super::hex(&pins.2),
        ))
        .join(format!(
            "headers-{}-{:x}",
            parent.height,
            Sha256::digest(serde_json::to_vec(&parent)?)
        ))
        .join(format!("{:020}.proof", head.height));
    let original = fs::read(&header)?;
    let mut changed = original.clone();
    *changed
        .last_mut()
        .context("finalized header file is empty")? ^= 1;
    fs::write(&header, changed)?;
    let result = sync(&mut owner, "sync-chain");
    fs::write(&header, original)?;
    let output = result?;
    ensure!(
        !output.status.success()
            && String::from_utf8_lossy(&output.stderr).contains("verifying stored header"),
        "corrupted finalized header reopened as verified"
    );
    println!("altered local header rejected before reconnect");

    let output = sync(&mut owner, "sync-state")?;
    ensure!(
        !output.status.success()
            && String::from_utf8_lossy(&output.stderr)
                .contains("cannot serve the certified state snapshot"),
        "raw state export was accessible with the disclosure flag disabled"
    );
    owner.rpc.node.public_state_sync = true;
    let mut first_node_count = None;
    for reopen in [false, true] {
        let output = sync(&mut owner, "sync-state")?;
        ensure!(
            output.status.success(),
            "state sync Client failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let nodes = report["verified_state_nodes"]
            .as_u64()
            .context("verified node count")?;
        let leaves = report["verified_state_leaves"]
            .as_u64()
            .context("verified leaf count")?;
        ensure!(
            report["verified_height"] == head.height
                && report["certified_state_root"] == super::hex(&head.state_root)
                && report["verified_state_version"] == head.state_version
                && leaves > 1
                && nodes == leaves * 2 - 1
                && if reopen {
                    first_node_count == Some(nodes) && report["downloaded_state_nodes"] == 0
                } else {
                    report["downloaded_state_nodes"] == nodes
                },
            "state synchronization did not verify the exact detached tree: {report}"
        );
        first_node_count = Some(nodes);
        println!("certified state sync: {report}");
    }
    let root_node = header
        .parent()
        .unwrap()
        .join("state-nodes")
        .join(format!("{}.node", super::hex(&head.state_root),));
    let original = fs::read(&root_node)?;
    use novovm_protocol::native_query::{Answer, Query, QueryError, StateNodeSelector};
    let unrelated = Query::StateNode(StateNodeSelector {
        height: head.height,
        root: head.state_root,
        hash: [0xff; 32],
        ancestry: original.clone(),
    });
    ensure!(
        owner.rpc.answer_carrier_query(&unrelated, Instant::now())?
            == Answer::Error(QueryError::Unavailable),
        "requested hash outside the finalized tree was not rejected"
    );
    let mut changed = original.clone();
    *changed.last_mut().context("empty root node")? ^= 1;
    fs::write(&root_node, changed)?;
    let result = sync(&mut owner, "sync-state");
    fs::write(&root_node, original)?;
    let output = result?;
    ensure!(
        !output.status.success()
            && String::from_utf8_lossy(&output.stderr).contains("state node content hash"),
        "corrupted state node reopened as verified"
    );
    ensure!(
        owner.rpc.node.controller.parent() == head,
        "sync altered the detached chain"
    );
    println!("disabled export, unrelated hash, and altered cached state node rejected");
    owner.novonet.as_mut().unwrap().close()?;
    owner.rpc.node.shutdown()
}

struct IsolatedFollowChild(std::process::Child);
impl Drop for IsolatedFollowChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LanAcceptance {
    server: std::net::Ipv4Addr,
    client: std::net::Ipv4Addr,
    client_key: Vec<u8>,
    #[serde(default)]
    receipt_only: bool,
}

#[test]
#[ignore = "requires a detached original-store copy and a real remote installed Client report"]
fn detached_snapshot_checks_lan_install_report_after_shutdown() -> Result<()> {
    let lan = LanAcceptance::load(&input("NOVOVM_QUERY_COPY_LAN")?)?;
    let owner = open_snapshot(false)?;
    let key = owner.rpc.node.transport_identity().public_key().to_vec();
    let report = input("NOVOVM_QUERY_COPY_LAN_INSTALL_REPORT")?;
    lan.verify_installed_state(&owner, &key, &report)?;
    owner.rpc.node.shutdown()
}

impl LanAcceptance {
    fn load(path: &Path) -> Result<Self> {
        ensure!(path.metadata()?.len() <= 16_384, "LAN fixture input size");
        let settings: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        ensure!(
            settings.server.is_private()
                && settings.client.is_private()
                && settings.client != settings.server
                && settings.client_key.len() == 1952,
            "LAN fixture requires two private interfaces and one ML-DSA-65 public key"
        );
        Ok(settings)
    }

    fn serve_installed_state(
        &self,
        owner: &mut QueryOwner,
        profile: &Path,
        server_key: &[u8],
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::fs;

        let stage = profile.parent().context("LAN installer acceptance stage")?;
        let ready = stage.join("lan-ready.json");
        let result = stage.join("lan-installed-state.json");
        ensure!(
            !ready.exists() && !result.exists(),
            "stale LAN install evidence"
        );
        let profile_pin = prepare_lan_profile(profile, None)?;
        let server_identity = format!("{:x}", Sha256::digest(server_key));
        let head = owner.rpc.node.controller.parent();
        let state_root = super::hex(&head.state_root);
        let block_hash = super::hex(&head.block_hash);
        let draft = ready.with_extension("tmp");
        fs::write(
            &draft,
            serde_json::to_vec(&serde_json::json!({
                "profile_sha256": profile_pin, "server_transport_sha256": server_identity,
                "height": head.height, "state_root": state_root, "state_version": head.state_version,
                "block_hash": block_hash,
            }))?,
        )?;
        fs::rename(draft, ready)?;

        let deadline = Instant::now() + Duration::from_secs(480);
        while !result.is_file() {
            ensure!(Instant::now() < deadline, "cross-device installer deadline");
            owner.poll()?;
            thread::sleep(Duration::from_millis(2));
        }
        self.verify_installed_state(owner, server_key, &result)
    }

    fn verify_installed_state(
        &self,
        owner: &QueryOwner,
        server_key: &[u8],
        result: &Path,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::fs;

        ensure!(
            result.metadata()?.len() <= 16_384,
            "LAN installer report size"
        );
        let report: Value = serde_json::from_slice(&fs::read(result)?)?;
        let head = owner.rpc.node.controller.parent();
        let state_root = super::hex(&head.state_root);
        let block_hash = super::hex(&head.block_hash);
        let prior_client = format!("{:x}", Sha256::digest(&self.client_key));
        let server_identity = format!("{:x}", Sha256::digest(server_key));
        let discovery = &report["discovery"];
        let proof = &report["proof"];
        let sources = discovery["authenticated_discovery_sources"]
            .as_array()
            .context("LAN installer discovery sources")?;
        let records = discovery["signed_locator_candidates"]
            .as_array()
            .context("LAN installer locator evidence")?;
        ensure!(
            report["cold_started"] == true
                && report["task_limited"] == true
                && report["client_pid"].as_u64().is_some_and(|pid| pid > 0)
                && discovery["transport_identity_sha256"]
                    .as_str()
                    .is_some_and(|identity| identity.len() == 64
                        && identity != prior_client
                        && identity != server_identity)
                && discovery["transport_identity_sha256"] == report["local_transport_sha256"]
                && sources.len() == 2
                && sources[0] != sources[1]
                && sources
                    .iter()
                    .any(|source| source.as_str() == Some(server_identity.as_str()))
                && records.len() == 2
                && sources.iter().all(|source| records.iter().any(|record| {
                    record["transport_identity_sha256"] == *source
                        && record["sequence"]
                            .as_u64()
                            .is_some_and(|sequence| sequence > 0)
                }))
                && proof["verification"] == "independent_uca_execution_replay"
                && proof["height"] == head.height
                && proof["state_version"] == head.state_version
                && proof["state_root"] == state_root
                && proof["block_hash"] == block_hash
                && proof["receipt_batch_commitment"] == super::hex(&head.receipt_batch_commitment)
                && owner.active_client().is_err(),
            "remote installed Client did not prove new custody and verified state: {report}"
        );
        println!("cross-device Windows installation verified: {report}");
        Ok(())
    }

    fn serve(
        &self,
        owner: &mut QueryOwner,
        profile: &Path,
        server_key: &[u8],
        roaming_relay: bool,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::{fs, io::ErrorKind, net::UdpSocket};

        let stage = profile.parent().context("LAN acceptance stage")?;
        let cut = stage.join("lan-cut");
        let restore = stage.join("lan-restore");
        let results = stage.join("lan-events.json");
        let ready = stage.join("lan-ready.json");
        ensure!(
            [&cut, &restore, &results, &ready]
                .into_iter()
                .all(|file| !file.exists()),
            "stale LAN acceptance control files"
        );
        let external = UdpSocket::bind((self.server, 0))?;
        let gateway = external.local_addr()?;
        external.set_nonblocking(true)?;
        let internal = UdpSocket::bind("127.0.0.1:0")?;
        internal.connect(owner.config.udp_addr)?;
        internal.set_nonblocking(true)?;
        let profile_pin = prepare_lan_profile(profile, Some((gateway, server_key)))?;
        let head = owner.rpc.node.controller.parent();
        let client_identity = format!("{:x}", Sha256::digest(&self.client_key));
        let server_identity = format!("{:x}", Sha256::digest(server_key));
        let state_root = super::hex(&head.state_root);
        let mut manifest = serde_json::json!({
            "native_gateway": gateway,
            "profile_sha256": profile_pin,
            "client_transport_sha256": client_identity,
            "server_transport_sha256": server_identity,
            "height": head.height,
            "state_root": state_root,
            "state_version": head.state_version,
        });
        if roaming_relay {
            add_roaming_route_metadata(stage, &mut manifest)?;
        }
        let manifest_draft = ready.with_extension("tmp");
        fs::write(&manifest_draft, serde_json::to_vec(&manifest)?)?;
        fs::rename(manifest_draft, ready)?;

        let deadline = Instant::now() + Duration::from_secs(480);
        let mut remote = None;
        let mut cut_utc_ms = None;
        let mut buffer = [0; 65_536];
        while !results.is_file() {
            ensure!(
                Instant::now() < deadline,
                "cross-device acceptance deadline"
            );
            owner.poll()?;
            let cut_active = cut.is_file() && !restore.is_file();
            if cut_active && cut_utc_ms.is_none() {
                let observed =
                    u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
                cut_utc_ms = Some(observed);
                println!("isolated_native_cut_utc_ms={observed}");
            }
            for _ in 0..128 {
                let (size, source) = match external.recv_from(&mut buffer) {
                    Ok(value) => value,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error.into()),
                };
                if !cut_active && source.ip() == self.client {
                    remote = Some(source);
                    internal.send(&buffer[..size])?;
                }
            }
            for _ in 0..128 {
                let size = match internal.recv(&mut buffer) {
                    Ok(value) => value,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error.into()),
                };
                if let Some(remote) = remote.filter(|_| !cut_active) {
                    external.send_to(&buffer[..size], remote)?;
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        ensure!(
            cut_utc_ms.is_some() && (roaming_relay || restore.is_file()) && remote.is_some(),
            "missing observed cut, restoration or remote traffic"
        );
        ensure!(
            results.metadata()?.len() <= 65_536,
            "LAN event evidence size"
        );
        let evidence: Value = serde_json::from_slice(&fs::read(results)?)?;
        let events = evidence["verified"]
            .as_array()
            .context("LAN verified events")?;
        let transports = if roaming_relay {
            ["native_novorudp", "iroh"].as_slice()
        } else {
            ["native_novorudp", "iroh", "native_novorudp"].as_slice()
        };
        ensure!(
            evidence["client_pid"].as_u64().is_some_and(|pid| pid > 0)
                && events.len() == transports.len(),
            "one remote Client must verify every required path"
        );
        for (event, &transport) in events.iter().zip(transports) {
            ensure!(
                event["event"] == "verified_state"
                    && event["transport"] == transport
                    && event["height"] == head.height
                    && event["state_version"] == head.state_version
                    && event["state_root"] == state_root
                    && event["verified_leaves"]
                        .as_u64()
                        .is_some_and(|count| count > 1)
                    && event["verified_nodes"] == events[0]["verified_nodes"]
                    && event["transport_identity_sha256"] == client_identity
                    && event["peer_transport_sha256"] == server_identity,
                "LAN Client verified a different route, state or identity: {event}"
            );
        }
        if roaming_relay {
            println!("cross-device relay query recovery verified: {evidence}");
        } else {
            println!("cross-device LAN cut and recovery verified: {evidence}");
        }
        Ok(())
    }

    fn serve_relay_preflight(
        &self,
        owner: &mut QueryOwner,
        profile: &Path,
        server_key: &[u8],
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::fs;

        let stage = profile.parent().context("relay preflight stage")?;
        let ready = stage.join("lan-ready.json");
        let report = stage.join("relay-preflight.jsonl");
        ensure!(
            !ready.exists() && !report.exists(),
            "stale relay preflight evidence"
        );
        let head = owner.rpc.node.controller.parent();
        let mut manifest = serde_json::json!({
            "profile_sha256": prepare_lan_profile(profile, None)?,
            "client_transport_sha256": format!("{:x}", Sha256::digest(&self.client_key)),
            "server_transport_sha256": format!("{:x}", Sha256::digest(server_key)),
            "height": head.height,
            "state_root": super::hex(&head.state_root),
            "state_version": head.state_version,
        });
        add_roaming_route_metadata(stage, &mut manifest)?;
        let draft = ready.with_extension("tmp");
        fs::write(&draft, serde_json::to_vec(&manifest)?)?;
        fs::rename(&draft, &ready)?;
        let deadline = Instant::now() + Duration::from_secs(480);
        while !report.is_file() {
            ensure!(Instant::now() < deadline, "relay preflight deadline");
            owner.poll()?;
            thread::sleep(Duration::from_millis(2));
        }
        ensure!(
            report.metadata()?.len() <= 65_536,
            "relay preflight evidence size"
        );
        let original = fs::read_to_string(&report)?;
        let events: Vec<Value> = original
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?;
        let (first, rest) = events
            .split_first()
            .context("empty relay preflight report")?;
        let (verified, seen) = rest.split_last().context("relay preflight state missing")?;
        ensure!(
            first["event"] == "starting"
                && first["reopened_height"] == 0
                && events.iter().all(|event| event["transport_identity_sha256"]
                    == manifest["client_transport_sha256"])
                && verified["event"] == "verified_state"
                && verified["transport"] == "iroh"
                && verified["height"] == manifest["height"]
                && verified["state_root"] == manifest["state_root"]
                && verified["state_version"] == manifest["state_version"]
                && verified["peer_transport_sha256"] == manifest["server_transport_sha256"]
                && verified["verified_nodes"].as_u64().is_some_and(|n| n > 0)
                && verified["downloaded_nodes"] == verified["verified_nodes"]
                && verified["locator_sequence"].as_u64().is_some_and(|n| n > 0)
                && seen
                    .iter()
                    .any(|event| event["event"] == "verified_locator_accepted"
                        && event["peer_transport_sha256"] == manifest["server_transport_sha256"]
                        && event["locator_sequence"] == verified["locator_sequence"]),
            "relay preflight did not verify the detached chain and its authenticated locator"
        );
        println!(
            "fixed contact relay preflight verified, raw_stdout_sha256={:x}",
            Sha256::digest(original.as_bytes())
        );
        Ok(())
    }

    fn serve_receipt(
        &self,
        owner: &mut QueryOwner,
        profile: &Path,
        server_key: &[u8],
        hash: &str,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::fs;

        let stage = profile.parent().context("LAN receipt acceptance stage")?;
        let ready = stage.join("lan-ready.json");
        let result = stage.join("lan-receipt.json");
        ensure!(
            !ready.exists() && !result.exists(),
            "stale LAN receipt files"
        );
        let profile_pin = prepare_lan_profile(profile, None)?;
        let client_identity = format!("{:x}", Sha256::digest(&self.client_key));
        let server_identity = format!("{:x}", Sha256::digest(server_key));
        let manifest_draft = ready.with_extension("tmp");
        let manifest = serde_json::json!({
            "profile_sha256": profile_pin,
            "client_transport_sha256": client_identity,
            "server_transport_sha256": server_identity,
            "tx_hash": hash,
        });
        fs::write(&manifest_draft, serde_json::to_vec(&manifest)?)?;
        fs::rename(manifest_draft, ready)?;
        let deadline = Instant::now() + Duration::from_secs(360);
        while !result.is_file() {
            ensure!(
                Instant::now() < deadline,
                "cross-device receipt acceptance deadline"
            );
            owner.poll()?;
            thread::sleep(Duration::from_millis(2));
        }
        ensure!(
            result.metadata()?.len() <= 16_384,
            "LAN receipt evidence size"
        );
        let report: Value = serde_json::from_slice(&fs::read(result)?)?;
        verify_original_discovered_receipt(&report, hash, &server_identity)?;
        ensure!(
            report["transport_identity_sha256"] == client_identity
                && owner.active_client().is_err(),
            "LAN receipt Client changed custody or used the native query slot"
        );
        println!("cross-device installed Client receipt verified: {report}");
        Ok(())
    }

    fn serve_discovery(
        &self,
        owner: &mut QueryOwner,
        profile: &Path,
        server_key: &[u8],
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::fs;

        let stage = profile.parent().context("LAN discovery acceptance stage")?;
        let ready = stage.join("lan-ready.json");
        let result = stage.join("lan-discovery.json");
        ensure!(
            !ready.exists() && !result.exists(),
            "stale LAN discovery files"
        );
        let profile_pin = prepare_lan_profile(profile, None)?;
        let server_identity = format!("{:x}", Sha256::digest(server_key));
        let manifest_draft = ready.with_extension("tmp");
        fs::write(
            &manifest_draft,
            serde_json::to_vec(&serde_json::json!({
                "profile_sha256": profile_pin,
                "server_transport_sha256": server_identity,
            }))?,
        )?;
        fs::rename(manifest_draft, ready)?;

        let deadline = Instant::now() + Duration::from_secs(360);
        while !result.is_file() {
            ensure!(
                Instant::now() < deadline,
                "cross-device discovery acceptance deadline"
            );
            owner.poll()?;
            thread::sleep(Duration::from_millis(2));
        }
        ensure!(
            result.metadata()?.len() <= 16_384,
            "LAN discovery evidence size"
        );
        let report: Value = serde_json::from_slice(&fs::read(result)?)?;
        let candidates = report["signed_locator_candidates"]
            .as_array()
            .context("LAN discovery locator evidence")?;
        ensure!(
            report["transport_identity_sha256"]
                .as_str()
                .is_some_and(|identity| identity.len() == 64 && identity != server_identity)
                && report["authenticated_discovery_sources"]
                    .as_array()
                    .is_some_and(|sources| {
                        sources
                            .iter()
                            .any(|source| source.as_str() == Some(server_identity.as_str()))
                    })
                && candidates.iter().any(|candidate| {
                    candidate["transport_identity_sha256"] == server_identity
                        && candidate["sequence"].as_u64().is_some_and(|seq| seq > 0)
                })
                && owner.active_client().is_err(),
            "remote Client did not authenticate and verify this isolated source: {report}"
        );
        println!("cross-device discovery verified: {report}");
        Ok(())
    }
}

fn add_roaming_route_metadata(stage: &Path, manifest: &mut Value) -> Result<()> {
    use novovm_network::{
        product_directory::PeerSignedRelayRecordV1, product_service_lease::IrohServiceEndpointV1,
    };
    let record: PeerSignedRelayRecordV1 =
        serde_json::from_slice(&std::fs::read(stage.join("local-secondary.json"))?)?;
    let route = IrohServiceEndpointV1::from_record(&record)?.address;
    manifest["isolated_iroh_direct"] = serde_json::to_value(route.direct)?;
    manifest["approved_relays"] = serde_json::to_value(route.relays)?;
    Ok(())
}

fn prepare_lan_profile(
    profile: &Path,
    native_gateway: Option<(std::net::SocketAddr, &[u8])>,
) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::fs;

    let mut profile_data: Value = serde_json::from_slice(&fs::read(profile)?)?;
    if let Some((gateway, server_key)) = native_gateway {
        profile_data["native_query_peers"] =
            serde_json::json!([{"address":gateway,"transport_key":server_key}]);
    }
    profile_data["library"] = serde_json::json!("../../aoem/windows/core/bin/aoem_ffi.dll");
    let profile_bytes = serde_json::to_vec(&profile_data)?;
    fs::write(profile, &profile_bytes)?;
    Ok(format!("{:x}", Sha256::digest(&profile_bytes)))
}

fn verify_original_discovered_receipt(report: &Value, hash: &str, server: &str) -> Result<()> {
    ensure!(
        report["authenticated_discovery_sources"] == serde_json::json!([server])
            && report["serving_transport_sha256"] == server
            && report["independently_verified_finality"] == true
            && report["tx_hash"] == hash
            && report["success"] == true
            && report["height"] == 1
            && report["block_hash"]
                == "0x9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926"
            && report["state_root"]
                == "0x4223c7609d20861ed8e549382ea279a685f3c3a742ee917d334ada9acb44cca0"
            && report["proof_bytes"].as_u64().is_some_and(|size| size > 0),
        "discovery did not verify the original proof via the carrier: {report}"
    );
    Ok(())
}

// The separate interactive-user installer consumes the issued bundle while this
// LocalService fixture only serves finalized headers and state on loopback.
fn serve_installed_windows_fixture(
    owner: &mut QueryOwner,
    profile: &Path,
    primary: &Path,
    secondary: &Path,
) -> Result<()> {
    use std::fs;
    let stage = profile
        .parent()
        .context("Windows installer fixture directory")?;
    let ready = stage.join("windows-install-ready.json");
    let complete = stage.join("windows-install-complete.txt");
    ensure!(
        !ready.exists() && !complete.exists(),
        "stale Windows install fixture"
    );
    let mut document: Value = serde_json::from_slice(&fs::read(profile)?)?;
    document["library"] = serde_json::json!("../../aoem/windows/core/bin/aoem_ffi.dll");
    for (seed, path) in [primary, secondary].into_iter().enumerate() {
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .context("UTF-8 locator filename required")?;
        document["seeds"][seed]["locator"] = serde_json::json!(name);
    }
    fs::write(profile, serde_json::to_vec(&document)?)?;
    let head = owner.rpc.node.controller.parent();
    fs::write(
        &ready,
        serde_json::to_vec(&serde_json::json!({
            "height": head.height,
            "block_hash": super::hex(&head.block_hash),
            "state_root": super::hex(&head.state_root),
            "state_version": head.state_version,
        }))?,
    )?;
    let deadline = Instant::now() + Duration::from_secs(480);
    while !complete.is_file() {
        ensure!(
            Instant::now() < deadline,
            "interactive Windows installer did not finish"
        );
        owner.poll()?;
        thread::sleep(Duration::from_millis(2));
    }
    ensure!(
        fs::read_to_string(complete)?.trim() == "verified",
        "interactive Windows install did not verify state"
    );
    println!(
        "interactive Windows Client completed loopback state verification at height {}",
        head.height
    );
    Ok(())
}

fn test_installed_uca_replay(
    owner: &mut QueryOwner,
    binary: &Path,
    profile: &Path,
    anchor_height: u64,
) -> Result<()> {
    use crate::native_pipeline::{
        consensus::statement::{finalized_header, ParentPoint},
        proof::{capture_budget, uca_replay},
        state::frontier::CaptureStep,
    };
    use novovm_exec::resident::AoemLibrarySelection;
    use sha2::{Digest, Sha256};
    use std::fs;

    let stage = profile.parent().context("UCA replay fixture directory")?;
    let data = stage.join("uca-replay-device");
    ensure!(
        !data.exists(),
        "UCA replay requires a clean Client directory"
    );
    let profile_bytes = fs::read(profile)?;
    let pin = format!("{:x}", Sha256::digest(&profile_bytes));
    let installation: Value = serde_json::from_slice(&profile_bytes)?;
    let anchor: ParentPoint =
        serde_json::from_value(installation["verification"]["trusted_parent"].clone())?;
    let chain_id = installation["chain_id"]
        .as_u64()
        .context("fixture chain id")?;
    let genesis: [u8; 32] = serde_json::from_value(installation["genesis"].clone())?;
    let protocol: [u8; 32] = serde_json::from_value(installation["protocol"].clone())?;
    let headers = data
        .join("NOVOVM/native-discovery")
        .join(format!(
            "{chain_id}-{}-{}",
            super::hex(&genesis),
            super::hex(&protocol)
        ))
        .join(format!(
            "headers-{anchor_height}-{:x}",
            Sha256::digest(serde_json::to_vec(&anchor)?)
        ));
    let run = |owner: &mut QueryOwner, action: &str, archive: Option<&Path>| {
        let mut command = Command::new(binary);
        command
            .arg(profile)
            .arg(action)
            .env("LOCALAPPDATA", &data)
            .env("XDG_STATE_HOME", &data);
        if let Some(archive) = archive {
            command.arg(&pin).arg(archive);
        }
        let worker = thread::spawn(move || command.output());
        owner.until(|_| Ok(worker.is_finished()))?;
        Ok::<_, anyhow::Error>(
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("UCA replay Client panicked"))??,
        )
    };
    let head = owner.rpc.node.controller.parent();
    ensure!(
        head.height >= anchor_height + 2,
        "UCA replay needs a finalized prefix"
    );
    let verifier = AoemLibrarySelection::new(&owner.config.library, Default::default())?
        .open_wire_facade()?
        .asset_spend_verifier_v3()?;

    for height in [anchor_height + 1, anchor_height + 2] {
        let mut read = ArchiveRead::new(
            height,
            head,
            owner.rpc.node.controller.context(),
            owner.rpc.node.validators.clone(),
        )?;
        let mut block = None;
        owner.until(|owner| {
            if let Some(artifact) = read.poll(&owner.rpc.node.pipeline)? {
                block = Some(artifact.context("archive lacks the requested UCA block")?);
            }
            Ok(block.is_some())
        })?;
        let block = block.context("missing UCA block")?;
        let plan = uca_replay::prepare_plan(
            &verifier,
            *block.stored().context(),
            block.stored().raw_transactions(),
            owner.config.genesis.policy.clone(),
        )
        .with_context(|| format!("preparing original UCA input at height {height}"))?;
        let mut capture = plan.begin_capture(capture_budget())?;
        while capture.advance(256)? != CaptureStep::Complete {
            if let Some(hashes) = capture.next_request()? {
                let mut ticket = None;
                owner.until(|owner| {
                    ticket = owner
                        .rpc
                        .node
                        .pipeline
                        .try_read_state_nodes(hashes.clone())?;
                    Ok(ticket.is_some())
                })?;
                let mut ticket = ticket.context("missing archived state read ticket")?;
                let mut values = None;
                owner.until(|_| {
                    values = ticket.try_take()?;
                    Ok(values.is_some())
                })?;
                capture.accept(values.context("missing archived state read reply")?)?;
            }
        }
        let header = finalized_header::encode(&block)?;
        let bytes = uca_replay::encode(&header, &capture.finish()?.uca_replay_input()?)?;
        let archive = stage.join(format!("archive-height-{height}.nvreplay"));
        fs::write(&archive, &bytes)?;

        let writer_lock = if height > anchor_height + 1 {
            let output = run(owner, "verify-uca-execution", Some(&archive))?;
            let error = String::from_utf8_lossy(&output.stderr);
            ensure!(
                !output.status.success()
                    && error.contains("is not present in the installed checkpoint history"),
                "archive without a locally certified parent was not refused: {error}"
            );
            let output = run(owner, "sync-chain", None)?;
            ensure!(
                output.status.success(),
                "could not verify the original header prefix: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let report: Value = serde_json::from_slice(&output.stdout)?;
            ensure!(
                report["verified_height"] == head.height,
                "incomplete header prefix: {report}"
            );
            println!("UCA archive without a certified parent rejected before chain sync");
            let lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(headers.join("writer.lock"))?;
            lock.try_lock()?;
            Some(lock)
        } else {
            None
        };

        let output = run(owner, "verify-uca-execution", Some(&archive))?;
        ensure!(
            output.status.success(),
            "original UCA execution at height {height} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let point = block.point();
        ensure!(
            report["verification"] == "independent_uca_execution_replay"
                && report["height"] == height
                && report["parent_height"] == height - 1
                && report["block_hash"] == super::hex(&point.block_hash)
                && report["state_root"] == super::hex(&point.state_root)
                && report["receipt_batch_commitment"]
                    == super::hex(&point.receipt_batch_commitment)
                && report["state_version"] == point.state_version,
            "UCA replay differed from the certified archive: {report}"
        );
        println!("original archived UCA execution: {report}");
        if writer_lock.is_some() {
            println!("offline UCA replay succeeded with the checkpoint writer lock held");
        } else {
            ensure!(
                !data.exists(),
                "offline replay created local network custody or state"
            );
            println!("offline UCA replay created no Client data or transport key");
        }

        let corrupted = stage.join("corrupt-uca-replay.nvreplay");
        let first_row = &block.stored().raw_transactions()[0];
        let row_offset = bytes
            .windows(first_row.len())
            .position(|candidate| candidate == first_row)
            .context("original signed row missing from replay archive")?;
        for (index, label, expected) in [
            (
                12 + header.len() - 1,
                "header",
                "invalid round-bft vote signature",
            ),
            (
                bytes.len() - 1,
                "witness",
                "state node content hash or byte bound mismatch",
            ),
            (
                row_offset + first_row.len() - 1,
                "spend signature",
                "invalid AOEM ML-DSA signature",
            ),
        ] {
            let mut invalid = bytes.clone();
            invalid[index] ^= 1;
            fs::write(&corrupted, invalid)?;
            let output = run(owner, "verify-uca-execution", Some(&corrupted))?;
            let error = String::from_utf8_lossy(&output.stderr);
            ensure!(
                !output.status.success() && error.contains(expected),
                "altered UCA {label} did not fail at its verifier: {error}"
            );
            println!("altered UCA {label} rejected: {}", error.trim());
        }
    }
    Ok(())
}

fn test_installed_checkpoint_policy(
    owner: &mut QueryOwner,
    binary: &Path,
    profile: &Path,
    key: &[u8],
) -> Result<()> {
    use crate::native_pipeline::consensus::statement::finalized_header;
    use sha2::{Digest, Sha256};
    use std::{fs, process::Stdio};
    let head = owner.rpc.node.controller.parent();
    let set = owner.rpc.node.validators.clone();
    let mut read = ArchiveRead::new(
        head.height,
        head,
        owner.rpc.node.controller.context(),
        set.clone(),
    )?;
    let mut block = None;
    owner.until(|owner| {
        if let Some(artifact) = read.poll(&owner.rpc.node.pipeline)? {
            block = Some(artifact.context("archive lacks the installed checkpoint")?);
        }
        Ok(block.is_some())
    })?;
    let block = block.context("missing archived checkpoint")?;
    let certified = finalized_header::verify(
        &finalized_header::encode(&block)?,
        &set,
        block.context(),
        &block.parent(),
    )?;
    ensure!(
        certified.point() == head
            && certified
                .direct_nov_expected_journal(block.stored().context().effect_contract)
                .is_err(),
        "the archived UCA block must not be interpreted as a direct NOV execution proof input"
    );
    let stage = profile.parent().context("checkpoint fixture directory")?;
    let timestamp = certified.timestamp_unix_ms();
    let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    let mut issued: Value = serde_json::from_slice(&fs::read(profile)?)?;
    issued["native_query_peers"] =
        serde_json::json!([{"address":owner.config.udp_addr, "transport_key":key}]);
    issued["verification"]["minimum_finalized_checkpoint"] = serde_json::json!({"height":head.height, "block_hash":head.block_hash, "valid_until_unix_ms":now + 1_800_000});
    let pinned = stage.join("installed-checkpoint.json");
    let data = stage.join("checkpoint-device");
    fs::write(&pinned, serde_json::to_vec(&issued)?)?;
    let run =
        |owner: &mut QueryOwner, config: &Path, data: &Path| -> Result<std::process::Output> {
            let mut command = Command::new(binary);
            command
                .arg(config)
                .arg("sync-state")
                .env("LOCALAPPDATA", data)
                .env("XDG_STATE_HOME", data);
            let worker = thread::spawn(move || command.output());
            owner.until(|_| Ok(worker.is_finished()))?;
            Ok(worker
                .join()
                .map_err(|_| anyhow::anyhow!("checkpoint sync Client panicked"))??)
        };
    let good = run(owner, &pinned, &data)?;
    ensure!(
        good.status.success(),
        "valid installed checkpoint refused: {}",
        String::from_utf8_lossy(&good.stderr)
    );
    let report: Value = serde_json::from_slice(&good.stdout)?;
    ensure!(
        report["verified_height"] == head.height
            && report["verified_state_nodes"]
                .as_u64()
                .is_some_and(|n| n > 0)
            && report["certified_state_root"] == super::hex(&head.state_root),
        "valid checkpoint did not verify the state: {report}"
    );
    let bad = stage.join("wrong-checkpoint.json");
    let mut changed = issued.clone();
    changed["verification"]["minimum_finalized_checkpoint"]["block_hash"] =
        serde_json::to_value([0xa5u8; 32])?;
    fs::write(&bad, serde_json::to_vec(&changed)?)?;
    for directory in [data.clone(), stage.join("wrong-checkpoint-device")] {
        let invalid = run(owner, &bad, &directory)?;
        ensure!(
            !invalid.status.success()
                && String::from_utf8_lossy(&invalid.stderr)
                    .contains("conflicts with the installed minimum checkpoint"),
            "wrong pinned block was accepted using {}",
            directory.display()
        );
    }
    let stale = stage.join("stale-checkpoint.json");
    let mut aged = issued.clone();
    aged["verification"]["minimum_finalized_checkpoint"]["valid_until_unix_ms"] =
        serde_json::json!(now - 1);
    fs::write(&stale, serde_json::to_vec(&aged)?)?;
    let cold = stage.join("expired-checkpoint-cold-device");
    let cold_entry = |path: &Path, action: &str, pinned: bool| -> Result<std::process::Output> {
        let mut command = Command::new(binary);
        command.arg(path).arg(action);
        if pinned {
            command.arg(format!("{:x}", Sha256::digest(fs::read(path)?)));
        }
        let output = command
            .env("LOCALAPPDATA", &cold)
            .env("XDG_STATE_HOME", &cold)
            .output()?;
        ensure!(!cold.exists(), "{action} created device custody or cache");
        Ok(output)
    };
    ensure!(
        cold_entry(&pinned, "check-installed-profile", true)?
            .status
            .success(),
        "unexpired installed checkpoint failed offline preflight"
    );
    for (action, requires_pin) in [
        ("check-installed-profile", true),
        ("follow-state", true),
        ("sync-state", false),
    ] {
        let rejected = cold_entry(&stale, action, requires_pin)?;
        ensure!(
            !rejected.status.success()
                && String::from_utf8_lossy(&rejected.stderr)
                    .contains("installed checkpoint policy expired"),
            "{action} accepted expired installed checkpoint"
        );
    }
    let old = run(owner, &stale, &data)?;
    ensure!(
        !old.status.success()
            && String::from_utf8_lossy(&old.stderr).contains("installed checkpoint policy expired"),
        "expired installed checkpoint made reused state ready"
    );

    let future = stage.join("future-checkpoint.json");
    issued["verification"]["minimum_finalized_checkpoint"]["height"] =
        serde_json::json!(head.height + 1);
    fs::write(&future, serde_json::to_vec(&issued)?)?;
    let events = stage.join("checkpoint-follow.jsonl");
    let mut client = IsolatedFollowChild(
        Command::new(binary)
            .arg(&future)
            .arg("follow-state")
            .env("LOCALAPPDATA", &data)
            .env("XDG_STATE_HOME", &data)
            .stdout(Stdio::from(fs::File::create(&events)?))
            .spawn()?,
    );
    owner.until(|_| {
        ensure!(
            client.0.try_wait()?.is_none(),
            "checkpoint follower exited instead of waiting"
        );
        let seen = fs::read_to_string(&events)?;
        for line in seen
            .split_inclusive('\n')
            .filter(|line| line.ends_with('\n'))
        {
            let event: Value = serde_json::from_str(line)?;
            ensure!(
                event["event"] != "verified_state",
                "stale serving peer unlocked installed state"
            );
            if event["event"] == "waiting_for_required_finality" {
                return Ok(
                    event["reason"]["checkpoint"]["required_height"] == head.height + 1
                        && event["reason"]["checkpoint"]["verified_height"] == head.height,
                );
            }
        }
        Ok(false)
    })?;
    println!("valid finalized checkpoint reached, certified_height={}, signed_execution_timestamp_ms={timestamp}; wrong checkpoint rejected on cold and cached paths; expired checkpoint and missing successor withheld state readiness; current UCA relation rejected as direct NOV", head.height);
    Ok(())
}

fn test_installed_follow(
    owner: &mut QueryOwner,
    binary: &Path,
    profile: &Path,
    live_seed: &Path,
    key: &[u8],
    signed_address: bool,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::{fs, io::ErrorKind, net::UdpSocket, process::Stdio};

    let stage = profile.parent().context("follow stage")?;
    let gateway = UdpSocket::bind("127.0.0.1:0")?;
    gateway.set_nonblocking(true)?;
    let node = owner.config.udp_addr;
    let old_address = gateway.local_addr()?;
    ensure!(
        old_address != node,
        "old route must use a different bound port"
    );
    let mut client_address = None;
    let mut forward = |enabled: bool| -> Result<()> {
        let mut buffer = [0; 65536];
        for _ in 0..128 {
            let (count, sender) = match gateway.recv_from(&mut buffer) {
                Ok(datagram) => datagram,
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
                Err(error) => return Err(error.into()),
            };
            if !enabled {
                continue;
            }
            if sender == node {
                if let Some(client) = client_address {
                    gateway.send_to(&buffer[..count], client)?;
                }
            } else {
                ensure!(
                    sender.ip().is_loopback(),
                    "test gateway attempted external traffic"
                );
                client_address = Some(sender);
                gateway.send_to(&buffer[..count], node)?;
            }
        }
        Ok(())
    };
    let opened_seed = stage.join("follow-bootstrap-secondary.json");
    let mut installed: Value = serde_json::from_slice(&fs::read(profile)?)?;
    installed["native_query_peers"] =
        serde_json::json!([{"address":gateway.local_addr()?, "transport_key":key}]);
    installed["seeds"][0]["locator"] = serde_json::json!(stage.join("missing-primary.json"));
    installed["seeds"][1]["locator"] = serde_json::json!(opened_seed);
    let profile_bytes = serde_json::to_vec(&installed)?;
    let pin = format!("{:x}", Sha256::digest(&profile_bytes));
    fs::write(profile, profile_bytes)?;

    let device = stage.join("follow-clean-device");
    ensure!(
        !device.exists() && !opened_seed.exists(),
        "follow test requires fresh files"
    );
    let wrong_pin = Command::new(binary)
        .arg(profile)
        .arg("follow-state")
        .arg("0".repeat(64))
        .env("LOCALAPPDATA", &device)
        .env("XDG_STATE_HOME", &device)
        .output()?;
    ensure!(
        !wrong_pin.status.success()
            && !device.exists()
            && String::from_utf8_lossy(&wrong_pin.stderr).contains("differs from its release pin"),
        "invalid startup trust pin did not fail before creating custody"
    );
    let events_path = stage.join("follow-events.jsonl");
    let events = fs::File::create(&events_path)?;
    let stderr_path = stage.join("follow-client-stderr.log");
    let mut child = IsolatedFollowChild(
        Command::new(binary)
            .arg(profile)
            .arg("follow-state")
            .arg(&pin)
            .env("LOCALAPPDATA", &device)
            .env("XDG_STATE_HOME", &device)
            .stdout(Stdio::from(events))
            .stderr(Stdio::from(fs::File::create(&stderr_path)?))
            .spawn()?,
    );
    let pid = child.0.id();
    let mut await_events = |owner: &mut QueryOwner,
                            native: bool,
                            predicate: &dyn Fn(&[Value]) -> bool|
     -> Result<Vec<Value>> {
        let mut collected = Vec::new();
        owner.until(|_| {
            forward(native)?;
            ensure!(
                child.0.try_wait()?.is_none(),
                "auto-start Client exited: {}",
                fs::read_to_string(&stderr_path).unwrap_or_default()
            );
            let log = fs::read_to_string(&events_path)?;
            collected = log
                .split_inclusive('\n')
                .filter(|line| line.ends_with('\n'))
                .map(serde_json::from_str)
                .collect::<std::result::Result<_, _>>()?;
            Ok(predicate(&collected))
        })?;
        Ok(collected)
    };
    let has = |records: &[Value], event: &str, route: &str| {
        records
            .iter()
            .any(|item| item["event"] == event && item["transport"] == route)
    };
    await_events(owner, true, &|records| {
        has(records, "verified_state", "native_novorudp")
    })
    .context("initial authenticated native synchronization")?;
    let cut_ms = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    await_events(owner, false, &|records| {
        records.iter().any(|item| item["event"] == "retrying")
    })
    .context("lost native route with no available bootstrap records")?;
    if signed_address {
        use novovm_network::product_directory::PeerSignedRelayRecordV1;
        owner
            .until(|_| {
                let current: PeerSignedRelayRecordV1 =
                    serde_json::from_slice(&fs::read(live_seed)?)?;
                Ok(current.issued_at_ms > cut_ms)
            })
            .context("wait for a signed locator issued after the isolated route cut")?;
    }
    fs::copy(live_seed, &opened_seed)?;
    let recovered = if signed_address {
        let locator: novovm_network::product_directory::PeerSignedRelayRecordV1 =
            serde_json::from_slice(&fs::read(&opened_seed)?)?;
        ensure!(
            novovm_network::discovery_control::native_chain_query_address(&locator)? == Some(node),
            "signed locator did not use the actual native server socket"
        );
        let node_address = node.to_string();
        let records = await_events(owner, false, &|records| {
            records.iter().any(|event| {
                event["event"] == "verified_state"
                    && event["transport"] == "native_novorudp"
                    && event["native_peer_address"] == node_address
                    && event["locator_sequence"]
                        .as_u64()
                        .is_some_and(|s| s >= locator.sequence)
            })
        })
        .context("new signed native address was not authenticated")?;
        ensure!(
            records.iter().any(
                |event| event["native_peer_address"] == old_address.to_string()
                    && event["locator_sequence"].is_null()
            ),
            "initial release route not observed"
        );
        let recovered_index = records
            .iter()
            .position(|event| {
                event["event"] == "verified_state"
                    && event["native_peer_address"] == node_address
                    && event["locator_sequence"].is_number()
            })
            .context("refreshed native state event missing")?;
        let latest = &records[recovered_index];
        let recovered_ms = latest["utc_unix_ms"].as_u64().context("state event time")?;
        ensure!(
            records[..recovered_index].iter().any(|event| {
                event["event"] == "verified_locator_accepted"
                    && event["peer_transport_sha256"] == latest["peer_transport_sha256"]
                    && event["locator_sequence"] == latest["locator_sequence"]
                    && event["locator_issued_at_ms"]
                        .as_u64()
                        .is_some_and(|issued| {
                            issued > cut_ms
                                && event["utc_unix_ms"].as_u64().is_some_and(|accepted| {
                                    issued <= accepted && accepted <= recovered_ms
                                })
                        })
                    && event["locator_expires_at_ms"]
                        .as_u64()
                        .is_some_and(|expiry| recovered_ms < expiry)
            }),
            "no verified post-cut locator matching the recovered route"
        );
        println!("isolated_native_cut_utc_ms={cut_ms}");
        records
    } else {
        await_events(owner, false, &|records| {
            has(records, "verified_state", "iroh")
        })
        .context("autonomous fallback through the only available signed source")?;
        await_events(owner, true, &|records| {
            records
                .iter()
                .filter(|item| {
                    item["event"] == "verified_state" && item["transport"] == "native_novorudp"
                })
                .count()
                == 2
        })
        .context("recovery to the independently authenticated native route")?
    };

    let ready: Vec<_> = recovered
        .iter()
        .filter(|event| event["event"] == "verified_state")
        .collect();
    let tip = owner.rpc.node.controller.parent();
    let expected_root = super::hex(&tip.state_root);
    let serving_key = format!("{:x}", Sha256::digest(key));
    let identity = ready[0]["transport_identity_sha256"].clone();
    ensure!(
        (if signed_address {
            (2..=3).contains(&ready.len())
        } else {
            ready.len() == 3
        }) && identity.is_string()
            && identity != serving_key
            && ready.iter().all(|event| event["height"] == tip.height
                && event["state_root"] == expected_root
                && event["verified_nodes"] == ready[0]["verified_nodes"]
                && event["downloaded_nodes"] == ready[0]["verified_nodes"]
                && event["transport_identity_sha256"] == identity
                && event["peer_transport_sha256"] == serving_key),
        "route recovery did not preserve verified state and custody: {ready:?}"
    );
    println!(
        "one installed Client recovered without restart, pid={pid}, events={}",
        serde_json::to_string(&recovered)?
    );
    drop(child);
    if signed_address {
        let discovery = device.join("NOVOVM/native-discovery");
        let domain = fs::read_dir(discovery)?
            .next()
            .context("installed domain")??
            .path();
        let checkpoint = fs::read_dir(domain)?
            .filter_map(|entry| entry.ok())
            .find_map(|entry| {
                let path = entry.path().join("peer-locators.json");
                path.is_file().then_some(path)
            })
            .context("signed route checkpoint was not persisted")?;
        let original = fs::read(&checkpoint)?;
        let mut changed: Value = serde_json::from_slice(&original)?;
        let uri = changed[0]["endpoints"][0]["uri"]
            .as_str()
            .context("cached query URI")?;
        let (prefix, _) = uri
            .rsplit_once('/')
            .context("signed native address suffix")?;
        changed[0]["endpoints"][0]["uri"] = serde_json::json!(format!("{prefix}/{old_address}"));
        fs::write(&checkpoint, serde_json::to_vec(&changed)?)?;
        let result = Command::new(binary)
            .arg(profile)
            .arg("follow-state")
            .arg(&pin)
            .env("LOCALAPPDATA", &device)
            .env("XDG_STATE_HOME", &device)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        let rejection = (|| -> Result<()> {
            let mut reject = IsolatedFollowChild(result?);
            let mut status = None;
            owner.until(|_| {
                status = reject.0.try_wait()?;
                Ok(status.is_some())
            })?;
            let mut error = String::new();
            use std::io::Read;
            reject
                .0
                .stderr
                .take()
                .context("diagnostic stream")?
                .read_to_string(&mut error)?;
            ensure!(
                !status.context("restarted Client did not finish")?.success()
                    && error.contains("authenticating installed locator checkpoint"),
                "changed on-disk address passed independent signature verification on restart"
            );
            Ok(())
        })();
        fs::write(&checkpoint, original)?;
        rejection?;
        println!("modified signed route checkpoint rejected on restart");

        if let Some(archived_path) = std::env::var_os("NOVOVM_QUERY_COPY_EXPIRED_LOCATOR") {
            use novovm_network::{
                discovery_control::native_chain_query_address,
                product_directory::PeerSignedRelayRecordV1,
            };
            let archived_bytes = fs::read(archived_path)?;
            let archived: PeerSignedRelayRecordV1 = serde_json::from_slice(&archived_bytes)?;
            let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
            ensure!(
                archived.relay_public_key == key
                    && archived.expires_at_ms <= now
                    && native_chain_query_address(&archived)?
                        .is_some_and(|address| address.ip().is_loopback() && address != old_address),
                "expected an already expired signed native route for the detached server"
            );
            fs::rename(&opened_seed, stage.join("retained-fresh-bootstrap.json"))?;
            let expired_checkpoint = serde_json::to_vec(&[&archived])?;
            fs::write(&checkpoint, &expired_checkpoint)?;
            let log = stage.join("follow-expired-locator.jsonl");
            let stderr = stage.join("follow-expired-locator-stderr.log");
            let mut restarted = IsolatedFollowChild(
                Command::new(binary)
                    .arg(profile)
                    .arg("follow-state")
                    .arg(&pin)
                    .env("LOCALAPPDATA", &device)
                    .env("XDG_STATE_HOME", &device)
                    .stdout(Stdio::from(fs::File::create(&log)?))
                    .stderr(Stdio::from(fs::File::create(&stderr)?))
                    .spawn()?,
            );
            owner.until(|_| {
                forward(true)?;
                ensure!(
                    restarted.0.try_wait()?.is_none(),
                    "expired locator follower exited: {}",
                    fs::read_to_string(&stderr).unwrap_or_default()
                );
                for line in fs::read_to_string(&log)?.split_inclusive('\n').filter(|line| line.ends_with('\n')) {
                    let event: Value = serde_json::from_str(line)?;
                    if event["event"] == "verified_state" {
                        ensure!(
                            event["height"] == tip.height
                                && event["state_root"] == expected_root
                                && event["transport"] == "native_novorudp"
                                && event["peer_transport_sha256"] == serving_key
                                && event["native_peer_address"] == old_address.to_string()
                                && event["locator_sequence"].is_null()
                                && event["transport_identity_sha256"] == identity,
                            "expired locator recovery did not preserve authenticated state and custody: {event}"
                        );
                        return Ok(true);
                    }
                }
                Ok(false)
            })
            .context("expired locator prevented recovery to the fixed native route")?;
            drop(restarted);
            ensure!(
                fs::read(&checkpoint)? == expired_checkpoint,
                "expired signed sequence floor was discarded or replaced"
            );
            println!(
                "expired signed locator sequence={} sha256={:x}; authenticated fixed NovoRUDP recovery with the same state and custody",
                archived.sequence,
                Sha256::digest(&archived_bytes)
            );
        }
    }
    Ok(())
}

fn verify_archived_receipt(owner: &mut QueryOwner, request: &Path, observed: &Value) -> Result<()> {
    let hash = receipt_request_hash(request)?;
    let expected = &observed["state"]["Finalized"];
    let height = expected["height"]
        .as_u64()
        .context("finalized receipt height")?;
    let core = &owner.rpc.node.controller;
    let set = owner.rpc.node.validators.clone();
    let mut archive = ArchiveRead::new(height, core.parent(), core.context(), set.clone())?;
    let mut block = None;
    owner.until(|owner| {
        if let Some(found) = archive.poll(&owner.rpc.node.pipeline)? {
            block = Some(found.context("verified archive missing")?);
            return Ok(true);
        }
        Ok(false)
    })?;
    let block = block.context("finalized archive missing")?;
    let bytes = ReceiptFinalityProof::from_archive(&block)?.encode()?;
    let proof = ReceiptFinalityProof::decode(&bytes)?;
    let (context, parent) = super::super::native_genesis_anchor(
        &owner.rpc.node.template,
        owner.config.genesis.validator_epoch,
        &set,
    );
    ensure!(
        height == context.height,
        "expected the original receipt in the block after genesis"
    );
    let verified = proof.verify(&set, context, &parent, hash)?;
    ensure!(
        *verified.block() == block.point()
            && serde_json::to_value(verified.block().block_hash)? == expected["block_hash"]
            && serde_json::to_value(verified.receipt().success)? == expected["success"]
            && serde_json::to_value(verified.receipt().nonce_after)? == expected["nonce_after"]
            && expected["charged_fee"].as_str() == Some(verified.receipt().charged_fee.as_str()),
        "independently verified proof differs from the original receipt"
    );
    let mut wrong_parent = parent;
    wrong_parent.state_root[0] ^= 1;
    ensure!(
        proof.verify(&set, context, &wrong_parent, hash).is_err(),
        "wrong parent passed proof verification"
    );
    let mut wrong_transaction = hash;
    wrong_transaction[0] ^= 1;
    ensure!(
        proof
            .verify(&set, context, &parent, wrong_transaction)
            .is_err(),
        "wrong transaction passed proof verification"
    );
    let certificate = wire::encode_quorum(block.certificate())?;
    let signature_byte = bytes
        .windows(certificate.len())
        .position(|window| window == certificate)
        .context("proof did not contain original certificate")?
        + certificate.len()
        - 1;
    let output_bytes = block
        .stored()
        .spend_outputs()
        .map_or(0, |outputs| outputs.len() * 32);
    for offset in [
        signature_byte,
        bytes.len() - output_bytes - 1,
        bytes.len() - 1,
    ] {
        let mut tampered = bytes.clone();
        tampered[offset] ^= 1;
        ensure!(
            ReceiptFinalityProof::decode(&tampered)?
                .verify(&set, context, &parent, hash)
                .is_err(),
            "modified signed proof contents passed verification"
        );
    }
    let mut extra_bytes = bytes;
    extra_bytes.push(0);
    ensure!(
        ReceiptFinalityProof::decode(&extra_bytes).is_err(),
        "trailing proof data was accepted"
    );
    println!("receipt proof independently verified for height {height}; tampering rejected");
    Ok(())
}
