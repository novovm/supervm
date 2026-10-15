//! Original tick, one native SDK/APFL submission entry, four-node final receipts.
//! Reuses ProductNodes and the shipped client; no test executor or generated QC.
//! Workload identities/order match resident_rpc_process/load + controller_workload:
//! 1024 independent senders, 64 nonces, one shared recipient, amount 1.
use super::*;
use crate::native_pipeline::{
    business::direct_nov_fee::{quote_transfer, TransferFeeRequest},
    consensus::{
        native::{LaneLimits, MessageDomain},
        transport::DecodeLimits,
    },
    ingress::apfl::{ApflLimits, ApflTransferBatch},
    service::{
        client::{NativeApplicationClient, PreparedApplicationInput, PreparedQuery},
        ResidentConfig,
    },
};
use novovm_network::novorudp::{
    endpoint::NativePeer,
    fragments::ReassemblyLimits,
    object_link::{NativeSendLimits, NativeSendPolicy},
    object_sender::ObjectSendState,
    NovoRudpPacingProfile, NovoRudpWindowConfig,
};
use novovm_protocol::native_query::{
    AccountId, Answer, Balance, Query, ReceiptState, Status, STREAM,
};
use serde::Serialize;
use std::{collections::BTreeSet, net::UdpSocket};

const WINDOW: usize = 8192;
const MAX_MESSAGE: usize = 512 * 1024;
// Existing endpoint/object sender/reassembly contracts all cap a turn at 64.
const IO_QUANTUM: usize = 64;

fn body_budget(batch: usize) -> usize {
    // Same original service::body_byte_limit, not a wider client-only budget.
    (batch * 1024).min(384 * 1024)
}

fn env_count(name: &str, default: usize, max: usize) -> Result<usize> {
    let value = std::env::var(name)
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(default);
    ensure!(
        (1..=max).contains(&value),
        "{name} outside bounded test range"
    );
    Ok(value)
}
fn fixture_key(domain: &[u8], index: u64) -> SigningKey {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(index.to_be_bytes());
    SigningKey::from_bytes(&hash.finalize().into())
}
fn fixture_account(key: &SigningKey) -> Account {
    Account::try_from(Sha256::digest(key.verifying_key().to_bytes())[12..].to_vec()).unwrap()
}
fn clock_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

// Configuration only: the implementation remains the existing shared SDK.
fn client(
    nodes: &ProductNodes,
    index: usize,
    set: Arc<ValidatorSet>,
    batch: usize,
) -> Result<NativeApplicationClient> {
    let config: ResidentConfig = serde_json::from_slice(&fs::read(&nodes.configs[index])?)?;
    NativeApplicationClient::connect(
        UdpSocket::bind("127.0.0.1:0")?,
        SigningKey::from_bytes(&[201; 32]),
        NativePeer {
            key: validator_key(index).verifying_key().to_bytes(),
            address: config.udp_addr,
        },
        MessageDomain {
            chain_id: CHAIN,
            genesis: config.genesis.genesis_config_commitment,
            protocol: config.genesis.protocol_commitment,
            validators: set,
        },
        DecodeLimits {
            transactions: batch,
            transaction_bytes: 1024,
            body_bytes: body_budget(batch),
            message_bytes: MAX_MESSAGE,
        },
        LaneLimits {
            receive: ReassemblyLimits {
                max_message_bytes: MAX_MESSAGE,
                messages: 8,
                bytes: 8 * MAX_MESSAGE,
                peer_messages: 8,
                peer_bytes: 8 * MAX_MESSAGE,
                ttl: Duration::from_secs(30),
            },
            send: NativeSendLimits {
                messages: 8,
                bytes: 8 * MAX_MESSAGE,
                peer_messages: 8,
                peer_bytes: 8 * MAX_MESSAGE,
            },
        },
        NativeSendPolicy {
            window: NovoRudpWindowConfig {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 32,
                batch_pause_ms: 0,
                ack_wait_ms: 10,
                ..Default::default()
            },
            pacing: NovoRudpPacingProfile {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 32,
                batch_pause_ms: 0,
                ack_wait_ms: 10,
                no_progress_backoff: false,
            },
            ttl: Duration::from_secs(30),
        },
        clock_ms()?,
        Instant::now(),
    )
}

