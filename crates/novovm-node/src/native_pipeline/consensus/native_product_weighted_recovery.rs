//! Two online processes below quorum, then two online processes at quorum.
use super::*;

const ROLE: [&str; 4] = ["A1", "A2", "C1", "D1"];
const WEIGHT: [u64; 4] = [1, 3, 2, 1];

struct Archived {
    record: Vec<u8>,
    outbox: Vec<u8>,
    signers: Vec<[u8; 32]>,
    weight: u64,
    summary: Value,
}

fn archived(
    nodes: &ProductNodes,
    index: usize,
    head: Status,
    set: &ValidatorSet,
) -> Result<Archived> {
    let store = open_stopped_store(nodes, index)?;
    let read = |key| -> Result<Vec<u8>> {
        store
            .read_metadata(&[key])?
            .values
            .pop()
            .flatten()
            .context("missing decision evidence")
    };
    let record = read(MetaKey::ChainBlock {
        height: head.height,
    })?;
    let decoded = ChainRecord::decode(&record)?;
    let point = decoded.point();
    ensure!(
        point.height == head.height
            && point.block_hash == head.block_hash
            && point.state_root == head.state_root
            && point.receipt_batch_commitment == head.receipt_batch_commitment,
        "stopped archive mismatches authenticated head"
    );
    let outbox = read(decoded.outbox_key())?;
    let (proposal, qc) = decoded.checked_outbox(&outbox, set)?;
    let weight = qc.verify(set)?.signed_weight();
    let signers: Vec<_> = qc.votes.iter().map(|vote| vote.validator_id).collect();
    let roles = signers
        .iter()
        .map(|id| -> Result<Value> {
            let member = set.member(id).context("unrecognized signer")?;
            Ok(json!({"id":hex(id),"weight":member.weight()}))
        })
        .collect::<Result<Vec<_>>>()?;
    let summary = json!({"node":ROLE[index],"height":head.height,"round":proposal.round,
        "block_hash":hex(&head.block_hash),"state_root":hex(&head.state_root),"receipt_commitment":hex(&head.receipt_batch_commitment),
        "signers":roles,"signed_weight":weight,"quorum_weight":set.quorum_weight(),
        "record_sha256":hex(&Sha256::digest(&record)), "outbox_sha256":hex(&Sha256::digest(&outbox))});
    Ok(Archived {
        record,
        outbox,
        signers,
        weight,
        summary,
    })
}

