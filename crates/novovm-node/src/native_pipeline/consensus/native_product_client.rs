//! Nonvalidator native client -> original node -> four real validators.
//! The retained test in this file uses HTTP observation only; `queries` instead
//! verifies submission AND observation over the native original-node socket.
use super::*;
use crate::native_pipeline::{
    consensus::{
        native::{LaneLimits, MessageDomain},
        transport::DecodeLimits,
    },
    ingress::apfl::{ApflLimits, ApflTransferBatch},
    service::{client::NativeApplicationClient, ResidentConfig},
};
use novovm_network::novorudp::{
    endpoint::NativePeer,
    fragments::ReassemblyLimits,
    object_link::{NativeSendLimits, NativeSendPolicy},
    object_sender::ObjectSendState,
    NovoRudpPacingProfile, NovoRudpWindowConfig,
};
use std::net::UdpSocket;

#[path = "native_product_queries.rs"]
mod queries;

fn connect(
    nodes: &ProductNodes,
    ingress: usize,
    set: Arc<ValidatorSet>,
) -> Result<NativeApplicationClient> {
    let config: ResidentConfig = serde_json::from_slice(&fs::read(&nodes.configs[ingress])?)?;
    // This device key is NOT in genesis and is never added to a node allowlist.
    let key = SigningKey::from_bytes(&[201; 32]);
    ensure!(
        config
            .genesis
            .validators
            .iter()
            .all(|v| v.public_key != key.verifying_key().to_bytes()),
        "fixture client is a validator"
    );
    NativeApplicationClient::connect(
        UdpSocket::bind("127.0.0.1:0")?,
        key,
        NativePeer {
            key: validator_key(ingress).verifying_key().to_bytes(),
            address: config.udp_addr,
        },
        MessageDomain {
            chain_id: CHAIN,
            genesis: config.genesis.genesis_config_commitment,
            protocol: config.genesis.protocol_commitment,
            validators: set,
        },
        DecodeLimits {
            transactions: 2,
            transaction_bytes: 1024,
            body_bytes: 2048,
            message_bytes: 512 * 1024,
        },
        LaneLimits {
            receive: ReassemblyLimits {
                max_message_bytes: 512 * 1024,
                messages: 2,
                bytes: 1024 * 1024,
                peer_messages: 2,
                peer_bytes: 1024 * 1024,
                ttl: Duration::from_secs(30),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: 1024 * 1024,
                peer_messages: 2,
                peer_bytes: 1024 * 1024,
            },
        },
        NativeSendPolicy {
            window: NovoRudpWindowConfig {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 32,
                batch_pause_ms: 0,
                ack_wait_ms: 10,
                ..Default::default()
            },
            pacing: NovoRudpPacingProfile {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 32,
                batch_pause_ms: 0,
                ack_wait_ms: 10,
                no_progress_backoff: false,
            },
            ttl: Duration::from_secs(30),
        },
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
        Instant::now(),
    )
}

