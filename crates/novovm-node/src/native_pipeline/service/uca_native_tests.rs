//! Four original services/four independent databases, in ONE process, actual
//! localhost NovoRUDP application and validator sockets. No direct transaction
//! injection, HTTP observer, synthetic vote or alternate execution authority.
//! This is not four-process/machine, mainnet, full PQ consensus or TPS evidence.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use config::uca_genesis::{
    UcaGenesisAccountV1, UcaGenesisApprovalV1, UcaGenesisGrantV1, UcaGenesisManifestV1,
    UcaInitializationConfigV1, INITIALIZATION_CONTEXT_V1,
};
use novovm_adapter_api::uca_spend_policy::SuppliedSpendGrantV1;
use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade, AoemExecOpenOptions};
use novovm_network::novorudp::object_sender::ObjectSendState;
use novovm_protocol::{
    native_query::{
        AccountId, Answer, Query, ReceiptState, UcaReadIntent, UcaSpendStateView, UcaState,
    },
    uca_spend::{spend_key_ref_v1, SignedUcaSpendV1, UcaSpendIntentV1, MAX_SIGNED_SPEND_BYTES},
    uca_spend_batch::{UcaSpendBatch, UcaSpendBatchLimits},
};
use sha2::{Digest, Sha256};
use std::{net::UdpSocket, path::PathBuf};

const IDS: [&str; 2] = ["uca:native-alice", "uca:native-bob"];
const ADDRESSES: [[u8; 20]; 2] = [[11; 20], [12; 20]];
const TIMEOUT: Duration = Duration::from_secs(45);

fn wall_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

// Identical development economics to the existing UCA publication fixture.
// Not an approved production distribution or new fee rule.
fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn validator_key(index: usize) -> SigningKey {
    SigningKey::from_bytes(&[31 + index as u8; 32])
}

fn start(configs: &[ResidentConfig], mode: StartMode) -> Result<Vec<rpc::RpcLifecycle>> {
    configs
        .iter()
        .map(|config| {
            rpc::RpcLifecycle::new(
                ResidentNode::start(config.clone(), mode)?,
                config.batch_size,
            )
        })
        .collect()
}

