//! Explicit local end-to-end baseline, not a component benchmark or mainnet TPS.
//! Disposable genesis/keys, real HTTP admission, WSS gossip, AOEM and BFT.
use super::*;
use anyhow::{bail, Context, Result};
use novovm_node::tx_ingress::fresh_genesis::{
    FreshGenesisConfigV1, GenesisAllocationV1, GENESIS_SCHEMA_RECORD_V2,
};
use novovm_protocol::{
    decode_nov_native_tx_wire_v1, encode_nov_native_tx_wire_v1, NovFeePolicyV1, NovNativeTxWireV1,
    NovTransferTxV1, NovTxKindV1,
};
use std::collections::BTreeSet;

pub(super) const LABEL: &str = "continuous-record-transfer-measurement";
pub(super) const DURABLE_RECEIPTS_LABEL: &str =
    "continuous-record-transfer-durable-receipts-measurement";
const SIGNERS: usize = 32;
const BATCHES: usize = 3;
const CONTINUOUS_ROUNDS: usize = 8;
const MAX_OUTSTANDING: usize = 128;
const OBSERVATIONS_PER_TURN: usize = 32;
const CLIENT_CONCURRENCY: usize = 4;
const POLL_MS: u64 = 100;
const DEADLINE: Duration = Duration::from_secs(300);

#[derive(Clone, Copy)]
pub(super) enum TransportProfile {
    LegacyLimits,
    Bounded64,
    Bounded64DurableReceipts,
    LegacyLimitsCollect250,
    Bounded64Collect250,
}

impl TransportProfile {
    fn label(self) -> &'static str {
        match self {
            Self::LegacyLimits => "legacy_limits_independent_transaction_lane",
            Self::Bounded64 => "explicit_bounded_64_transaction_lane",
            Self::Bounded64DurableReceipts => "explicit_bounded_64_durable_receipts",
            Self::LegacyLimitsCollect250 => "legacy_limits_collect_250ms",
            Self::Bounded64Collect250 => "explicit_bounded_64_collect_250ms",
        }
    }

    fn configuration(self) -> Option<Value> {
        match self {
            Self::LegacyLimits | Self::LegacyLimitsCollect250 => None,
            Self::Bounded64 | Self::Bounded64Collect250 | Self::Bounded64DurableReceipts => {
                let mut config = serde_json::json!({
                "per_peer_queue":64,"ingress_per_source_per_second":64,
                "ingress_per_poll":64,"gossip_per_peer_per_second":64,
                "gossip_per_poll":192,"bytes_per_poll":1048576,
                });
                if self.durable_receipts() {
                    config["durable_receipts"] = true.into();
                }
                Some(config)
            }
        }
    }

    fn collect_ms(self) -> u64 {
        match self {
            Self::LegacyLimits | Self::Bounded64 | Self::Bounded64DurableReceipts => 0,
            Self::LegacyLimitsCollect250 | Self::Bounded64Collect250 => 250,
        }
    }

    fn durable_receipts(self) -> bool {
        matches!(self, Self::Bounded64DurableReceipts)
    }

    fn measurement_label(self) -> &'static str {
        if self.durable_receipts() {
            DURABLE_RECEIPTS_LABEL
        } else {
            LABEL
        }
    }
}

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

pub(super) fn address_for(node: &Node, label: &str) -> Option<String> {
    fs::read_to_string(node.0.join(format!("{label}.stdout.log")))
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("native_fresh_rpc_listening: ")
                .map(str::to_owned)
        })
}