fn send(client: &mut NativeApplicationClient, raw: &[Vec<u8>]) -> Result<()> {
    let batch = Arc::new(ApflTransferBatch::from_raw(
        raw,
        ApflLimits {
            transactions: 2,
            transaction_bytes: 1024,
            body_bytes: 2048,
        },
    )?);
    let input = client.prepare(batch)?;
    let mut sent = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        ensure!(
            Instant::now() < deadline,
            "native application transport timeout"
        );
        let now = Instant::now();
        let report = client.poll(
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
            now,
            64,
        )?;
        for completion in report.completed {
            if completion.stream == novovm_protocol::native_query::STREAM {
                continue;
            }
            ensure!(
                completion.body.id() == input.id()
                    && completion.state == ObjectSendState::Delivered,
                "native client completion mismatch/expiry"
            );
            return Ok(()); // Transport only, subsequent receipt observation required.
        }
        if !sent {
            sent = client.try_send(&input, now)?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[ignore = "real AOEM/original node binary; native nonvalidator client; run alone"]
fn native_application_apfl_fees_failure_and_cold_restart_without_http_submission() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY").context("explicit binary required")?,
    )
    .canonicalize()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-rpc")
        .join(format!(
            "application-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    eprintln!("native application artifacts={}", directory.display());
    let (mut nodes, set) = setup_with_genesis(&directory, binary, genesis()?, 2)?;
    let ingress = (0..4)
        .find(|i| {
            Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                != set.leader(1, 0).unwrap()
        })
        .unwrap();
    // Enable only a bounded open policy, with NO client identity in configuration.
    let mut config: Value = serde_json::from_slice(&fs::read(&nodes.configs[ingress])?)?;
    config["native_client_limit"] = json!(2);
    fs::write(&nodes.configs[ingress], serde_json::to_vec_pretty(&config)?)?;
    for i in 0..4 {
        nodes.start(i, "create")?;
    }
    nodes.wait_ready(&[0, 1, 2, 3])?;
    let mut client = connect(&nodes, ingress, set.clone())?;
    let mut bad = signed(1, 0, 100)?;
    *bad.last_mut().unwrap() ^= 1;
    send(&mut client, &[bad])?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = nodes.rpc(ingress, "nov_chainStatus", json!([]))?;
        if status["gossip_rejected_inputs"].as_u64().unwrap_or(0) > 0 {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "bad native signature not independently rejected"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut hashes = Vec::new();
    let mut receipts = Vec::new();
    for raw in [
        vec![signed(1, 0, 100)?, signed(3, 0, 50)?],
        vec![signed(1, 1, 1_000_001)?, signed(3, 1, 25)?],
        vec![signed(1, 2, 50)?, signed(3, 2, 25)?],
    ] {
        send(&mut client, &raw)?;
        let batch_hashes = transaction_hashes(&raw)?;
        let done = nodes.wait_receipts(&[0, 1, 2, 3], &batch_hashes)?;
        receipts.extend(done[0].clone());
        hashes.extend(batch_hashes);
    }
    ensure!(
        receipts
            .iter()
            .enumerate()
            .all(|(i, r)| r["success"] == (i != 2)),
        "native business/nonce failure semantics differ"
    );
    let balances = nodes.balances(&[0, 1, 2, 3], &receipts, [150, 100])?;
    let before = nodes.wait_ready(&[0, 1, 2, 3])?;
    ensure!(
        before.iter().all(|s| s["head"] == before[0]["head"]
            && s["execution_apfl_view_transactions_total"]
                .as_u64()
                .unwrap_or(0)
                >= 6),
        "native APFL not executed on every node"
    );
    if test_execution_backend()? == "aoem_integer_v7" {
        ensure!(
            before.iter().all(
                |s| s["execution_integer_rows_total"].as_u64().unwrap_or(0) >= 6
                    && s["execution_integer_plans_total"].as_u64().unwrap_or(0) >= 3
            ),
            "integer plan did not execute"
        );
    }
    drop(client);
    nodes.stop_all()?;
    for i in 0..4 {
        nodes.start(i, "existing")?;
    }
    let recovered = nodes.wait_ready(&[0, 1, 2, 3])?;
    ensure!(
        recovered.iter().all(|s| s["head"] == before[0]["head"]),
        "native cold head differs"
    );
    ensure!(
        nodes.wait_receipts(&[0, 1, 2, 3], &hashes)?[0] == receipts,
        "native cold receipt differs"
    );
    ensure!(
        nodes.balances(&[0, 1, 2, 3], &receipts, [150, 100])? == balances,
        "native cold economics differs"
    );
    let mut client = connect(&nodes, ingress, set)?;
    send(&mut client, &[signed(1, 0, 100)?])?; // Replay, same original signed bytes.
    let next = vec![signed(1, 3, 1)?, signed(3, 3, 1)?];
    send(&mut client, &next)?;
    let after = nodes.wait_receipts(&[0, 1, 2, 3], &transaction_hashes(&next)?)?;
    let mut all_receipts = receipts.clone();
    all_receipts.extend(after[0].clone());
    nodes.balances(&[0, 1, 2, 3], &all_receipts, [151, 101])?;
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&json!({
            "backend":test_execution_backend()?, "submission":"native_novorudp_apfl", "http_transaction_submissions":0,
            "nonvalidator_client":true,"client_allowlist":false,"single_ingress":ingress,
            "receipts":receipts,"balances":balances,"before":before,"recovered":recovered,"after":after,
            "query_transport":"http_compatibility_only","transport_done_is_finality":false,
            "real_processes":4,"real_machines":1,"throughput_acceptance":false
        }))?,
    )?;
    nodes.stop_all()?;
    Ok(())
}
