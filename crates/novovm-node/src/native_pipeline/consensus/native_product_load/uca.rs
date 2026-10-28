//! NUAS workload on the original process fixture and native load observer.
//! Only workload construction differs from the retained V3 comparison. Neither
//! this test nor its oracle executes/publishes business effects for the nodes.
use super::*;
use crate::native_pipeline::service::config::test_asset_genesis::approve_manifest;
use crate::native_pipeline::{
    business::{
        direct_nov_fee::{quote_and_settle, FeeState},
        nov_transfer_batch::fee_record_changes,
    },
    consensus::chain::ChainRecord,
    service::config::uca_genesis::{
        UcaGenesisAccountV2, UcaGenesisApprovalV2, UcaGenesisGrantV2, UcaGenesisIdentityV2,
        UcaGenesisManifestV2, UcaInitializationConfigV2,
    },
};
use novovm_adapter_api::uca_spend_policy::SuppliedAssetSpendGrantV2;
use novovm_exec::pq_signature::AoemMldsaParameterSet;
use novovm_protocol::{
    native_query::{AssetReadIntentV2, AssetSpendStateViewV2, AssetState},
    uca_spend::{spend_key_ref_v1, AssetSpendIntentV2, SignedAssetSpendV2},
    uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2},
};
use std::collections::BTreeMap;

const CONFIGURED_BATCH: usize = 1024;
// Preserve the existing 32-account initialization bound, including recipient.
const SENDERS: usize = 16;
const FUNDING: u128 = 1_000_000;

#[path = "uca/rolling.rs"]
mod rolling;
use rolling::{LoadMode, SpendWindow};

// Isolated acceptance replay only. These are synthetic process-fixture keys,
// never operator/deployment custody. The three runs retain separate original
// databases and signing journals; nothing is cleared or restored for reuse.
// Keeping generated signatures here makes input equality real, rather than
// merely regenerating logically equivalent randomized signatures each round.
type CorpusKeyPair = (Vec<u8>, zeroize::Zeroizing<Vec<u8>>);

#[derive(Default)]
struct PairedCorpus {
    keys: Option<Vec<CorpusKeyPair>>,
    approvals: Option<Vec<UcaGenesisApprovalV2>>,
    signed: Option<Vec<Vec<u8>>>,
    identity: Option<(Hash, Hash, Hash)>,
    rolling_identity: Option<(Hash, Hash)>,
}

fn account_bytes(index: usize) -> [u8; 20] {
    let mut value = [0x6b; 20];
    value[12..].copy_from_slice(&(index as u64).to_be_bytes());
    value
}

fn account_name(index: usize) -> String {
    format!("uca:native-load-{index}")
}

// Re-encoding for size evidence happens after the four-receipt stopwatch, never
// in the measured submission loop. These are logical bytes, not UDP traffic.
fn apfl_frame_sizes(raw: &[Vec<u8>], frame_rows: usize) -> Result<Vec<Value>> {
    let codec = native_decode_limits(frame_rows, true);
    raw.chunks(frame_rows)
        .map(|rows| {
            let batch = AssetSpendBatchV2::from_raw(
                rows,
                AssetSpendBatchLimitsV2 {
                    transactions: codec.transactions,
                    transaction_bytes: codec.transaction_bytes,
                    body_bytes: codec.body_bytes,
                },
            )?;
            Ok(json!({
                "rows":batch.len(),
                "canonical_expanded_bytes":batch.canonical_bytes(),
                "apfl_encoded_bytes":batch.encode()?.len(),
                "independent_signature_bytes":batch.len() * novovm_protocol::uca_spend::MLDSA65_SIGNATURE_BYTES,
            }))
        })
        .collect()
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
    intent: &AssetSpendIntentV2,
    _payer: usize,
) -> Result<AssetSpendStateViewV2> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        ensure!(Instant::now() < deadline, "NUAS load state query deadline");
        match query(
            client,
            Query::AssetSpendState {
                intent: AssetReadIntentV2::new(intent).map_err(anyhow::Error::msg)?,
            },
        )? {
            Answer::AssetSpendState(AssetState::Present(value)) => return Ok(*value),
            Answer::AssetSpendState(AssetState::Pending) => {}
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
            let signed = SignedAssetSpendV2::decode(bytes)?;
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
                signed.intent.payer == account_bytes(payer)
                    && signed.intent.recipient == account_bytes(SENDERS)
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
    run_native_uca_load(None, &mut PairedCorpus::default(), None).map(|_| ())
}