fn connect(
    config: &ResidentConfig,
    validators: Arc<ValidatorSet>,
) -> Result<client::NativeApplicationClient> {
    let key = SigningKey::from_bytes(&[201; 32]);
    ensure!(
        config
            .genesis
            .validators
            .iter()
            .all(|v| v.public_key != key.verifying_key().to_bytes()),
        "client is validator"
    );
    client::NativeApplicationClient::connect(
        UdpSocket::bind("127.0.0.1:0")?,
        key,
        NativePeer {
            key: read_signer(config)?.verifying_key().to_bytes(),
            address: config.udp_addr,
        },
        MessageDomain {
            chain_id: config.genesis.chain_id,
            genesis: config.genesis.genesis_config_commitment,
            protocol: config.genesis.protocol_commitment,
            validators,
        },
        DecodeLimits {
            transactions: 2,
            transaction_bytes: MAX_SIGNED_SPEND_BYTES,
            body_bytes: 2 * MAX_SIGNED_SPEND_BYTES,
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
        wall_ms()?,
        Instant::now(),
    )
}

fn tick(nodes: &mut [rpc::RpcLifecycle]) -> Result<()> {
    for (index, node) in nodes.iter_mut().enumerate() {
        node.poll(Instant::now())
            .with_context(|| format!("node {index} poll"))?;
        ensure!(
            node.status()["projection_error"].is_null(),
            "node {index} projection: {}",
            node.status()
        );
    }
    Ok(())
}

fn send(
    nodes: &mut [rpc::RpcLifecycle],
    client: &mut client::NativeApplicationClient,
    signed: &SignedUcaSpendV1,
    payer: usize,
    recipient: usize,
) -> Result<()> {
    let batch = Arc::new(UcaSpendBatch::from_raw(
        &[signed.encode()?],
        &[(ADDRESSES[payer].to_vec(), ADDRESSES[recipient].to_vec())],
        UcaSpendBatchLimits {
            transactions: 2,
            transaction_bytes: MAX_SIGNED_SPEND_BYTES,
            body_bytes: 2 * MAX_SIGNED_SPEND_BYTES,
        },
    )?);
    let prepared = client.prepare_uca(batch)?;
    let deadline = Instant::now() + TIMEOUT;
    let mut sent = false;
    loop {
        ensure!(Instant::now() < deadline, "native NUAS delivery timeout");
        tick(nodes)?;
        let now = Instant::now();
        for completion in client.poll(wall_ms()?, now, 64)?.completed {
            if completion.body.id() == prepared.id() {
                ensure!(
                    completion.state == ObjectSendState::Delivered,
                    "native input expired"
                );
                return Ok(()); // Only custody. Never used as final receipt.
            }
        }
        if !sent {
            sent = client.try_send(&prepared, now)?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn query(
    nodes: &mut [rpc::RpcLifecycle],
    client: &mut client::NativeApplicationClient,
    selector: Query,
) -> Result<Answer> {
    let prepared = client.prepare_query(selector)?;
    let deadline = Instant::now() + TIMEOUT;
    let mut sent = false;
    loop {
        ensure!(Instant::now() < deadline, "native query timeout");
        tick(nodes)?;
        let now = Instant::now();
        let _ = client.poll(wall_ms()?, now, 64)?;
        if !sent {
            sent = client.try_send_query(&prepared, now)?;
        }
        if let Some(response) = client.poll_query_response(&prepared, now)? {
            return Ok(response.answer);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn receipt(
    nodes: &mut [rpc::RpcLifecycle],
    client: &mut client::NativeApplicationClient,
    signed: &SignedUcaSpendV1,
) -> Result<ReceiptState> {
    let hash = Sha256::digest(signed.intent.signing_bytes()?).into();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        ensure!(
            Instant::now() < deadline,
            "NUAS final receipt timeout: {:?}",
            nodes.iter().map(|n| n.status()).collect::<Vec<_>>()
        );
        if let Answer::Receipt {
            state: state @ ReceiptState::Finalized { .. },
            ..
        } = query(nodes, client, Query::Receipt { hash })?
        {
            return Ok(state);
        }
    }
}

fn state(
    nodes: &mut [rpc::RpcLifecycle],
    client: &mut client::NativeApplicationClient,
    intent: &UcaSpendIntentV1,
    payer: usize,
    recipient: usize,
) -> Result<UcaSpendStateView> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        ensure!(Instant::now() < deadline, "native UCA state timeout");
        if let Answer::UcaSpendState(UcaState::Present(view)) = query(
            nodes,
            client,
            Query::UcaSpendState {
                intent: UcaReadIntent::new(intent).map_err(anyhow::Error::msg)?,
                payer_hint: AccountId::Bytes20(ADDRESSES[payer]),
                recipient_hint: AccountId::Bytes20(ADDRESSES[recipient]),
            },
        )? {
            return Ok(view);
        }
    }
}

fn fee(receipt: ReceiptState, height: u64) -> Result<u128> {
    if let ReceiptState::Finalized {
        success: true,
        nonce_after: 1,
        charged_fee,
        height: actual,
        ..
    } = receipt
    {
        ensure!(
            actual == height && charged_fee > 0,
            "wrong finalized height/fee"
        );
        Ok(charged_fee)
    } else {
        anyhow::bail!("unexpected UCA receipt {receipt:?}")
    }
}

#[test]
#[ignore = "real trusted AOEM; four original services and DBs in one process, actual UDP, run alone"]
fn actual_native_uca_single_client_respend_and_cold_recovery() -> Result<()> {
    let library = PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .context("explicit trusted library required")?,
    )
    .canonicalize()?;
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == library,
        "keygen/library mismatch"
    );
    let runtime = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
    let keys = (0..4)
        .map(|_| aoem_bindings::mldsa_keygen_v1_auto(65)?.context("ML-DSA-65 keygen unavailable"))
        .collect::<Result<Vec<_>>>()?;
    let manifest = UcaGenesisManifestV1 {
        genesis: GenesisConfig {
            chain_id: 781,
            validator_epoch: 1,
            protocol_commitment: config::protocol_commitment(),
            genesis_config_commitment: [0; 32],
            timestamp_unix_ms: 172_800_500,
            validators: (0..4)
                .map(|i| GenesisValidator {
                    public_key: validator_key(i).verifying_key().to_bytes(),
                    weight: 1,
                })
                .collect(),
            allocations: vec![GenesisAllocation {
                account_hex: "0b".repeat(20),
                amount: "1000".into(),
            }],
            policy: policy(),
        },
        accounts: (0..2)
            .map(|i| {
                Ok(UcaGenesisAccountV1 {
                    uca_id: IDS[i].into(),
                    asset_account: ADDRESSES[i].to_vec(),
                    management_public_key: keys[i].0.clone(),
                    grants: vec![UcaGenesisGrantV1 {
                        spend_public_key: keys[i + 2].0.clone(),
                        grant: SuppliedSpendGrantV1 {
                            account_id: IDS[i].into(),
                            key_ref: spend_key_ref_v1(&keys[i + 2].0)?,
                            authorization_epoch: 1,
                            permits_nov_transfer: true,
                            revoked: false,
                            not_before_height: 1,
                            expires_at_height: 100,
                            max_debit_per_intent: 500,
                            remaining_debit_budget: 1000,
                        },
                    }],
                })
            })
            .collect::<Result<Vec<_>>>()?,
    };
    let domain = manifest.signing_domain()?;
    let management = runtime.mldsa_signer_v1(AoemMldsaParameterSet::MlDsa65)?;
    let approvals = (0..2)
        .map(|i| {
            Ok(UcaGenesisApprovalV1 {
                uca_id: IDS[i].into(),
                signature: management.sign(
                    &keys[i].0,
                    &keys[i].1,
                    &manifest.canonical_bytes()?,
                    INITIALIZATION_CONTEXT_V1,
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let initial = manifest.compile(&runtime, domain, &approvals)?;
    let material = UcaInitializationConfigV1 {
        accounts: manifest.accounts.clone(),
        approvals,
        expected_signing_domain: domain,
        expected_initial_root: initial.state().root(),
        expected_final_commitment: initial.final_commitment(),
    };
    let mut genesis = manifest.genesis;
    genesis.genesis_config_commitment = domain;
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests/uca-native-services")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    std::fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    println!(
        "UCA original native service evidence={}",
        directory.display()
    );
    let reserved = (0..4)
        .map(|_| UdpSocket::bind("127.0.0.1:0"))
        .collect::<std::io::Result<Vec<_>>>()?;
    let addresses = reserved
        .iter()
        .map(UdpSocket::local_addr)
        .collect::<std::io::Result<Vec<_>>>()?;
    let configs = (0..4)
        .map(|i| {
            let key_file = directory.join(format!("test-validator-{i}.key"));
            std::fs::write(&key_file, validator_key(i).to_bytes())?;
            Ok(ResidentConfig {
                profile: config::ORIGINAL_PROFILE.into(),
                library: library.clone(),
                database: directory.join(format!("node-{i}.rocksdb")),
                rpc_addr: None,
                signing_key_file: key_file,
                udp_addr: addresses[i],
                peers: (0..4)
                    .filter(|j| *j != i)
                    .map(|j| NativePeerConfig {
                        public_key: validator_key(j).verifying_key().to_bytes(),
                        address: addresses[j],
                    })
                    .collect(),
                native_client_limit: 2,
                genesis: genesis.clone(),
                uca_initialization: Some(material.clone()),
                workers: 2,
                execution_backend: Default::default(),
                batch_size: 2,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    drop(reserved);
    let mut nodes = start(&configs, StartMode::CreateNew)?;
    let validators = nodes[0].node.validators.clone();
    let ingress = (0..4)
        .find(|i| !nodes[*i].node.controller.is_local_leader().unwrap())
        .context("nonleader ingress missing")?;
    let mut clients = configs
        .iter()
        .map(|config| connect(config, validators.clone()))
        .collect::<Result<Vec<_>>>()?;
    let intent =
        |payer: usize, recipient: usize, amount: u128, epoch: u64| -> Result<UcaSpendIntentV1> {
            Ok(UcaSpendIntentV1 {
                chain_id: genesis.chain_id,
                genesis: domain,
                account_id: IDS[payer].into(),
                authorization_epoch: epoch,
                spend_key_ref: spend_key_ref_v1(&keys[payer + 2].0)?,
                recipient_uca: IDS[recipient].into(),
                amount,
                max_fee: 50,
                nonce: 0,
                not_before_height: 1,
                expires_at_height: 100,
            })
        };
    let signer = runtime.uca_spend_signer_v1()?;
    let transfer = signer.sign(intent(0, 1, 100, 1)?, &keys[2].0, &keys[2].1)?;
    let mut tampered = transfer.clone();
    tampered.signature[0] ^= 1;
    let stale = signer.sign(intent(0, 1, 100, 2)?, &keys[2].0, &keys[2].1)?;
    for rejected in [&tampered, &stale] {
        let baseline = match query(&mut nodes, &mut clients[ingress], Query::Status)? {
            Answer::Status(s) => s.rejected_inputs,
            other => anyhow::bail!("status {other:?}"),
        };
        send(&mut nodes, &mut clients[ingress], rejected, 0, 1)?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            ensure!(Instant::now() < deadline, "invalid NUAS not rejected");
            if let Answer::Status(s) = query(&mut nodes, &mut clients[ingress], Query::Status)? {
                ensure!(
                    s.height == 0 && s.pending == 0,
                    "invalid input created state/work"
                );
                if s.rejected_inputs > baseline {
                    break;
                }
            }
        }
    }
    send(&mut nodes, &mut clients[ingress], &transfer, 0, 1)?;
    let first = clients
        .iter_mut()
        .map(|client| receipt(&mut nodes, client, &transfer))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        first.iter().all(|r| r == &first[0]),
        "first receipts differ"
    );
    let first_fee = fee(first[0], 1)?;
    let respend = signer.sign(intent(1, 0, 40, 1)?, &keys[3].0, &keys[3].1)?;
    send(&mut nodes, &mut clients[ingress], &respend, 1, 0)?;
    let second = clients
        .iter_mut()
        .map(|client| receipt(&mut nodes, client, &respend))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        second.iter().all(|r| r == &second[0]),
        "second receipts differ"
    );
    let second_fee = fee(second[0], 2)?;
    let mut before = Vec::new();
    for client in &mut clients {
        let alice = state(&mut nodes, client, &transfer.intent, 0, 1)?;
        let bob = state(&mut nodes, client, &respend.intent, 1, 0)?;
        ensure!(
            alice.height == 2 && bob.height == 2 && alice.state_root == bob.state_root,
            "mixed published roots"
        );
        ensure!(
            alice.next_nonce == 1 && bob.next_nonce == 1,
            "account nonce differs"
        );
        ensure!(
            alice.remaining_debit_budget == 1000 - 100 - first_fee
                && bob.remaining_debit_budget == 1000 - 40 - second_fee,
            "grant consumption differs"
        );
        ensure!(
            alice.payer_balance == 940 - first_fee
                && bob.payer_balance == 60 - second_fee
                && alice.recipient_balance == bob.payer_balance
                && bob.recipient_balance == alice.payer_balance,
            "debit/credit or respend mismatch"
        );
        before.push((alice, bob));
    }
    ensure!(
        before.iter().all(|s| s == &before[0]),
        "four-node UCA state differs"
    );
    // Exact already-published bytes cannot consume a second nonce or budget.
    send(&mut nodes, &mut clients[ingress], &transfer, 0, 1)?;
    assert_eq!(
        receipt(&mut nodes, &mut clients[ingress], &transfer)?,
        first[0]
    );
    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    let mut nodes = start(&configs, StartMode::Existing)?;
    let mut clients = configs
        .iter()
        .map(|config| connect(config, validators.clone()))
        .collect::<Result<Vec<_>>>()?;
    for (index, client) in clients.iter_mut().enumerate() {
        assert_eq!(receipt(&mut nodes, client, &transfer)?, first[0]);
        assert_eq!(receipt(&mut nodes, client, &respend)?, second[0]);
        let after = (
            state(&mut nodes, client, &transfer.intent, 0, 1)?,
            state(&mut nodes, client, &respend.intent, 1, 0)?,
        );
        assert_eq!(after, before[index]);
    }
    std::fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "real_services":4, "real_databases":4, "processes":1, "machines":1,
            "transport":"original NovoRUDP/APFL application and validator sockets", "single_submission_ingress":ingress,
            "http":false, "direct_transaction_injection":false, "first":first, "second":second,
            "state_before_and_after":before, "invalid_signature_rejected":true, "wrong_authorization_epoch_rejected":true,
            "published_replay_unchanged":true, "cold_restart":true, "TPS_acceptance":false, "production_activation":false,
        }))?,
    )?;
    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    println!("NUAS original application -> four original services/DBs -> durable receipts -> recipient respend -> cold state equality; no HTTP, no direct input, no TPS claim");
    Ok(())
}
