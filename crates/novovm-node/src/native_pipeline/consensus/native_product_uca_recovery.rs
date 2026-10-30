//! NUAS on the existing independent original-node recovery harness. No in-process
//! validator service, wallet replay to the laggard, or alternate publication path.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::{quote_and_settle, TransferFeeRequest};
use crate::native_pipeline::service::config::test_asset_genesis::approve_manifest;
use crate::native_pipeline::service::config::uca_genesis::{
    UcaGenesisAccountV2, UcaGenesisGrantV2, UcaGenesisIdentityV2, UcaGenesisManifestV2,
    UcaInitializationConfigV2,
};
use novovm_adapter_api::uca_spend_policy::SuppliedAssetSpendGrantV2;
use novovm_exec::{pq_signature::AoemMldsaParameterSet, AoemExecFacade, AoemExecOpenOptions};
use novovm_protocol::{
    native_query::{AssetReadIntentV2, AssetSpendStateViewV2, AssetState},
    uca_spend::{spend_key_ref_v1, AssetSpendIntentV2, SignedAssetSpendV2, MAX_SIGNED_SPEND_BYTES},
    uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2},
};

#[path = "native_product_uca_opening.rs"]
mod runtime_opening;

const ID: &str = "uca:process-owner";
const ADDRESS: [[u8; 20]; 2] = [[11; 20], [12; 20]];
const OFFLINE_HEIGHTS: u64 = 40;
const LARGE_BODY_ROWS: usize = 1024;
const OFFLINE_TRANSACTIONS: u64 = OFFLINE_HEIGHTS - 1 + LARGE_BODY_ROWS as u64;
const INITIAL: u128 = 1_000_000;

