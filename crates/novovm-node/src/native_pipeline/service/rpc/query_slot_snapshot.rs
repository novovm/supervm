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
    let sync = |owner: &mut QueryOwner| -> Result<std::process::Output> {
        let mut command = Command::new(&binary);
        command
            .arg(&profile)
            .arg("sync-chain")
            .env("LOCALAPPDATA", &data);
        let worker = thread::spawn(move || command.output());
        owner.until(|_| Ok(worker.is_finished()))?;
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("chain sync Client panicked"))?
            .map_err(Into::into)
    };
    for expected_reopen in [parent.height, head.height] {
        let output = sync(&mut owner)?;
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
    let result = sync(&mut owner);
    fs::write(&header, original)?;
    let output = result?;
    ensure!(
        !output.status.success()
            && String::from_utf8_lossy(&output.stderr).contains("verifying stored header"),
        "corrupted finalized header reopened as verified"
    );
    ensure!(
        owner.rpc.node.controller.parent() == head,
        "header sync altered the detached chain"
    );
    println!("altered local header rejected before reconnect");
    owner.novonet.as_mut().unwrap().close()?;
    owner.rpc.node.shutdown()
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
