//! Four original services/four independent databases, in ONE process, actual
//! localhost NovoRUDP application and validator sockets. The UDP closure has no
//! direct transaction injection, HTTP observer, synthetic vote or alternate execution authority.
//! This is not four-process/machine, mainnet, full PQ consensus or TPS evidence.
//! The explicitly labelled admission-boundary subcase injects real signed
//! batches into the original consumer without a network/consensus tick, then
//! reopens the same genesis databases before the unchanged UDP closure below.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use crate::native_pipeline::business::fixed_supply::NOV_FIXED_SUPPLY_BASE_UNITS;
use crate::native_pipeline::consensus::crypto::AoemConsensusCrypto;
use crate::native_pipeline::service::config::test_asset_genesis::approve_manifest;
use config::uca_genesis::{
    UcaGenesisAccountV2, UcaGenesisGrantV2, UcaGenesisIdentityV2, UcaGenesisManifestV2,
    UcaInitializationConfigV2,
};
use novovm_adapter_api::uca_spend_policy::SuppliedAssetSpendGrantV2;
use novovm_consensus::round_bft::crypto::ConsensusSigner;
use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade, AoemExecOpenOptions};
use novovm_network::novorudp::object_sender::ObjectSendState;
use novovm_protocol::{
    native_query::{
        Answer, AssetReadIntentV2, AssetSpendStateViewV2, AssetState, Query, ReceiptState,
    },
    uca_spend::{
        spend_key_ref_v1, AssetAuthorityUpdateV2, AssetGrantConfigV2, AssetSpendIntentV2,
        SignedAssetSpendV2, MAX_SIGNED_SPEND_BYTES,
    },
    uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2},
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
        config.genesis.validators.iter().all(|v| {
            v.public_key != key.verifying_key().to_bytes()
                && v.transport_public_key != key.verifying_key().to_bytes()
        }),
        "client is validator"
    );
    let transport_public = read_key(
        &config.transport_key_file,
        config.key_storage,
        PurposeV1::Transport,
        config.genesis.chain_id,
    )?
    .verifying_key()
    .to_bytes();
    client::NativeApplicationClient::connect(
        UdpSocket::bind("127.0.0.1:0")?,
        key,
        NativePeer {
            key: transport_public,
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
    signed: &SignedAssetSpendV2,
    _payer: usize,
    _recipient: usize,
) -> Result<()> {
    let batch = Arc::new(AssetSpendBatchV2::from_raw(
        &[signed.encode()?],
        AssetSpendBatchLimitsV2 {
            transactions: 2,
            transaction_bytes: MAX_SIGNED_SPEND_BYTES,
            body_bytes: 2 * MAX_SIGNED_SPEND_BYTES,
        },
    )?);
    let prepared = client.prepare_asset_spend(batch)?;
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
    signed: &SignedAssetSpendV2,
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
    intent: &AssetSpendIntentV2,
    _payer: usize,
    _recipient: usize,
) -> Result<AssetSpendStateViewV2> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        ensure!(Instant::now() < deadline, "native UCA state timeout");
        if let Answer::AssetSpendState(AssetState::Present(view)) = query(
            nodes,
            client,
            Query::AssetSpendState {
                intent: AssetReadIntentV2::new(intent).map_err(anyhow::Error::msg)?,
            },
        )? {
            return Ok(*view);
        }
    }
}

fn fee(receipt: ReceiptState, height: u64, expected_nonce: u64) -> Result<u128> {
    if let ReceiptState::Finalized {
        success: true,
        nonce_after,
        charged_fee,
        height: actual,
        ..
    } = receipt
    {
        ensure!(
            actual == height && nonce_after == expected_nonce && charged_fee > 0,
            "wrong finalized height/nonce/fee"
        );
        Ok(charged_fee)
    } else {
        anyhow::bail!("unexpected UCA receipt {receipt:?}")
    }
}

fn publish(
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
    ingress: usize,
    signed: &SignedAssetSpendV2,
) -> Result<ReceiptState> {
    send(nodes, &mut clients[ingress], signed, 0, 0)?;
    let receipts = clients
        .iter_mut()
        .map(|client| receipt(nodes, client, signed))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        receipts.iter().all(|r| r == &receipts[0]),
        "asset lifecycle four-node receipts differ"
    );
    Ok(receipts[0])
}

