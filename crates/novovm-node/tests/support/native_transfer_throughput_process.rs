//! Explicit local end-to-end baseline, not a component benchmark or mainnet TPS.
//! Disposable genesis/keys, real HTTP admission, WSS gossip, AOEM and BFT.
use super::*;
use anyhow::{bail, Context, Result};
use novovm_node::tx_ingress::fresh_genesis::{
    FreshGenesisConfigV1, GenesisAllocationV1, GENESIS_SCHEMA_RECORD_V2,
};
use novovm_protocol::{
    encode_nov_native_tx_wire_v1, NovFeePolicyV1, NovNativeTxWireV1, NovTransferTxV1, NovTxKindV1,
};
use std::collections::BTreeSet;

pub(super) const LABEL: &str = "continuous-record-transfer-measurement";
const SIGNERS: usize = 32;
const BATCHES: usize = 3;
const CLIENT_CONCURRENCY: usize = 4;
const POLL_MS: u64 = 100;
const DEADLINE: Duration = Duration::from_secs(300);

pub(super) fn diagnostics_enabled() -> bool {
    std::env::var("NOVOVM_NATIVE_FRESH_TIMING").as_deref() == Ok("1")
}

fn raw(signer: usize, nonce: u64, amount: u128) -> Vec<u8> {
    let mut tx = NovNativeTxWireV1 {
        chain_id: CHAIN,
        kind: NovTxKindV1::Transfer(NovTransferTxV1 {
            from: Vec::new(),
            to: novovm_adapter_novovm::address_from_seed_v1([128 + signer as u8; 32]),
            asset: "NOV".into(),
            amount,
            nonce,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "NOV".into(),
                max_pay_amount: 1000,
                slippage_bps: 0,
            },
        }),
        signature: Vec::new(),
    };
    sign_nov_native_tx_with_seed_v1(&mut tx, [64 + signer as u8; 32]).unwrap();
    encode_nov_native_tx_wire_v1(&tx).unwrap()
}

pub(super) fn inputs() -> (FreshGenesisConfigV1, NovNativeCandidateExecutionPlanV1) {
    let (mut genesis, previous) = super::super::native_fresh_genesis_cli::inputs();
    genesis.schema = GENESIS_SCHEMA_RECORD_V2.into();
    genesis.allocations = (0..SIGNERS)
        .map(|index| GenesisAllocationV1 {
            account: novovm_adapter_novovm::address_from_seed_v1([64 + index as u8; 32])
                .try_into()
                .unwrap(),
            nov: "1000000".into(),
        })
        .collect();
    genesis.total_initial_nov = (SIGNERS as u128 * 1_000_000).to_string();
    let raw = raw(0, 0, 1);
    let plan = NovNativeCandidateExecutionPlanV1::new(
        previous.context,
        genesis.protocol_config_commitment,
        genesis.compile().unwrap().state_root(),
        None,
        vec![canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap()],
        vec![raw],
    )
    .unwrap();
    (genesis, plan)
}

fn address(node: &Node) -> Option<String> {
    fs::read_to_string(node.0.join(format!("{LABEL}.stdout.log")))
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("native_fresh_rpc_listening: ")
                .map(str::to_owned)
        })
}

// No retry: uncertain admission is an observed failure, never hidden by a helper.
fn rpc_once(address: &str, method: &str, params: Value) -> Result<Value> {
    let response = ureq::post(&format!("http://{address}/"))
        .timeout(Duration::from_secs(10))
        .set("Content-Type", "application/json")
        .send_string(
            &serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
                .to_string(),
        )?;
    let result: Value = serde_json::from_str(&response.into_string()?)?;
    if result.get("error").is_some() {
        bail!("RPC {method}: {result}");
    }
    result.get("result").cloned().context("RPC result missing")
}

fn bytes_under(path: &std::path::Path) -> u64 {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let meta = entry.metadata().unwrap();
            if meta.is_dir() {
                bytes_under(&entry.path())
            } else {
                meta.len()
            }
        })
        .sum()
}

fn storage(node: &Node) -> Value {
    serde_json::json!({
        "aoem_owner_directory_bytes":bytes_under(&node.0.join("owner.rocksdb")),
        "block_ledger_directory_bytes":bytes_under(&nov_native_block_ledger_rocksdb_path_v1(&node.0.join("native.json"))),
        "physical_directory_bytes_not_live_state_capacity":true,
    })
}