fn operator_call(
    nodes: &ProductNodes,
    config: &Path,
    action: &str,
    input: Option<&Path>,
) -> Result<Value> {
    let binary = nodes
        .binary
        .parent()
        .context("node binary directory")?
        .join(format!(
            "novovm-native-client{}",
            std::env::consts::EXE_SUFFIX
        ));
    ensure!(
        binary.is_file(),
        "build novovm-native-client beside the explicit node binary"
    );
    let mut command = std::process::Command::new(binary);
    command.arg(config).arg(action);
    if let Some(input) = input {
        command.arg(input);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "operator CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

// Exercise the operator consumer against actual child nodes, not a mock SDK.
fn operator_submit(
    nodes: &ProductNodes,
    index: usize,
    signed: &SignedAssetSpendV2,
) -> Result<Value> {
    use crate::native_pipeline::service::{client::operator, ResidentConfig};
    let node = ResidentConfig::load(&nodes.configs[index])?;
    let config_path = nodes.directory.join("operator-client.json");
    let transport_key_file = nodes.directory.join("test-only-operator-transport.key");
    fs::write(&transport_key_file, [202u8; 32])?;
    let config = operator::ClientConfig {
        chain_id: node.genesis.chain_id,
        genesis: node.genesis.genesis_config_commitment,
        protocol: node.genesis.protocol_commitment,
        validator_epoch: node.genesis.validator_epoch,
        validators: node.genesis.validators.clone(),
        node: crate::native_pipeline::service::NativePeerConfig {
            public_key: validator_key(index).public_key().to_vec(),
            address: node.udp_addr,
        },
        bind_addr: "127.0.0.1:0".parse()?,
        transport_key_file,
        timeout_ms: 15_000,
    };
    fs::write(&config_path, serde_json::to_vec_pretty(&config)?)?;
    let status = operator_call(nodes, &config_path, "status", None)?;
    ensure!(
        status["result"]["answer"]["Status"]["height"] == 0,
        "operator initial status"
    );
    let query_path = nodes.directory.join("operator-uca-state.nvquery");
    fs::write(
        &query_path,
        novovm_protocol::native_query::encode(novovm_protocol::native_query::Frame::Request(
            novovm_protocol::native_query::Request {
                session: [9; 32],
                sequence: 7,
                query: Query::AssetSpendState {
                    intent: AssetReadIntentV2::new(&signed.intent).map_err(anyhow::Error::msg)?,
                },
            },
        ))
        .map_err(anyhow::Error::msg)?,
    )?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let query = loop {
        ensure!(
            Instant::now() < deadline,
            "operator UCA state remained pending"
        );
        let observed = operator_call(nodes, &config_path, "query", Some(&query_path))?;
        if observed["result"]["answer"]["AssetSpendState"] != "Pending" {
            break observed;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    ensure!(
        query["result"]["answer"]["AssetSpendState"]["Present"]["payer_balance"]
            == INITIAL.to_string()
            && query["result"]["answer"]["AssetSpendState"]["Present"]["remaining_debit_budget"]
                == INITIAL.to_string(),
        "binary UCA query amount/budget parity"
    );
    let batch = AssetSpendBatchV2::from_raw(
        &[signed.encode()?],
        AssetSpendBatchLimitsV2 {
            transactions: 2,
            transaction_bytes: MAX_SIGNED_SPEND_BYTES,
            body_bytes: 2 * MAX_SIGNED_SPEND_BYTES,
        },
    )?;
    let batch_path = nodes.directory.join("operator-signed.nuapfl");
    let bytes = batch.encode()?;
    fs::write(&batch_path, &bytes)?;
    let submitted = operator_call(nodes, &config_path, "submit", Some(&batch_path))?;
    ensure!(
        submitted["result"]["finalized"] == false
            && submitted["result"]["kind"] == "transport_delivery_only",
        "delivery is not finality"
    );
    ensure!(
        fs::read(batch_path)? == bytes,
        "operator altered signed input"
    );
    Ok(json!({"status":status,"binary_query":query,"submission":submitted}))
}

fn send_uca(
    client: &mut NativeApplicationClient,
    signed: &SignedAssetSpendV2,
    _payer: usize,
) -> Result<()> {
    send_uca_batch(client, &[signed.encode()?])
}

fn send_uca_batch(client: &mut NativeApplicationClient, raw: &[Vec<u8>]) -> Result<()> {
    let codec = crate::native_pipeline::service::native_decode_limits(LARGE_BODY_ROWS, true);
    let batch = Arc::new(AssetSpendBatchV2::from_raw(
        raw,
        AssetSpendBatchLimitsV2 {
            transactions: codec.transactions,
            transaction_bytes: codec.transaction_bytes,
            body_bytes: codec.body_bytes,
        },
    )?);
    let prepared = client.prepare_asset_spend(batch)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut sent = false;
    loop {
        ensure!(
            Instant::now() < deadline,
            "NUAS transport delivery deadline"
        );
        let now = Instant::now();
        for completion in client
            .poll(
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
                now,
                64,
            )?
            .completed
        {
            if completion.body.id() == prepared.id() {
                ensure!(
                    completion.state == ObjectSendState::Delivered,
                    "NUAS transport expired"
                );
                return Ok(()); // Transport completion, never finality.
            }
        }
        if !sent {
            sent = client.try_send(&prepared, now)?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn state(
    client: &mut NativeApplicationClient,
    intent: &AssetSpendIntentV2,
    _payer: usize,
) -> Result<AssetSpendStateViewV2> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ensure!(Instant::now() < deadline, "NUAS typed state deadline");
        match query(
            client,
            Query::AssetSpendState {
                intent: AssetReadIntentV2::new(intent).map_err(anyhow::Error::msg)?,
            },
        )? {
            Answer::AssetSpendState(AssetState::Present(value)) => return Ok(*value),
            Answer::AssetSpendState(AssetState::Pending) => {}
            other => anyhow::bail!("NUAS authoritative state unavailable: {other:?}"),
        }
    }
}

fn checked_fee(
    receipt: &ReceiptState,
    signed: &SignedAssetSpendV2,
    payer: usize,
    genesis: &GenesisConfig,
    fees: &mut FeeState,
    payer_before: u128,
) -> Result<u128> {
    match receipt {
        ReceiptState::Finalized {
            success: true,
            nonce_after,
            charged_fee,
            height,
            ..
        } if *nonce_after == signed.intent.nonce + 1 => {
            let settled = quote_and_settle(
                &TransferFeeRequest {
                    tx_hash: Sha256::digest(signed.intent.signing_bytes()?).into(),
                    payer: ADDRESS[payer].into(),
                    recipient: ADDRESS[1 - payer].into(),
                    asset: "NOV".into(),
                    amount: signed.intent.amount,
                    pay_asset: "NOV".into(),
                    max_pay_amount: signed.intent.max_fee,
                    slippage_bps: 0,
                },
                &genesis.policy,
                fees,
                payer_before,
                u128::from(genesis.timestamp_unix_ms + height),
            )?;
            ensure!(settled.failure.is_none(), "NUAS oracle settlement rejected");
            let expected = payer_before - settled.payer_after;
            ensure!(
                *charged_fee == expected,
                "NUAS fee differs from original quotation: {charged_fee} != {expected}"
            );
            *fees = settled.after_fee_state;
            Ok(expected)
        }
        other => anyhow::bail!("NUAS success/nonce/fee mismatch {other:?}"),
    }
}

#[test]
#[ignore = "real four original-node processes, trusted AOEM, 40-height offline with 1024-row archive and interrupted NUAS replay; run alone"]
fn native_uca_process_late_validator_interrupted_catchup_respend_and_cold_recovery() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
            .context("explicit original node binary required")?,
    )
    .canonicalize()?;
    let library = library()?.canonicalize()?;
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == library,
        "keygen and node library differ"
    );
    let runtime = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
    let keygen = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let keys = (0..5)
        .map(|_| {
            let pair = keygen.generate_keypair()?;
            Ok((pair.public_key, pair.expanded_secret_key))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut base = genesis()?;
    base.allocations = vec![GenesisAllocation {
        account_hex: hex(&ADDRESS[0]),
        amount: INITIAL.to_string(),
    }];
    complete_test_supply(&mut base.allocations)?;
    base.genesis_config_commitment = base.derive_commitment()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-rpc")
        .join(format!(
            "uca-recovery-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory =
        crate::native_pipeline::service::config::native_path(&directory.canonicalize()?);
    let (mut nodes, set) = setup_with_genesis(&directory, binary, base.clone(), LARGE_BODY_ROWS)?;
    base.genesis_config_commitment = [0; 32];
    let manifest = UcaGenesisManifestV2 {
        genesis: base,
        identities: vec![UcaGenesisIdentityV2 {
            uca_id: ID.into(),
            identity_public_key: keys[4].0.clone(),
        }],
        accounts: (0..2)
            .map(|i| {
                Ok(UcaGenesisAccountV2 {
                    uca_id: ID.into(),
                    asset_account: ADDRESS[i].to_vec(),
                    management_public_key: keys[i].0.clone(),
                    recovery_public_key: None,
                    grants: vec![UcaGenesisGrantV2 {
                        spend_public_key: keys[i + 2].0.clone(),
                        grant: SuppliedAssetSpendGrantV2 {
                            asset: 1,
                            account: ADDRESS[i].to_vec(),
                            key_ref: spend_key_ref_v1(&keys[i + 2].0)?,
                            authorization_epoch: 1,
                            permits_nov_transfer: true,
                            revoked: false,
                            not_before_height: 1,
                            expires_at_height: 1000,
                            max_debit_per_intent: 1000,
                            remaining_debit_budget: INITIAL,
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
        pages: vec![],
        identities: manifest.identities.clone(),
        accounts: manifest.accounts.clone(),
        approvals,
        expected_signing_domain: domain,
        expected_initial_root: initial.state().root(),
        expected_final_commitment: initial.final_commitment(),
    };
    let mut configured_genesis = manifest.genesis;
    configured_genesis.genesis_config_commitment = domain;
    let missing = (0..4)
        .max_by_key(|index| validator_key(*index).public_key().to_vec())
        .unwrap();
    for (index, path) in nodes.configs.iter().enumerate() {
        let mut value: Value = serde_json::from_slice(&fs::read(path)?)?;
        value["rpc_addr"] = Value::Null;
        value["profile"] = json!(ORIGINAL_PROFILE);
        value["native_client_limit"] = json!(2);
        // A smaller local proposal target must still accept and recover the
        // common 1,024-row capacity selected by another validator.
        value["batch_size"] = json!(if index == missing {
            64
        } else {
            LARGE_BODY_ROWS
        });
        value["genesis"] = serde_json::to_value(&configured_genesis)?;
        value["uca_initialization"] = serde_json::to_value(&material)?;
        fs::write(path, serde_json::to_vec_pretty(&value)?)?;
    }
    let configs = nodes
        .configs
        .iter()
        .map(fs::read)
        .collect::<std::io::Result<Vec<_>>>()?;
    let key_files = nodes
        .configs
        .iter()
        .map(|path| {
            let config: ResidentConfig = serde_json::from_slice(&fs::read(path)?)?;
            Ok([
                (
                    config.signing_key_file.clone(),
                    fs::read(&config.signing_key_file)?,
                ),
                (
                    config.transport_key_file.clone(),
                    fs::read(&config.transport_key_file)?,
                ),
            ])
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let healthy = (0..4).filter(|i| *i != missing).collect::<Vec<_>>();
    let signer = runtime.asset_spend_signer_v2()?;
    let intent = |payer: usize, nonce, amount| -> Result<AssetSpendIntentV2> {
        Ok(AssetSpendIntentV2 {
            asset: 1,
            authority_update: None,
            recipient_creation: None,
            chain_id: CHAIN,
            genesis: domain,
            payer: ADDRESS[payer].to_vec(),
            authorization_epoch: 1,
            spend_key_ref: spend_key_ref_v1(&keys[payer + 2].0)?,
            recipient: ADDRESS[1 - payer].to_vec(),
            amount,
            max_fee: 50,
            nonce,
            not_before_height: 1,
            expires_at_height: 100,
        })
    };
    let mut evidence = json!({"completed":false,"scope":"four_original_node_processes_one_host_nuas_recovery",
        "real_processes":4,"real_machines":1,"http_listeners":0,"fixed_quorum":3,"shared_identity_distinct_asset_accounts":true,
        "missing_validator":missing,"offline_heights":OFFLINE_HEIGHTS,"mainnet":false,"tps_acceptance":false,
        "late_validator_local_proposal_target":64,"common_validation_rows":LARGE_BODY_ROWS,
        "offline_transactions":OFFLINE_TRANSACTIONS,"total_transactions":OFFLINE_TRANSACTIONS+1,
        "large_body_height":OFFLINE_HEIGHTS,"large_body_rows":LARGE_BODY_ROWS,
        "wallet_replay_to_laggard":false,"database_copy":false,"business_proof_verified":false,
        "binary_sha256":hex(&Sha256::digest(fs::read(&nodes.binary)?)),"library_sha256":hex(&Sha256::digest(fs::read(&library)?)),
        "offline_progress":[]});
    let report = directory.join("uca-recovery-report.json");
    eprintln!("NUAS process recovery evidence={}", directory.display());
    let started = Instant::now();
    let result = (|| -> Result<()> {
        for index in &healthy {
            nodes.start(*index, "create")?;
        }
        evidence["healthy_process_ids"] = json!(nodes
            .processes
            .iter()
            .map(|p| p.as_ref().map(Child::id))
            .collect::<Vec<_>>());
        let absent_since = Instant::now();
        let mut clients = healthy
            .iter()
            .map(|i| connect(&nodes, *i, set.clone()))
            .collect::<Result<Vec<_>>>()?;
        ready(&mut nodes, &mut clients)?;
        let mut hashes = Vec::new();
        let mut final_receipts = Vec::new();
        let mut alice_fees = 0u128;
        let mut fee_oracle = FeeState::default();
        let mut large_raw = Vec::new();
        let credited = u128::from(OFFLINE_TRANSACTIONS) * 100;
        ensure!(
            u128::from(OFFLINE_TRANSACTIONS) * 150 <= INITIAL,
            "original recovery funding cannot cover amount plus maximum fee"
        );
        for round in 0..OFFLINE_HEIGHTS {
            let rows = if round + 1 == OFFLINE_HEIGHTS {
                LARGE_BODY_ROWS
            } else {
                1
            };
            let first_nonce = hashes.len() as u64;
            let signed = (0..rows)
                .map(|row| {
                    signer.sign(
                        intent(0, first_nonce + row as u64, 100)?,
                        &keys[2].0,
                        &keys[2].1,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let raw = signed
                .iter()
                .map(SignedAssetSpendV2::encode)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let batch_hashes = signed
                .iter()
                .map(|signed| Ok(Sha256::digest(signed.intent.signing_bytes()?).into()))
                .collect::<Result<Vec<[u8; 32]>>>()?;
            if round == 0 {
                evidence["operator_client"] = operator_submit(&nodes, healthy[0], &signed[0])?;
            } else {
                send_uca_batch(&mut clients[0], &raw)?;
            }
            let batch = receipts(&mut nodes, &mut clients, &batch_hashes)?;
            ensure!(batch.len() == rows, "recovery input/receipt count mismatch");
            for (signed, receipt) in signed.iter().zip(&batch) {
                ensure!(
                    matches!(receipt, ReceiptState::Finalized {height,..} if *height == round+1),
                    "recovery input split across heights instead of one exact candidate"
                );
                alice_fees += checked_fee(
                    receipt,
                    signed,
                    0,
                    &configured_genesis,
                    &mut fee_oracle,
                    INITIAL - u128::from(signed.intent.nonce) * 100 - alice_fees,
                )?;
            }
            hashes.extend(batch_hashes);
            final_receipts.extend(batch);
            if rows == LARGE_BODY_ROWS {
                large_raw = raw;
            }
            let heads = recovery::wait_height(&mut nodes, &mut clients, round + 1)?;
            evidence["offline_progress"]
                .as_array_mut()
                .unwrap()
                .push(json!({"height":round+1,"transactions":rows,"head":heads[0]}));
            fs::write(&report, serde_json::to_vec_pretty(&evidence)?)?;
        }
        while absent_since.elapsed() < Duration::from_secs(31) {
            nodes.alive()?;
            std::thread::sleep(Duration::from_millis(20));
        }
        evidence["late_start_after_ms"] = json!(absent_since.elapsed().as_millis());
        let target = status(&mut clients[0])?;
        let mut late = connect(&nodes, missing, set.clone())?;
        nodes.start(missing, "create")?;
        evidence["late_process_id"] = json!(nodes.processes[missing].as_ref().unwrap().id());
        let deadline = Instant::now() + Duration::from_secs(90);
        let interrupted = loop {
            nodes.alive()?;
            let observed = recovery_status(&mut nodes, &mut late, deadline)?;
            if observed.height > 0 {
                ensure!(
                    observed.height == 1 && observed.height < target.height,
                    "catch-up observation missed height one; cannot claim the required interruption"
                );
                break observed;
            }
            ensure!(
                Instant::now() < deadline,
                "NUAS late validator failed to replay"
            );
        };
        recovery::stop_one(&mut nodes, missing)?;
        drop(late);
        let (pinned_key, pinned_outbox, crash) =
            recovery::interrupted_archive(&nodes, missing, interrupted, target.height, &set)?;
        evidence["catchup_crash"] = crash;
        nodes.start(missing, "existing")?;
        evidence["interrupted_restart_process_id"] =
            json!(nodes.processes[missing].as_ref().unwrap().id());
        clients.push(connect(&nodes, missing, set.clone())?);
        recovery::wait_height(&mut nodes, &mut clients, target.height)?;
        ensure!(
            receipts(&mut nodes, &mut clients, &hashes)? == final_receipts,
            "NUAS catch-up receipts differ"
        );
        let alice_intent = intent(0, OFFLINE_TRANSACTIONS, 1)?;
        for client in &mut clients {
            let view = state(client, &alice_intent, 0)?;
            ensure!(
                view.next_nonce == OFFLINE_TRANSACTIONS
                    && view.payer_balance == INITIAL - credited - alice_fees
                    && view.remaining_debit_budget == INITIAL - credited - alice_fees
                    && view.recipient_balance == credited,
                "replay economic state {view:?}"
            );
        }
        // Already-established same validator must reconnect again. Recipients
        // spend actual credited NOV while it is absent; no alternate wallet feed.
        recovery::stop_one(&mut nodes, missing)?;
        clients.pop();
        let respend = signer.sign(intent(1, 0, 40)?, &keys[3].0, &keys[3].1)?;
        let id = Sha256::digest(respend.intent.signing_bytes()?).into();
        send_uca(&mut clients[0], &respend, 1)?;
        let respend_receipts = receipts(&mut nodes, &mut clients, &[id])?;
        let bob_fee = checked_fee(
            &respend_receipts[0],
            &respend,
            1,
            &configured_genesis,
            &mut fee_oracle,
            credited,
        )?;
        hashes.push(id);
        final_receipts.extend(respend_receipts);
        nodes.start(missing, "existing")?;
        clients.push(connect(&nodes, missing, set.clone())?);
        let final_heads = recovery::wait_height(&mut nodes, &mut clients, OFFLINE_HEIGHTS + 1)?;
        ensure!(
            receipts(&mut nodes, &mut clients, &hashes)? == final_receipts,
            "respend archive mismatch"
        );
        let mut states = Vec::new();
        for client in &mut clients {
            let alice = state(client, &alice_intent, 0)?;
            let bob = state(client, &intent(1, 1, 1)?, 1)?;
            ensure!(
                alice.state_root == final_heads[0].state_root
                    && bob.state_root == alice.state_root
                    && alice.next_nonce == OFFLINE_TRANSACTIONS
                    && bob.next_nonce == 1
                    && alice.remaining_debit_budget == INITIAL - credited - alice_fees
                    && bob.remaining_debit_budget == INITIAL - 40 - bob_fee
                    && alice.payer_balance == INITIAL - credited + 40 - alice_fees
                    && bob.payer_balance == credited - 40 - bob_fee
                    && alice.recipient_balance == bob.payer_balance
                    && bob.recipient_balance == alice.payer_balance,
                "NUAS balances/nonce/budgets differ {alice:?} {bob:?}"
            );
            states.push((alice, bob));
        }
        ensure!(
            states.iter().all(|s| *s == states[0]),
            "four UCA views differ"
        );
        evidence["final_heads"] = json!(final_heads);
        evidence["final_states"] = json!(states);
        evidence["receipts"] = json!(final_receipts);
        nodes.stop_all()?;
        // Read the ORIGINAL stopped stores at the verified final root. Compare
        // every canonical fee/policy page, including absent tails, with the
        // independent ordered oracle; never install this oracle into the ledger.
        let fee_records = fee_record_changes(&configured_genesis.policy, &fee_oracle)?;
        ensure!(
            large_raw.len() == LARGE_BODY_ROWS,
            "large recovery body was not submitted"
        );
        let mut large_archives = Vec::new();
        for index in 0..4 {
            let store = recovery::open_stopped_store(&nodes, index)?;
            for change in &fee_records {
                let (key, expected) = match change {
                    StateChange::Put { key, value } => (key, Some(value.clone())),
                    StateChange::Delete { key } => (key, None),
                };
                ensure!(
                    crate::native_pipeline::state::tree::read_state_value(
                        &store,
                        final_heads[0].state_root,
                        key
                    )? == expected,
                    "node {index} durable fee page differs from oracle"
                );
            }
            // Verify the actual stopped archive, including the laggard that
            // received no wallet inputs. Row count alone must not turn 1024
            // small candidates into a claimed multi-megabyte recovery body.
            let bytes = store
                .read_metadata(&[MetaKey::ChainBlock {
                    height: OFFLINE_HEIGHTS,
                }])?
                .values
                .pop()
                .flatten()
                .context("large NUAS archive height missing")?;
            let record = crate::native_pipeline::consensus::chain::ChainRecord::decode(&bytes)?;
            let stored = store
                .recover(record.candidate_id())?
                .context("large NUAS archived candidate missing")?;
            ensure!(
                record.point().height == OFFLINE_HEIGHTS
                    && stored.raw_transactions() == large_raw.as_slice(),
                "node {index} large archive changed input bytes/order"
            );
            let codec =
                crate::native_pipeline::service::native_decode_limits(LARGE_BODY_ROWS, true);
            let batch = AssetSpendBatchV2::from_raw(
                stored.raw_transactions(),
                AssetSpendBatchLimitsV2 {
                    transactions: codec.transactions,
                    transaction_bytes: codec.transaction_bytes,
                    body_bytes: codec.body_bytes,
                },
            )?;
            let apfl = batch.encode()?;
            ensure!(
                batch.len() == LARGE_BODY_ROWS
                    && batch.canonical_bytes() > 512 * 1024
                    && apfl.len() > 512 * 1024,
                "large recovery body did not cross the former message limit"
            );
            large_archives.push(json!({
                "node":index,"height":OFFLINE_HEIGHTS,"transactions":batch.len(),
                "canonical_expanded_bytes":batch.canonical_bytes(),"apfl_encoded_bytes":apfl.len(),
                "apfl_sha256":hex(&Sha256::digest(&apfl)),"exact_submitted_bytes":true,
            }));
        }
        evidence["large_body_archives"] = json!(large_archives);
        evidence["durable_fee_oracle"] = json!(fee_oracle);
        clients.clear();
        {
            let store = recovery::open_stopped_store(&nodes, missing)?;
            ensure!(
                store.read_metadata(&[pinned_key])?.values == vec![Some(pinned_outbox)],
                "signing outbox rewritten"
            );
        }
        for index in 0..4 {
            nodes.start(index, "existing")?;
        }
        clients = (0..4)
            .map(|i| connect(&nodes, i, set.clone()))
            .collect::<Result<Vec<_>>>()?;
        recovery::wait_height(&mut nodes, &mut clients, OFFLINE_HEIGHTS + 1)?;
        ensure!(
            receipts(&mut nodes, &mut clients, &hashes)? == final_receipts,
            "cold receipts differ"
        );
        for client in &mut clients {
            ensure!(
                (
                    state(client, &alice_intent, 0)?,
                    state(client, &intent(1, 1, 1)?, 1)?
                ) == states[0],
                "cold UCA state differs"
            );
        }
        ensure!(
            nodes
                .configs
                .iter()
                .map(fs::read)
                .collect::<std::io::Result<Vec<_>>>()?
                == configs,
            "config or authority changed"
        );
        for (path, original) in &key_files {
            ensure!(
                fs::read(path)? == *original,
                "validator or transport key changed during recovery"
            );
        }
        evidence["runtime_account_opening"] = json!({"completed":false});
        runtime_opening::run(
            &mut nodes,
            &mut clients,
            set.clone(),
            &runtime,
            (&keys[2].0, &keys[2].1),
            alice_intent,
            &mut evidence["runtime_account_opening"],
        )?;
        nodes.stop_all()?;
        evidence["same_config_and_signing_outbox_preserved"] = json!(true);
        evidence["cold_reopen_verified"] = json!(true);
        evidence["completed"] = json!(true);
        Ok(())
    })();
    evidence["elapsed_ms"] = json!(started.elapsed().as_millis());
    if let Err(error) = &result {
        evidence["failure"] = json!(format!("{error:#}"));
    }
    fs::write(&report, serde_json::to_vec_pretty(&evidence)?)?;
    result
}
