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
    let hash = super::hex(&receipt_request_hash(&request)?);
    let old_profile: ClientConfig =
        serde_json::from_slice(&fs::read(input("NOVOVM_QUERY_COPY_FIRST_PROFILE")?)?)?;
    let (unreachable, _) = novovm_relay::transport_identity::ephemeral_transport_identity(
        &old_profile.library,
        &old_profile.library_sha256,
    )?;
    let mut owner = open_snapshot(true)?;
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

    owner.novonet = Some(Driver::new(
        Config {
            probe_control_file: None,
            relays: Vec::new(),
            nameserver: "127.0.0.1".parse()?,
            bind_address: "127.0.0.1:0".parse()?,
            public_record_file: secondary.clone(),
            carrier_identity_file: None,
            peer_records: Vec::new(),
            discovery: true,
            discovery_seeds: Vec::new(),
            publish_native_query_address: std::env::var("NOVOVM_QUERY_COPY_FOLLOW").as_deref()
                == Ok("2"),
            relay_forwarding: false,
            fallback_after_ms: 1000,
            retry_ms: 500,
            udp_recovery_ms: 2000,
        },
        &mut owner.rpc.node,
    )?);
    owner.until(|_| Ok(secondary.is_file()))?;
    let locator: PeerSignedRelayRecordV1 = serde_json::from_slice(&fs::read(&secondary)?)?;
    let endpoint = IrohServiceEndpointV1::from_record(&locator)?.address;
    ensure!(
        !endpoint.direct.is_empty()
            && endpoint.direct.iter().all(|entry| entry.ip().is_loopback())
            && endpoint.relays.is_empty(),
        "detached proof carrier must bind loopback only"
    );
    let key = owner.rpc.node.transport_identity().public_key().to_vec();
    ensure!(locator.relay_public_key == key, "locator identity differs");
    let profile = stage.join("installed-discovery.json");
    fs::write(
        &profile,
        serde_json::to_vec(&serde_json::json!({
            "chain_id": pins.0, "genesis": pins.1, "protocol": pins.2,
            "library": old_profile.library, "library_sha256": old_profile.library_sha256,
            "nameserver": "127.0.0.1", "relays": [],
            "seeds": [
                { "locator": primary, "transport_key": first.relay_public_key },
                { "locator": secondary, "transport_key": key },
            ],
            "lookup_transport_keys": [key],
            "verification": {
                "validator_epoch": context.epoch,
                "validator_set_hash": context.validator_set_hash,
                "validators": owner.config.genesis.validators,
                "trusted_parent": parent,
            },
        }))?,
    )?;
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
    command
        .arg(&profile)
        .arg("discover-verified-receipt")
        .arg(&hash)
        .env("LOCALAPPDATA", &data);
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
    ensure!(
        report["authenticated_discovery_sources"] == serde_json::json!([fingerprint])
            && report["serving_transport_sha256"] == fingerprint
            && report["independently_verified_finality"] == true
            && report["tx_hash"] == hash
            && report["success"] == true
            && report["block_hash"]
                == "0x9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926"
            && owner.rpc.node.controller.parent() == head
            && owner.active_client().is_err(),
        "discovery did not verify the original proof via the carrier: {report}"
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
    await_events(owner, false, &|records| {
        records.iter().any(|item| item["event"] == "retrying")
    })
    .context("lost native route with no available bootstrap records")?;
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
            .arg(pin)
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
