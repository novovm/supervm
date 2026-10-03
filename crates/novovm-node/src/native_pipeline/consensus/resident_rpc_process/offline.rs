//! Three online original executables must not wait for an absent fourth peer's
//! gossip TTL. The fourth later joins from the unchanged genesis/archive, then
//! all four cold-reopen. No externally manufactured votes or reduced quorum.
use super::*;

const BATCHES: u64 = 16;

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
    let mut sampled = started - Duration::from_secs(1);
    loop {
        nodes.alive()?;
        let statuses = online.iter().map(|index| {
            let status = nodes.rpc(*index, "nov_chainStatus", json!([]))?;
            Ok(json!({"node":index,"height":status["head"]["height"].as_u64().unwrap_or(0),
                "round":status["round"],"pending":status["pending"],"leader":status["scheduled_proposer"],
                "local_leader":status["local_proposer"],"last_error":status["last_error"]}))
        }).collect::<Result<Vec<_>>>()?;
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
        if advanced {
            common_height = minimum;
            progressed = Instant::now();
        }
        if advanced || sampled.elapsed() >= Duration::from_secs(1) {
            history.push(json!({"elapsed_ms":started.elapsed().as_millis(),"nodes":statuses}));
            evidence["online_progress"] = json!(history);
            sampled = Instant::now();
        }
        ensure!(
            started.elapsed() < Duration::from_secs(240)
                && progressed.elapsed() < Duration::from_secs(90),
            "online quorum stopped advancing or exceeded total fault budget; last={statuses:?}"
        );
        if minimum >= BATCHES {
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
        "online_total_fault_deadline_seconds":240,"production_acceptance":false,"performance_measured":false});
    let mut phase = "startup";
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
        phase = "single HTTP admission";
        let started = Instant::now();
        let admitted = nodes.submit_batch(0, &raw)?;
        ensure!(
            admitted.len() == raw.len() && admitted.iter().all(|r| r["signature_verified"] == true),
            "offline fixture signed admission changed"
        );
        evidence["admission"] = json!(admitted);
        phase = "propagate beyond global capacity before TTL";
        loop {
            nodes.alive()?;
            let status = nodes.rpc(0, "nov_chainStatus", json!([]))?;
            evidence["ingress_before_ttl"] = status.clone();
            evidence["propagation_elapsed_ms"] = json!(started.elapsed().as_millis());
            let stats = &status["transaction_gossip"];
            ensure!(
                stats["expired_batches"] == 0 && stats["channel_expired_sends"] == 0,
                "peer isolation only progressed by expiring accepted sends"
            );
            ensure!(
                started.elapsed() < Duration::from_secs(8),
                "offline recipient pinned global propagation capacity before TTL: {stats}"
            );
            if stats["outbound_batches_accepted"].as_u64().unwrap_or(0) >= BATCHES {
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
        phase =
            "online quorum finality (offline node excluded from observation, not validator set)";
        wait_online_progress(&mut nodes, &online, &mut evidence)?;
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
        evidence["online_status"] = json!(three_status);
        evidence["online_receipts"] = json!(three_receipts);
        evidence["online_balances"] = json!(online_balances);
        evidence["online_partition"] = json!(partition);
        ensure!(
            nodes.processes[offline].is_none(),
            "absent node was secretly helping quorum"
        );
        phase = "fourth node archive catchup without wallet resubmission";
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
        let pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        phase = "four-process cold recovery";
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
        phase = "post-restart successor";
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
        phase = "complete";
        Ok(())
    })();
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
