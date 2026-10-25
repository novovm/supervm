//! NUAS workload on the original process fixture and native load observer.
//! Only workload construction differs from the retained V3 comparison. Neither
//! this test nor its oracle executes/publishes business effects for the nodes.
use super::*;
use crate::native_pipeline::{
    business::{
        direct_nov_fee::{quote_and_settle, FeeState},
        nov_transfer_batch::fee_record_changes,
    },
    consensus::chain::ChainRecord,
    service::config::uca_genesis::{
        UcaGenesisAccountV1, UcaGenesisApprovalV1, UcaGenesisGrantV1, UcaGenesisManifestV1,
        UcaInitializationConfigV1, INITIALIZATION_CONTEXT_V1,
    },
};
use novovm_adapter_api::uca_spend_policy::SuppliedSpendGrantV1;
use novovm_exec::pq_signature::AoemMldsaParameterSet;
use novovm_protocol::{
    native_query::{UcaReadIntent, UcaSpendStateView, UcaState},
    uca_spend::{spend_key_ref_v1, SignedUcaSpendV1, UcaSpendIntentV1, MAX_SIGNED_SPEND_BYTES},
    uca_spend_batch::{UcaSpendBatch, UcaSpendBatchLimits},
};
use std::collections::BTreeMap;

const CONFIGURED_BATCH: usize = 1024;
// Preserve the existing 32-account initialization bound, including recipient.
const SENDERS: usize = 16;
const FUNDING: u128 = 1_000_000;
const CANONICAL_BODY_BYTES: usize = 384 * 1024;

fn account_bytes(index: usize) -> [u8; 20] {
    let mut value = [0x6b; 20];
    value[12..].copy_from_slice(&(index as u64).to_be_bytes());
    value
}

fn account_name(index: usize) -> String {
    format!("uca:native-load-{index}")
}

fn progress_matches_final_head(progress: &Value, head: Status) -> bool {
    progress.pointer("/head/height") == Some(&json!(head.height))
        && progress.pointer("/head/block_hash") == Some(&json!(head.block_hash))
        && progress.pointer("/head/state_root") == Some(&json!(head.state_root))
        && progress.pointer("/head/receipt_batch_commitment")
            == Some(&json!(head.receipt_batch_commitment))
        && progress.pointer("/head/state_version") == Some(&json!(head.state_version))
        && progress.get("rpc_indexed_height").and_then(Value::as_u64) >= Some(head.height)
        && progress.get("recovery_in_progress") == Some(&json!(false))
        && progress.get("projection_error") == Some(&Value::Null)
}

// Consume the existing tick writer only after the transaction stopwatch stops.
// Its fs::write can be observed mid-write; incomplete JSON is retried within one
// shared bound, never treated as zero work or a reason to change node timeouts.
fn collect_final_progress(nodes: &mut ProductNodes, head: Status) -> Result<Value> {
    if std::env::var("NOVOVM_NATIVE_LOAD_DIAGNOSTICS").as_deref() != Ok("1") {
        return Ok(json!({"status":"NOT_ENABLED","nodes":null}));
    }
    let started = Instant::now();
    let deadline = started + Duration::from_secs(15);
    let paths = (0..4)
        .map(|index| {
            nodes
                .directory
                .join(format!("create-{index}.progress.json"))
        })
        .collect::<Vec<_>>();
    let mut snapshots = vec![Value::Null; 4];
    let mut errors: Vec<Option<String>> = vec![None; 4];
    let mut matched = [false; 4];
    loop {
        nodes.alive()?;
        for index in 0..4 {
            if matched[index] {
                continue;
            }
            match fs::read(&paths[index]) {
                Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                    Ok(snapshot) => {
                        matched[index] = progress_matches_final_head(&snapshot, head);
                        snapshots[index] = snapshot;
                        errors[index] = None;
                    }
                    Err(error) => errors[index] = Some(format!("progress JSON: {error}")),
                },
                Err(error) => errors[index] = Some(format!("progress read: {error}")),
            }
        }
        if matched.iter().all(|value| *value) || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(json!({
        "status":if matched.iter().all(|value| *value) {"COMPLETE"} else {"INCOMPLETE"},
        "captured_after_transaction_window":true,
        "included_in_transaction_wall":false,
        "shared_wait_limit_seconds":15,
        "capture_elapsed_seconds":started.elapsed().as_secs_f64(),
        "scope":"existing local process-lifetime tick snapshots including startup and post-load queries; nested/concurrent stage times are not additive critical-path shares; INCOMPLETE forbids complete attribution but does not alter economic assertions",
        "nodes":(0..4).map(|index| json!({
            "node_index":index,"path":paths[index],"matched_final_head":matched[index],
            "last_read_error":errors[index],"snapshot":snapshots[index]
        })).collect::<Vec<_>>()
    }))
}