fn run_native_uca_load(
    signature_override: Option<novovm_exec::pq_signature::MldsaVerificationBackendV1>,
    corpus: &mut PairedCorpus,
    binary_override: Option<&Path>,
) -> Result<Value> {
    run_native_uca_load_mode(
        signature_override,
        corpus,
        binary_override,
        LoadMode::Static,
    )
}

fn run_native_uca_load_mode(
    signature_override: Option<novovm_exec::pq_signature::MldsaVerificationBackendV1>,
    corpus: &mut PairedCorpus,
    binary_override: Option<&Path>,
    mode: LoadMode,
) -> Result<Value> {
    let count = env_count(
        "NOVOVM_NATIVE_UCA_LOAD_TRANSACTIONS",
        if mode.dynamic_windows() { 65_536 } else { 1024 },
        65_536,
    )?;
    let frame_rows = env_count("NOVOVM_NATIVE_UCA_FRAME_ROWS", 32, CONFIGURED_BATCH)?;
    let codec = native_decode_limits(CONFIGURED_BATCH, true);
    ensure!(
        count % SENDERS == 0 && count % frame_rows == 0,
        "NUAS load must contain whole sender rounds and wire frames"
    );
    let rounds = count / SENDERS;
    mode.validate_count(count, frame_rows)?;
    let grant_expiry = mode.grant_expiry(count)?;
    ensure!(
        (rounds as u128)
            .checked_mul(51)
            .is_some_and(|value| value <= FUNDING),
        "NUAS original funded balance/budget cannot cover amount plus maximum fee"
    );
    let timeout = Duration::from_secs(env_count(
        "NOVOVM_NATIVE_LOAD_TIMEOUT_SECONDS",
        if mode.dynamic_windows() { 90 } else { 600 },
        3600,
    )? as u64);
    let binary = match binary_override {
        Some(path) => fs::canonicalize(path)?,
        None => fs::canonicalize(
            std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
                .context("explicit original node binary required")?,
        )?,
    };
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
        "NOVOVM_PROFILE_CRITICAL_PATH",
        "NOVOVM_NATIVE_LOAD_DIAGNOSTICS",
        "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS",
        "NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND",
        "NOVOVM_RESIDENT_TEST_SIGNATURE_BACKEND",
        "NOVOVM_NATIVE_LOAD_SIGNERS",
    ]
    .into_iter()
    .map(|name| (name, std::env::var(name).ok()))
    .collect::<BTreeMap<_, _>>();
    let mut evidence = json!({"scope":"one_host_four_original_processes_bounded_nuas_load",
        "transactions":count,"configured_candidate_batch_limit":CONFIGURED_BATCH,"wire_frame_rows":frame_rows,
        "senders":SENDERS,"window":if mode.is_rolling() {rolling::WAVE_ROWS} else {WINDOW},
        "original_sdk_outstanding_ceiling":WINDOW,"sdk_io_quantum":IO_QUANTUM,"canonical_body_budget":codec.body_bytes,
        "wire_message_budget":codec.message_bytes,"query_batch":env_count("NOVOVM_NATIVE_QUERY_BATCH",1,MAX_RECEIPTS)?,
        "backend":test_execution_backend()?,"binary":binary,"binary_sha256":hex(&Sha256::digest(fs::read(&binary)?)),
        "signature_backend":signature_override.unwrap_or(crate::native_pipeline::service::config::test_signature_backend()?),
        "aoem_sha256":hex(&Sha256::digest(fs::read(&library)?)),"real_processes":4,"real_machines":1,
        "asset_signature":"ML-DSA-65","transport":"native_novorudp_apfl","http_listeners":0,
        "requested_full_65536_workload":count == 65_536,
        "full_65536_workload":false,
        "static_intent_window":if mode.dynamic_windows() {Value::Null} else {json!([1,257])},
        "static_65536_feasibility":"1024 full candidates need 64 heights; actual packing/rounds must still finish every signed row within [1,257), otherwise fail without extending or renewing signed intents",
        "load_strategy":mode.label(),"grant_expires_at_height":grant_expiry,
        "rolling_wave_transactions":if mode.is_rolling() {Some(rolling::WAVE_ROWS)} else {None},
        "effective_outstanding_limit":if mode.is_rolling() {rolling::WAVE_ROWS} else {WINDOW},
        "rolling_signature_65536":if mode.is_rolling() {"PENDING"} else {"NOT_EXECUTED"},
        "continuous_signature_65536":if mode == LoadMode::Continuous {"PENDING"} else {"NOT_EXECUTED"},
        "input_identity":if mode.dynamic_windows() {"same logical payer/recipient/amount/nonce and approved genesis; receipt-anchored expiry and signatures can differ across runs, NOT byte-identical"} else if signature_override.is_some() {"one in-memory signed corpus replayed byte-identically across CPU/GPU/CPU"} else {"fresh generated per invocation; same logical workload is not byte-identical input"},
        "laboratory_replay_only":signature_override.is_some(),
        "deployment_replay_authorized":false,
        "signature_generation_timed":mode.dynamic_windows(),
        "signature_generation_timing":if mode.dynamic_windows() {"initial bounded input preparation excluded before first submission; ALL later signing/preparation remain inside the uninterrupted wall; continuous mode overlaps signing with original SDK polling"} else {"all workload signing excluded before first submission"},
        "overall_deadline_seconds":timeout.as_secs(),
        "diagnostic_extended_deadline":mode.dynamic_windows() && timeout != Duration::from_secs(90),
        "system_capacity_claim":false,"business_proof_verified":false,
        "diagnostics_enabled":std::env::var("NOVOVM_PROFILE_STAGES").as_deref()==Ok("1"),
        "critical_path_diagnostics_enabled":std::env::var("NOVOVM_PROFILE_CRITICAL_PATH").as_deref()==Ok("1"),
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
    let mut measured: Option<Duration> = None;
    let mut timed_attempt = None;
    let mut phase = "prepare approved NUAS fixture and signed workload";
    let result = (|| -> Result<()> {
        let runtime = AoemExecFacade::open(&library, AoemExecOpenOptions::default())?;
        let keygen = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
        let keys = match &corpus.keys {
            Some(keys) => keys.clone(),
            None => {
                let keys = (0..2 * (SENDERS + 1) + 1)
                    .map(|_| {
                        let pair = keygen.generate_keypair()?;
                        Ok((pair.public_key, pair.expanded_secret_key))
                    })
                    .collect::<Result<Vec<_>>>()?;
                corpus.keys = Some(keys.clone());
                keys
            }
        };
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
        let manifest = UcaGenesisManifestV2 {
            genesis,
            identities: vec![UcaGenesisIdentityV2 {
                uca_id: account_name(0),
                identity_public_key: keys[2 * (SENDERS + 1)].0.clone(),
            }],
            accounts: (0..=SENDERS)
                .map(|i| {
                    Ok(UcaGenesisAccountV2 {
                        uca_id: account_name(0),
                        asset_account: account_bytes(i).to_vec(),
                        management_public_key: keys[i].0.clone(),
                        recovery_public_key: None,
                        grants: vec![UcaGenesisGrantV2 {
                            spend_public_key: keys[SENDERS + 1 + i].0.clone(),
                            grant: SuppliedAssetSpendGrantV2 {
                                asset: 1,
                                account: account_bytes(i).to_vec(),
                                key_ref: spend_key_ref_v1(&keys[SENDERS + 1 + i].0)?,
                                authorization_epoch: 1,
                                permits_nov_transfer: true,
                                revoked: false,
                                not_before_height: 1,
                                expires_at_height: grant_expiry,
                                max_debit_per_intent: 1000,
                                remaining_debit_budget: FUNDING,
                            },
                        }],
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };
        let domain = manifest.signing_domain()?;
        let approvals = match &corpus.approvals {
            Some(approvals) => approvals.clone(),
            None => {
                let approval_keys = keys
                    .iter()
                    .map(|(p, s)| (p.as_slice(), s.as_slice()))
                    .collect::<Vec<_>>();
                let approvals = approve_manifest(&runtime, &manifest, &approval_keys)?;
                corpus.approvals = Some(approvals.clone());
                approvals
            }
        };
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
        for path in &nodes.configs {
            let mut config: Value = serde_json::from_slice(&fs::read(path)?)?;
            config["rpc_addr"] = Value::Null;
            config["profile"] = json!(ORIGINAL_PROFILE);
            config["native_client_limit"] = json!(2);
            config["genesis"] = serde_json::to_value(&genesis)?;
            config["uca_initialization"] = serde_json::to_value(&material)?;
            if let Some(backend) = signature_override {
                config["signature_backend"] = serde_json::to_value(backend)?;
            }
            fs::write(path, serde_json::to_vec_pretty(&config)?)?;
        }
        evidence["signing_domain"] = json!(hex(&domain));
        evidence["initial_root"] = json!(hex(&material.expected_initial_root));
        evidence["genesis_commitment"] = json!(hex(&material.expected_final_commitment));
        let signer = runtime.asset_spend_signer_v2()?;
        let make_intent =
            |payer: usize, nonce: u64, window: SpendWindow| -> Result<AssetSpendIntentV2> {
                Ok(AssetSpendIntentV2 {
                    asset: 1,
                    authority_update: None,
                    chain_id: CHAIN,
                    genesis: domain,
                    payer: account_bytes(payer).to_vec(),
                    authorization_epoch: 1,
                    spend_key_ref: spend_key_ref_v1(&keys[SENDERS + 1 + payer].0)?,
                    recipient: account_bytes(SENDERS).to_vec(),
                    amount: 1,
                    max_fee: 50,
                    nonce,
                    not_before_height: window.not_before,
                    expires_at_height: window.expires,
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
        ensure!(
            mode.dynamic_windows() || corpus.signed.as_ref().is_none_or(|raw| raw.len() == count),
            "paired corpus transaction count changed"
        );
        let mut raw = Vec::with_capacity(count);
        let mut workload_hash = Sha256::new();
        workload_hash.update(b"novovm/nuas-load/ordered-signed-inputs/v1\0");
        let make_signed = |index: usize, window: SpendWindow| -> Result<SignedAssetSpendV2> {
            let payer = index % SENDERS;
            let nonce = (index / SENDERS) as u64;
            let key = &keys[SENDERS + 1 + payer];
            let signed = match corpus.signed.as_ref().filter(|_| !mode.dynamic_windows()) {
                Some(raw) => SignedAssetSpendV2::decode(&raw[index])?,
                None => signer.sign(make_intent(payer, nonce, window)?, &key.0, &key.1)?,
            };
            ensure!(
                signed.intent == make_intent(payer, nonce, window)?,
                "paired corpus intent changed"
            );
            Ok(signed)
        };
        let first_count = mode.first_rows(count);
        let initial_preparation_started = Instant::now();
        if mode == LoadMode::Continuous {
            let generated = rolling::generate_rows(
                0,
                first_count,
                SpendWindow::after(0)?,
                env_count("NOVOVM_NATIVE_LOAD_SIGNERS", 4, 16)?,
                &make_signed,
            )?;
            rolling::append_generated_rows(
                &mut raw,
                &mut observations,
                &mut workload_hash,
                generated.into_iter().map(Ok),
                SpendWindow::after(0)?,
                fee,
            )?;
        } else {
            rolling::append_signed_rows(
                &mut raw,
                &mut observations,
                &mut workload_hash,
                first_count,
                SpendWindow::after(0)?,
                fee,
                &make_signed,
            )?;
        }
        evidence["initial_signed_corpus_rows"] = json!(first_count);
        evidence["initial_signed_corpus_preparation_us"] =
            json!(initial_preparation_started.elapsed().as_micros() as u64);
        if mode.dynamic_windows() {
            ensure!(
                corpus
                    .rolling_identity
                    .is_none_or(|identity| identity == (domain, material.expected_initial_root)),
                "rolling approved genesis/root changed"
            );
        } else if let Some(expected) = corpus.identity {
            let input_hash: Hash = workload_hash.clone().finalize().into();
            ensure!(
                expected == (domain, material.expected_initial_root, input_hash),
                "paired genesis/root/signed bytes changed before node startup"
            );
        }
        evidence["signed_workload_sha256"] = json!(hex(&workload_hash.clone().finalize()));
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
        let inputs = rolling::prepare_inputs(&mut clients[ingress], &raw, 0, frame_rows)?;
        evidence["application_object_bytes"] = json!(inputs
            .iter()
            .map(PreparedApplicationInput::byte_len)
            .collect::<Vec<_>>());
        phase = "native submission through four-node final receipt observation";
        timed_attempt = Some(Instant::now());
        measured = Some(if mode == LoadMode::Continuous {
            rolling::drive_continuous(
                &mut nodes,
                &mut clients,
                ingress,
                inputs,
                frame_rows,
                &mut raw,
                &mut observations,
                &mut workload_hash,
                &mut counts,
                timeout,
                count,
                fee,
                &make_signed,
                &mut evidence,
            )?
        } else if mode.is_rolling() {
            rolling::drive_rolling(
                &mut nodes,
                &mut clients,
                ingress,
                inputs,
                frame_rows,
                &mut raw,
                &mut observations,
                &mut workload_hash,
                &mut counts,
                timeout,
                count,
                fee,
                &make_signed,
                &mut evidence,
            )?
        } else {
            drive(
                &mut nodes,
                &mut clients,
                ingress,
                &inputs,
                frame_rows,
                &mut observations,
                &mut counts,
                timeout,
            )?
        });
        timed_attempt = None;
        let workload_hash: Hash = workload_hash.finalize().into();
        if mode.dynamic_windows() {
            corpus.rolling_identity = Some((domain, material.expected_initial_root));
            let load_status = if mode == LoadMode::Continuous {
                "continuous_signature_65536"
            } else {
                "rolling_signature_65536"
            };
            evidence[load_status] = json!(if count == 65_536 {
                "EXECUTED"
            } else {
                "REDUCED_DIAGNOSTIC"
            });
        } else {
            let identity = (domain, material.expected_initial_root, workload_hash);
            if let Some(expected) = corpus.identity {
                ensure!(
                    identity == expected,
                    "paired genesis/root/signed bytes changed"
                );
            } else {
                corpus.identity = Some(identity);
                corpus.signed = Some(raw.clone());
            }
        }
        evidence["signed_workload_sha256"] = json!(hex(&workload_hash));
        evidence["canonical_signed_input_bytes"] = json!(raw.iter().map(Vec::len).sum::<usize>());
        evidence["apfl_frame_sizes"] = json!(apfl_frame_sizes(&raw, frame_rows)?);
        evidence["apfl_size_measurement_scope"] = json!("untimed reconstruction from the exact submitted signed corpus; canonical expanded bytes and lossless APFL bytes preserve every signature; application_object_bytes also includes original native envelope, neither is physical/retransmitted UDP traffic");
        ensure!(
            counts.successful_transactions == count && counts.business_failed_transactions == 0,
            "NUAS load did not finalize all successful transactions"
        );
        let before = ready(&mut nodes, &mut clients)?;
        same_head(&before, before[0])?;
        if mode.is_rolling() {
            ensure!(
                evidence["rolling_waves"]
                    .as_array()
                    .and_then(|waves| waves.last())
                    .and_then(|wave| wave["final_height"].as_u64())
                    == Some(before[0].height),
                "rolling final receipt height differs from original four-node final audit"
            );
        }
        ensure!(
            mode.dynamic_windows() || before[0].height < 257,
            "static NUAS intent window exceeded"
        );
        let query_window = if mode.dynamic_windows() {
            SpendWindow::after(before[0].height)?
        } else {
            SpendWindow::after(0)?
        };
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
                let view = read_uca_state(
                    client,
                    &make_intent(payer, rounds as u64, query_window)?,
                    payer,
                )?;
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
                        && view.expires_at_height == grant_expiry
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
            row.receipt_query_observed_us = [None; 4];
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
                    read_uca_state(
                        client,
                        &make_intent(payer, rounds as u64, query_window)?,
                        payer
                    )? == *expected_authority,
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
            row.receipt_query_observed_us
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
    evidence["full_65536_workload"] = json!(result.is_ok() && count == 65_536);
    evidence["error"] = json!(result.as_ref().err().map(|e| format!("{e:#}")));
    evidence["counts"] = json!(counts);
    evidence["elapsed_seconds"] = json!(measured.map(|d| d.as_secs_f64()));
    evidence["successful_tps"] =
        json!(measured.map(|d| counts.successful_transactions as f64 / d.as_secs_f64()));
    evidence["incomplete_timed_attempt_seconds"] =
        json!(timed_attempt.map(|s| s.elapsed().as_secs_f64()));
    evidence["transactions_not_receipt_query_observed_on_all_four"] =
        json!(count.saturating_sub(observations.iter().filter(|r| r.complete()).count()));
    evidence["prepared_transactions"] = json!(observations.len());
    evidence["first_height_all_receipt_queries_observed_us"] = json!(observations
        .iter()
        .filter(|r| matches!(r.receipt, Some(ReceiptState::Finalized { height: 1, .. })))
        .flat_map(|r| r.receipt_query_observed_us.iter().flatten())
        .max());
    evidence["latency_us"] = json!({"p50":percentile(&latency,50),"p95":percentile(&latency,95),
        "p99":percentile(&latency,99),"sample_count":latency.len(),
        "definition":"first native wire-frame submission attempt to slowest of four receipt-query observations; includes initial execution/consensus wait and bounded observer; not an internal finality timestamp"});
    evidence["timing"] = json!(if mode == LoadMode::Continuous {
        "original native SDK continuous bounded supply: first submission attempt through final unique four-node receipt observation on ONE unpaused clock; later parallel signing and preparation overlap client polling and remain included; initial bounded preparation/startup/final audit/cold recovery excluded; receipt-anchored signatures are not byte-identical across backend runs"
    } else if mode.is_rolling() {
        "same native load observer: first SDK submission attempt through final unique four-node receipt observation on ONE unpaused clock; all intervening signing/structure/head queries/transport drains included; startup/keygen/first-wave preparation/final audit/cold recovery excluded; not an 8192-inflight static-input comparison"
    } else {
        "same native load drive: first SDK submission attempt through every unique four-node receipt-query observation; startup/keygen/signing/structure preparation/untimed economics/cold recovery excluded"
    });
    fs::write(
        directory.join("observations.json"),
        serde_json::to_vec(&observations)?,
    )?;
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    result?;
    Ok(evidence)
}

#[test]
#[ignore = "isolated laboratory CPU/GPU/CPU replay of one NUAS corpus on four original processes; never deployment keys; run alone"]
fn native_uca_same_corpus_signature_cpu_gpu_cpu() -> Result<()> {
    use novovm_exec::pq_signature::MldsaVerificationBackendV1::{Cpu, GpuRequired};
    let mut corpus = PairedCorpus::default();
    let mut reports = Vec::new();
    for backend in [Cpu, GpuRequired, Cpu] {
        reports.push(run_native_uca_load(Some(backend), &mut corpus, None)?);
    }
    ensure!(reports.len() == 3, "paired run count");
    for report in &reports {
        ensure!(
            report["signed_workload_sha256"] == reports[0]["signed_workload_sha256"]
                && report["signing_domain"] == reports[0]["signing_domain"]
                && report["genesis_commitment"] == reports[0]["genesis_commitment"]
                && report["initial_root"] == reports[0]["initial_root"]
                && report["binary_sha256"] == reports[0]["binary_sha256"]
                && report["aoem_sha256"] == reports[0]["aoem_sha256"]
                && report["backend"] == reports[0]["backend"]
                && report["transactions"] == reports[0]["transactions"]
                && report["wire_frame_rows"] == reports[0]["wire_frame_rows"]
                && report["canonical_body_budget"] == reports[0]["canonical_body_budget"]
                && report["wire_message_budget"] == reports[0]["wire_message_budget"]
                && report["static_intent_window"] == reports[0]["static_intent_window"]
                && report["query_batch"] == reports[0]["query_batch"]
                && report["cold_recovery_pass"] == true
                && report["full_fee_state_audited"] == true,
            "paired fixture/software/validation conditions changed"
        );
        ensure!(
            authority_economics(report)? == authority_economics(&reports[0])?,
            "paired committed balances/authorization/nonce differ"
        );
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-load")
        .join(format!(
            "paired-{}-{}.json",
            std::process::id(),
            clock_ms()?
        ));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "scope":"one-host four-process isolated laboratory acceptance replay, not a deployment reset recipe",
            "same_signed_corpus_and_initial_state":true,
            "signature_backend_order":["cpu","gpu_required","cpu"],
            "business_backend_unchanged":true,
            "committed_economics_equal_excluding_height_and_root":true,
            "all_round_databases_and_journals_retained":true,
            "fresh_fixture_stores_same_synthetic_domain":true,
            "deployment_custody_used":false,
            "full_65536_workload":reports.iter().all(|report|report["full_65536_workload"] == true),
            "validity":"all static intents retain [1,257); no automatic renewal if actual packing takes too many heights",
            "confirmation_time":"client all-four receipt observations, not internal durable timestamp",
            "rounds":reports
        }))?,
    )?;
    eprintln!("NUAS paired report={}", path.display());
    Ok(())
}

fn authority_economics(report: &Value) -> Result<Value> {
    let mut authority = report["authority"]
        .as_array()
        .context("paired authority missing")?
        .clone();
    ensure!(authority.len() == SENDERS + 1, "paired authority coverage");
    for state in &mut authority {
        let state = state.as_object_mut().context("paired authority shape")?;
        // Independent schedules can commit the same funded effects at different
        // heights/roots. These remain fully checked within every four-node run;
        // only the cross-run economic comparison omits these two metadata fields.
        ensure!(
            state.remove("height").is_some() && state.remove("state_root").is_some(),
            "paired authority metadata missing"
        );
    }
    Ok(json!({
        "authority":authority,
        "oracle_balances":report.pointer("/durable_economics/oracle_sender_balances"),
        "oracle_nonces":report.pointer("/durable_economics/oracle_sender_nonces"),
        "archived_transactions":report.pointer("/durable_economics/archived_transactions")
    }))
}

#[test]
#[ignore = "isolated same-corpus before/after original-node authority capture; retains all journals; run alone"]
fn native_uca_same_corpus_bulk_authority_before_after() -> Result<()> {
    use novovm_exec::pq_signature::MldsaVerificationBackendV1::{Cpu, GpuRequired};
    let baseline = fs::canonicalize(
        std::env::var_os("NOVOVM_NATIVE_UCA_BASELINE_NODE_BINARY")
            .context("explicit pre-change original node binary required")?,
    )?;
    let candidate = fs::canonicalize(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
            .context("explicit candidate original node binary required")?,
    )?;
    ensure!(
        baseline != candidate,
        "before/after binary paths must differ"
    );
    let mut corpus = PairedCorpus::default();
    let mut rounds = Vec::new();
    for (label, binary, backend) in [
        ("before_cpu", &baseline, Cpu),
        ("after_cpu", &candidate, Cpu),
        ("before_gpu", &baseline, GpuRequired),
        ("after_gpu", &candidate, GpuRequired),
        ("before_cpu_repeat", &baseline, Cpu),
    ] {
        let report = run_native_uca_load(Some(backend), &mut corpus, Some(binary))?;
        if let Some((_, first)) = rounds.first() {
            let first: &Value = first;
            for field in [
                "signed_workload_sha256",
                "signing_domain",
                "genesis_commitment",
                "initial_root",
                "aoem_sha256",
                "backend",
                "transactions",
                "wire_frame_rows",
                "query_batch",
                "canonical_body_budget",
            ] {
                ensure!(
                    report[field] == first[field],
                    "before/after {field} differs"
                );
            }
            ensure!(
                authority_economics(&report)? == authority_economics(first)?,
                "before/after committed economics differ"
            );
        }
        ensure!(
            report["passed"] == true
                && report["cold_recovery_pass"] == true
                && report["full_fee_state_audited"] == true,
            "before/after validation incomplete"
        );
        rounds.push((label, report));
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-load")
        .join(format!(
            "bulk-authority-paired-{}-{}.json",
            std::process::id(),
            clock_ms()?
        ));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "scope":"one-host four-process laboratory before/after; no deployed identity or journal reset",
            "same_signed_corpus_and_initial_state":true,
            "all_round_databases_and_journals_retained":true,
            "business_backend_unchanged":true,
            "full_65536":rounds.iter().all(|(_,report)|report["full_65536_workload"] == true),
            "rounds":rounds.into_iter().map(|(label,report)|json!({"label":label,"report":report})).collect::<Vec<_>>()
        }))?,
    )?;
    eprintln!("NUAS bulk authority paired report={}", path.display());
    Ok(())
}

#[test]
fn paired_authority_comparison_excludes_only_schedule_metadata() -> Result<()> {
    let mut first = json!({
        "authority":(0..=SENDERS).map(|_| json!({
            "height":1,"state_root":vec![1u8;32],"next_nonce":64,
            "remaining_debit_budget":999_000,"payer_balance":999_000,
            "recipient_balance":1024,"revoked":false,"grant_epoch":1
        })).collect::<Vec<_>>(),
        "durable_economics":{"oracle_sender_balances":[999_000],
            "oracle_sender_nonces":[64],"archived_transactions":1024}
    });
    let expected = authority_economics(&first)?;
    first["authority"][0]["height"] = json!(9);
    first["authority"][0]["state_root"] = json!(vec![9u8; 32]);
    assert_eq!(expected, authority_economics(&first)?);
    first["authority"][0]["next_nonce"] = json!(65);
    assert_ne!(expected, authority_economics(&first)?);
    first["authority"][0]["next_nonce"] = json!(64);
    first["authority"][0]["revoked"] = json!(true);
    assert_ne!(expected, authority_economics(&first)?);
    first["authority"][0]
        .as_object_mut()
        .unwrap()
        .remove("height");
    assert!(authority_economics(&first).is_err());
    Ok(())
}