fn query(client: &mut NativeApplicationClient, selector: Query) -> Result<Answer> {
    let request = client.prepare_query(selector)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut sent = false;
    loop {
        let now = Instant::now();
        if now >= deadline {
            client.abandon_query(&request)?;
            bail!("native read deadline: {selector:?}");
        }
        let _ = client.poll(clock_ms()?, now, IO_QUANTUM)?;
        if !sent {
            sent = client.try_send_query(&request, now)?;
        }
        if let Some(reply) = client.poll_query_response(&request, now)? {
            let _ = client.poll(clock_ms()?, Instant::now(), IO_QUANTUM)?;
            return Ok(reply.answer);
        }
        std::thread::yield_now();
    }
}
fn ready(nodes: &mut ProductNodes, clients: &mut [NativeApplicationClient]) -> Result<Vec<Status>> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        nodes.alive()?;
        let mut statuses = Vec::new();
        for client in clients.iter_mut() {
            let Answer::Status(status) = query(client, Query::Status)? else {
                bail!("native status missing");
            };
            ensure!(!status.projection_failed, "native projection failure");
            statuses.push(status);
        }
        if statuses
            .iter()
            .all(|s| !s.recovering && s.indexed_height == s.height)
        {
            return Ok(statuses);
        }
        ensure!(
            Instant::now() < deadline,
            "native startup/recovery deadline"
        );
    }
}

#[derive(Clone, Serialize)]
struct Observation {
    hash: Hash,
    first_submit_us: Option<u64>,
    finalized_us: [Option<u64>; 4],
    receipt: Option<ReceiptState>,
    expected_nonce: u64,
    expected_fee: u128,
}
impl Observation {
    fn complete(&self) -> bool {
        self.finalized_us.iter().all(Option::is_some)
    }
}
#[derive(Default, Serialize)]
struct Counts {
    application_batches_sent: usize,
    application_batches_transport_delivered: usize,
    transport_failures: usize,
    observation_requests: usize,
    observation_not_final: usize,
    observation_service_busy: usize,
    successful_transactions: usize,
    business_failed_transactions: usize,
    rejected_nonce: usize,
    deadline_expirations: usize,
    maximum_outstanding: usize,
}
struct PendingRead {
    request: PreparedQuery,
    index: usize,
    sent: bool,
    deadline: Instant,
}