// No retry: uncertain admission is an observed failure, never hidden by a helper.
pub(super) fn rpc_once(address: &str, method: &str, params: Value) -> Result<Value> {
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

pub(super) fn finalized_blocks(node: &Node, last: u64) -> Vec<NovNativeDurableBlockV1> {
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

type SignedRaw = ([u8; 32], Vec<u8>);
type Admission = (usize, [u8; 32], f64, f64, Result<Value>);

#[derive(Default, serde::Serialize)]
struct ContinuousEvidence {
    max_outstanding_including_dispatched: usize,
    max_concurrent_requests: usize,
    submissions_while_earlier_nonce_round_pending: usize,
    capacity_limited_turns: usize,
    capacity_limited_turn_ms: f64,
    observation_requests: usize,
    completed_observation_turn_ms: f64,
    max_observation_turn_ms: f64,
    max_observation_sweep_wall_ms: f64,
    pool_samples: Vec<Value>,
    observation_errors: Vec<Value>,
}

#[derive(Default)]
struct ContinuousMeasurement {
    samples: Vec<Sample>,
    attempts: usize,
    admission_outcomes: Vec<Value>,
    successful_receipts: BTreeSet<String>,
    evidence: ContinuousEvidence,
}

impl ContinuousMeasurement {
    fn record_admissions(&mut self, results: Vec<Admission>, nodes: usize) -> Result<()> {
        let mut first_error = None;
        for (round, hash, submit_ms, admission_ms, result) in results {
            let result = result.and_then(|value| {
                if value["tx_hash"] != hex(&hash)
                    || !matches!(value["status"].as_str(), Some("queued" | "finalized"))
                {
                    bail!("unexpected admission: {value}");
                }
                Ok(value)
            });
            let outcome = match &result {
                Ok(_) => "admitted",
                Err(error) if error.to_string().starts_with("RPC nov_sendRawTransaction:") => {
                    "rpc_rejected"
                }
                Err(_) => "uncertain_admission",
            };
            self.admission_outcomes.push(serde_json::json!({
                "tx_hash":hex(&hash),"batch":round,"submit_ms":submit_ms,"admission_ms":admission_ms,
                "outcome":outcome,"result":result.as_ref().ok(),"error":result.as_ref().err().map(|e|format!("{e:#}")),
            }));
            match result {
                Ok(_) => self.samples.push(Sample {
                    tx_hash: hex(&hash),
                    batch: round,
                    submit_ms,
                    admission_ms,
                    finalized_ms_by_node: vec![None; nodes],
                }),
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        // Never lose later successes/uncertain outcomes in the same dispatched group.
        first_error.map_or(Ok(()), Err)
    }
}

fn outstanding(samples: &[Sample]) -> usize {
    samples
        .iter()
        .filter(|sample| sample.finalized_ms_by_node.iter().any(Option::is_none))
        .count()
}

fn admission_slots(
    attempted: usize,
    total: usize,
    pending: usize,
    elapsed: Duration,
) -> Result<usize> {
    if elapsed >= DEADLINE {
        bail!("continuous RPC-to-finality deadline");
    }
    if attempted > total || pending > MAX_OUTSTANDING {
        bail!("continuous client budget invariant");
    }
    Ok((total - attempted)
        .min(MAX_OUTSTANDING - pending)
        .min(CLIENT_CONCURRENCY))
}

/// Fixed transaction/node slots keep new admissions from resetting older readers.
/// A turn never scans more than one bounded ring or queries an item twice.
fn observation_window(
    samples: &[Sample],
    nodes: usize,
    total: usize,
    cursor: &mut usize,
    limit: usize,
) -> (Vec<(usize, usize)>, bool) {
    let slots = total * nodes;
    assert!(slots > 0 && *cursor < slots && samples.len() <= total);
    assert!(limit > 0 && limit <= OBSERVATIONS_PER_TURN);
    let mut selected = Vec::new();
    let mut wrapped = false;
    for _ in 0..slots {
        let position = *cursor;
        *cursor = (*cursor + 1) % slots;
        wrapped |= *cursor == 0;
        let index = position / nodes;
        let node = position % nodes;
        if samples
            .get(index)
            .is_some_and(|sample| sample.finalized_ms_by_node[node].is_none())
        {
            selected.push((index, node));
            if selected.len() == limit {
                break;
            }
        }
    }
    (selected, wrapped)
}

fn observed_success(value: &Value, hash: &str) -> Result<bool> {
    if value["tx_hash"] != hash {
        bail!("RPC transaction binding mismatch: {value}");
    }
    match value["status"].as_str() {
        Some("queued" | "unknown") => return Ok(false),
        Some("finalized") => (),
        _ => bail!("unexpected observation: {value}"),
    }
    if value["receipt"]["status"] != true || value["receipt"]["tx_hash"] != hash {
        bail!("failed or mismatched business receipt: {value}");
    }
    if !value["receipt"]["logs"]
        .as_array()
        .context("receipt logs missing")?
        .iter()
        .any(|log| {
            log["event"] == "aoem.native_transfer.computed"
                && log["data"]["scheduler"] == "aoem_generic_compute_v2"
                && log["data"]["tx_hash"] == hash
        })
    {
        bail!("receipt lacks actual AOEM transfer computation");
    }
    Ok(true)
}

fn measure_continuous(
    rounds: &[Vec<SignedRaw>],
    addresses: &[String],
    children: &mut [(usize, Child)],
    clock: Instant,
) -> (Result<()>, ContinuousMeasurement) {
    let total = SIGNERS * CONTINUOUS_ROUNDS;
    assert_eq!(rounds.len(), CONTINUOUS_ROUNDS);
    assert!(rounds.iter().all(|round| round.len() == SIGNERS));
    assert_eq!(addresses.len(), 4);
    let mut measured = ContinuousMeasurement::default();
    let mut cursor = 0;
    let mut sweep_started = Instant::now();
    let mut next_pool_sample = Duration::ZERO;
    let result = (|| -> Result<()> {
        loop {
            let pending = outstanding(&measured.samples);
            let send_count = admission_slots(measured.attempts, total, pending, clock.elapsed())?;
            if measured.attempts == total && pending == 0 {
                break;
            }
            for (_, child) in children.iter_mut() {
                if child.0.try_wait()?.is_some() {
                    bail!("node exited during continuous measurement");
                }
            }
            let turn_started = Instant::now();
            let capacity_limited = measured.attempts < total && pending == MAX_OUTSTANDING;
            let start = measured.attempts;
            measured.attempts += send_count;
            measured.evidence.max_outstanding_including_dispatched = measured
                .evidence
                .max_outstanding_including_dispatched
                .max(pending + send_count);
            measured.evidence.max_concurrent_requests =
                measured.evidence.max_concurrent_requests.max(send_count);
            measured
                .evidence
                .submissions_while_earlier_nonce_round_pending += (start..start + send_count)
                .filter(|index| {
                    measured.samples.iter().any(|sample| {
                        sample.batch < *index / SIGNERS
                            && sample.finalized_ms_by_node.iter().any(Option::is_none)
                    })
                })
                .count();
            if send_count > 0 {
                let results = std::thread::scope(|scope| {
                    let handles: Vec<_> = (start..start + send_count)
                        .map(|index| {
                            let (hash, raw) = &rounds[index / SIGNERS][index % SIGNERS];
                            let address = &addresses[0];
                            scope.spawn(move || {
                                let submitted = clock.elapsed().as_secs_f64() * 1000.0;
                                let result = rpc_once(
                                    address,
                                    "nov_sendRawTransaction",
                                    serde_json::json!([hex(raw)]),
                                );
                                (
                                    index / SIGNERS,
                                    *hash,
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
                        .collect()
                });
                measured.record_admissions(results, addresses.len())?;
                // Recheck the deadline and the existing credit on every group.
                // Queries cannot preempt refill: at most MAX_OUTSTANDING / 4
                // successful groups reach the same fixed outstanding cap.
                continue;
            }

            let observation_started = Instant::now();
            let mut wrapped = false;
            let mut observed_this_turn = BTreeSet::new();
            for _ in 0..OBSERVATIONS_PER_TURN.div_ceil(CLIENT_CONCURRENCY) {
                if admission_slots(
                    measured.attempts,
                    total,
                    outstanding(&measured.samples),
                    clock.elapsed(),
                )? > 0
                {
                    break;
                }
                // Commit the fair cursor only for a dispatched group. A newly
                // released credit must not wait behind the rest of a 32-query
                // window, or lose the unqueried entries from that window.
                let (mut chunk, group_wrapped) = observation_window(
                    &measured.samples,
                    addresses.len(),
                    total,
                    &mut cursor,
                    CLIENT_CONCURRENCY,
                );
                wrapped |= group_wrapped;
                chunk.retain(|pair| observed_this_turn.insert(*pair));
                if chunk.is_empty() {
                    break;
                }
                measured.evidence.observation_requests += chunk.len();
                measured.evidence.max_concurrent_requests =
                    measured.evidence.max_concurrent_requests.max(chunk.len());
                let results = std::thread::scope(|scope| {
                    let handles: Vec<_> = chunk
                        .iter()
                        .map(|&(index, node)| {
                            let address = &addresses[node];
                            let hash = &measured.samples[index].tx_hash;
                            scope.spawn(move || {
                                (
                                    index,
                                    node,
                                    rpc_once(
                                        address,
                                        "nov_getTransactionStatus",
                                        serde_json::json!([hash]),
                                    ),
                                    clock.elapsed().as_secs_f64() * 1000.0,
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
                for (index, node, result, observed) in results {
                    let checked = match &result {
                        Ok(value) => observed_success(value, &measured.samples[index].tx_hash),
                        Err(error) => Err(anyhow::anyhow!("{error:#}")),
                    };
                    match checked {
                        Ok(true) => {
                            measured.samples[index].finalized_ms_by_node[node] = Some(observed);
                            measured
                                .successful_receipts
                                .insert(measured.samples[index].tx_hash.clone());
                        }
                        Ok(false) => (),
                        Err(error) => {
                            measured.evidence.observation_errors.push(serde_json::json!({"index":index,"node":node,"observed_ms":observed,"response":result.as_ref().ok(),"error":format!("{error:#}")}));
                            if first_error.is_none() {
                                first_error = Some(error);
                            }
                        }
                    }
                }
                if let Some(error) = first_error {
                    return Err(error);
                }
            }
            let observation_ms = observation_started.elapsed().as_secs_f64() * 1000.0;
            measured.evidence.completed_observation_turn_ms += observation_ms;
            measured.evidence.max_observation_turn_ms = measured
                .evidence
                .max_observation_turn_ms
                .max(observation_ms);
            if wrapped {
                measured.evidence.max_observation_sweep_wall_ms = measured
                    .evidence
                    .max_observation_sweep_wall_ms
                    .max(sweep_started.elapsed().as_secs_f64() * 1000.0);
                sweep_started = Instant::now();
            }

            if admission_slots(
                measured.attempts,
                total,
                outstanding(&measured.samples),
                clock.elapsed(),
            )? > 0
            {
                // Preserve the original turn-start-at-cap measurement, while
                // giving freed credit priority over chain-status sampling too.
                if capacity_limited {
                    measured.evidence.capacity_limited_turns += 1;
                    measured.evidence.capacity_limited_turn_ms +=
                        turn_started.elapsed().as_secs_f64() * 1000.0;
                }
                continue;
            }

            if clock.elapsed() >= next_pool_sample {
                admission_slots(
                    measured.attempts,
                    total,
                    outstanding(&measured.samples),
                    clock.elapsed(),
                )?;
                let results = std::thread::scope(|scope| {
                    let handles: Vec<_> = addresses
                        .iter()
                        .enumerate()
                        .map(|(node, address)| {
                            scope.spawn(move || {
                                (
                                    node,
                                    rpc_once(address, "nov_chainStatus", serde_json::json!([])),
                                    clock.elapsed().as_secs_f64() * 1000.0,
                                )
                            })
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|handle| handle.join().unwrap())
                        .collect::<Vec<_>>()
                });
                measured.evidence.max_concurrent_requests = measured
                    .evidence
                    .max_concurrent_requests
                    .max(addresses.len());
                let mut first_error = None;
                let mut states = Vec::new();
                for (node, result, observed) in results {
                    states.push(serde_json::json!({"node":node,"observed_ms":observed,
                        "height":result.as_ref().ok().map(|v|&v["height"]),
                        "durable_pending_transactions":result.as_ref().ok().map(|v|&v["durable_pending_transactions"]),
                        "phase":result.as_ref().ok().map(|v|&v["phase"]),
                        "lifecycle_halted":result.as_ref().ok().map(|v|&v["lifecycle_halted"]),
                        "transaction_transport":result.as_ref().ok().map(|v|&v["transaction_transport"]),
                        "error":result.as_ref().err().map(|e|format!("{e:#}")),
                    }));
                    let result = result.and_then(|value| {
                        if value["lifecycle_halted"] != false
                            || value["durable_pending_transactions"].as_u64().is_none()
                        {
                            bail!("invalid live pool sample: {value}");
                        }
                        Ok(())
                    });
                    if first_error.is_none() {
                        first_error = result.err();
                    }
                }
                measured.evidence.pool_samples.push(serde_json::json!({"attempted":measured.attempts,"admitted":measured.samples.len(),"client_outstanding":outstanding(&measured.samples),"nodes":states}));
                if let Some(error) = first_error {
                    return Err(error);
                }
                next_pool_sample = clock.elapsed() + Duration::from_secs(1);
            }
            // Pause once per completed observation sweep, not per four queries.
            // Available admission capacity always gets another immediate turn.
            if wrapped
                && (measured.attempts == total || outstanding(&measured.samples) == MAX_OUTSTANDING)
            {
                std::thread::sleep(
                    Duration::from_millis(POLL_MS).min(DEADLINE.saturating_sub(clock.elapsed())),
                );
            }
            if capacity_limited {
                measured.evidence.capacity_limited_turns += 1;
                measured.evidence.capacity_limited_turn_ms +=
                    turn_started.elapsed().as_secs_f64() * 1000.0;
            }
        }
        Ok(())
    })();
    (result, measured)
}

fn verify_continuous_nonce_order(blocks: &[NovNativeDurableBlockV1], rounds: &[Vec<SignedRaw>]) {
    let senders: Vec<_> = (0..SIGNERS)
        .map(|index| novovm_adapter_novovm::address_from_seed_v1([64 + index as u8; 32]))
        .collect();
    let mut next = vec![0u64; SIGNERS];
    next[0] = 1; // bootstrap already consumed signer zero's nonce zero.
    let mut seen = BTreeSet::new();
    for block in blocks.iter().skip(1) {
        assert_eq!(block.body.raw_txs.len(), block.body.tx_hashes.len());
        for (raw, hash) in block.body.raw_txs.iter().zip(&block.body.tx_hashes) {
            let NovTxKindV1::Transfer(transfer) = decode_nov_native_tx_wire_v1(raw).unwrap().kind
            else {
                panic!("expected signed Transfer");
            };
            let signer = senders
                .iter()
                .position(|sender| sender == &transfer.from)
                .unwrap();
            assert_eq!(transfer.nonce, next[signer]);
            let round = (transfer.nonce - u64::from(signer == 0)) as usize;
            assert_eq!(
                (hash, raw),
                (&rounds[round][signer].0, &rounds[round][signer].1)
            );
            assert!(seen.insert(*hash));
            next[signer] += 1;
        }
    }
    assert_eq!(seen.len(), SIGNERS * CONTINUOUS_ROUNDS);
    assert_eq!(
        next,
        (0..SIGNERS)
            .map(|signer| CONTINUOUS_ROUNDS as u64 + u64::from(signer == 0))
            .collect::<Vec<_>>()
    );
}

pub(super) fn exercise(nodes: &[Node], evidence: &std::path::Path, profile: TransportProfile) {
    exercise_workload(nodes, evidence, profile, false);
}

pub(super) fn exercise_continuous(nodes: &[Node], evidence: &std::path::Path) {
    exercise_workload(nodes, evidence, TransportProfile::Bounded64, true);
}

pub(super) fn exercise_continuous_durable_receipts(nodes: &[Node], evidence: &std::path::Path) {
    exercise_workload(
        nodes,
        evidence,
        TransportProfile::Bounded64DurableReceipts,
        true,
    );
}

fn exercise_workload(
    nodes: &[Node],
    evidence: &std::path::Path,
    profile: TransportProfile,
    continuous: bool,
) {
    let rounds = if continuous {
        CONTINUOUS_ROUNDS
    } else {
        BATCHES
    };
    // Bootstrap/genesis checks above are outside the measurement window.
    let before: Vec<_> = nodes.iter().map(storage).collect();
    for node in nodes {
        let path = node.0.join("seal.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["follow_finalized_tip"] = true.into();
        config["receive_successors"] = true.into();
        config["propose_successors"] = true.into();
        config["proposal_max_transactions"] = SIGNERS.into();
        config["proposal_collect_ms"] = profile.collect_ms().into();
        if let Some(transport) = profile.configuration() {
            config["transaction_transport"] = transport;
        }
        fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    }
    let original_configs: Vec<_> = nodes
        .iter()
        .map(|node| fs::read(node.0.join("seal.json")).unwrap())
        .collect();
    let batches: Vec<Vec<_>> = (0..rounds)
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
    let mut children = start_cluster(nodes, &active, profile.measurement_label(), 0);
    let readiness = Instant::now();
    let addresses = loop {
        if let Some(addresses) = nodes
            .iter()
            .map(|node| address_for(node, profile.measurement_label()))
            .collect::<Option<Vec<_>>>()
        {
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
    let mut continuous_evidence = None;
    let measurement = if continuous {
        let (result, measured) = measure_continuous(&batches, &addresses, &mut children, clock);
        samples = measured.samples;
        attempts = measured.attempts;
        admission_outcomes = measured.admission_outcomes;
        successful_receipts = measured.successful_receipts;
        continuous_evidence = Some(measured.evidence);
        result
    } else {
        (|| -> Result<()> {
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
                                || !matches!(
                                    result["status"].as_str(),
                                    Some("queued" | "finalized")
                                )
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
                                .filter_map(move |(node, done)| {
                                    done.is_none().then_some((index, node))
                                })
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
                                        (
                                            index,
                                            node,
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
        })()
    };
    // Preserve failed attempts and partial observations before any assertion.
    fs::write(
        evidence.join("transfer-finality-observations.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "attempted":attempts,"accepted":samples.len(),"samples":samples,
            "admission_outcomes":admission_outcomes,
            "error":measurement.as_ref().err().map(|error| format!("{error:#}")),
            "implicit_rpc_retries":0,"batch_count":if continuous {None}else{Some(BATCHES)},"signers":SIGNERS,
            "nonce_rounds":rounds,"continuous_backlog":continuous_evidence,
            "slow_call_diagnostics_enabled":diagnostics_enabled(),
            "transaction_transport_profile":profile.label(),
            "durable_receipts_enabled":profile.durable_receipts(),
            "measurement_label":profile.measurement_label(),
            "proposal_collect_ms":profile.collect_ms(),
            "client_refill_policy":if continuous {"credit_first_bounded_v2"} else {"batch_finality_barrier"},
        }))
        .unwrap(),
    )
    .unwrap();
    measurement.unwrap_or_else(|error| panic!("{error:#}; evidence: {}", evidence.display()));
    let statuses: Vec<_> = addresses
        .iter()
        .map(|address| rpc_once(address, "nov_chainStatus", serde_json::json!([])).unwrap())
        .collect();
    let ack_acceptances: Vec<_> = statuses
        .iter()
        .map(|status| {
            status["transaction_transport"]["recipient_acks_accepted"]
                .as_u64()
                .expect("running node must report durable receipt acceptance count")
        })
        .collect();
    if profile.durable_receipts() {
        // Save actual counters before assertions, including when this profile
        // was configured but did not exercise a genuine multi-node ACK route.
        fs::write(
            evidence.join("transfer-recipient-ack-observations.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "transaction_transport_profile":profile.label(),
                "measurement_label":profile.measurement_label(),
                "recipient_ack_acceptances_by_node":ack_acceptances,
                "accepted_counts_include_duplicate_valid_acks":true,
                "accepted_counts_are_not_unique_transactions_or_finality":true,
                "transaction_transport_status_by_node":statuses.iter()
                    .map(|status|&status["transaction_transport"]).collect::<Vec<_>>(),
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(
            ack_acceptances[0] > 0,
            "single-ingress node did not consume durable peer receipts"
        );
        assert!(
            ack_acceptances.iter().filter(|count| **count > 0).count() >= 2,
            "durable receipt profile must exercise acceptance on at least two nodes"
        );
    }
    let tip = statuses[0]["height"].as_u64().unwrap();
    let expected_transport = profile.configuration().unwrap_or_else(|| {
        serde_json::json!({
            "per_peer_queue":4,"ingress_per_source_per_second":8,
            "ingress_per_poll":16,"gossip_per_peer_per_second":4,
            "gossip_per_poll":64,"bytes_per_poll":1048576,
        })
    });
    for status in &statuses {
        assert_eq!(status["height"], tip);
        assert_eq!(status["finalized"], true);
        assert_eq!(status["lifecycle_halted"], false);
        if continuous {
            assert_eq!(status["durable_pending_transactions"], 0);
        }
        assert_eq!(status["proposal_collect_ms"], profile.collect_ms());
        assert_eq!(
            status["transaction_transport"]["limits"]["durable_receipts"],
            profile.durable_receipts()
        );
        for (field, expected) in expected_transport.as_object().unwrap() {
            assert_eq!(
                &status["transaction_transport"]["limits"][field], expected,
                "running node must use the declared transaction transport profile"
            );
        }
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
    assert_eq!(actual.len(), SIGNERS * rounds);
    assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), expected);
    assert_eq!(successful_receipts, expected);
    assert_eq!(
        blocks.last().unwrap().header.state_version - blocks[0].header.state_version,
        (SIGNERS * rounds) as u64
    );
    if continuous {
        verify_continuous_nonce_order(&blocks, &batches);
    }
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
    let last_submit = samples
        .iter()
        .map(|sample| sample.submit_ms)
        .fold(0.0, f64::max);
    let last_admission = samples
        .iter()
        .map(|sample| sample.admission_ms)
        .fold(0.0, f64::max);
    let after: Vec<_> = nodes.iter().map(storage).collect();
    run_cluster_at_height(
        nodes,
        &active,
        if profile.durable_receipts() {
            "continuous-transfer-durable-receipts-measurement-restart"
        } else {
            "continuous-transfer-measurement-restart"
        },
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
        "seal_ingress_per_source_per_second":8,"seal_ingress_per_poll":16,
        "transaction_transport_profile":profile.label(),
        "durable_receipts_enabled":profile.durable_receipts(),
        "measurement_label":profile.measurement_label(),
        "recipient_ack_acceptances_by_node":ack_acceptances,
        "recipient_ack_counts_include_duplicates_not_unique_finality":true,
        "transaction_transport_status_by_node":statuses.iter().map(|status|&status["transaction_transport"]).collect::<Vec<_>>(),
        "proposal_collection_status_by_node":statuses.iter().map(|status|&status["proposal_collection"]).collect::<Vec<_>>(),
    });
    let workload = serde_json::json!({
        "batches":if continuous {None}else{Some(BATCHES)},"transactions_per_batch":if continuous {None}else{Some(SIGNERS)},"disjoint_account_pairs_per_batch":if continuous {None}else{Some(SIGNERS)},
        "submission_mode":if continuous {"bounded_continuous_replenishment"} else {"batch_finality_barrier"},
        "client_refill_policy":if continuous {"credit_first_bounded_v2"} else {"batch_finality_barrier"},
        "observation_yields_to_available_admission_credit":continuous,
        "nonce_rounds":rounds,"transactions_per_signer":rounds,
        "same_signer_transactions_conflict":true,"distinct_signer_account_pairs_disjoint":true,
        "client_outstanding_cap":if continuous {Some(MAX_OUTSTANDING)}else{None},
        "observation_requests_per_turn":if continuous {Some(OBSERVATIONS_PER_TURN)}else{None},
        "deadline_seconds":DEADLINE.as_secs(),"rpc_timeout_seconds":10,
        "deadline_stops_new_dispatch":continuous,
        "capacity_limited_time_scope":"completed turns beginning at client outstanding cap",
        "single_ingress_validator_index":0,"replicated_client_fanout":false,"client_concurrency":CLIENT_CONCURRENCY,
        "proposal_max_transactions":SIGNERS,"pre_signed_transactions":true,"signing_time_included":false,"bootstrap_time_included":false,
        "proposal_collect_ms":profile.collect_ms(),
        "genesis_schema":GENESIS_SCHEMA_RECORD_V2,"first_measured_height":2,"last_finalized_height":tip,
    });
    let verification = serde_json::json!({
        "full_blocks_equal_on_all_nodes":true,"all_durable_BFT_proofs_verified":true,"restart_readback_equal":true,
        "physical_lan_executed":false,"public_network_executed":false,"production_signoff":false,
        "report_is_short_baseline_not_saturation_or_long_soak":true,
    });
    // This report is reached only after full-block membership, successful
    // receipts on all four nodes, and restart checks. The failure count below
    // is therefore derived from those observations, not a mixed-load result.
    let finalized_transactions: usize = blocks
        .iter()
        .skip(1)
        .map(|block| block.body.tx_hashes.len())
        .sum();
    let finalized_successful = samples
        .iter()
        .filter(|sample| {
            sample.finalized_ms_by_node.iter().all(Option::is_some)
                && successful_receipts.contains(&sample.tx_hash)
        })
        .count();
    let admission_errors = admission_outcomes
        .iter()
        .filter_map(|outcome| outcome["error"].as_str())
        .count();
    let admission_rejected = admission_outcomes
        .iter()
        .filter_map(|outcome| outcome["error"].as_str())
        .filter(|error| error.starts_with("RPC nov_sendRawTransaction:"))
        .count();
    let report = serde_json::json!({
        "scope":if continuous {"same_host_four_process_record_transfer_continuous_backlog_v1"}else{"same_host_four_process_record_transfer_rpc_finality_baseline_v1"},
        "accepted":true,"attempted":attempts,"admitted":samples.len(),
        "successful_finalized_transactions":expected.len(),"success_ratio":expected.len() as f64 / attempts as f64,
        "finalized_successful_transactions":finalized_successful,
        "finalized_failed_business_transactions":finalized_transactions-finalized_successful,
        "admission_rejected_transactions":admission_rejected,
        "admission_uncertain_transactions":admission_errors-admission_rejected,
        "admitted_unresolved_transactions":outstanding(&samples),
        "dispatched_without_admission_outcome":attempts-admission_outcomes.len(),
        "not_attempted_transactions":SIGNERS*rounds-attempts,
        "counting_scope":{
            "finalized":"unique measured transactions in verified durable blocks, excluding bootstrap",
            "successful":"finalized with successful receipts observed on all four nodes",
            "failed_business":"durable measured transaction count minus all-four successful count, after existing all-success assertions",
            "admission_uncertain_is_not_business_failure":true,
            "business_failure_workload_exercised":false,
            "report_requires_all_success":"failed runs retain transfer-finality-observations.json and do not produce this accepted performance report"
        },
        "window_seconds":duration,"finalized_tps":expected.len() as f64 / duration,
        "latency_ms_p50":percentile(&latency,50),"latency_ms_p95":percentile(&latency,95),"latency_ms_p99":percentile(&latency,99),
        "percentile_method":"nearest_rank","confirmation_observation":"all_four_nodes_RPC_finalized_and_successful_receipt",
        "latency_includes_client_HTTP_and_observation_polling":true,"poll_pause_ms":POLL_MS,
        "observation_latency_upper_bound_not_exact_commit_timestamp":true,
        "block_tx_counts":blocks.iter().map(|block| block.header.tx_count).collect::<Vec<_>>(),
        "before_storage":before,"after_storage":after,"state_version_growth":SIGNERS*rounds,
        "first_submit_ms":first_submit,"last_submit_ms":last_submit,"last_admission_ms":last_admission,
        "submission_window_ms":last_submit-first_submit,"continuous_backlog":continuous_evidence,
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

#[cfg(test)]
mod continuous_scheduler_tests {
    use super::*;

    #[test]
    fn durable_receipt_profile_changes_only_the_explicit_receipt_flag() {
        let baseline = TransportProfile::Bounded64;
        let enabled = TransportProfile::Bounded64DurableReceipts;
        let mut config = enabled.configuration().unwrap();
        assert_eq!(
            config.as_object_mut().unwrap().remove("durable_receipts"),
            Some(true.into())
        );
        assert_eq!(Some(config), baseline.configuration());
        assert!(!baseline.durable_receipts());
        assert!(enabled.durable_receipts());
        assert_eq!(enabled.collect_ms(), baseline.collect_ms());
        assert_ne!(enabled.label(), baseline.label());
        assert_ne!(enabled.measurement_label(), baseline.measurement_label());
        assert_eq!(SIGNERS * BATCHES, 96);
        assert_eq!(SIGNERS * CONTINUOUS_ROUNDS, 256);
        assert_eq!(DEADLINE, Duration::from_secs(300));
    }

    fn sample(index: usize) -> Sample {
        Sample {
            tx_hash: format!("{index:064x}"),
            batch: index / SIGNERS,
            submit_ms: 0.0,
            admission_ms: 1.0,
            finalized_ms_by_node: vec![None; 4],
        }
    }

    #[test]
    fn continuous_backlog_refills_without_a_batch_barrier() {
        let total = SIGNERS * CONTINUOUS_ROUNDS;
        let mut samples: Vec<_> = (0..MAX_OUTSTANDING).map(sample).collect();
        assert_eq!(
            admission_slots(128, total, outstanding(&samples), Duration::ZERO).unwrap(),
            0
        );
        samples[0].finalized_ms_by_node = vec![Some(2.0), Some(2.0), Some(2.0), None];
        assert_eq!(outstanding(&samples), 128);
        samples[0].finalized_ms_by_node[3] = Some(3.0);
        // Refill one slot while the other 127 transactions are still outstanding.
        assert_eq!(
            admission_slots(128, total, outstanding(&samples), Duration::ZERO).unwrap(),
            1
        );
        assert_eq!(
            admission_slots(128, total, 0, Duration::ZERO).unwrap(),
            CLIENT_CONCURRENCY
        );
        assert_eq!(
            admission_slots(total - 2, total, 0, Duration::ZERO).unwrap(),
            2
        );
        assert_eq!(admission_slots(total, total, 1, Duration::ZERO).unwrap(), 0);
        assert!(admission_slots(0, total, 129, Duration::ZERO).is_err());
        assert!(admission_slots(total + 1, total, 0, Duration::ZERO).is_err());
        assert!(admission_slots(0, total, 0, DEADLINE).is_err());
        assert!(admission_slots(0, total, 0, DEADLINE - Duration::from_nanos(1)).is_ok());
    }

    #[test]
    fn credit_first_refill_reaches_existing_cap_before_observation() {
        let total = SIGNERS * CONTINUOUS_ROUNDS;
        let mut measured = ContinuousMeasurement::default();
        let mut groups = Vec::new();
        loop {
            let count = admission_slots(
                measured.attempts,
                total,
                outstanding(&measured.samples),
                Duration::ZERO,
            )
            .unwrap();
            if count == 0 {
                break;
            }
            let start = measured.attempts;
            measured.attempts += count;
            let results = (start..start + count)
                .map(|index| {
                    let mut hash = [0; 32];
                    hash[..8].copy_from_slice(&(index as u64).to_le_bytes());
                    (
                        index / SIGNERS,
                        hash,
                        0.0,
                        1.0,
                        Ok(serde_json::json!({"tx_hash":hex(&hash),"status":"queued"})),
                    )
                })
                .collect();
            measured.record_admissions(results, 4).unwrap();
            groups.push(count);
        }
        assert_eq!(groups, vec![CLIENT_CONCURRENCY; 32]);
        assert_eq!(measured.attempts, MAX_OUTSTANDING);
        assert_eq!(measured.admission_outcomes.len(), MAX_OUTSTANDING);
        assert_eq!(outstanding(&measured.samples), MAX_OUTSTANDING);

        // Refill groups cannot silently extend the global deadline.
        assert!(admission_slots(4, total, 4, DEADLINE).is_err());
        assert_eq!(
            admission_slots(total - 2, total, 0, Duration::ZERO).unwrap(),
            2
        );
        assert_eq!(admission_slots(total, total, 2, Duration::ZERO).unwrap(), 0);
    }

    #[test]
    fn credit_first_observation_yields_without_skipping_unqueried_entries() {
        let total = SIGNERS * CONTINUOUS_ROUNDS;
        let mut samples: Vec<_> = (0..MAX_OUTSTANDING).map(sample).collect();
        let mut attempts = samples.len();
        let mut cursor = 0;
        let (group, _) = observation_window(&samples, 4, total, &mut cursor, CLIENT_CONCURRENCY);
        assert_eq!(group, vec![(0, 0), (0, 1), (0, 2), (0, 3)]);
        samples[0].finalized_ms_by_node[..3].fill(Some(2.0));
        assert_eq!(
            admission_slots(attempts, total, outstanding(&samples), Duration::ZERO).unwrap(),
            0,
            "three observers are not four-node finality"
        );
        samples[0].finalized_ms_by_node[3] = Some(2.0);
        assert_eq!(
            admission_slots(attempts, total, outstanding(&samples), Duration::ZERO).unwrap(),
            1
        );
        // The first completed observation group returns immediately to refill;
        // it did not advance the cursor over the other 28 unissued requests.
        samples.push(sample(attempts));
        attempts += 1;
        let (next, _) = observation_window(&samples, 4, total, &mut cursor, CLIENT_CONCURRENCY);
        assert_eq!(next, vec![(1, 0), (1, 1), (1, 2), (1, 3)]);

        for sample in &mut samples[1..10] {
            sample.finalized_ms_by_node.fill(Some(3.0));
        }
        let cursor_before_refill = cursor;
        let mut groups = Vec::new();
        loop {
            let count =
                admission_slots(attempts, total, outstanding(&samples), Duration::ZERO).unwrap();
            if count == 0 {
                break;
            }
            samples.extend((attempts..attempts + count).map(sample));
            attempts += count;
            groups.push(count);
        }
        assert_eq!(groups, vec![4, 4, 1]);
        assert_eq!(outstanding(&samples), MAX_OUTSTANDING);
        assert_eq!(cursor, cursor_before_refill);
    }

    #[test]
    fn credit_first_small_observation_groups_do_not_duplicate_sparse_queries() {
        let samples = vec![sample(0)];
        let total = SIGNERS * CONTINUOUS_ROUNDS;
        let mut cursor = 0;
        let mut observed = BTreeSet::new();
        let mut dispatched = Vec::new();
        for _ in 0..OBSERVATIONS_PER_TURN.div_ceil(CLIENT_CONCURRENCY) {
            let (mut chunk, _) =
                observation_window(&samples, 4, total, &mut cursor, CLIENT_CONCURRENCY);
            chunk.retain(|pair| observed.insert(*pair));
            if chunk.is_empty() {
                break;
            }
            dispatched.extend(chunk);
        }
        assert_eq!(dispatched, vec![(0, 0), (0, 1), (0, 2), (0, 3)]);
    }

    #[test]
    fn continuous_backlog_observation_cursor_is_bounded_and_fair() {
        let total = SIGNERS * CONTINUOUS_ROUNDS;
        let mut samples: Vec<_> = (0..MAX_OUTSTANDING).map(sample).collect();
        let mut cursor = 0;
        let mut seen = BTreeSet::new();
        for _ in 0..(MAX_OUTSTANDING * 4 / OBSERVATIONS_PER_TURN) {
            let (window, _) =
                observation_window(&samples, 4, total, &mut cursor, OBSERVATIONS_PER_TURN);
            assert_eq!(window.len(), OBSERVATIONS_PER_TURN);
            for pair in window {
                assert!(seen.insert(pair));
            }
        }
        assert_eq!(seen.len(), MAX_OUTSTANDING * 4);
        samples[0].finalized_ms_by_node.fill(Some(2.0));
        samples.push(sample(MAX_OUTSTANDING));
        let mut after_growth = BTreeSet::new();
        let mut wrapped = false;
        for _ in 0..32 {
            let (window, did_wrap) =
                observation_window(&samples, 4, total, &mut cursor, OBSERVATIONS_PER_TURN);
            wrapped |= did_wrap;
            assert!(window.len() <= OBSERVATIONS_PER_TURN);
            assert_eq!(
                window.iter().copied().collect::<BTreeSet<_>>().len(),
                window.len()
            );
            assert!(window.iter().all(|(index, _)| *index != 0));
            after_growth.extend(window);
        }
        assert!(wrapped);
        for index in 1..=MAX_OUTSTANDING {
            for node in 0..4 {
                assert!(after_growth.contains(&(index, node)));
            }
        }
    }

    #[test]
    fn continuous_backlog_preserves_every_dispatched_admission_outcome() {
        let admitted =
            |hash: [u8; 32]| Ok(serde_json::json!({"tx_hash":hex(&hash),"status":"queued"}));
        let results = vec![
            (
                0,
                [1; 32],
                1.0,
                2.0,
                Err(anyhow::anyhow!("RPC nov_sendRawTransaction: rejected")),
            ),
            (0, [2; 32], 1.0, 2.0, admitted([2; 32])),
            (
                0,
                [3; 32],
                1.0,
                3.0,
                Err(anyhow::anyhow!("network timeout after sending")),
            ),
            (0, [4; 32], 1.0, 2.0, admitted([4; 32])),
        ];
        let mut measured = ContinuousMeasurement {
            attempts: results.len(),
            ..Default::default()
        };
        assert!(measured.record_admissions(results, 4).is_err());
        assert_eq!(measured.attempts, 4);
        assert_eq!(measured.admission_outcomes.len(), 4);
        assert_eq!(measured.admission_outcomes[0]["outcome"], "rpc_rejected");
        assert_eq!(
            measured.admission_outcomes[2]["outcome"],
            "uncertain_admission"
        );
        assert_eq!(measured.samples.len(), 2);
        assert_eq!(measured.samples[0].tx_hash, hex(&[2; 32]));
        assert_eq!(measured.samples[1].tx_hash, hex(&[4; 32]));
        assert_eq!(outstanding(&measured.samples), 2);
    }
}
