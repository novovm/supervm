//! Current original-node process recovery, not the retired controller fixture.
//! Wallets submit once to one healthy native APFL entry. The absent validator
//! receives no wallet replay, database copy, generated proposal, vote or QC.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::{quote_transfer, TransferFeeRequest};
use crate::native_pipeline::consensus::chain::ChainRecord;
use novovm_consensus::round_bft::{journal::codec::decode_archived_decision, VerifiedDecision};

const OFFLINE_BATCHES: u64 = 40;
const RESTART_BATCHES: u64 = 4;
const LATE_START: Duration = Duration::from_secs(31);

fn checkpoint(nodes: &ProductNodes, evidence: &Value) -> Result<()> {
    fs::write(
        nodes.directory.join("recovery-report.json"),
        serde_json::to_vec_pretty(evidence)?,
    )?;
    Ok(())
}

fn phase(nodes: &ProductNodes, evidence: &mut Value, name: &str, started: Instant) -> Result<()> {
    evidence["phase"] = json!(name);
    evidence["elapsed_ms"] = json!(started.elapsed().as_millis());
    checkpoint(nodes, evidence)
}

pub(super) fn stop_one(nodes: &mut ProductNodes, index: usize) -> Result<()> {
    let mut process = nodes.processes[index]
        .take()
        .context("recovery process was not running")?;
    ensure!(
        process.try_wait()?.is_none(),
        "recovery child already exited"
    );
    process.kill()?; // Deliberate crash: do not request graceful journal cleanup.
    let _ = process.wait()?;
    Ok(())
}

fn same_head(values: &[Status], reference: Status) -> Result<()> {
    ensure!(
        values.iter().all(|value| value.height == reference.height
            && value.block_hash == reference.block_hash
            && value.state_root == reference.state_root
            && value.receipt_batch_commitment == reference.receipt_batch_commitment
            && value.state_version == reference.state_version
            && value.indexed_height == reference.height
            && !value.projection_failed),
        "recovery published heads differ: {values:?} reference={reference:?}"
    );
    Ok(())
}

