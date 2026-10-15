//! Original native_execution_tick processes, without any HTTP listener or observation.
//! All reads and signed APFL submissions reuse authenticated native sessions.
use super::*;
use novovm_protocol::native_query::{AccountId, Answer, Balance, Query, ReceiptState, Status};

fn query(client: &mut NativeApplicationClient, selector: Query) -> Result<Answer> {
    let request = client.prepare_query(selector)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut sent = false;
    loop {
        let now = Instant::now();
        if now >= deadline {
            client.abandon_query(&request)?;
            anyhow::bail!("native query timeout: {selector:?}");
        }
        let _ = client.poll(
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
            now,
            64,
        )?;
        if !sent {
            sent = client.try_send_query(&request, now)?;
        }
        if let Some(reply) = client.poll_query_response(&request, now)? {
            // Service the original DONE queue, not a business acknowledgement.
            let _ = client.poll(
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
                Instant::now(),
                64,
            )?;
            return Ok(reply.answer);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn status(client: &mut NativeApplicationClient) -> Result<Status> {
    let Answer::Status(value) = query(client, Query::Status)? else {
        anyhow::bail!("native status missing");
    };
    ensure!(!value.projection_failed, "native archive projection failed");
    Ok(value)
}

fn ready(nodes: &mut ProductNodes, clients: &mut [NativeApplicationClient]) -> Result<Vec<Status>> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        nodes.alive()?;
        let values = clients.iter_mut().map(status).collect::<Result<Vec<_>>>()?;
        if values
            .iter()
            .all(|s| !s.recovering && s.indexed_height == s.height)
        {
            return Ok(values);
        }
        ensure!(
            Instant::now() < deadline,
            "native recovery timeout: {values:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn receipts(
    nodes: &mut ProductNodes,
    clients: &mut [NativeApplicationClient],
    hashes: &[[u8; 32]],
) -> Result<Vec<ReceiptState>> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        nodes.alive()?;
        let mut all = Vec::new();
        for client in clients.iter_mut() {
            let mut rows = Vec::new();
            for hash in hashes {
                if let Answer::Receipt {
                    state: state @ ReceiptState::Finalized { .. },
                    ..
                } = query(client, Query::Receipt { hash: *hash })?
                {
                    rows.push(state);
                }
            }
            all.push(rows);
        }
        if all.iter().all(|rows| rows.len() == hashes.len()) {
            ensure!(
                all.iter().all(|rows| rows == &all[0]),
                "four native receipts differ: {all:?}"
            );
            return Ok(all.remove(0));
        }
        ensure!(
            Instant::now() < deadline,
            "native final receipt timeout: {all:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn balances(
    clients: &mut [NativeApplicationClient],
    done: &[ReceiptState],
    moved: [u128; 2],
) -> Result<Vec<Balance>> {
    let fee = |parity: usize| -> Result<u128> {
        done.iter()
            .skip(parity)
            .step_by(2)
            .try_fold(0u128, |sum, r| {
                let ReceiptState::Finalized { charged_fee, .. } = r else {
                    anyhow::bail!("nonfinal fee");
                };
                sum.checked_add(*charged_fee)
                    .context("fixture fee overflow")
            })
    };
    let expected = [
        1_000_000 - moved[0] - fee(0)?,
        moved[0] + moved[1],
        1_000_000 - moved[1] - fee(1)?,
    ];
    let mut snapshots = Vec::new();
    for client in clients {
        let mut values = Vec::new();
        for (seed, amount) in [1, 2, 3].into_iter().zip(expected) {
            let bytes = account(seed).as_bytes().to_vec();
            let account = match bytes.len() {
                20 => AccountId::Bytes20(bytes.try_into().unwrap()),
                32 => AccountId::Bytes32(bytes.try_into().unwrap()),
                _ => anyhow::bail!("fixture account width"),
            };
            let deadline = Instant::now() + Duration::from_secs(10);
            let value = loop {
                if let Answer::NovBalance(Some(value)) =
                    query(client, Query::NovBalance { account })?
                {
                    break value;
                }
                ensure!(Instant::now() < deadline, "native balance timeout");
                std::thread::sleep(Duration::from_millis(2));
            };
            ensure!(
                value.amount == amount,
                "native economics differ: {value:?} expected={amount}"
            );
            values.push(value);
        }
        snapshots.push(values);
    }
    ensure!(
        snapshots.iter().all(|s| s == &snapshots[0]),
        "native balance roots differ"
    );
    Ok(snapshots.remove(0))
}

fn hashes(raw: &[Vec<u8>]) -> Result<Vec<[u8; 32]>> {
    raw.iter()
        .map(|bytes| Ok(authenticate_transfer_v3(bytes, CHAIN, 1024)?.tx_hash()))
        .collect()
}

#[test]
#[ignore = "two then four actual nodes, real AOEM, no HTTP; run alone"]
fn native_only_minority_and_pending_signature_conflict_recover() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY").context("explicit binary required")?,
    )
    .canonicalize()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-rpc")
        .join(format!(
            "native-minority-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    eprintln!("native minority artifacts={}", directory.display());
    let (mut nodes, set) = setup_with_genesis(&directory, binary, genesis()?, 2)?;
    for path in &nodes.configs {
        let mut config: Value = serde_json::from_slice(&fs::read(path)?)?;
        config["rpc_addr"] = Value::Null;
        config["profile"] = json!(crate::native_pipeline::service::config::ORIGINAL_PROFILE);
        config["native_client_limit"] = json!(2);
        fs::write(path, serde_json::to_vec_pretty(&config)?)?;
    }
    let proposer = (0..4)
        .find(|i| {
            Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == set.leader(1, 0).unwrap()
        })
        .unwrap();
    let ingress = (proposer + 1) % 4;
    let online = [proposer, ingress];
    for i in online {
        nodes.start(i, "create")?;
    }
    let mut clients = online
        .into_iter()
        .map(|i| connect(&nodes, i, set.clone()))
        .collect::<Result<Vec<_>>>()?;
    ready(&mut nodes, &mut clients)?;
    let raw = vec![signed(1, 0, 100)?, signed(3, 0, 50)?];
    let ids = hashes(&raw)?;
    send(&mut clients[1], &raw)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let minority = loop {
        nodes.alive()?;
        let observations = clients.iter_mut().map(status).collect::<Result<Vec<_>>>()?;
        ensure!(
            observations.iter().all(|s| s.height == 0),
            "2/4 published a head"
        );
        if observations.iter().all(|s| s.apfl_rows >= 2) {
            break observations;
        }
        ensure!(
            Instant::now() < deadline,
            "minority execution timeout: {observations:?}"
        );
    };
    ensure!(
        matches!(
            query(&mut clients[1], Query::Receipt { hash: ids[0] })?,
            Answer::Receipt {
                state: ReceiptState::Received,
                ..
            }
        ),
        "transport/execution fabricated final receipt"
    );
    let mut bad = raw[0].clone();
    *bad.last_mut().unwrap() ^= 1;
    let conflict = signed(1, 0, 101)?;
    let conflict_id = hashes(std::slice::from_ref(&conflict))?[0];
    send(&mut clients[1], &[bad, conflict])?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let value = status(&mut clients[1])?;
        ensure!(
            value.height == 0,
            "minority published while rejecting input"
        );
        if value.rejected_inputs >= 2 {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "signature/nonce conflict not rejected"
        );
    }
    ensure!(
        matches!(
            query(&mut clients[1], Query::Receipt { hash: conflict_id })?,
            Answer::Receipt {
                state: ReceiptState::NotRecent,
                ..
            }
        ),
        "conflict entered pending"
    );
    send(&mut clients[1], &raw)?;
    // Extend the SAME sessions; do not reset signers or fabricate missing votes.
    for i in 0..4 {
        if !online.contains(&i) {
            nodes.start(i, "create")?;
            clients.push(connect(&nodes, i, set.clone())?);
        }
    }
    ready(&mut nodes, &mut clients)?;
    let done = receipts(&mut nodes, &mut clients, &ids)?;
    ensure!(
        done.iter()
            .all(|r| matches!(r, ReceiptState::Finalized { success: true, .. })),
        "recovered minority result differs"
    );
    let balances = balances(&mut clients, &done, [100, 50])?;
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&json!({
            "source":"original_native_execution_tick",
            "transport":"native_novorudp_apfl","http_listeners":0,"minority":minority,
            "pending_signature_rechecked":true,"nonce_conflict_rejected":true,
            "real_processes":4,"real_machines":1,"receipts":done,"balances":balances,
            "throughput_acceptance":false
        }))?,
    )?;
    nodes.stop_all()?;
    Ok(())
}