/// Four existing native sessions are driven together. One outstanding read per
/// session is the real SDK contract, not HTTP batching or a privileged readback.
/// Supply uses the historical 8192 window and only four final replies free it.
fn drive(
    nodes: &mut ProductNodes,
    clients: &mut [NativeApplicationClient],
    ingress: usize,
    inputs: &[PreparedApplicationInput],
    batch: usize,
    observations: &mut [Observation],
    counts: &mut Counts,
    timeout: Duration,
) -> Result<Duration> {
    let start = Instant::now();
    let cold = inputs.is_empty();
    let mut dispatched = if cold { observations.len() } else { 0 };
    let mut complete = 0;
    let mut next_batch = 0;
    let mut cursors = [0usize; 4];
    let mut reads: [Option<PendingRead>; 4] = std::array::from_fn(|_| None);
    let mut delivered = BTreeSet::new();
    let ids: BTreeSet<_> = inputs.iter().map(PreparedApplicationInput::id).collect();
    let mut last_alive = Instant::now();
    while complete < observations.len() {
        let now = Instant::now();
        if start.elapsed() >= timeout {
            counts.deadline_expirations += 1;
            bail!("native load timeout: completed={complete}/{}, dispatched={dispatched}, cursors={cursors:?}", observations.len());
        }
        if last_alive.elapsed() >= Duration::from_millis(100) {
            nodes.alive()?;
            last_alive = now;
        }
        let wall = clock_ms()?;
        for (node, client) in clients.iter_mut().enumerate() {
            for done in client.poll(wall, now, IO_QUANTUM)?.completed {
                if done.stream == STREAM {
                    continue;
                }
                if node != ingress
                    || !ids.contains(&done.body.id())
                    || done.state != ObjectSendState::Delivered
                {
                    counts.transport_failures += 1;
                    bail!(
                        "native application completion mismatch/expiry: node={node}, state={:?}",
                        done.state
                    );
                }
                if delivered.insert(done.body.id()) {
                    counts.application_batches_transport_delivered += 1;
                }
            }
        }
        if !cold && next_batch < inputs.len() {
            let end = (dispatched + batch).min(observations.len());
            if end - complete <= WINDOW {
                let elapsed = start.elapsed().as_micros() as u64;
                for row in &mut observations[dispatched..end] {
                    row.first_submit_us.get_or_insert(elapsed);
                }
                if clients[ingress].try_send(&inputs[next_batch], now)? {
                    dispatched = end;
                    next_batch += 1;
                    counts.application_batches_sent += 1;
                    counts.maximum_outstanding =
                        counts.maximum_outstanding.max(dispatched - complete);
                }
            }
        }
        for node in 0..4 {
            if reads[node].is_none() && cursors[node] < dispatched {
                let index = cursors[node];
                reads[node] = Some(PendingRead {
                    request: clients[node].prepare_query(Query::Receipt {
                        hash: observations[index].hash,
                    })?,
                    index,
                    sent: false,
                    deadline: now + Duration::from_secs(5),
                });
                counts.observation_requests += 1;
            }
            let Some(read) = reads[node].as_mut() else {
                continue;
            };
            if now >= read.deadline {
                clients[node].abandon_query(&read.request)?;
                counts.deadline_expirations += 1;
                bail!(
                    "native receipt request deadline node={node} transaction={}",
                    read.index
                );
            }
            if !read.sent {
                read.sent = clients[node].try_send_query(&read.request, now)?;
            }
            let Some(response) = clients[node].poll_query_response(&read.request, now)? else {
                continue;
            };
            let index = read.index;
            reads[node] = None;
            match response.answer {
                Answer::Receipt {
                    state:
                        value @ ReceiptState::Finalized {
                            success,
                            nonce_after,
                            charged_fee,
                            height,
                            block_hash,
                        },
                    ..
                } => {
                    let row = &mut observations[index];
                    if let Some(expected) = row.receipt {
                        ensure!(
                            expected == value,
                            "four-node/cold receipt mismatch at {index}"
                        );
                    } else {
                        row.receipt = Some(value);
                    }
                    ensure!(
                        nonce_after == row.expected_nonce
                            && charged_fee == row.expected_fee
                            && height > 0
                            && block_hash != [0; 32],
                        "receipt economics/nonce mismatch at {index}"
                    );
                    row.finalized_us[node] = Some(start.elapsed().as_micros() as u64);
                    cursors[node] += 1;
                    if row.complete() {
                        complete += 1;
                        if success {
                            counts.successful_transactions += 1;
                        } else {
                            counts.business_failed_transactions += 1;
                        }
                    }
                }
                Answer::Receipt {
                    state: ReceiptState::RejectedNonce,
                    ..
                } => {
                    counts.rejected_nonce += 1;
                    bail!("valid workload nonce rejected at {index}");
                }
                Answer::Receipt { .. } => counts.observation_not_final += 1,
                Answer::Error(_) => counts.observation_service_busy += 1,
                other => bail!("unexpected receipt answer: {other:?}"),
            }
        }
        std::thread::yield_now();
    }
    let completed_us = observations
        .iter()
        .flat_map(|row| row.finalized_us.iter().flatten().copied())
        .max()
        .context("no finalized receipt observation")?;
    let first_submit_us = if cold {
        0
    } else {
        observations
            .iter()
            .filter_map(|row| row.first_submit_us)
            .min()
            .context("no native submission attempt")?
    };
    Ok(Duration::from_micros(
        completed_us
            .checked_sub(first_submit_us)
            .context("receipt observation preceded first submission")?,
    ))
}

fn verify_balances(
    clients: &mut [NativeApplicationClient],
    expected: &[(Account, u128)],
    reference: Status,
) -> Result<Vec<Balance>> {
    let mut first = Vec::new();
    for (node, client) in clients.iter_mut().enumerate() {
        let mut values = Vec::new();
        for (account, amount) in expected {
            let account = AccountId::Bytes20(account.as_bytes().try_into()?);
            let deadline = Instant::now() + Duration::from_secs(10);
            let balance = loop {
                if let Answer::NovBalance(Some(value)) =
                    query(client, Query::NovBalance { account })?
                {
                    break value;
                }
                ensure!(Instant::now() < deadline, "native balance read timeout");
            };
            ensure!(
                balance.amount == *amount
                    && balance.height == reference.height
                    && balance.state_root == reference.state_root,
                "native balance/fee conservation mismatch node={node}"
            );
            values.push(balance);
        }
        if node == 0 {
            first = values;
        } else {
            ensure!(values == first, "four-node balances differ");
        }
    }
    Ok(first)
}

