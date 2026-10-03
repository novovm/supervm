//! Three online original executables must not wait for an absent fourth peer's
//! gossip TTL. The fourth later joins from the unchanged genesis/archive, then
//! all four cold-reopen. No externally manufactured votes or reduced quorum.
use super::*;

const BATCHES: u64 = 16;
const MAX_PROGRESS_SAMPLES: usize = 512;

fn enter_phase(
    evidence: &mut Value,
    phase: &mut &'static str,
    phase_started: &mut Instant,
    next: &'static str,
) {
    record_phase(evidence, phase, *phase_started, true);
    *phase = next;
    *phase_started = Instant::now();
}

fn record_phase(evidence: &mut Value, phase: &str, started: Instant, completed: bool) {
    evidence["phase_timings"]
        .as_array_mut()
        .expect("fixture phase timings initialized")
        .push(
            json!({"phase":phase,"elapsed_ms":started.elapsed().as_millis(),
            "completed":completed}),
        );
}

// These are separately queried observations, not an atomic cluster snapshot.
// In particular pending > 0 does not prove an executable nonce prefix or a
// current candidate body. Older binaries do not expose input_availability;
// absence of this diagnostic must not be interpreted as absence of a body.
fn progress_node_status(
    index: usize,
    status: &Value,
    validator_indices: &BTreeMap<String, usize>,
    online: &[usize],
) -> Value {
    let proposer = status["scheduled_proposer"]
        .as_str()
        .and_then(|id| validator_indices.get(id))
        .copied();
    let pool_observation = if status["local_proposer"] != true {
        "not_local_proposer"
    } else {
        match status["pending"].as_u64() {
            Some(0) => "empty_pending_observed",
            Some(_) => "nonempty_pending_observed_executability_unknown",
            None => "pending_not_observable",
        }
    };
    json!({"node":index,"height":status["head"]["height"].as_u64().unwrap_or(0),
        "current_height":status["current_height"],"round":status["round"],
        "pending":status["pending"],"pending_bytes":status["pending_bytes"],
        "leader":status["scheduled_proposer"],"local_leader":status["local_proposer"],
        "scheduled_proposer_fixture_index":proposer,
        "scheduled_proposer_configured_online":proposer.map(|node|online.contains(&node)),
        "local_proposer_pool_observation":pool_observation,
        "input_availability":status["input_availability"],
        "current_body_observation":if status["input_availability"].is_null() {
            "not_exposed_by_public_chain_status"
        } else { "see_input_availability_not_a_root_cause_inference" },
        "rpc_indexed_height":status["rpc_indexed_height"],
        "projection_error":status["projection_error"],
        "recovery_in_progress":status["recovery_in_progress"],
        "executed_batches":status["executed_batches"],
        "durable_decisions":status["durable_decisions"],
        "execution_failures":status["execution_failures"],
        "early_authentication_started":status["early_authentication_started"],
        "early_authentication_completed":status["early_authentication_completed"],
        "early_bind_reused":status["early_bind_reused"],
        "gossip_verified_inputs":status["gossip_verified_inputs"],
        "gossip_rejected_inputs":status["gossip_rejected_inputs"],
        "rpc_authentication":status["rpc_authentication"],
        "transaction_gossip":status["transaction_gossip"],
        "gossip_repair":status["gossip_repair"],
        "last_error":status["last_error"]})
}

fn propagation_counts(status: &Value) -> Result<(u64, u64, u64)> {
    let total = status["transaction_gossip"]["outbound_batches_accepted"]
        .as_u64()
        .context("accepted gossip batch count")?;
    let repair = status["gossip_repair"]
        .get("repair_batches_reserved")
        .map(|value| value.as_u64().context("reserved repair batch count"))
        .transpose()?
        .unwrap_or(0); // The old comparison executable has no repair counter.
    let fresh = total
        .checked_sub(repair)
        .context("repair reservations exceed total gossip acceptances")?;
    Ok((total, repair, fresh))
}