fn finalized_blocks(node: &Node, last: u64) -> Vec<NovNativeDurableBlockV1> {
    let genesis: FreshGenesisConfigV1 =
        serde_json::from_slice(&fs::read(node.0.join("genesis.json")).unwrap()).unwrap();
    let pin = genesis.compile().unwrap().config_commitment();
    let mut digest = Sha256::new();
    digest.update(b"novovm-native-aoem-state-namespace-v1");
    digest.update(node.0.to_str().unwrap().as_bytes());
    let namespace = digest.finalize().into();
    let path = nov_native_block_ledger_rocksdb_path_v1(&node.0.join("native.json"));
    (1..=last).map(|height| {
        let (block, proof) = NovNativeBlockLedgerV1::load_fresh_finalized_block_by_height_v1(&path, pin, namespace, height)
            .unwrap().unwrap();
        let novovm_node::native_block_seal::round_message::NovNativeSealRoundMessageV1::DecisionCertificateV3 { decision, .. } = &proof.witness else {
            panic!("measurement requires real decision V3 finality")
        };
        // The public finalized getter verifies the pinned full archive;
        // independently check the returned decision's quorum signatures.
        decision.verify(&proof.authority.validator_set).unwrap();
        assert!(decision.votes.len() >= 3 && decision.votes.len() <= 4);
        assert!(decision.signed_weight >= 3);
        block
    }).collect()
}

#[derive(serde::Serialize)]
struct Sample {
    tx_hash: String,
    batch: usize,
    submit_ms: f64,
    admission_ms: f64,
    finalized_ms_by_node: Vec<Option<f64>>,
}

fn percentile(sorted: &[f64], percent: usize) -> f64 {
    sorted[(sorted.len() * percent).div_ceil(100).saturating_sub(1)]
}