fn account_views(
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
    alice: &AssetSpendIntentV2,
    bob: &AssetSpendIntentV2,
) -> Result<Vec<(AssetSpendStateViewV2, AssetSpendStateViewV2)>> {
    let views = clients
        .iter_mut()
        .map(|client| {
            Ok((
                state(nodes, client, alice, 0, 1)?,
                state(nodes, client, bob, 1, 0)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        views.iter().all(|v| v == &views[0]) && views[0].0.state_root == views[0].1.state_root,
        "asset lifecycle four-node authority/ledger roots differ"
    );
    Ok(views)
}

/// The original public ingress must reject unauthorized management/spending
/// without a candidate, fee, nonce, grant or authority mutation on ANY node.
fn reject_unchanged(
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
    ingress: usize,
    signed: &SignedAssetSpendV2,
    observations: (&AssetSpendIntentV2, &AssetSpendIntentV2),
) -> Result<()> {
    let before = account_views(nodes, clients, observations.0, observations.1)?;
    let baseline = match query(nodes, &mut clients[ingress], Query::Status)? {
        Answer::Status(status) => status.rejected_inputs,
        other => anyhow::bail!("asset lifecycle status {other:?}"),
    };
    send(nodes, &mut clients[ingress], signed, 0, 0)?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        ensure!(
            Instant::now() < deadline,
            "unauthorized lifecycle input not rejected"
        );
        if let Answer::Status(status) = query(nodes, &mut clients[ingress], Query::Status)? {
            ensure!(
                status.height == before[0].0.height && status.pending == 0,
                "unauthorized lifecycle input created state/work"
            );
            if status.rejected_inputs > baseline {
                break;
            }
        }
    }
    assert_eq!(
        account_views(nodes, clients, observations.0, observations.1)?,
        before,
        "authorization refusal changed authority, budget, nonce, balance or root"
    );
    Ok(())
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
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests/uca-native-services")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    std::fs::create_dir_all(&directory)?;
    let directory = config::native_path(&directory.canonicalize()?);
    let _credential = config::test_custody::TestCredentialGuard::new(&directory)?;
    let crypto = AoemConsensusCrypto::new(&runtime)?;
    let node_keys = (0..4)
        .map(|index| config::test_custody::generate_node_keys(&directory, 781, index, &crypto))
        .collect::<Result<Vec<_>>>()?;
    let keygen = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    // Independent: two managers, two spending keys, one shared identity, two
    // recovery keys, two replacement spending keys and a replacement manager.
    let keys = (0..10)
        .map(|_| {
            let pair = keygen.generate_keypair()?;
            Ok((pair.public_key, pair.expanded_secret_key))
        })
        .collect::<Result<Vec<_>>>()?;
    let manifest = UcaGenesisManifestV2 {
        genesis: GenesisConfig {
            chain_id: 781,
            validator_epoch: 1,
            protocol_commitment: config::protocol_commitment(),
            genesis_config_commitment: [0; 32],
            timestamp_unix_ms: 172_800_500,
            validators: (0..4)
                .map(|i| GenesisValidator {
                    public_key: node_keys[i].signer.public_key().to_vec(),
                    transport_public_key: node_keys[i].transport.verifying_key().to_bytes(),
                    weight: 1,
                })
                .collect(),
            allocations: vec![
                GenesisAllocation {
                    account_hex: "0b".repeat(20),
                    amount: "1000".into(),
                },
                // Explicit fixture reserve: issued, but no spend grant. This
                // is not an approved deployment allocation.
                GenesisAllocation {
                    account_hex: "fe".repeat(20),
                    amount: (NOV_FIXED_SUPPLY_BASE_UNITS - 1000).to_string(),
                },
            ],
            policy: policy(),
        },
        identities: vec![UcaGenesisIdentityV2 {
            uca_id: IDS[0].into(),
            identity_public_key: keys[4].0.clone(),
        }],
        accounts: (0..2)
            .map(|i| {
                Ok(UcaGenesisAccountV2 {
                    uca_id: IDS[0].into(),
                    asset_account: ADDRESSES[i].to_vec(),
                    management_public_key: keys[i].0.clone(),
                    recovery_public_key: Some(keys[i + 5].0.clone()),
                    grants: vec![UcaGenesisGrantV2 {
                        spend_public_key: keys[i + 2].0.clone(),
                        grant: SuppliedAssetSpendGrantV2 {
                            asset: 1,
                            account: ADDRESSES[i].to_vec(),
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
    let approval_keys = keys
        .iter()
        .map(|(p, s)| (p.as_slice(), s.as_slice()))
        .collect::<Vec<_>>();
    let approvals = approve_manifest(&runtime, &manifest, &approval_keys)?;
    let initial = manifest.compile(&runtime, domain, &approvals)?;
    let material = UcaInitializationConfigV2 {
        identities: manifest.identities.clone(),
        accounts: manifest.accounts.clone(),
        approvals,
        expected_signing_domain: domain,
        expected_initial_root: initial.state().root(),
        expected_final_commitment: initial.final_commitment(),
    };
    let mut genesis = manifest.genesis;
    genesis.genesis_config_commitment = domain;
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
            Ok(ResidentConfig {
                profile: config::ORIGINAL_PROFILE.into(),
                library: library.clone(),
                database: directory.join(format!("node-{i}.rocksdb")),
                rpc_addr: None,
                signing_key_file: node_keys[i].validator_path.clone(),
                transport_key_file: node_keys[i].transport_path.clone(),
                key_storage: config::NodeKeyStorage::ProtectedV1,
                udp_addr: addresses[i],
                peers: (0..4)
                    .filter(|j| *j != i)
                    .map(|j| NativePeerConfig {
                        public_key: node_keys[j].signer.public_key().to_vec(),
                        address: addresses[j],
                    })
                    .collect(),
                native_client_limit: 2,
                genesis: genesis.clone(),
                uca_initialization: Some(material.clone()),
                workers: 2,
                execution_backend: Default::default(),
                signature_backend: config::test_signature_backend()?,
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
    let clients = configs
        .iter()
        .map(|config| connect(config, validators.clone()))
        .collect::<Result<Vec<_>>>()?;
    let intent =
        |payer: usize, recipient: usize, amount: u128, epoch: u64| -> Result<AssetSpendIntentV2> {
            Ok(AssetSpendIntentV2 {
                asset: 1,
                authority_update: None,
                chain_id: genesis.chain_id,
                genesis: domain,
                payer: ADDRESSES[payer].to_vec(),
                authorization_epoch: epoch,
                spend_key_ref: spend_key_ref_v1(&keys[payer + 2].0)?,
                recipient: ADDRESSES[recipient].to_vec(),
                amount,
                max_fee: 50,
                nonce: 0,
                not_before_height: 1,
                expires_at_height: 100,
            })
        };
    let signer = runtime.asset_spend_signer_v2()?;
    let transfer = signer.sign(intent(0, 1, 100, 1)?, &keys[2].0, &keys[2].1)?;
    let make_batch = |spends: &[SignedAssetSpendV2]| -> Result<Arc<AssetSpendBatchV2>> {
        let raws = spends
            .iter()
            .map(|spend| spend.encode().map_err(anyhow::Error::msg))
            .collect::<Result<Vec<_>>>()?;
        Ok(Arc::new(AssetSpendBatchV2::from_raw(
            &raws,
            AssetSpendBatchLimitsV2 {
                transactions: 3,
                transaction_bytes: MAX_SIGNED_SPEND_BYTES,
                body_bytes: 3 * MAX_SIGNED_SPEND_BYTES,
            },
        )?))
    };
    let mut prefix = Vec::new();
    for nonce in 1..=1 {
        let mut next = intent(0, 1, 1, 1)?;
        next.nonce = nonce;
        prefix.push(signer.sign(next, &keys[2].0, &keys[2].1)?);
    }
    prefix.push(transfer.clone()); // Entire final chunk is an exact cache hit.
    let mut bad = signer.sign(intent(0, 1, 1, 1)?, &keys[2].0, &keys[2].1)?;
    bad.signature[0] ^= 1;
    println!("UCA admission-only overlap/cached-tail/next-body counterexample");
    nodes[ingress].assert_uca_overlap_body_boundaries_for_test(
        make_batch(std::slice::from_ref(&transfer))?,
        make_batch(&prefix)?,
        make_batch(&[bad.clone(), bad])?,
    )?;
    // Admission is volatile: no controller tick, candidate publication, reset,
    // deletion or fabricated authorization. Reopen the SAME original genesis
    // databases before the existing real UDP/respend/recovery acceptance.
    drop(clients);
    drop(nodes);
    let mut nodes = start(&configs, StartMode::Existing)?;
    let mut clients = configs
        .iter()
        .map(|config| connect(config, validators.clone()))
        .collect::<Result<Vec<_>>>()?;
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
    let first_fee = fee(first[0], 1, 1)?;
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
    let second_fee = fee(second[0], 2, 1)?;
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

    // Continue on these SAME recovered databases. Every management/recovery
    // transition is signed by its independent account-purpose key, submitted
    // through the original UDP client, confirmed by the four original journals
    // and observed through the existing typed published-state query.
    let refs = keys
        .iter()
        .map(|(public, _)| spend_key_ref_v1(public).map_err(anyhow::Error::msg))
        .collect::<Result<Vec<_>>>()?;
    let replacement_grant = |key: usize, budget: u128| AssetGrantConfigV2 {
        key_ref: refs[key],
        permits_nov_transfer: true,
        not_before_height: 1,
        expires_at_height: 100,
        max_debit_per_intent: 100,
        remaining_debit_budget: budget,
    };
    let management = |epoch: u64,
                      nonce: u64,
                      key: usize,
                      use_recovery: bool,
                      active: bool,
                      grants: Vec<AssetGrantConfigV2>|
     -> Result<SignedAssetSpendV2> {
        let mut action = intent(0, 0, 0, epoch)?;
        action.nonce = nonce;
        action.spend_key_ref = refs[key];
        action.authority_update = Some(AssetAuthorityUpdateV2 {
            use_recovery,
            next_epoch: epoch + 1,
            active,
            management_key_ref: refs[9],
            recovery_key_ref: Some(refs[5]),
            grants,
        });
        signer.sign(action, &keys[key].0, &keys[key].1)
    };
    let rotate = management(1, 1, 0, false, true, vec![replacement_grant(7, 200)])?;
    let rotated_receipt = publish(&mut nodes, &mut clients, ingress, &rotate)?;
    let rotate_fee = fee(rotated_receipt, 3, 2)?;
    let mut new_spend_intent = intent(0, 1, 10, 2)?;
    new_spend_intent.nonce = 2;
    new_spend_intent.spend_key_ref = refs[7];
    let new_spend = signer.sign(new_spend_intent, &keys[7].0, &keys[7].1)?;
    let rotated = account_views(&mut nodes, &mut clients, &new_spend.intent, &respend.intent)?;
    for (alice, bob) in &rotated {
        ensure!(
            alice.height == 3
                && alice.current_spend_epoch == 2
                && alice.account_active
                && alice.management_key_ref == refs[9]
                && alice.recovery_key_ref == Some(refs[5])
                && alice.next_nonce == 2
                && alice.grant_epoch == 2
                && alice.remaining_debit_budget == 200
                && alice.payer_balance == before[0].0.payer_balance - rotate_fee
                && bob.next_nonce == 1
                && bob.remaining_debit_budget == before[0].1.remaining_debit_budget
                && bob.payer_balance == before[0].1.payer_balance,
            "published manager rotation changed wrong account or consumed spend budget"
        );
    }
    let mut old_spend_intent = new_spend.intent.clone();
    old_spend_intent.spend_key_ref = refs[2];
    let revoked = signer.sign(old_spend_intent, &keys[2].0, &keys[2].1)?;
    reject_unchanged(
        &mut nodes,
        &mut clients,
        ingress,
        &revoked,
        (&new_spend.intent, &respend.intent),
    )?;
    // Obsolete manager, another asset's manager under the SAME UCA, and the
    // UCA identity key cannot acquire this concrete account's authority.
    for key in [0, 1, 4] {
        let wrong = management(2, 2, key, false, false, vec![])?;
        reject_unchanged(
            &mut nodes,
            &mut clients,
            ingress,
            &wrong,
            (&new_spend.intent, &respend.intent),
        )?;
    }
    let new_spend_receipt = publish(&mut nodes, &mut clients, ingress, &new_spend)?;
    let new_spend_fee = fee(new_spend_receipt, 4, 3)?;
    let spent = account_views(&mut nodes, &mut clients, &new_spend.intent, &respend.intent)?;
    for (alice, bob) in &spent {
        ensure!(
            alice.height == 4
                && alice.next_nonce == 3
                && alice.remaining_debit_budget == 200 - 10 - new_spend_fee
                && alice.payer_balance == rotated[0].0.payer_balance - 10 - new_spend_fee
                && bob.payer_balance == rotated[0].1.payer_balance + 10
                && bob.next_nonce == 1,
            "replacement key did not spend with continuous account nonce"
        );
    }
    let mut cross_account_intent = intent(1, 0, 1, 1)?;
    cross_account_intent.nonce = 1;
    cross_account_intent.spend_key_ref = refs[7];
    let cross_account = signer.sign(cross_account_intent, &keys[7].0, &keys[7].1)?;
    reject_unchanged(
        &mut nodes,
        &mut clients,
        ingress,
        &cross_account,
        (&new_spend.intent, &respend.intent),
    )?;
    let freeze = management(2, 3, 9, false, false, vec![])?;
    let freeze_receipt = publish(&mut nodes, &mut clients, ingress, &freeze)?;
    let freeze_fee = fee(freeze_receipt, 5, 4)?;
    let frozen = account_views(&mut nodes, &mut clients, &new_spend.intent, &respend.intent)?;
    for (alice, bob) in &frozen {
        ensure!(
            alice.height == 5
                && !alice.account_active
                && alice.current_spend_epoch == 3
                && alice.next_nonce == 4
                && alice.management_key_ref == refs[9]
                && alice.recovery_key_ref == Some(refs[5])
                && alice.payer_balance == spent[0].0.payer_balance - freeze_fee
                && alice.remaining_debit_budget == spent[0].0.remaining_debit_budget
                && bob.payer_balance == spent[0].1.payer_balance
                && bob.next_nonce == 1,
            "freeze authority/fees/nonce were not atomically published"
        );
    }
    let mut frozen_spend_intent = new_spend.intent.clone();
    frozen_spend_intent.authorization_epoch = 3;
    frozen_spend_intent.nonce = 4;
    let frozen_spend = signer.sign(frozen_spend_intent, &keys[7].0, &keys[7].1)?;
    reject_unchanged(
        &mut nodes,
        &mut clients,
        ingress,
        &frozen_spend,
        (&new_spend.intent, &respend.intent),
    )?;
    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    let mut nodes = start(&configs, StartMode::Existing)?;
    let mut clients = configs
        .iter()
        .map(|config| connect(config, validators.clone()))
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(
        account_views(&mut nodes, &mut clients, &new_spend.intent, &respend.intent)?,
        frozen,
        "cold restart lost the published frozen authority"
    );
    for client in &mut clients {
        assert_eq!(receipt(&mut nodes, client, &rotate)?, rotated_receipt);
        assert_eq!(receipt(&mut nodes, client, &new_spend)?, new_spend_receipt);
        assert_eq!(receipt(&mut nodes, client, &freeze)?, freeze_receipt);
    }
    // Recovery is the configured ASSET recovery key, never identity recovery,
    // another account's recovery key or an obsolete spending key.
    for key in [4, 6, 7] {
        let wrong = management(3, 4, key, true, true, vec![replacement_grant(8, 150)])?;
        reject_unchanged(
            &mut nodes,
            &mut clients,
            ingress,
            &wrong,
            (&new_spend.intent, &respend.intent),
        )?;
    }
    let recover = management(3, 4, 5, true, true, vec![replacement_grant(8, 150)])?;
    let recovery_receipt = publish(&mut nodes, &mut clients, ingress, &recover)?;
    let recovery_fee = fee(recovery_receipt, 6, 5)?;
    let mut recovered_spend_intent = intent(0, 1, 7, 4)?;
    recovered_spend_intent.nonce = 5;
    recovered_spend_intent.spend_key_ref = refs[8];
    let recovered_spend = signer.sign(recovered_spend_intent, &keys[8].0, &keys[8].1)?;
    let recovered = account_views(
        &mut nodes,
        &mut clients,
        &recovered_spend.intent,
        &respend.intent,
    )?;
    for (alice, bob) in &recovered {
        ensure!(
            alice.height == 6
                && alice.account_active
                && alice.current_spend_epoch == 4
                && alice.next_nonce == 5
                && alice.management_key_ref == refs[9]
                && alice.recovery_key_ref == Some(refs[5])
                && alice.grant_epoch == 4
                && alice.remaining_debit_budget == 150
                && alice.payer_balance == frozen[0].0.payer_balance - recovery_fee
                && bob.payer_balance == frozen[0].1.payer_balance
                && bob.next_nonce == 1,
            "asset recovery reset nonce, charged spend budget or changed another account"
        );
    }
    let mut obsolete_after_recovery = recovered_spend.intent.clone();
    obsolete_after_recovery.spend_key_ref = refs[7];
    let obsolete_after_recovery = signer.sign(obsolete_after_recovery, &keys[7].0, &keys[7].1)?;
    reject_unchanged(
        &mut nodes,
        &mut clients,
        ingress,
        &obsolete_after_recovery,
        (&recovered_spend.intent, &respend.intent),
    )?;
    let recovered_spend_receipt = publish(&mut nodes, &mut clients, ingress, &recovered_spend)?;
    let recovered_spend_fee = fee(recovered_spend_receipt, 7, 6)?;
    let lifecycle_before = account_views(
        &mut nodes,
        &mut clients,
        &recovered_spend.intent,
        &respend.intent,
    )?;
    let lifecycle_receipts = [
        (&rotate, rotated_receipt),
        (&new_spend, new_spend_receipt),
        (&freeze, freeze_receipt),
        (&recover, recovery_receipt),
        (&recovered_spend, recovered_spend_receipt),
    ];
    for (alice, bob) in &lifecycle_before {
        ensure!(
            alice.height == 7
                && alice.next_nonce == 6
                && alice.current_spend_epoch == 4
                && alice.account_active
                && alice.remaining_debit_budget == 150 - 7 - recovered_spend_fee
                && alice.payer_balance
                    == 923
                        - first_fee
                        - rotate_fee
                        - new_spend_fee
                        - freeze_fee
                        - recovery_fee
                        - recovered_spend_fee
                && bob.payer_balance == 77 - second_fee
                && bob.remaining_debit_budget == 1000 - 40 - second_fee
                && bob.next_nonce == 1,
            "durable lifecycle fees, nonce, balances or budgets differ"
        );
    }
    for (signed, expected) in &lifecycle_receipts {
        send(&mut nodes, &mut clients[ingress], signed, 0, 0)?;
        assert_eq!(
            receipt(&mut nodes, &mut clients[ingress], signed)?,
            *expected
        );
    }
    assert_eq!(
        account_views(
            &mut nodes,
            &mut clients,
            &recovered_spend.intent,
            &respend.intent
        )?,
        lifecycle_before,
        "published authority replay consumed nonce/fees or reset grant budget"
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
    assert_eq!(
        account_views(
            &mut nodes,
            &mut clients,
            &recovered_spend.intent,
            &respend.intent
        )?,
        lifecycle_before,
        "cold lifecycle authority, fees, nonce, balances or budgets differ"
    );
    for client in &mut clients {
        assert_eq!(receipt(&mut nodes, client, &transfer)?, first[0]);
        assert_eq!(receipt(&mut nodes, client, &respend)?, second[0]);
        for (signed, expected) in &lifecycle_receipts {
            assert_eq!(receipt(&mut nodes, client, signed)?, *expected);
        }
    }
    std::fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "real_services":4, "real_databases":4, "processes":1, "machines":1,
            "transport":"original NovoRUDP/APFL application and validator sockets", "single_submission_ingress":ingress,
            "http":false, "direct_transaction_injection":false, "first":first, "second":second,
            "state_before_and_after":before, "invalid_signature_rejected":true, "wrong_authorization_epoch_rejected":true,
            "published_replay_unchanged":true, "cold_restart":true, "TPS_acceptance":false, "production_activation":false,
            "asset_lifecycle": {
                "rotate":rotated_receipt, "replacement_key_spend":new_spend_receipt,
                "freeze":freeze_receipt, "recover":recovery_receipt, "recovered_key_spend":recovered_spend_receipt,
                "frozen_state_before_and_after_restart":frozen,
                "final_state_before_and_after_restart":lifecycle_before,
                "revoked_key_rejected":true, "old_manager_rejected":true,
                "same_identity_cross_account_spend_rejected":true,
                "identity_and_other_account_authority_rejected":true,
                "identity_and_other_account_recovery_rejected":true,
                "rejected_inputs_no_fee_nonce_or_authority_change":true,
                "original_journal_durable_queries":true, "published_management_replay_unchanged":true,
                "forced_crash_during_management_publish":false, "offline_management_catchup":false,
            },
        }))?,
    )?;
    drop(clients);
    for node in nodes {
        node.node.shutdown()?;
    }
    println!("NUAS original application -> four original services/DBs -> durable transfer/respend, manager and spend-key rotation, freeze/recovery, rejected cross-account/identity authority -> repeated cold state/receipt equality; no HTTP, no direct input, no forced publish crash, no offline management catchup, no TPS claim");
    Ok(())
}