// A deliberately absent rotating proposer consumes the unchanged production
// 5s propose/prevote/precommit timers at several of the sixteen heights. Bound
// a stall separately from the sum of those faults, and record actual progress.
// This is NOT a throughput timer or permission to expire the propagation gate.
fn wait_online_progress(
    nodes: &mut ProductNodes,
    online: &[usize],
    evidence: &mut Value,
) -> Result<()> {
    let started = Instant::now();
    let mut progressed = started;
    let mut common_height = 0;
    let mut history = Vec::new();
    let mut samples_dropped = 0;
    let mut maximum_no_progress = Duration::ZERO;
    let validator_indices = (0..nodes.processes.len())
        .map(|index| {
            let validator = Validator::new(validator_key(index).verifying_key().to_bytes(), 1)?;
            Ok((hex(&validator.id()), index))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut sampled = started - Duration::from_secs(1);
    loop {
        nodes.alive()?;
        let statuses = online
            .iter()
            .map(|index| {
                let status = nodes.rpc(*index, "nov_chainStatus", json!([]))?;
                Ok(progress_node_status(
                    *index,
                    &status,
                    &validator_indices,
                    online,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let minimum = statuses
            .iter()
            .filter_map(|status| status["height"].as_u64())
            .min()
            .unwrap_or(0);
        ensure!(
            minimum >= common_height,
            "online published height regressed"
        );
        let advanced = minimum > common_height;
        let now = Instant::now();
        maximum_no_progress = maximum_no_progress.max(now.duration_since(progressed));
        if advanced {
            common_height = minimum;
            progressed = now;
        }
        // Update this even if no periodic sample is due, so timeout/failure
        // reports retain the actual last queried states and observed stall.
        evidence["online_last_status"] = json!(statuses);
        evidence["online_progress_summary"] = json!({
            "elapsed_ms":started.elapsed().as_millis(),"common_published_height":common_height,
            "since_common_height_progress_ms":progressed.elapsed().as_millis(),
            "max_observed_no_progress_ms":maximum_no_progress.as_millis(),
            "clock_origin":"after propagation gate; before published-height polling",
            "observation_scope":"sequential HTTP observations, not atomic or internal commit timestamps",
            "input_availability_reported_by_all_nodes":statuses.iter()
                .all(|status|!status["input_availability"].is_null())});
        if advanced || sampled.elapsed() >= Duration::from_secs(1) {
            if history.len() == MAX_PROGRESS_SAMPLES {
                history.remove(0);
                samples_dropped += 1;
            }
            history.push(json!({"elapsed_ms":started.elapsed().as_millis(),"nodes":statuses}));
            evidence["online_progress"] = json!(history);
            evidence["online_progress_samples_dropped"] = json!(samples_dropped);
            sampled = Instant::now();
        }
        ensure!(
            started.elapsed() < Duration::from_secs(240)
                && progressed.elapsed() < Duration::from_secs(90),
            "online quorum stopped advancing or exceeded total fault budget; last={statuses:?}"
        );
        if minimum >= BATCHES {
            evidence["online_quorum_publication_elapsed_ms"] = json!(started.elapsed().as_millis());
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "requires actual NOVOVM_RESIDENT_NODE_BINARY and real AOEM; offline recipient isolation, three-node quorum, fourth catchup and cold restart; run alone"]
fn actual_product_rpc_offline_peer_isolation_quorum_catchup_and_restart() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
            .context("explicit NOVOVM_RESIDENT_NODE_BINARY required")?,
    )
    .canonicalize()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/audit/resident-rpc-offline")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    eprintln!("offline original RPC artifacts={}", directory.display());
    let mut relay = Relay::start(&directory.join("relay"))?;
    let (mut nodes, set) = setup(&directory, &relay, binary)?;
    let genesis = genesis()?;
    let first_leader = set.leader(1, 0)?;
    let offline = (1..4)
        .find(|index| {
            Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)
                .is_ok_and(|validator| validator.id() != first_leader)
        })
        .context("offline non-ingress, non-first-leader")?;
    let online: Vec<_> = (0..4).filter(|index| *index != offline).collect();
    let all: Vec<_> = (0..4).collect();
    let mut evidence = json!({"binary_sha256":hex(&Sha256::digest(fs::read(&nodes.binary)?)),
        "aoem_sha256":hex(&Sha256::digest(fs::read(library()?)?)),
        "offline_index":offline,"online_indices":online,"ingress_index":0,
        "topology":"one host; three original binaries online, fourth absent then original binary joins; same four-validator set",
        "external_votes_or_qcs":false,"legacy_host_permission":false,"reduced_quorum":false,
        "batch_cap":2,"signed_transactions":BATCHES*2,"channel_ttl_seconds":30,
        "propagation_deadline_seconds":8,"online_no_progress_deadline_seconds":90,
        "online_total_fault_deadline_seconds":240,"production_acceptance":false,"performance_measured":false,
        "phase_timings":[],"diagnostic_sample_limit":MAX_PROGRESS_SAMPLES,
        "phase_timing_scope":"wall-clock intervals including HTTP observation and fixture checks; not production latency or TPS"});
    let mut phase = "startup";
    let mut phase_started = Instant::now();
    let result = (|| -> Result<()> {
        for index in &online {
            nodes.start(*index, "create")?;
        }
        nodes.wait_ready(&online)?;
        let raw = (0..BATCHES)
            .flat_map(|nonce| [(1, nonce, 100), (3, nonce, 50)])
            .map(|(seed, nonce, amount)| signed(seed, nonce, amount))
            .collect::<Result<Vec<_>>>()?;
        let mut hashes = transaction_hashes(&raw)?;
        enter_phase(
            &mut evidence,
            &mut phase,
            &mut phase_started,
            "single HTTP admission",
        );
        let started = Instant::now();
        let admitted = nodes.submit_batch(0, &raw)?;
        ensure!(
            admitted.len() == raw.len() && admitted.iter().all(|r| r["signature_verified"] == true),
            "offline fixture signed admission changed"
        );
        evidence["admission"] = json!(admitted);
        enter_phase(
            &mut evidence,
            &mut phase,
            &mut phase_started,
            "propagate beyond global capacity before TTL",
        );
        loop {
            nodes.alive()?;
            let status = nodes.rpc(0, "nov_chainStatus", json!([]))?;
            evidence["ingress_before_ttl"] = status.clone();
            evidence["propagation_elapsed_ms"] = json!(started.elapsed().as_millis());
            let stats = &status["transaction_gossip"];
            let (total_accepted, repair_reserved, new_input_accepted) =
                propagation_counts(&status)?;
            evidence["propagation_batch_counts"] = json!({
                "outbound_batches_accepted":total_accepted,
                "repair_batches_reserved":repair_reserved,
                "new_input_batches_accepted":new_input_accepted,
                "scope":"local queue admission, excludes repeated repair offers; not peer receipt"});
            ensure!(
                stats["expired_batches"] == 0 && stats["channel_expired_sends"] == 0,
                "peer isolation only progressed by expiring accepted sends"
            );
            ensure!(
                started.elapsed() < Duration::from_secs(8),
                "offline recipient pinned global propagation capacity before TTL: {stats}"
            );
            if new_input_accepted >= BATCHES {
                let capacity = stats["outbound_capacity"]
                    .as_u64()
                    .context("outbox capacity")?;
                let peer_capacity = stats["outbound_peer_capacity"]
                    .as_u64()
                    .context("peer capacity")?;
                ensure!(
                    capacity < BATCHES
                        && stats["outbound_recipient_skips"].as_u64().unwrap_or(0) > 0,
                    "fixture did not exercise bounded recipient isolation"
                );
                ensure!(
                    stats["outbound_peer_pending"]
                        .as_object()
                        .context("per-peer reservations")?
                        .values()
                        .all(|pending| pending.as_u64().is_some_and(|n| n <= peer_capacity)),
                    "a recipient exceeded its retained obligation budget"
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        enter_phase(
            &mut evidence,
            &mut phase,
            &mut phase_started,
            "online quorum finality (offline node excluded from observation, not validator set)",
        );
        wait_online_progress(&mut nodes, &online, &mut evidence)?;
        let online_checks_started = Instant::now();
        let three_receipts = nodes.wait_receipts(&online, &hashes)?;
        ensure!(
            three_receipts
                .iter()
                .flatten()
                .all(|r| r["success"] == true),
            "business outcome differs"
        );
        let three_status = nodes.wait_ready(&online)?;
        ensure!(
            three_status
                .iter()
                .all(|s| s["head"] == three_status[0]["head"]
                    && s["head"]["state_version"] == BATCHES * 2),
            "online quorum heads differ"
        );
        let partition = receipt_partition(&three_receipts[0], &hashes, 2)?;
        let online_balances = nodes.balances(&online, &three_receipts[0], [1600, 800])?;
        evidence["online_elapsed_ms"] = json!(started.elapsed().as_millis());
        evidence["online_receipt_and_economics_elapsed_ms"] =
            json!(online_checks_started.elapsed().as_millis());
        evidence["online_status"] = json!(three_status);
        evidence["online_receipts"] = json!(three_receipts);
        evidence["online_balances"] = json!(online_balances);
        evidence["online_partition"] = json!(partition);
        ensure!(
            nodes.processes[offline].is_none(),
            "absent node was secretly helping quorum"
        );
        enter_phase(
            &mut evidence,
            &mut phase,
            &mut phase_started,
            "fourth node archive catchup without wallet resubmission",
        );
        nodes.start(offline, "create")?;
        nodes.wait_ready(&[offline])?;
        let four_receipts = nodes.wait_receipts(&all, &hashes)?;
        ensure!(
            four_receipts.iter().all(|r| r == &three_receipts[0]),
            "catchup changed finalized receipts"
        );
        let four_status = nodes.wait_ready(&all)?;
        verify_partition_heads(&four_status, &partition, BATCHES * 2)?;
        let before_balances = nodes.balances(&all, &four_receipts[0], [1600, 800])?;
        evidence["fourth_caught_up_status"] = json!(four_status);
        evidence["fourth_catchup_elapsed_ms"] = json!(phase_started.elapsed().as_millis());
        let pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        enter_phase(
            &mut evidence,
            &mut phase,
            &mut phase_started,
            "stop and audit confirmed archive",
        );
        let restart_started = Instant::now();
        nodes.stop_all()?;
        let online_signers = online
            .iter()
            .map(|index| {
                Ok(Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)?.id())
            })
            .collect::<Result<BTreeSet<_>>>()?;
        let head: ParentPoint = serde_json::from_value(four_status[0]["head"].clone())?;
        evidence["before_restart_archive"] = super::aab::audit_archived_quorums(
            &nodes,
            &all,
            &genesis,
            &set,
            head,
            &online_signers,
        )?;
        enter_phase(
            &mut evidence,
            &mut phase,
            &mut phase_started,
            "four-process cold recovery",
        );
        for index in &all {
            nodes.start(*index, "existing")?;
        }
        nodes.wait_ready(&all)?;
        ensure!(
            nodes
                .processes
                .iter()
                .flatten()
                .all(|p| !pids.contains(&p.id())),
            "cold restart reused process"
        );
        let cold = nodes.wait_receipts(&all, &hashes)?;
        ensure!(cold == four_receipts, "cold receipt recovery differs");
        ensure!(
            nodes.balances(&all, &cold[0], [1600, 800])? == before_balances,
            "cold balances/root differ"
        );
        evidence["cold_restart_elapsed_ms"] = json!(phase_started.elapsed().as_millis());
        enter_phase(
            &mut evidence,
            &mut phase,
            &mut phase_started,
            "post-restart successor",
        );
        let next = vec![signed(1, BATCHES, 100)?, signed(3, BATCHES, 50)?];
        nodes.submit_batch(0, &next)?;
        hashes.extend(transaction_hashes(&next)?);
        let final_receipts = nodes.wait_receipts(&all, &hashes)?;
        let final_status = nodes.wait_ready(&all)?;
        let final_partition = receipt_partition(&final_receipts[0], &hashes, 2)?;
        verify_partition_heads(&final_status, &final_partition, BATCHES * 2 + 2)?;
        ensure!(
            final_receipts[0][..raw.len()] == four_receipts[0]
                && final_receipts[0][raw.len()..]
                    .iter()
                    .all(|r| r["success"] == true && r["nonce_after"] == BATCHES + 1),
            "successor altered old receipts or nonce"
        );
        let balances = nodes.balances(&all, &final_receipts[0], [1700, 850])?;
        evidence["final_status"] = json!(final_status);
        evidence["final_receipts"] = json!(final_receipts);
        evidence["final_balances"] = json!(balances);
        evidence["live_pids"] = json!(pids);
        evidence["cold_pids"] = json!(nodes
            .processes
            .iter()
            .flatten()
            .map(Child::id)
            .collect::<Vec<_>>());
        evidence["successful_transactions"] = json!(BATCHES * 2 + 2);
        evidence["business_failed_transactions"] = json!(0);
        evidence["post_restart_successor_elapsed_ms"] = json!(phase_started.elapsed().as_millis());
        evidence["restart_and_successor_elapsed_ms"] = json!(restart_started.elapsed().as_millis());
        evidence["restart_and_successor_timing_scope"] =
            json!("stop confirmed nodes, audit archive, cold-open, verify old state, submit and observe successor; excludes final archive audit");
        enter_phase(
            &mut evidence,
            &mut phase,
            &mut phase_started,
            "final archive verification",
        );
        nodes.stop_all()?;
        let all_signers = all
            .iter()
            .map(|index| {
                Ok(Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)?.id())
            })
            .collect::<Result<BTreeSet<_>>>()?;
        let head: ParentPoint = serde_json::from_value(final_status[0]["head"].clone())?;
        evidence["final_archive"] =
            super::aab::audit_archived_quorums(&nodes, &all, &genesis, &set, head, &all_signers)?;
        Ok(())
    })();
    record_phase(&mut evidence, phase, phase_started, result.is_ok());
    if result.is_ok() {
        phase = "complete";
    }
    if result.is_err() {
        evidence["failure_status"] = json!(all
            .iter()
            .map(|index| {
                if nodes.processes[*index].is_none() {
                    json!({"node":index,"running":false})
                } else {
                    match nodes.rpc(*index, "nov_chainStatus", json!([])) {
                        Ok(status) => json!({"node":index,"status":status}),
                        Err(error) => json!({"node":index,"error":format!("{error:#}")}),
                    }
                }
            })
            .collect::<Vec<_>>());
    }
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&json!({
        "schema":"novovm/resident-offline-rpc/v1","passed":result.is_ok(),"phase":phase,
        "failure":result.as_ref().err().map(|e|format!("{e:#}")),"evidence":evidence}))?,
    )?;
    nodes.stop_all()?;
    relay.shutdown()?;
    eprintln!(
        "offline original RPC report={}",
        directory.join("result.json").display()
    );
    result
}

#[test]
fn offline_diagnostics_distinguish_absent_proposer_from_observed_empty_pool() {
    let validators = BTreeMap::from([("online".to_owned(), 0), ("absent".to_owned(), 2)]);
    let status = json!({"head":{"height":3},"current_height":4,"round":0,
        "scheduled_proposer":"absent","local_proposer":false,"pending":12,
        "rpc_indexed_height":3,"rpc_authentication":{"signature_batch_pending":false}});
    let absent = progress_node_status(0, &status, &validators, &[0, 1, 3]);
    assert_eq!(absent["height"], 3);
    assert_eq!(absent["current_height"], 4);
    assert_eq!(absent["scheduled_proposer_fixture_index"], 2);
    assert_eq!(absent["scheduled_proposer_configured_online"], false);
    assert_eq!(
        absent["local_proposer_pool_observation"],
        "not_local_proposer"
    );
    assert!(absent["input_availability"].is_null());

    let mut status = status;
    status["scheduled_proposer"] = json!("online");
    status["local_proposer"] = json!(true);
    status["pending"] = json!(0);
    status["rpc_authentication"]["signature_batch_pending"] = json!(true);
    let empty = progress_node_status(0, &status, &validators, &[0, 1, 3]);
    assert_eq!(empty["scheduled_proposer_configured_online"], true);
    assert_eq!(
        empty["local_proposer_pool_observation"],
        "empty_pending_observed"
    );
    // Empty pending does not mean no in-flight authentication or no candidate.
    assert_eq!(empty["rpc_authentication"]["signature_batch_pending"], true);
    assert_eq!(
        empty["current_body_observation"],
        "not_exposed_by_public_chain_status"
    );
}

#[test]
fn offline_diagnostics_do_not_infer_executable_body_from_nonempty_pool() {
    let validators = BTreeMap::from([("online".to_owned(), 0)]);
    let status = json!({"head":{"height":8},"current_height":9,"round":1,
        "scheduled_proposer":"online","local_proposer":true,"pending":2,
        "rpc_indexed_height":7,"projection_error":null,
        "input_availability":{"fixture_marker":"observed verbatim"}});
    let observed = progress_node_status(0, &status, &validators, &[0]);
    assert_eq!(
        observed["local_proposer_pool_observation"],
        "nonempty_pending_observed_executability_unknown"
    );
    assert_eq!(observed["rpc_indexed_height"], 7);
    assert_eq!(observed["input_availability"], status["input_availability"]);
    let mut unknown = status;
    unknown["scheduled_proposer"] = json!("unknown identity");
    let unknown = progress_node_status(0, &unknown, &validators, &[0]);
    assert!(unknown["scheduled_proposer_fixture_index"].is_null());
    assert!(unknown["scheduled_proposer_configured_online"].is_null());
}

#[test]
fn offline_propagation_gate_does_not_count_repair_reacceptance_as_new_input() -> Result<()> {
    let mut status = json!({"transaction_gossip":{"outbound_batches_accepted":16}});
    assert_eq!(propagation_counts(&status)?, (16, 0, 16));
    status["gossip_repair"] = json!({"repair_batches_reserved":8});
    let (total, repair, fresh) = propagation_counts(&status)?;
    assert_eq!((total, repair, fresh), (16, 8, 8));
    assert!(
        fresh < BATCHES,
        "repeated repair offers cannot pass the gate"
    );
    status["transaction_gossip"]["outbound_batches_accepted"] = json!(24);
    assert_eq!(propagation_counts(&status)?, (24, 8, 16));
    status["gossip_repair"]["repair_batches_reserved"] = json!(25);
    assert!(propagation_counts(&status).is_err());
    status["gossip_repair"]["repair_batches_reserved"] = json!("unknown");
    assert!(propagation_counts(&status).is_err());
    Ok(())
}