#[test]
#[ignore = "four actual original nodes, real AOEM; native-only; run alone"]
fn native_only_genesis_transactions_queries_and_cold_recovery() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY").context("explicit binary required")?,
    )
    .canonicalize()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-rpc")
        .join(format!(
            "native-only-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    eprintln!("native-only artifacts={}", directory.display());
    let (mut nodes, set) = setup_with_genesis(&directory, binary, genesis()?, 2)?;
    let ingress = (0..4)
        .find(|i| {
            Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                != set.leader(1, 0).unwrap()
        })
        .unwrap();
    for path in &nodes.configs {
        let mut config: Value = serde_json::from_slice(&fs::read(path)?)?;
        config["rpc_addr"] = Value::Null;
        config["profile"] = json!(crate::native_pipeline::service::config::ORIGINAL_PROFILE);
        config["native_client_limit"] = json!(2);
        fs::write(path, serde_json::to_vec_pretty(&config)?)?;
    }
    for i in 0..4 {
        nodes.start(i, "create")?;
    }
    let mut clients = (0..4)
        .map(|i| connect(&nodes, i, set.clone()))
        .collect::<Result<Vec<_>>>()?;
    let old_query = clients[0].prepare_query(Query::Status)?;
    ensure!(
        clients[0].prepare_query(Query::Status).is_err(),
        "query budget bypass"
    );
    ensure!(
        clients[1]
            .try_send_query(&old_query, Instant::now())
            .is_err(),
        "foreign query owner accepted"
    );
    clients[0].abandon_query(&old_query)?;
    ensure!(
        clients[0]
            .try_send_query(&old_query, Instant::now())
            .is_err(),
        "abandoned query accepted"
    );
    let initial = ready(&mut nodes, &mut clients)?;
    ensure!(initial.iter().all(|s| s.height == 0), "not fresh genesis");
    balances(&mut clients, &[], [0, 0])?;
    ensure!(
        matches!(
            query(&mut clients[0], Query::Receipt { hash: [99; 32] })?,
            Answer::Receipt {
                state: ReceiptState::NotRecent,
                indexed_height: 0
            }
        ),
        "unknown receipt must not imply execution or proven absence"
    );
    // Configuration disables HTTP even if a comparison binary was supplied.
    for endpoint in &nodes.endpoints {
        let address: std::net::SocketAddr = endpoint
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .parse()?;
        ensure!(
            std::net::TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_err(),
            "HTTP listener unexpectedly active"
        );
    }
    let mut bad = signed(1, 0, 100)?;
    *bad.last_mut().unwrap() ^= 1;
    send(&mut clients[ingress], &[bad])?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while status(&mut clients[ingress])?.rejected_inputs == 0 {
        ensure!(Instant::now() < deadline, "bad signature not rejected");
    }
    let mut all_hashes = Vec::new();
    let mut done = Vec::new();
    for raw in [
        vec![signed(1, 0, 100)?, signed(3, 0, 50)?],
        vec![signed(1, 1, 1_000_001)?, signed(3, 1, 25)?],
        vec![signed(1, 2, 50)?, signed(3, 2, 25)?],
    ] {
        send(&mut clients[ingress], &raw)?;
        let batch = hashes(&raw)?;
        done.extend(receipts(&mut nodes, &mut clients, &batch)?);
        all_hashes.extend(batch);
    }
    ensure!(
        done.iter().enumerate().all(
            |(i, r)| matches!(r, ReceiptState::Finalized { success, .. } if *success == (i!=2))
        ),
        "business failure differs"
    );
    let before_balances = balances(&mut clients, &done, [150, 100])?;
    let before = ready(&mut nodes, &mut clients)?;
    ensure!(
        before.iter().all(|s| s.height == before[0].height
            && s.block_hash == before[0].block_hash
            && s.state_root == before[0].state_root
            && s.apfl_rows >= 6),
        "APFL execution/head differs"
    );
    if test_execution_backend()? == "aoem_integer_v7" {
        ensure!(
            before
                .iter()
                .all(|s| s.integer_rows >= 6 && s.integer_plans >= 3),
            "integer plan not used"
        );
    }
    drop(clients);
    nodes.stop_all()?;
    for i in 0..4 {
        nodes.start(i, "existing")?;
    }
    let mut clients = (0..4)
        .map(|i| connect(&nodes, i, set.clone()))
        .collect::<Result<Vec<_>>>()?;
    let recovered = ready(&mut nodes, &mut clients)?;
    ensure!(
        recovered.iter().all(|s| s.height == before[0].height
            && s.block_hash == before[0].block_hash
            && s.state_root == before[0].state_root
            && s.receipt_batch_commitment == before[0].receipt_batch_commitment),
        "cold head differs"
    );
    ensure!(
        receipts(&mut nodes, &mut clients, &all_hashes)? == done,
        "cold receipts differ"
    );
    ensure!(
        balances(&mut clients, &done, [150, 100])? == before_balances,
        "cold economics differ"
    );
    send(&mut clients[ingress], &[signed(1, 0, 100)?])?;
    let next = vec![signed(1, 3, 1)?, signed(3, 3, 1)?];
    send(&mut clients[ingress], &next)?;
    done.extend(receipts(&mut nodes, &mut clients, &hashes(&next)?)?);
    let final_balances = balances(&mut clients, &done, [151, 101])?;
    fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&json!({
            "source":"original_native_execution_tick",
            "backend":test_execution_backend()?,"transport":"native_novorudp_apfl","http_listeners":0,
            "submission_and_queries_same_session":true,"single_nonvalidator_ingress":ingress,
            "real_processes":4,"real_machines":1,"receipts":done,"before":before,"recovered":recovered,
            "balances":final_balances,"throughput_acceptance":false,"business_proof_verified":false
        }))?,
    )?;
    nodes.stop_all()?;
    Ok(())
}