fn read_uca_state(
    client: &mut NativeApplicationClient,
    intent: &UcaSpendIntentV1,
    payer: usize,
) -> Result<UcaSpendStateView> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ensure!(Instant::now() < deadline, "NUAS load state query deadline");
        match query(
            client,
            Query::UcaSpendState {
                intent: UcaReadIntent::new(intent).map_err(anyhow::Error::msg)?,
                payer_hint: AccountId::Bytes20(account_bytes(payer)),
                recipient_hint: AccountId::Bytes20(account_bytes(SENDERS)),
            },
        )? {
            Answer::UcaSpendState(UcaState::Present(value)) => return Ok(value),
            Answer::UcaSpendState(UcaState::Pending) => {}
            other => bail!("NUAS load authoritative state unavailable: {other:?}"),
        }
    }
}

fn stopped_store(nodes: &ProductNodes, index: usize) -> Result<CandidateStore> {
    ensure!(
        nodes.processes[index].is_none(),
        "NUAS audit requires stopped signer"
    );
    let config = ResidentConfig::load(&nodes.configs[index])?;
    CandidateStore::open(
        StoreConfig {
            library: config.library,
            database: config.database,
            domain: StorageDomain {
                chain_id: config.genesis.chain_id,
                genesis_config_commitment: config.genesis.genesis_config_commitment,
                protocol_commitment: config.genesis.protocol_commitment,
            },
            storage: StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        },
        OpenMode::Existing,
    )
}

// Read the actual committed order, not the wallet's submission order: different
// independent senders may be selected in another order by the existing builder.
fn audit_fee_pages(
    nodes: &ProductNodes,
    head: Status,
    raw: &[Vec<u8>],
    observations: &[Observation],
) -> Result<Value> {
    let config = ResidentConfig::load(&nodes.configs[0])?;
    let by_hash: BTreeMap<_, _> = observations
        .iter()
        .enumerate()
        .map(|(i, r)| (r.hash, i))
        .collect();
    ensure!(by_hash.len() == raw.len(), "NUAS workload hash collision");
    let mut seen = BTreeSet::new();
    let mut actual_batch_sizes = Vec::new();
    let mut fees = FeeState::default();
    let mut balances = [FUNDING; SENDERS];
    let mut nonces = [0u64; SENDERS];
    let store = stopped_store(nodes, 0)?;
    for height in 1..=head.height {
        let record = store
            .read_metadata(&[MetaKey::ChainBlock { height }])?
            .values
            .pop()
            .flatten()
            .context("NUAS load archive height missing")?;
        let record = ChainRecord::decode(&record)?;
        let stored = store
            .recover(record.candidate_id())?
            .context("NUAS archived candidate missing")?;
        actual_batch_sizes.push(stored.raw_transactions().len());
        for bytes in stored.raw_transactions() {
            let signed = SignedUcaSpendV1::decode(bytes)?;
            let hash: Hash = Sha256::digest(signed.intent.signing_bytes()?).into();
            let index = *by_hash
                .get(&hash)
                .context("archive contains unsubmitted NUAS")?;
            ensure!(
                seen.insert(hash) && bytes == &raw[index],
                "NUAS archive changed or duplicated input"
            );
            let payer = index % SENDERS;
            ensure!(
                signed.intent.account_id == account_name(payer)
                    && signed.intent.recipient_uca == account_name(SENDERS)
                    && signed.intent.nonce == nonces[payer],
                "NUAS archive payer/nonce order"
            );
            let settled = quote_and_settle(
                &TransferFeeRequest {
                    tx_hash: hash,
                    payer: account_bytes(payer).into(),
                    recipient: account_bytes(SENDERS).into(),
                    asset: "NOV".into(),
                    amount: signed.intent.amount,
                    pay_asset: "NOV".into(),
                    max_pay_amount: signed.intent.max_fee,
                    slippage_bps: 0,
                },
                &config.genesis.policy,
                &fees,
                balances[payer],
                u128::from(config.genesis.timestamp_unix_ms + height),
            )?;
            ensure!(settled.failure.is_none(), "NUAS load fee oracle rejected");
            let charged = balances[payer] - settled.payer_after;
            ensure!(
                matches!(observations[index].receipt,
                Some(ReceiptState::Finalized { success: true, nonce_after, charged_fee, height: h, .. })
                    if nonce_after == nonces[payer] + 1 && charged_fee == charged && h == height),
                "NUAS archive and final receipt economics differ"
            );
            balances[payer] = settled
                .payer_after
                .checked_sub(signed.intent.amount)
                .context("NUAS oracle amount underflow")?;
            nonces[payer] += 1;
            fees = settled.after_fee_state;
        }
    }
    drop(store);
    ensure!(
        seen.len() == raw.len(),
        "NUAS archive omitted finalized input"
    );
    let expected = fee_record_changes(&config.genesis.policy, &fees)?;
    for index in 0..4 {
        let store = stopped_store(nodes, index)?;
        for change in &expected {
            let (key, value) = match change {
                StateChange::Put { key, value } => (key, Some(value.clone())),
                StateChange::Delete { key } => (key, None),
            };
            ensure!(
                crate::native_pipeline::state::tree::read_state_value(
                    &store,
                    head.state_root,
                    key
                )? == value,
                "NUAS node {index} durable fee page differs from ordered oracle"
            );
        }
    }
    Ok(
        json!({"actual_candidate_rows":actual_batch_sizes,"fee_state":fees,
        "archived_transactions":seen.len(),"oracle_sender_balances":balances,
        "oracle_sender_nonces":nonces,"all_four_fee_pages_match":true}),
    )
}