pub(super) fn wait_height(
    nodes: &mut ProductNodes,
    clients: &mut [NativeApplicationClient],
    height: u64,
) -> Result<Vec<Status>> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        nodes.alive()?;
        let values = clients.iter_mut().map(status).collect::<Result<Vec<_>>>()?;
        ensure!(
            values.iter().all(|value| value.height <= height),
            "fixture published unsubmitted height: {values:?} expected={height}"
        );
        if values
            .iter()
            .all(|value| value.height == height && value.indexed_height == height)
        {
            same_head(&values, values[0])?;
            return Ok(values);
        }
        ensure!(
            Instant::now() < deadline,
            "current native recovery height timeout: {values:?} expected={height}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn open_stopped_store(nodes: &ProductNodes, index: usize) -> Result<CandidateStore> {
    ensure!(
        nodes.processes[index].is_none(),
        "cannot read a live signer DB"
    );
    let config = crate::native_pipeline::service::ResidentConfig::load(&nodes.configs[index])?;
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

/// Read only while the process is dead. Prove the crash actually interrupted
/// durable replay, rather than merely querying an old projection after replay
/// had already finished. Preserve the exact old signing outbox as a restart pin.
pub(super) fn interrupted_archive(
    nodes: &ProductNodes,
    index: usize,
    observed: Status,
    target: u64,
    set: &ValidatorSet,
) -> Result<(MetaKey, Vec<u8>, Value)> {
    let store = open_stopped_store(nodes, index)?;
    let mut values = store
        .read_metadata(&[
            MetaKey::ChainBlock {
                height: observed.height,
            },
            MetaKey::ChainBlock { height: target },
        ])?
        .values;
    ensure!(values.len() == 2, "interrupted archive response shape");
    ensure!(
        values.pop().flatten().is_none(),
        "crash missed catch-up: target was already durably published"
    );
    let record = ChainRecord::decode(
        &values
            .pop()
            .flatten()
            .context("observed interrupted height was not durable")?,
    )?;
    let point = record.point();
    ensure!(
        point.height == observed.height
            && point.block_hash == observed.block_hash
            && point.state_root == observed.state_root,
        "interrupted native observation differs from archive"
    );
    let key = record.outbox_key();
    let bytes = store
        .read_metadata(std::slice::from_ref(&key))?
        .values
        .pop()
        .flatten()
        .context("interrupted signing outbox missing")?;
    let MetaKey::ConsensusOutbox { sequence, .. } = key else {
        anyhow::bail!("archive outbox is not a signing revision");
    };
    let (proposal, certificate) = decode_archived_decision(&bytes, sequence)?;
    VerifiedDecision::verify(
        &proposal,
        &certificate,
        set,
        record.context(),
        point.block_hash,
    )?;
    ensure!(certificate.votes.len() >= 3, "replay lowered 3/4 quorum");
    let summary = json!({
        "durable_interrupted_height":point.height,
        "target_height":target,"target_archive_absent_at_crash":true,
        "qc_signatures":certificate.votes.len(),"outbox_revision":sequence,
        "outbox_sha256":hex(&Sha256::digest(&bytes)),
        "source":"original stopped database; no copy or mutation"
    });
    Ok((key, bytes, summary))
}

fn append_height(
    nodes: &mut ProductNodes,
    clients: &mut [NativeApplicationClient],
    nonce: u64,
    hashes_seen: &mut Vec<[u8; 32]>,
    done: &mut Vec<ReceiptState>,
) -> Result<Vec<Status>> {
    let previous = status(&mut clients[0])?.height;
    let amount = if nonce == 7 { 1_000_001 } else { 100 };
    let raw = vec![signed(1, nonce, amount)?, signed(3, nonce, 50)?];
    let ids = hashes(&raw)?;
    send(&mut clients[0], &raw)?;
    let batch = receipts(nodes, clients, &ids)?;
    for ((receipt, id), (seed, amount)) in batch.iter().zip(&ids).zip([(1, amount), (3, 50)]) {
        let expected = quote_transfer(
            &TransferFeeRequest {
                tx_hash: *id,
                payer: account(seed),
                recipient: account(2),
                asset: "NOV".into(),
                amount,
                pay_asset: "NOV".into(),
                max_pay_amount: 0,
                slippage_bps: 0,
            },
            &policy(),
            0,
        )??
        .nov_amount;
        ensure!(
            matches!(receipt, ReceiptState::Finalized { charged_fee, .. } if *charged_fee == expected),
            "original fee rule changed during recovery: {receipt:?} expected={expected}"
        );
    }
    ensure!(
        batch
            .iter()
            .enumerate()
            .all(|(index, value)| matches!(value,
            ReceiptState::Finalized { success, nonce_after, height, .. }
            if *success != (nonce == 7 && index == 0)
                && *nonce_after == nonce + 1 && *height > previous)),
        "recovery signed fees/failed nonce/order differ: {batch:?}"
    );
    let height = batch
        .iter()
        .filter_map(|value| match value {
            ReceiptState::Finalized { height, .. } => Some(*height),
            _ => None,
        })
        .max()
        .context("batch has no final height")?;
    hashes_seen.extend(ids);
    done.extend(batch);
    wait_height(nodes, clients, height)
}

fn run_recovery(highest_identity: bool) -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY").context("explicit node binary required")?,
    )
    .canonicalize()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-rpc")
        .join(format!(
            "native-recovery-{}-{}-{}",
            if highest_identity {
                "highest"
            } else {
                "lowest"
            },
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory =
        crate::native_pipeline::service::config::native_path(&directory.canonicalize()?);
    eprintln!("native recovery artifacts={}", directory.display());
    let (mut nodes, set) = setup_with_genesis(&directory, binary, genesis()?, 2)?;
    for path in &nodes.configs {
        let mut config: Value = serde_json::from_slice(&fs::read(path)?)?;
        config["rpc_addr"] = Value::Null;
        config["profile"] = json!(ORIGINAL_PROFILE);
        config["native_client_limit"] = json!(2);
        fs::write(path, serde_json::to_vec_pretty(&config)?)?;
    }
    let mut ordered = (0..4)
        .map(|index| {
            Ok((
                novovm_network::product_overlay::peer_id_from_ed25519_public_key_v1(
                    &validator_key(index).verifying_key().to_bytes(),
                ),
                Validator::new(validator_key(index).verifying_key().to_bytes(), 1)?.id(),
                index,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    ordered.sort_unstable();
    // Configured handshake tie-breaking compares native peer IDs (public-key
    // hex), not the independently domain-hashed consensus validator IDs.
    let missing = ordered[if highest_identity { 3 } else { 0 }].2;
    let healthy: Vec<_> = (0..4).filter(|index| *index != missing).collect();
    let config: crate::native_pipeline::service::ResidentConfig =
        serde_json::from_slice(&fs::read(&nodes.configs[missing])?)?;
    let key_bytes = fs::read(&config.signing_key_file)?;
    let configs = nodes
        .configs
        .iter()
        .map(fs::read)
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut evidence = json!({
        "source":"current_original_native_execution_tick",
        "transport":"native_novorudp_apfl","http_listeners":0,
        "direction":if highest_identity {"late_highest_validator"} else {"late_lowest_validator"},
        "withheld_validator":missing,"healthy_validators":healthy,
        "validator_order":ordered.iter().map(|(peer_id, validator_id, index)|
            json!({"native_peer_id":peer_id,"validator_id":validator_id,"node":index})
        ).collect::<Vec<_>>(),"fixed_validators":4,"quorum":3,
        "offline_batches":OFFLINE_BATCHES,
        "minimum_distinct_vote_objects_per_healthy_validator":2 * OFFLINE_BATCHES,
        "old_per_destination_message_limit":32,
        "offline_wallet_resubmissions":0,"database_copy":false,
        "real_processes":4,"real_machines":1,"throughput_acceptance":false,
        "business_proof_verified":false,"completed":false,
        "binary_sha256":hex(&Sha256::digest(fs::read(&nodes.binary)?)),
        "library_path":config.library,
        "library_sha256":hex(&Sha256::digest(fs::read(&config.library)?)),
        "validator_key_sha256":hex(&Sha256::digest(&key_bytes)),
        "offline_progress":[],"catchup_progress":[]
    });
    let started = Instant::now();
    let result = (|| -> Result<()> {
        phase(&nodes, &mut evidence, "start_three_current_nodes", started)?;
        for index in &healthy {
            nodes.start(*index, "create")?;
        }
        let absent_since = Instant::now();
        let mut clients = healthy
            .iter()
            .map(|index| connect(&nodes, *index, set.clone()))
            .collect::<Result<Vec<_>>>()?;
        ready(&mut nodes, &mut clients)?;
        let mut ids = Vec::new();
        let mut done = Vec::new();
        phase(
            &nodes,
            &mut evidence,
            "healthy_three_exceed_old_queue_limit",
            started,
        )?;
        let offline_started = Instant::now();
        for nonce in 0..OFFLINE_BATCHES {
            let heads = append_height(&mut nodes, &mut clients, nonce, &mut ids, &mut done)?;
            evidence["offline_progress"]
                .as_array_mut()
                .unwrap()
                .push(json!({
                    "elapsed_ms":offline_started.elapsed().as_millis(),"statuses":heads
                }));
            checkpoint(&nodes, &evidence)?;
        }
        let target = status(&mut clients[0])?;
        ensure!(
            target.height >= OFFLINE_BATCHES,
            "offline height coverage too short"
        );
        let moved = [
            (OFFLINE_BATCHES as u128 - 1) * 100,
            OFFLINE_BATCHES as u128 * 50,
        ];
        let before_balances = balances(&mut clients, &done, moved)?;
        // Do not shorten production handshake expiry or use a virtual clock.
        while absent_since.elapsed() < LATE_START {
            nodes.alive()?;
            std::thread::sleep(Duration::from_millis(20));
        }
        evidence["offline_healthy_confirmation_ms"] = json!(offline_started.elapsed().as_millis());
        evidence["late_start_after_ms"] = json!(absent_since.elapsed().as_millis());
        evidence["healthy_before_late_start"] = json!(target);
        evidence["healthy_economics_before_late_start"] = json!(before_balances);
        phase(
            &nodes,
            &mut evidence,
            "late_validator_catchup_then_crash",
            started,
        )?;
        let mut late_client = connect(&nodes, missing, set.clone())?;
        nodes.start(missing, "create")?;
        let catchup_started = Instant::now();
        let deadline = catchup_started + Duration::from_secs(90);
        let interrupted = loop {
            nodes.alive()?;
            let value = status(&mut late_client)?;
            evidence["catchup_last_status"] = json!(value);
            if value.height > 0 {
                evidence["catchup_progress"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({
                        "elapsed_ms":catchup_started.elapsed().as_millis(),"status":value
                    }));
                checkpoint(&nodes, &evidence)?;
                ensure!(
                    value.height < target.height,
                    "late validator finished before interruption; do not claim interrupted replay"
                );
                break value;
            }
            ensure!(
                Instant::now() < deadline,
                "late validator never entered archive replay"
            );
        };
        stop_one(&mut nodes, missing)?;
        drop(late_client);
        let (pinned_key, pinned_outbox, crash) =
            interrupted_archive(&nodes, missing, interrupted, target.height, &set)?;
        evidence["catchup_crash"] = crash;
        phase(
            &nodes,
            &mut evidence,
            "resume_original_database_without_wallet_replay",
            started,
        )?;
        nodes.start(missing, "existing")?;
        clients.push(connect(&nodes, missing, set.clone())?);
        let recovered = wait_height(&mut nodes, &mut clients, target.height)?;
        ensure!(
            receipts(&mut nodes, &mut clients, &ids)? == done,
            "archive receipts differ"
        );
        ensure!(
            balances(&mut clients, &done, moved)? == before_balances,
            "replay charged twice"
        );
        evidence["after_interrupted_catchup"] = json!(recovered);
        evidence["late_start_and_interrupted_recovery_ms"] =
            json!(catchup_started.elapsed().as_millis());
        // This time the validator already has an authenticated live session.
        // Its same key and same signing DB must reconnect after an isolated kill.
        phase(
            &nodes,
            &mut evidence,
            "established_validator_restart_while_others_advance",
            started,
        )?;
        stop_one(&mut nodes, missing)?;
        let _ = clients.pop();
        for nonce in OFFLINE_BATCHES..OFFLINE_BATCHES + RESTART_BATCHES {
            append_height(&mut nodes, &mut clients, nonce, &mut ids, &mut done)?;
        }
        let restart_target = status(&mut clients[0])?;
        nodes.start(missing, "existing")?;
        clients.push(connect(&nodes, missing, set.clone())?);
        let restarted = wait_height(&mut nodes, &mut clients, restart_target.height)?;
        ensure!(
            receipts(&mut nodes, &mut clients, &ids)? == done,
            "individual restart receipts differ"
        );
        evidence["after_established_restart"] = json!(restarted);
        let next_nonce = OFFLINE_BATCHES + RESTART_BATCHES;
        let successor = append_height(&mut nodes, &mut clients, next_nonce, &mut ids, &mut done)?;
        let minority_height = successor[0].height;
        evidence["all_four_successor"] = json!(successor);
        phase(
            &nodes,
            &mut evidence,
            "two_of_four_must_not_publish",
            started,
        )?;
        stop_one(&mut nodes, healthy[1])?;
        stop_one(&mut nodes, healthy[2])?;
        // Keep only the still running original ingress and returned validator.
        let mut minority = vec![clients.remove(0), clients.pop().unwrap()];
        let pending_nonce = next_nonce + 1;
        let pending = vec![
            signed(1, pending_nonce, 100)?,
            signed(3, pending_nonce, 50)?,
        ];
        let pending_ids = hashes(&pending)?;
        send(&mut minority[0], &pending)?;
        let minority_started = Instant::now();
        while minority_started.elapsed() < Duration::from_secs(12) {
            nodes.alive()?;
            let snapshots = minority
                .iter_mut()
                .map(status)
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                snapshots
                    .iter()
                    .all(|value| value.height == minority_height),
                "2/4 published while validators absent: {snapshots:?}"
            );
            for client in &mut minority {
                let Answer::Receipt { state, .. } = query(
                    client,
                    Query::Receipt {
                        hash: pending_ids[0],
                    },
                )?
                else {
                    anyhow::bail!("minority native receipt missing");
                };
                ensure!(
                    !matches!(state, ReceiptState::Finalized { .. }),
                    "2/4 fabricated finality"
                );
            }
            evidence["minority_last_status"] = json!(snapshots);
            std::thread::sleep(Duration::from_millis(20));
        }
        evidence["minority_no_publication_ms"] = json!(minority_started.elapsed().as_millis());
        phase(
            &nodes,
            &mut evidence,
            "restore_third_then_fourth_without_pending_replay",
            started,
        )?;
        nodes.start(healthy[1], "existing")?;
        minority.push(connect(&nodes, healthy[1], set.clone())?);
        let pending_done = receipts(&mut nodes, &mut minority, &pending_ids)?;
        ensure!(pending_done.iter().all(|value| matches!(value,
            ReceiptState::Finalized { success: true, nonce_after, .. } if *nonce_after == pending_nonce+1)),
            "restored quorum did not finish original pending input");
        let final_height = pending_done
            .iter()
            .filter_map(|value| match value {
                ReceiptState::Finalized { height, .. } => Some(*height),
                _ => None,
            })
            .max()
            .context("restored quorum has no final height")?;
        ids.extend(pending_ids);
        done.extend(pending_done);
        nodes.start(healthy[2], "existing")?;
        minority.push(connect(&nodes, healthy[2], set.clone())?);
        let final_heads = wait_height(&mut nodes, &mut minority, final_height)?;
        ensure!(
            receipts(&mut nodes, &mut minority, &ids)? == done,
            "final archive differs"
        );
        let final_balances = balances(
            &mut minority,
            &done,
            [
                pending_nonce as u128 * 100,
                (pending_nonce as u128 + 1) * 50,
            ],
        )?;
        evidence["final_heads"] = json!(final_heads);
        evidence["final_balances"] = json!(final_balances);
        evidence["final_receipts"] = json!(done);
        nodes.stop_all()?;
        let store = open_stopped_store(&nodes, missing)?;
        ensure!(
            store.read_metadata(&[pinned_key])?.values == vec![Some(pinned_outbox)],
            "original durable signing outbox was removed or rewritten on reconnect"
        );
        ensure!(
            fs::read(&config.signing_key_file)? == key_bytes
                && nodes
                    .configs
                    .iter()
                    .map(fs::read)
                    .collect::<std::io::Result<Vec<_>>>()?
                    == configs,
            "recovery replaced validator identity, configuration or genesis"
        );
        evidence["same_key_config_and_old_signing_outbox_preserved"] = json!(true);
        evidence["completed"] = json!(true);
        phase(&nodes, &mut evidence, "complete", started)
    })();
    if let Err(error) = &result {
        evidence["failure"] = json!(format!("{error:#}"));
        evidence["elapsed_ms"] = json!(started.elapsed().as_millis());
    }
    checkpoint(&nodes, &evidence)?;
    result
}

#[test]
#[ignore = "current original node, real AOEM, 31s+ offline/crash recovery; run alone"]
fn native_only_recovery_late_lowest_validator_original_database_and_minority() -> Result<()> {
    run_recovery(false)
}

#[test]
#[ignore = "current original node, real AOEM, 31s+ offline/crash recovery; run alone"]
fn native_only_recovery_late_highest_validator_original_database_and_minority() -> Result<()> {
    run_recovery(true)
}