pub(super) fn exercise(nodes: &[Node], evidence: &std::path::Path) {
    // Bootstrap/genesis checks above are outside the measurement window.
    let before: Vec<_> = nodes.iter().map(storage).collect();
    for node in nodes {
        let path = node.0.join("seal.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["follow_finalized_tip"] = true.into();
        config["receive_successors"] = true.into();
        config["propose_successors"] = true.into();
        config["proposal_max_transactions"] = SIGNERS.into();
        fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    }
    let original_configs: Vec<_> = nodes
        .iter()
        .map(|node| fs::read(node.0.join("seal.json")).unwrap())
        .collect();
    let batches: Vec<Vec<_>> = (0..BATCHES)
        .map(|batch| {
            (0..SIGNERS)
                .map(|signer| {
                    let raw = raw(signer, batch as u64 + u64::from(signer == 0), 7);
                    (
                        canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap(),
                        raw,
                    )
                })
                .collect()
        })
        .collect();
    let active: Vec<_> = (0..nodes.len()).collect();
    let mut children = start_cluster(nodes, &active, LABEL, 0);
    let readiness = Instant::now();
    let addresses = loop {
        if let Some(addresses) = nodes.iter().map(address).collect::<Option<Vec<_>>>() {
            break addresses;
        }
        for (_, child) in &mut children {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "node exited during startup; {}",
                evidence.display()
            );
        }
        assert!(
            readiness.elapsed() < Duration::from_secs(60),
            "RPC startup; {}",
            evidence.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    for address in &addresses {
        rpc_once(address, "nov_chainStatus", serde_json::json!([])).unwrap();
    }
    let clock = Instant::now();
    let mut samples = Vec::new();
    let mut attempts = 0;
    let mut admission_outcomes = Vec::new();
    let mut successful_receipts = BTreeSet::new();
    let measurement = (|| -> Result<()> {
        for (batch, raws) in batches.iter().enumerate() {
            let start = samples.len();
            // One fixed ingress node; do not fan out the same raw to validators
            // to conceal the production gossip budget. Only four HTTP clients.
            for chunk in raws.chunks(CLIENT_CONCURRENCY) {
                attempts += chunk.len();
                let results = std::thread::scope(|scope| {
                    let handles: Vec<_> = chunk
                        .iter()
                        .map(|(hash, raw)| {
                            let address = &addresses[0];
                            scope.spawn(move || {
                                let submitted = clock.elapsed().as_secs_f64() * 1000.0;
                                let result = rpc_once(
                                    address,
                                    "nov_sendRawTransaction",
                                    serde_json::json!([hex(raw)]),
                                );
                                (
                                    hash,
                                    submitted,
                                    clock.elapsed().as_secs_f64() * 1000.0,
                                    result,
                                )
                            })
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|handle| handle.join().unwrap())
                        .collect::<Vec<_>>()
                });
                let mut first_error = None;
                for (hash, submit_ms, admission_ms, result) in results {
                    let result = result.and_then(|result| {
                        if result["tx_hash"] != hex(hash)
                            || !matches!(result["status"].as_str(), Some("queued" | "finalized"))
                        {
                            bail!("unexpected admission: {result}");
                        }
                        Ok(result)
                    });
                    admission_outcomes.push(serde_json::json!({
                        "tx_hash":hex(hash),"batch":batch,"submit_ms":submit_ms,
                        "admission_ms":admission_ms,"result":result.as_ref().ok(),
                        "error":result.as_ref().err().map(|error|format!("{error:#}")),
                    }));
                    match result {
                        Ok(_) => samples.push(Sample {
                            tx_hash: hex(hash),
                            batch,
                            submit_ms,
                            admission_ms,
                            finalized_ms_by_node: vec![None; nodes.len()],
                        }),
                        Err(error) => {
                            if first_error.is_none() {
                                first_error = Some(error);
                            }
                        }
                    }
                }
                // All concurrent attempts completed, so retain every outcome
                // before returning any error. Never undercount known admissions.
                if let Some(error) = first_error {
                    return Err(error);
                }
            }
            loop {
                let pending: Vec<_> = (start..samples.len())
                    .flat_map(|index| {
                        samples[index]
                            .finalized_ms_by_node
                            .iter()
                            .enumerate()
                            .filter_map(move |(node, done)| done.is_none().then_some((index, node)))
                    })
                    .collect();
                if pending.is_empty() {
                    break;
                }
                for (_, child) in &mut children {
                    if child.0.try_wait()?.is_some() {
                        bail!("node exited during measurement");
                    }
                }
                if clock.elapsed() > DEADLINE {
                    bail!("RPC-to-finality measurement deadline");
                }
                for chunk in pending.chunks(CLIENT_CONCURRENCY) {
                    let results = std::thread::scope(|scope| {
                        let handles: Vec<_> = chunk
                            .iter()
                            .map(|&(index, node)| {
                                let address = &addresses[node];
                                let hash = &samples[index].tx_hash;
                                scope.spawn(move || {
                                    let result = rpc_once(
                                        address,
                                        "nov_getTransactionStatus",
                                        serde_json::json!([hash]),
                                    );
                                    (index, node, clock.elapsed().as_secs_f64() * 1000.0, result)
                                })
                            })
                            .collect();
                        handles
                            .into_iter()
                            .map(|handle| handle.join().unwrap())
                            .collect::<Vec<_>>()
                    });
                    for (index, node, observed, result) in results {
                        let result = result?;
                        if result["status"] == "finalized" {
                            if result["receipt"]["status"] != true {
                                bail!("failed business receipt: {result}");
                            }
                            if result["tx_hash"] != samples[index].tx_hash {
                                bail!("RPC transaction binding mismatch");
                            }
                            let logs = result["receipt"]["logs"]
                                .as_array()
                                .context("receipt logs missing")?;
                            if !logs.iter().any(|log| {
                                log["event"] == "aoem.native_transfer.computed"
                                    && log["data"]["scheduler"] == "aoem_generic_compute_v2"
                            }) {
                                bail!("receipt lacks actual AOEM transfer computation");
                            }
                            samples[index].finalized_ms_by_node[node] = Some(observed);
                            successful_receipts.insert(samples[index].tx_hash.clone());
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(POLL_MS));
            }
        }
        Ok(())
    })();
    // Preserve failed attempts and partial observations before any assertion.
    fs::write(
        evidence.join("transfer-finality-observations.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "attempted":attempts,"accepted":samples.len(),"samples":samples,
            "admission_outcomes":admission_outcomes,
            "error":measurement.as_ref().err().map(|error| format!("{error:#}")),
            "implicit_rpc_retries":0,"batch_count":BATCHES,"signers":SIGNERS,
            "slow_call_diagnostics_enabled":diagnostics_enabled(),
        }))
        .unwrap(),
    )
    .unwrap();
    measurement.unwrap_or_else(|error| panic!("{error:#}; evidence: {}", evidence.display()));
    let statuses: Vec<_> = addresses
        .iter()
        .map(|address| rpc_once(address, "nov_chainStatus", serde_json::json!([])).unwrap())
        .collect();
    let tip = statuses[0]["height"].as_u64().unwrap();
    for status in &statuses {
        assert_eq!(status["height"], tip);
        assert_eq!(status["finalized"], true);
        assert_eq!(status["lifecycle_halted"], false);
    }
    for (_, child) in &mut children {
        child.0.kill().unwrap();
        child.0.wait().unwrap();
    }
    drop(children);
    let blocks = finalized_blocks(&nodes[0], tip);
    for node in &nodes[1..] {
        assert_eq!(finalized_blocks(node, tip), blocks);
    }
    let actual: Vec<_> = blocks
        .iter()
        .skip(1)
        .flat_map(|block| block.body.tx_hashes.iter().map(|hash| hex(hash)))
        .collect();
    let expected: BTreeSet<_> = samples
        .iter()
        .map(|sample| sample.tx_hash.clone())
        .collect();
    assert_eq!(actual.len(), SIGNERS * BATCHES);
    assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), expected);
    assert_eq!(successful_receipts, expected);
    assert_eq!(
        blocks.last().unwrap().header.state_version - blocks[0].header.state_version,
        (SIGNERS * BATCHES) as u64
    );
    let mut latency: Vec<_> = samples
        .iter()
        .map(|sample| {
            sample
                .finalized_ms_by_node
                .iter()
                .map(|time| time.unwrap())
                .fold(0.0, f64::max)
                - sample.submit_ms
        })
        .collect();
    latency.sort_by(f64::total_cmp);
    let first_submit = samples
        .iter()
        .map(|sample| sample.submit_ms)
        .fold(f64::INFINITY, f64::min);
    let last_final = samples
        .iter()
        .flat_map(|sample| sample.finalized_ms_by_node.iter())
        .map(|time| time.unwrap())
        .fold(0.0, f64::max);
    let duration = (last_final - first_submit) / 1000.0;
    let after: Vec<_> = nodes.iter().map(storage).collect();
    run_cluster_at_height(
        nodes,
        &active,
        "continuous-transfer-measurement-restart",
        8,
        true,
        true,
        true,
        tip,
        None,
        false,
    );
    for (node, original) in nodes.iter().zip(original_configs) {
        assert_eq!(finalized_blocks(node, tip), blocks);
        assert_eq!(fs::read(node.0.join("seal.json")).unwrap(), original);
    }
    let environment = serde_json::json!({
        "aoem_backend":"rocksdb","build_profile":if cfg!(debug_assertions){"debug"}else{"release"},
        "slow_call_diagnostics_enabled":diagnostics_enabled(),
        "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"logical_parallelism":std::thread::available_parallelism().unwrap().get(),
        "cpu_description":std::env::var("PROCESSOR_IDENTIFIER").ok(),
        "node_executable_sha256":hex(&Sha256::digest(fs::read(env!("CARGO_BIN_EXE_novovm-node")).unwrap())),
        "node_evidence_paths":nodes.iter().map(|node| &node.0).collect::<Vec<_>>(),
        "tick_interval_ms":250,"consensus_poll_interval_ms":100,
        "gossip_transactions_per_peer_per_second":4,"ingress_per_source_per_second":8,"ingress_per_poll":16,
    });
    let workload = serde_json::json!({
        "batches":BATCHES,"transactions_per_batch":SIGNERS,"disjoint_account_pairs_per_batch":SIGNERS,
        "single_ingress_validator_index":0,"replicated_client_fanout":false,"client_concurrency":CLIENT_CONCURRENCY,
        "proposal_max_transactions":SIGNERS,"pre_signed_transactions":true,"signing_time_included":false,"bootstrap_time_included":false,
        "genesis_schema":GENESIS_SCHEMA_RECORD_V2,"first_measured_height":2,"last_finalized_height":tip,
    });
    let verification = serde_json::json!({
        "full_blocks_equal_on_all_nodes":true,"all_durable_BFT_proofs_verified":true,"restart_readback_equal":true,
        "physical_lan_executed":false,"public_network_executed":false,"production_signoff":false,
        "report_is_short_baseline_not_saturation_or_long_soak":true,
    });
    let report = serde_json::json!({
        "scope":"same_host_four_process_record_transfer_rpc_finality_baseline_v1",
        "accepted":true,"attempted":attempts,"admitted":samples.len(),
        "successful_finalized_transactions":expected.len(),"success_ratio":expected.len() as f64 / attempts as f64,
        "window_seconds":duration,"finalized_tps":expected.len() as f64 / duration,
        "latency_ms_p50":percentile(&latency,50),"latency_ms_p95":percentile(&latency,95),"latency_ms_p99":percentile(&latency,99),
        "percentile_method":"nearest_rank","confirmation_observation":"all_four_nodes_RPC_finalized_and_successful_receipt",
        "latency_includes_client_HTTP_and_observation_polling":true,"poll_pause_ms":POLL_MS,
        "observation_latency_upper_bound_not_exact_commit_timestamp":true,
        "block_tx_counts":blocks.iter().map(|block| block.header.tx_count).collect::<Vec<_>>(),
        "before_storage":before,"after_storage":after,"state_version_growth":SIGNERS*BATCHES,
        "environment":environment,"workload":workload,"verification":verification,
    });
    fs::write(
        evidence.join("transfer-finality-performance.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    eprintln!(
        "record Transfer RPC-to-finality measurement: {}",
        evidence
            .join("transfer-finality-performance.json")
            .display()
    );
    eprintln!("{report}");
}