#[test]
#[ignore = "four isolated original product processes, actual AOEM and 12-second minority observation"]
fn weighted_pair_pending_once_and_existing_outbox() -> Result<()> {
    let root = PathBuf::from(
        std::env::var_os("NOVOVM_WEIGHTED_RECOVERY_DIRECTORY")
            .context("explicit new output directory required")?,
    );
    fs::create_dir(&root)?;
    let directory = crate::native_pipeline::service::config::native_path(&root.canonicalize()?);
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY").context("explicit node binary required")?,
    )
    .canonicalize()?;
    let chain = 7_000_000_000 + rand::random::<u64>() % 1_000_000_000;
    let keys = prepare_process_identities(chain)?;
    let validators: Vec<_> = WEIGHT
        .iter()
        .enumerate()
        .map(|(i, weight)| GenesisValidator {
            public_key: keys.signers[i].public_key().to_vec(),
            transport_public_key: keys.transports[i].public_key().to_vec(),
            weight: *weight,
        })
        .collect();
    let ids = validators
        .iter()
        .map(|v| Ok(Validator::new(&v.public_key, v.weight)?.id()))
        .collect::<Result<Vec<_>>>()?;
    let genesis = fixture_genesis(chain, validators)?;
    let mut evidence = json!({"completed":false,"chain":chain,"genesis":hex(&genesis.genesis_config_commitment),
        "roles":ROLE, "weights":WEIGHT,"validator_ids":ids.iter().map(|id|hex(id)).collect::<Vec<_>>(),
        "node_sha256":hex(&Sha256::digest(fs::read(&binary)?)),"harness_sha256":hex(&Sha256::digest(fs::read(std::env::current_exe()?)?)),
        "aoem_sha256":hex(&Sha256::digest(fs::read(library()?)?)),"wallet_submissions":[]});
    let (mut nodes, set) = setup_with_identities(&directory, binary, genesis, 2, &keys)?;
    ensure!(
        set.total_weight() == 7 && set.quorum_weight() == 5,
        "unexpected genesis voting weights"
    );
    let mut configs = Vec::new();
    for p in &nodes.configs {
        let mut config: Value = serde_json::from_slice(&fs::read(p)?)?;
        config["profile"] = json!(ORIGINAL_PROFILE);
        config["rpc_addr"] = Value::Null;
        config["native_client_limit"] = json!(2);
        let bytes = serde_json::to_vec_pretty(&config)?;
        fs::write(p, &bytes)?;
        configs.push(bytes);
    }
    let client = |index: usize| -> Result<NativeApplicationClient> {
        let conf: ResidentConfig = serde_json::from_slice(&configs[index])?;
        connect_fixture(
            &conf,
            set.clone(),
            keys.client_transport.clone(),
            keys.transports[index].public_key(),
            "127.0.0.1:0".parse()?,
        )
    };
    let input = |nonce: u64| -> Result<(Vec<Vec<u8>>, Vec<[u8; 32]>)> {
        let raw = vec![
            signed_in_chain(chain, 1, nonce, 100)?,
            signed_in_chain(chain, 3, nonce, 50)?,
        ];
        let ids = raw
            .iter()
            .map(|bytes| Ok(authenticate_transfer_v3(bytes, chain, 1024)?.tx_hash()))
            .collect::<Result<Vec<_>>>()?;
        Ok((raw, ids))
    };
    let started = Instant::now();
    let result = (|| -> Result<()> {
        phase(&nodes, &mut evidence, "all_four_create", started)?;
        for i in 0..4 {
            nodes.start(i, "create")?;
        }
        let mut clients = (0..4).map(client).collect::<Result<Vec<_>>>()?;
        ready(&mut nodes, &mut clients)?;
        let (baseline, baseline_ids) = input(0)?;
        evidence["wallet_submissions"] = json!([{"nonce":0,"target":"C1","hashes":baseline_ids.iter().map(|id|hex(id)).collect::<Vec<_>>()}]);
        send(&mut clients[2], &baseline)?;
        let mut all_receipts = receipts(&mut nodes, &mut clients, &baseline_ids)?;
        let before = wait_height(&mut nodes, &mut clients, 1)?[0];
        stop_one(&mut nodes, 3)?;
        drop(clients.remove(3));
        stop_one(&mut nodes, 1)?;
        drop(clients.remove(1));
        let pinned = archived(&nodes, 3, before, &set)?;
        evidence["original_d1_decision"] = pinned.summary.clone();
        phase(&nodes, &mut evidence, "weak_pair_A1_C1", started)?;
        let (pending, pending_ids) = input(1)?;
        evidence["wallet_submissions"].as_array_mut().unwrap().push(json!({"nonce":1,"target":"C1","hashes":pending_ids.iter().map(|id|hex(id)).collect::<Vec<_>>()}));
        checkpoint(&nodes, &evidence)?;
        send(&mut clients[1], &pending)?;
        // A transport completion can precede the local application projection.
        // Start the hold only after C1 reports both original marker hashes.
        let received_deadline = Instant::now() + Duration::from_secs(10);
        loop {
            nodes.alive()?;
            let mut received = 0;
            for id in &pending_ids {
                match query(&mut clients[1], Query::Receipt { hash: *id })? {
                    Answer::Receipt {
                        state: ReceiptState::Received,
                        ..
                    } => received += 1,
                    Answer::Receipt {
                        state: ReceiptState::NotRecent,
                        ..
                    } => (),
                    answer => anyhow::bail!("unexpected C1 minority answer {answer:?}"),
                }
            }
            if received == pending_ids.len() {
                break;
            }
            ensure!(
                Instant::now() < received_deadline,
                "C1 did not receive original markers"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let hold = Instant::now();
        let mut observations = 0;
        loop {
            nodes.alive()?;
            let heads = clients.iter_mut().map(status).collect::<Result<Vec<_>>>()?;
            same_head(&heads, before)?;
            for (i, c) in clients.iter_mut().enumerate() {
                for id in &pending_ids {
                    let Answer::Receipt { state, .. } = query(c, Query::Receipt { hash: *id })?
                    else {
                        anyhow::bail!("missing minority Receipt")
                    };
                    ensure!(
                        matches!(state, ReceiptState::Received)
                            || (i == 0 && matches!(state, ReceiptState::NotRecent)),
                        "unexpected minority receipt on {}: {state:?}",
                        if i == 0 { "A1" } else { "C1" }
                    );
                }
            }
            observations += 1;
            if hold.elapsed() >= Duration::from_secs(12) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        evidence["weak_pair"] = json!({"roles":["A1","C1"],"online_weight":3,"height":before.height,
            "hold_ms":hold.elapsed().as_millis(),"observations":observations,"received_at":"C1"});
        stop_one(&mut nodes, 0)?;
        drop(clients.remove(0));
        phase(&nodes, &mut evidence, "strong_pair_C1_A2", started)?;
        nodes.start(1, "existing")?;
        clients.push(client(1)?);
        let limit = Instant::now() + Duration::from_secs(90);
        recovery_status(&mut nodes, &mut clients[1], limit)?;
        let finalized = receipts_until(&mut nodes, &mut clients, &pending_ids, limit)?;
        ensure!(
            finalized.iter().all(|r| matches!(
                r,
                ReceiptState::Finalized {
                    height: 2,
                    nonce_after: 2,
                    success: true,
                    ..
                }
            )),
            "original input failed to finalize at height 2: {finalized:?}"
        );
        let head = wait_height(&mut nodes, &mut clients, 2)?[0];
        all_receipts.extend(finalized);
        evidence["strong_pair"] = json!({"roles":["C1","A2"],"online_weight":5,"status":head,"finalized":all_receipts[2..]});
        phase(&nodes, &mut evidence, "restore_A1_D1_existing", started)?;
        for i in [0, 3] {
            nodes.start(i, "existing")?;
            clients.push(client(i)?);
        }
        let heads = wait_height(&mut nodes, &mut clients, 2)?;
        let all_ids = [baseline_ids, pending_ids].concat();
        ensure!(
            receipts(&mut nodes, &mut clients, &all_ids)? == all_receipts,
            "reopened original receipts differ"
        );
        evidence["balances"] = json!(balances(&mut clients, &all_receipts, [200, 100])?);
        evidence["matching_heads"] = json!(heads);
        nodes.stop_all()?;
        let old = archived(&nodes, 3, before, &set)?;
        ensure!(
            old.outbox == pinned.outbox && old.record == pinned.record,
            "D1 original decision/outbox changed"
        );
        evidence["reopened_d1_decision"] = old.summary;
        let mut expected = [ids[1], ids[2]];
        expected.sort();
        let mut reports = Vec::new();
        for i in 0..4 {
            ensure!(
                fs::read(&nodes.configs[i])? == configs[i],
                "fixture replaced a configuration"
            );
            let decision = archived(&nodes, i, head, &set)?;
            ensure!(
                decision.weight == 5 && decision.signers == expected,
                "new decision did not carry the two original votes"
            );
            fs::write(
                directory.join(format!("{}-h2.outbox", ROLE[i])),
                decision.outbox,
            )?;
            reports.push(decision.summary);
        }
        evidence["archived_final_qcs"] = json!(reports);
        evidence["completed"] = json!(true);
        phase(&nodes, &mut evidence, "complete", started)
    })();
    if let Err(e) = &result {
        evidence["failure"] = json!(format!("{e:#}"));
        checkpoint(&nodes, &evidence)?;
    }
    result
}