fn same_head(statuses: &[Status], reference: Status) -> Result<()> {
    ensure!(
        statuses.iter().all(|s| s.height == reference.height
            && s.block_hash == reference.block_hash
            && s.state_root == reference.state_root
            && s.receipt_batch_commitment == reference.receipt_batch_commitment
            && s.state_version == reference.state_version
            && s.indexed_height == s.height),
        "four-node/cold authoritative head mismatch"
    );
    Ok(())
}
fn percentile(sorted: &[u64], percent: usize) -> Option<u64> {
    (!sorted.is_empty()).then(|| sorted[(sorted.len() * percent).div_ceil(100).saturating_sub(1)])
}

#[test]
#[ignore = "four actual original-tick nodes, AOEM, native-only 65536 signed transactions and cold restart; run alone"]
fn original_native_sdk_65536_final_receipts_and_cold_economics() -> Result<()> {
    let count = env_count("NOVOVM_NATIVE_LOAD_TRANSACTIONS", 65_536, 65_536)?;
    let batch = env_count("NOVOVM_NATIVE_LOAD_BATCH_SIZE", 1024, 1024)?;
    let timeout =
        Duration::from_secs(env_count("NOVOVM_NATIVE_LOAD_TIMEOUT_SECONDS", 600, 3600)? as u64);
    ensure!(
        count % batch == 0,
        "load count must be a whole number of signer/nonce batches"
    );
    let rounds = count / batch;
    let binary = fs::canonicalize(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
            .context("explicit original binary required")?,
    )?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-load")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    eprintln!(
        "native original-tick load artifacts={}",
        directory.display()
    );
    let mut evidence = json!({"source":"original_native_execution_tick","transactions":count,"batch_size":batch,
        "full_65536_workload":count==65536 && batch==1024,"window":WINDOW,"sdk_io_quantum":IO_QUANTUM,"body_byte_budget":body_budget(batch),"real_processes":4,"real_machines":1,
        "transport":"native_novorudp_apfl","http_listeners":0,"signature_generation_timed":false,
        "binary":binary,"binary_sha256":hex(&Sha256::digest(fs::read(&binary)?)),
        "aoem_sha256":hex(&Sha256::digest(fs::read(library()?)?)),"backend":test_execution_backend()?,
        "timeout_seconds":timeout.as_secs(),"cold_recovery_pass":false,"full_fee_state_audited":false,
        "business_proof_verified":false,"system_capacity_claim":false,
        "diagnostics_enabled":std::env::var("NOVOVM_NATIVE_LOAD_DIAGNOSTICS").as_deref()==Ok("1")});
    let mut phase = "prepare signed workload";
    let mut observations = Vec::new();
    let mut counts = Counts::default();
    let mut measured = None;
    let mut timed_attempt = None;
    let result = (|| -> Result<()> {
        let recipient = fixture_account(&fixture_key(
            b"novovm/controller-workload/recipient/v1\0",
            0,
        ));
        let keys: Vec<_> = (0..batch)
            .map(|i| fixture_key(b"novovm/controller-workload/sender/v1\0", i as u64))
            .collect();
        let senders: Vec<_> = keys.iter().map(fixture_account).collect();
        ensure!(
            senders.iter().collect::<BTreeSet<_>>().len() == batch && !senders.contains(&recipient),
            "workload identity collision"
        );
        let fee = quote_transfer(
            &TransferFeeRequest {
                tx_hash: [0; 32],
                payer: senders[0].clone(),
                recipient: recipient.clone(),
                asset: "NOV".into(),
                amount: 1,
                pay_asset: "NOV".into(),
                max_pay_amount: 0,
                slippage_bps: 0,
            },
            &policy(),
            0,
        )??
        .nov_amount;
        let debit = fee.checked_add(1).context("workload debit overflow")?;
        let funding = debit
            .checked_mul(rounds as u128)
            .and_then(|v| v.checked_add(1_000_000))
            .context("workload funding overflow")?;
        let mut raw = Vec::with_capacity(count);
        for nonce in 0..rounds {
            for (key, sender) in keys.iter().zip(&senders) {
                let mut tx = TransferV3 {
                    chain_id: CHAIN,
                    from: sender.as_bytes().to_vec(),
                    to: recipient.as_bytes().to_vec(),
                    asset: "NOV".into(),
                    amount: 1,
                    nonce: nonce as u64,
                    fee_policy: FeePolicy {
                        pay_asset: "NOV".into(),
                        max_pay_amount: 0,
                        slippage_bps: 0,
                    },
                    signature: Vec::new(),
                };
                let signature = key.sign(&signing_message(&tx)?);
                tx.signature
                    .extend_from_slice(&key.verifying_key().to_bytes());
                tx.signature.extend_from_slice(&signature.to_bytes());
                let bytes = encode_transfer_v3(&tx)?;
                let checked = authenticate_transfer_v3(&bytes, CHAIN, 1024)?;
                observations.push(Observation {
                    hash: checked.tx_hash(),
                    first_submit_us: None,
                    finalized_us: [None; 4],
                    receipt: None,
                    expected_nonce: nonce as u64 + 1,
                    expected_fee: fee,
                });
                raw.push(bytes);
            }
        }
        let mut genesis = genesis()?;
        genesis.allocations = senders
            .iter()
            .map(|sender| GenesisAllocation {
                account_hex: hex(sender.as_bytes()),
                amount: funding.to_string(),
            })
            .collect();
        genesis.genesis_config_commitment = genesis.derive_commitment()?;
        let (mut nodes, set) = setup_with_genesis(&directory, binary.clone(), genesis, batch)?;
        for path in &nodes.configs {
            let mut config: Value = serde_json::from_slice(&fs::read(path)?)?;
            config["rpc_addr"] = Value::Null;
            config["profile"] = json!(ORIGINAL_PROFILE);
            config["native_client_limit"] = json!(2);
            fs::write(path, serde_json::to_vec_pretty(&config)?)?;
        }
        let ingress = (0..4)
            .find(|i| {
                Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                    .unwrap()
                    .id()
                    != set.leader(1, 0).unwrap()
            })
            .unwrap();
        evidence["single_submission_node"] = json!(ingress);
        evidence["first_proposer_is_submission_node"] = json!(false);
        phase = "start four original nodes";
        for i in 0..4 {
            nodes.start(i, "create")?;
        }
        let mut clients = (0..4)
            .map(|i| client(&nodes, i, set.clone(), batch))
            .collect::<Result<Vec<_>>>()?;
        let initial = ready(&mut nodes, &mut clients)?;
        ensure!(initial.iter().all(|s| s.height == 0), "not a fresh genesis");
        for endpoint in &nodes.endpoints {
            let address: SocketAddr = endpoint.trim_start_matches("http://").parse()?;
            ensure!(
                std::net::TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_err(),
                "HTTP listener unexpectedly open"
            );
        }
        let inputs = raw
            .chunks(batch)
            .map(|rows| {
                clients[ingress].prepare(Arc::new(ApflTransferBatch::from_raw(
                    rows,
                    ApflLimits {
                        transactions: batch,
                        transaction_bytes: 1024,
                        body_bytes: body_budget(batch),
                    },
                )?))
            })
            .collect::<Result<Vec<_>>>()?;
        phase = "native submission and four-node final receipt observation";
        timed_attempt = Some(Instant::now());
        measured = Some(drive(
            &mut nodes,
            &mut clients,
            ingress,
            &inputs,
            batch,
            &mut observations,
            &mut counts,
            timeout,
        )?);
        timed_attempt = None;
        ensure!(
            counts.successful_transactions == count && counts.business_failed_transactions == 0,
            "successful workload has business failures"
        );
        phase = "full native balances and head";
        let before = ready(&mut nodes, &mut clients)?;
        same_head(&before, before[0])?;
        ensure!(
            before.iter().all(|s| s.apfl_rows >= count as u64),
            "APFL execution not observed on all nodes"
        );
        let mut expected: Vec<_> = senders
            .iter()
            .map(|a| (a.clone(), funding - rounds as u128 * debit))
            .collect();
        expected.push((recipient, count as u128));
        let balances = verify_balances(&mut clients, &expected, before[0])?;
        evidence["before"] = json!(before);
        evidence["balances"] = json!(balances);
        evidence["receipt_fee_total"] = json!((count as u128 * fee).to_string());
        evidence["account_amount_and_receipt_fee_conservation"] = json!(true);
        evidence["receipt_nonce_sequence_verified"] = json!(true);
        let old_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        drop(clients);
        nodes.stop_all()?;
        phase = "cold restart, all receipts and balances";
        for i in 0..4 {
            nodes.start(i, "existing")?;
        }
        let new_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        ensure!(
            new_pids.len() == 4 && new_pids.iter().all(|pid| !old_pids.contains(pid)),
            "four real processes did not restart"
        );
        let mut clients = (0..4)
            .map(|i| client(&nodes, i, set.clone(), batch))
            .collect::<Result<Vec<_>>>()?;
        let recovered = ready(&mut nodes, &mut clients)?;
        same_head(&recovered, before[0])?;
        let mut cold = observations.clone();
        for row in &mut cold {
            row.finalized_us = [None; 4];
        }
        let mut cold_counts = Counts::default();
        let cold_elapsed = drive(
            &mut nodes,
            &mut clients,
            ingress,
            &[],
            batch,
            &mut cold,
            &mut cold_counts,
            timeout,
        )?;
        ensure!(
            cold_counts.successful_transactions == count,
            "cold receipt count changed"
        );
        ensure!(
            verify_balances(&mut clients, &expected, before[0])? == balances,
            "cold balances differ"
        );
        evidence["recovered"] = json!(recovered);
        evidence["cold_receipt_observation_seconds"] = json!(cold_elapsed.as_secs_f64());
        evidence["cold_counts"] = json!(cold_counts);
        evidence["cold_recovery_pass"] = json!(true);
        nodes.stop_all()?;
        Ok(())
    })();
    let mut latency: Vec<_> = observations
        .iter()
        .filter(|r| r.complete())
        .map(|r| {
            r.finalized_us
                .iter()
                .flatten()
                .copied()
                .max()
                .unwrap()
                .saturating_sub(r.first_submit_us.unwrap())
        })
        .collect();
    latency.sort_unstable();
    evidence["phase"] = json!(phase);
    evidence["passed"] = json!(result.is_ok());
    evidence["error"] = json!(result.as_ref().err().map(|e| format!("{e:#}")));
    evidence["counts"] = json!(counts);
    evidence["incomplete_timed_attempt_seconds"] =
        json!(timed_attempt.map(|start| start.elapsed().as_secs_f64()));
    evidence["transactions_not_confirmed_on_all_four"] =
        json!(observations.iter().filter(|row| !row.complete()).count());
    evidence["elapsed_seconds"] = json!(measured.map(|d| d.as_secs_f64()));
    evidence["successful_tps"] =
        json!(measured.map(|d| counts.successful_transactions as f64 / d.as_secs_f64()));
    evidence["business_failed_tps"] =
        json!(measured.map(|d| counts.business_failed_transactions as f64 / d.as_secs_f64()));
    evidence["latency_us"] = json!({"p95":percentile(&latency,95),"p99":percentile(&latency,99),"sample_count":latency.len(),
        "definition":"nearest-rank first native batch submission attempt to slowest of four final receipt replies; includes bounded one-query-per-session observer and submission-window scheduling"});
    evidence["timing"]=json!("first native SDK submission attempt through all unique four-node finalized receipt responses; signatures, startup, APFL preparation, post-run economics and cold restart excluded");
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

#[test]
fn native_load_wallet_identity_and_percentile_contract() {
    let domain = b"novovm/controller-workload/sender/v1\0";
    assert_ne!(
        fixture_key(domain, 0).to_bytes(),
        fixture_key(domain, 256).to_bytes()
    );
    assert_eq!(percentile(&[], 95), None);
    assert_eq!(percentile(&[1, 2, 3, 4], 95), Some(4));
    assert_eq!(percentile(&(1..=100).collect::<Vec<_>>(), 95), Some(95));
    assert_eq!(percentile(&(1..=100).collect::<Vec<_>>(), 99), Some(99));
}