#[test]
#[ignore = "real four original nodes; bounded ML-DSA-65 NUAS load, all receipts and cold economics; run alone"]
fn native_uca_load_final_receipts_and_cold_economics() -> Result<()> {
    let count = env_count("NOVOVM_NATIVE_UCA_LOAD_TRANSACTIONS", 1024, 4096)?;
    let frame_rows = env_count("NOVOVM_NATIVE_UCA_FRAME_ROWS", 32, 64)?;
    ensure!(
        count % SENDERS == 0 && count % frame_rows == 0,
        "NUAS load must contain whole sender rounds and wire frames"
    );
    let rounds = count / SENDERS;
    let timeout =
        Duration::from_secs(env_count("NOVOVM_NATIVE_LOAD_TIMEOUT_SECONDS", 600, 3600)? as u64);
    let binary = fs::canonicalize(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
            .context("explicit original node binary required")?,
    )?;
    let library = library()?.canonicalize()?;
    ensure!(
        aoem_bindings::default_host_dll_path().canonicalize()? == library,
        "NUAS keygen and node AOEM library differ"
    );
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-load")
        .join(format!(
            "uca-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory =
        crate::native_pipeline::service::config::native_path(&directory.canonicalize()?);
    eprintln!("NUAS native load artifacts={}", directory.display());
    let environment = [
        "AOEM_GPU_AUTO_THRESHOLD_ENFORCE",
        "AOEM_MLDSA_GPU_NTT_PREPASS",
        "AOEM_MLDSA_VERIFY_BATCH_PAR_MIN",
        "AOEM_SEMANTIC_GRAPH_V2_WORKERS",
        "RAYON_NUM_THREADS",
        "NOVOVM_PROFILE_STAGES",
        "NOVOVM_NATIVE_LOAD_DIAGNOSTICS",
        "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS",
        "NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND",
    ]
    .into_iter()
    .map(|name| (name, std::env::var(name).ok()))
    .collect::<BTreeMap<_, _>>();
    let mut evidence = json!({"scope":"one_host_four_original_processes_bounded_nuas_load",
        "transactions":count,"configured_candidate_batch_limit":CONFIGURED_BATCH,"wire_frame_rows":frame_rows,
        "senders":SENDERS,"window":WINDOW,"sdk_io_quantum":IO_QUANTUM,"canonical_body_budget":CANONICAL_BODY_BYTES,
        "wire_message_budget":MAX_MESSAGE,"query_batch":env_count("NOVOVM_NATIVE_QUERY_BATCH",1,MAX_RECEIPTS)?,
        "backend":test_execution_backend()?,"binary":binary,"binary_sha256":hex(&Sha256::digest(fs::read(&binary)?)),
        "aoem_sha256":hex(&Sha256::digest(fs::read(&library)?)),"real_processes":4,"real_machines":1,
        "asset_signature":"ML-DSA-65","transport":"native_novorudp_apfl","http_listeners":0,
        "full_65536_workload":false,"static_intent_window":[1,257],"rolling_signature_65536":"NOT_EXECUTED",
        "input_identity":"fresh generated per invocation; same logical workload is not byte-identical input",
        "signature_generation_timed":false,"system_capacity_claim":false,"business_proof_verified":false,
        "diagnostics_enabled":std::env::var("NOVOVM_PROFILE_STAGES").as_deref()==Ok("1"),
        "progress_reports_enabled":std::env::var("NOVOVM_NATIVE_LOAD_DIAGNOSTICS").as_deref()==Ok("1"),
        "environment":environment,
        "environment_scope":"explicit inherited settings; null means unset, not an effective AOEM default or device identity",
        "final_local_progress":{"status":"NOT_COLLECTED","nodes":null},
        "unmeasured":{"status":"NOT_MEASURED","physical_network_bytes":null,
            "network_retransmissions":null,"peak_memory_bytes":null,
            "scope":"logical signed-input and application-object sizes are not physical wire traffic or retransmission counts"},
        "cold_recovery_pass":false,"full_fee_state_audited":false});
    let mut observations = Vec::new();
    let mut counts = Counts::default();
    let mut measured = None;
    let mut timed_attempt = None;
    let mut phase = "prepare approved NUAS fixture and signed workload";
    let result = (|| -> Result<()> {
        let runtime = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
        let keygen = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
        let keys = (0..2 * (SENDERS + 1))
            .map(|_| {
                let pair = keygen.generate_keypair()?;
                Ok((pair.public_key, pair.expanded_secret_key))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut genesis = genesis()?;
        genesis.allocations = (0..SENDERS)
            .map(|i| GenesisAllocation {
                account_hex: hex(&account_bytes(i)),
                amount: FUNDING.to_string(),
            })
            .collect();
        complete_test_supply(&mut genesis.allocations)?;
        genesis.genesis_config_commitment = genesis.derive_commitment()?;
        let (mut nodes, set) = setup_with_genesis(
            &directory,
            binary.clone(),
            genesis.clone(),
            CONFIGURED_BATCH,
        )?;
        genesis.genesis_config_commitment = [0; 32];
        let manifest = UcaGenesisManifestV1 {
            genesis,
            accounts: (0..=SENDERS)
                .map(|i| {
                    Ok(UcaGenesisAccountV1 {
                        uca_id: account_name(i),
                        asset_account: account_bytes(i).to_vec(),
                        management_public_key: keys[i].0.clone(),
                        grants: vec![UcaGenesisGrantV1 {
                            spend_public_key: keys[SENDERS + 1 + i].0.clone(),
                            grant: SuppliedSpendGrantV1 {
                                account_id: account_name(i),
                                key_ref: spend_key_ref_v1(&keys[SENDERS + 1 + i].0)?,
                                authorization_epoch: 1,
                                permits_nov_transfer: true,
                                revoked: false,
                                not_before_height: 1,
                                expires_at_height: 1000,
                                max_debit_per_intent: 1000,
                                remaining_debit_budget: FUNDING,
                            },
                        }],
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };
        let domain = manifest.signing_domain()?;
        let manager = runtime.mldsa_signer_v1(AoemMldsaParameterSet::MlDsa65)?;
        let canonical = manifest.canonical_bytes()?;
        let approvals = (0..=SENDERS)
            .map(|i| {
                Ok(UcaGenesisApprovalV1 {
                    uca_id: account_name(i),
                    signature: manager.sign(
                        &keys[i].0,
                        &keys[i].1,
                        &canonical,
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
        for path in &nodes.configs {
            let mut config: Value = serde_json::from_slice(&fs::read(path)?)?;
            config["rpc_addr"] = Value::Null;
            config["profile"] = json!(ORIGINAL_PROFILE);
            config["native_client_limit"] = json!(2);
            config["genesis"] = serde_json::to_value(&genesis)?;
            config["uca_initialization"] = serde_json::to_value(&material)?;
            fs::write(path, serde_json::to_vec_pretty(&config)?)?;
        }
        evidence["signing_domain"] = json!(hex(&domain));
        evidence["initial_root"] = json!(hex(&material.expected_initial_root));
        evidence["genesis_commitment"] = json!(hex(&material.expected_final_commitment));
        let signer = runtime.uca_spend_signer_v1()?;
        let make_intent = |payer: usize, nonce: u64| -> Result<UcaSpendIntentV1> {
            Ok(UcaSpendIntentV1 {
                chain_id: CHAIN,
                genesis: domain,
                account_id: account_name(payer),
                authorization_epoch: 1,
                spend_key_ref: spend_key_ref_v1(&keys[SENDERS + 1 + payer].0)?,
                recipient_uca: account_name(SENDERS),
                amount: 1,
                max_fee: 50,
                nonce,
                not_before_height: 1,
                expires_at_height: 257,
            })
        };
        let fee = quote_transfer(
            &TransferFeeRequest {
                tx_hash: [0; 32],
                payer: account_bytes(0).into(),
                recipient: account_bytes(SENDERS).into(),
                asset: "NOV".into(),
                amount: 1,
                pay_asset: "NOV".into(),
                max_pay_amount: 50,
                slippage_bps: 0,
            },
            &genesis.policy,
            u128::from(genesis.timestamp_unix_ms + 1),
        )??
        .nov_amount;
        let debit = fee.checked_add(1).context("NUAS fee overflow")?;
        let mut raw = Vec::with_capacity(count);
        let mut workload_hash = Sha256::new();
        workload_hash.update(b"novovm/nuas-load/ordered-signed-inputs/v1\0");
        for nonce in 0..rounds {
            for payer in 0..SENDERS {
                let key = &keys[SENDERS + 1 + payer];
                let signed = signer.sign(make_intent(payer, nonce as u64)?, &key.0, &key.1)?;
                let hash = Sha256::digest(signed.intent.signing_bytes()?).into();
                let bytes = signed.encode()?;
                workload_hash.update((bytes.len() as u64).to_be_bytes());
                workload_hash.update(&bytes);
                observations.push(Observation {
                    hash,
                    first_submit_us: None,
                    finalized_us: [None; 4],
                    receipt: None,
                    expected_nonce: nonce as u64 + 1,
                    expected_fee: fee,
                });
                raw.push(bytes);
            }
        }
        evidence["signed_workload_sha256"] = json!(hex(&workload_hash.finalize()));
        evidence["canonical_signed_input_bytes"] = json!(raw.iter().map(Vec::len).sum::<usize>());
        let ingress = (0..4)
            .find(|i| {
                Validator::new(validator_key(*i).public_key(), 1)
                    .unwrap()
                    .id()
                    != set.leader(1, 0).unwrap()
            })
            .context("NUAS nonleader ingress missing")?;
        evidence["single_submission_node"] = json!(ingress);
        evidence["first_proposer_is_submission_node"] = json!(false);
        phase = "start four original nodes";
        for i in 0..4 {
            nodes.start(i, "create")?;
        }
        let mut clients = (0..4)
            .map(|i| client(&nodes, i, set.clone(), CONFIGURED_BATCH))
            .collect::<Result<Vec<_>>>()?;
        let initial = ready(&mut nodes, &mut clients)?;
        ensure!(
            initial.iter().all(|s| s.height == 0),
            "NUAS load requires fresh genesis"
        );
        evidence["startup_status_before_apfl_preparation"] = json!(initial);
        for endpoint in &nodes.endpoints {
            let address: SocketAddr = endpoint.trim_start_matches("http://").parse()?;
            ensure!(
                std::net::TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_err(),
                "NUAS load HTTP listener unexpectedly open"
            );
        }
        let inputs = raw
            .chunks(frame_rows)
            .enumerate()
            .map(|(batch, rows)| {
                let hints = (0..rows.len())
                    .map(|i| {
                        (
                            account_bytes((batch * frame_rows + i) % SENDERS).to_vec(),
                            account_bytes(SENDERS).to_vec(),
                        )
                    })
                    .collect::<Vec<_>>();
                clients[ingress].prepare_uca(Arc::new(UcaSpendBatch::from_raw(
                    rows,
                    &hints,
                    UcaSpendBatchLimits {
                        transactions: frame_rows,
                        transaction_bytes: MAX_SIGNED_SPEND_BYTES,
                        body_bytes: CANONICAL_BODY_BYTES,
                    },
                )?))
            })
            .collect::<Result<Vec<_>>>()?;
        evidence["application_object_bytes"] = json!(inputs
            .iter()
            .map(PreparedApplicationInput::byte_len)
            .collect::<Vec<_>>());
        phase = "native submission through four-node final receipt observation";
        timed_attempt = Some(Instant::now());
        measured = Some(drive(
            &mut nodes,
            &mut clients,
            ingress,
            &inputs,
            frame_rows,
            &mut observations,
            &mut counts,
            timeout,
        )?);
        timed_attempt = None;
        ensure!(
            counts.successful_transactions == count && counts.business_failed_transactions == 0,
            "NUAS load did not finalize all successful transactions"
        );
        let before = ready(&mut nodes, &mut clients)?;
        same_head(&before, before[0])?;
        ensure!(
            before[0].height <= 257,
            "static NUAS intent window exceeded"
        );
        evidence["before"] = json!(before);
        let remaining = FUNDING
            .checked_sub(rounds as u128 * debit)
            .context("NUAS funding exhausted")?;
        let mut expected = (0..SENDERS)
            .map(|i| (Account::from(account_bytes(i)), remaining))
            .collect::<Vec<_>>();
        expected.push((account_bytes(SENDERS).into(), count as u128));
        expected.push((
            [0xfe; 20].into(),
            crate::native_pipeline::business::fixed_supply::NOV_FIXED_SUPPLY_BASE_UNITS
                - FUNDING * SENDERS as u128,
        ));
        phase = "authoritative UCA balances budgets and nonce";
        let balances = verify_balances(&mut clients, &expected, before[0])?;
        let mut authority = Vec::new();
        for client in &mut clients {
            let mut states = Vec::new();
            for payer in 0..=SENDERS {
                let view = read_uca_state(client, &make_intent(payer, rounds as u64)?, payer)?;
                let expected_nonce = if payer == SENDERS { 0 } else { rounds as u64 };
                let expected_budget = if payer == SENDERS { FUNDING } else { remaining };
                let expected_balance = if payer == SENDERS {
                    count as u128
                } else {
                    remaining
                };
                ensure!(
                    view.state_root == before[0].state_root
                        && view.height == before[0].height
                        && view.account_active
                        && view.current_spend_epoch == 1
                        && view.grant_epoch == 1
                        && view.permits_nov_transfer
                        && !view.revoked
                        && view.not_before_height == 1
                        && view.expires_at_height == 1000
                        && view.max_debit_per_intent == 1000
                        && view.next_nonce == expected_nonce
                        && view.remaining_debit_budget == expected_budget
                        && view.payer_balance == expected_balance
                        && view.recipient_balance == count as u128,
                    "NUAS authoritative account/budget/nonce mismatch: {view:?}"
                );
                states.push(view);
            }
            authority.push(states);
        }
        ensure!(
            authority.iter().all(|s| *s == authority[0]),
            "NUAS four-node authority mismatch"
        );
        evidence["balances"] = json!(balances);
        evidence["authority"] = json!(authority[0]);
        phase = "untimed final local progress capture";
        evidence["final_local_progress"] = collect_final_progress(&mut nodes, before[0])?;
        let old_pids = nodes
            .processes
            .iter()
            .flatten()
            .map(Child::id)
            .collect::<Vec<_>>();
        drop(clients);
        nodes.stop_all()?;
        phase = "stopped original stores ordered fee audit";
        evidence["durable_economics"] = audit_fee_pages(&nodes, before[0], &raw, &observations)?;
        evidence["full_fee_state_audited"] = json!(true);
        phase = "cold restart all receipts balances and authorizations";
        for i in 0..4 {
            nodes.start(i, "existing")?;
        }
        let new_pids = nodes
            .processes
            .iter()
            .flatten()
            .map(Child::id)
            .collect::<Vec<_>>();
        ensure!(
            new_pids.len() == 4 && new_pids.iter().all(|p| !old_pids.contains(p)),
            "NUAS four real processes did not restart"
        );
        let mut clients = (0..4)
            .map(|i| client(&nodes, i, set.clone(), CONFIGURED_BATCH))
            .collect::<Result<Vec<_>>>()?;
        let recovered = ready(&mut nodes, &mut clients)?;
        same_head(&recovered, before[0])?;
        let mut cold = observations.clone();
        for row in &mut cold {
            row.finalized_us = [None; 4];
        }
        let mut cold_counts = Counts::default();
        let elapsed = drive(
            &mut nodes,
            &mut clients,
            ingress,
            &[],
            frame_rows,
            &mut cold,
            &mut cold_counts,
            timeout,
        )?;
        ensure!(
            cold_counts.successful_transactions == count
                && verify_balances(&mut clients, &expected, before[0])? == balances,
            "NUAS cold receipts/balances changed"
        );
        ensure!(authority[0].len() == SENDERS + 1, "NUAS authority coverage");
        for client in &mut clients {
            for (payer, expected_authority) in authority[0].iter().enumerate() {
                ensure!(
                    read_uca_state(client, &make_intent(payer, rounds as u64)?, payer)?
                        == *expected_authority,
                    "NUAS cold authorization/nonce changed"
                );
            }
        }
        nodes.stop_all()?;
        ensure!(
            audit_fee_pages(&nodes, before[0], &raw, &observations)?
                == evidence["durable_economics"],
            "NUAS cold durable fee state changed"
        );
        evidence["recovered"] = json!(recovered);
        evidence["cold_counts"] = json!(cold_counts);
        evidence["cold_receipt_observation_seconds"] = json!(elapsed.as_secs_f64());
        evidence["cold_recovery_pass"] = json!(true);
        Ok(())
    })();
    let mut latency = observations
        .iter()
        .filter(|row| row.complete())
        .map(|row| {
            row.finalized_us
                .iter()
                .flatten()
                .copied()
                .max()
                .unwrap()
                .saturating_sub(row.first_submit_us.unwrap())
        })
        .collect::<Vec<_>>();
    latency.sort_unstable();
    evidence["phase"] = json!(phase);
    evidence["passed"] = json!(result.is_ok());
    evidence["error"] = json!(result.as_ref().err().map(|e| format!("{e:#}")));
    evidence["counts"] = json!(counts);
    evidence["elapsed_seconds"] = json!(measured.map(|d| d.as_secs_f64()));
    evidence["successful_tps"] =
        json!(measured.map(|d| counts.successful_transactions as f64 / d.as_secs_f64()));
    evidence["incomplete_timed_attempt_seconds"] =
        json!(timed_attempt.map(|s| s.elapsed().as_secs_f64()));
    evidence["transactions_not_confirmed_on_all_four"] =
        json!(observations.iter().filter(|r| !r.complete()).count());
    evidence["first_height_all_receipts_observed_us"] = json!(observations
        .iter()
        .filter(|r| matches!(r.receipt, Some(ReceiptState::Finalized { height: 1, .. })))
        .flat_map(|r| r.finalized_us.iter().flatten())
        .max());
    evidence["latency_us"] = json!({"p50":percentile(&latency,50),"p95":percentile(&latency,95),
        "p99":percentile(&latency,99),"sample_count":latency.len(),
        "definition":"first native wire-frame submission attempt to slowest of four final receipt replies; includes initial execution/consensus wait and bounded observer"});
    evidence["timing"] = json!("same native load drive: first SDK submission attempt through every unique four-node final receipt; startup/keygen/signing/structure preparation/untimed economics/cold recovery excluded");
    fs::write(
        directory.join("observations.json"),
        serde_json::to_vec(&observations)?,
    )?;
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    result
}
