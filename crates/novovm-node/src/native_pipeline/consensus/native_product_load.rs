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
    AccountId, Answer, Balance, Query, ReceiptState, Status, MAX_RECEIPTS, STREAM,
};
use serde::Serialize;
use std::{collections::BTreeSet, net::UdpSocket};

const WINDOW: usize = 8192;
const MAX_MESSAGE: usize = 512 * 1024;
// Existing endpoint/object sender/reassembly contracts all cap a turn at 64.
const IO_QUANTUM: usize = 64;

/// Diagnostic only. The ABI opens databases read/write even for reads: callers
/// must supply an offline COPY, never the measured store or a running signer.
/// Reuses the product archive codec and signature verifier; creates no votes.
#[test]
#[ignore = "requires explicit offline database copy and its original config"]
fn inspect_copied_first_height_decision() -> Result<()> {
    use crate::native_pipeline::consensus::chain::ChainRecord;
    use novovm_consensus::round_bft::{journal::codec::decode_archived_decision, VerifiedDecision};
    let database = PathBuf::from(std::env::var("NOVOVM_AUDIT_DATABASE_COPY")?);
    ensure!(
        database
            .file_name()
            .is_some_and(|n| n == "audit-copy.rocksdb"),
        "audit requires an explicitly named offline copy"
    );
    let config: ResidentConfig =
        serde_json::from_slice(&fs::read(std::env::var("NOVOVM_AUDIT_ORIGINAL_CONFIG")?)?)?;
    let genesis = config.genesis;
    let set = ValidatorSet::new(
        genesis.chain_id,
        genesis.validator_epoch,
        1,
        genesis
            .validators
            .iter()
            .map(|v| Validator::new(v.public_key, v.weight))
            .collect::<Result<Vec<_>>>()?,
    )?;
    let store = CandidateStore::open(
        StoreConfig {
            library: library()?,
            database,
            domain: StorageDomain {
                chain_id: genesis.chain_id,
                genesis_config_commitment: genesis.genesis_config_commitment,
                protocol_commitment: genesis.protocol_commitment,
            },
            storage: StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        },
        OpenMode::Existing,
    )?;
    let read = |key| -> Result<Vec<u8>> {
        store
            .read_metadata(&[key])?
            .values
            .pop()
            .flatten()
            .context("archive metadata missing")
    };
    let record = ChainRecord::decode(&read(MetaKey::ChainBlock { height: 1 })?)?;
    let outbox = record.outbox_key();
    let MetaKey::ConsensusOutbox {
        sequence,
        validator,
    } = outbox
    else {
        bail!("wrong outbox key")
    };
    let (proposal, certificate) = decode_archived_decision(&read(outbox)?, sequence)?;
    let point = record.point();
    VerifiedDecision::verify(
        &proposal,
        &certificate,
        &set,
        record.context(),
        point.block_hash,
    )?;
    let stored = store
        .recover(record.candidate_id())?
        .context("candidate absent")?;
    let statement = BlockStatement::from_stored(&stored, record.context(), &set, &record.parent())?;
    ensure!(
        statement.hash() == point.block_hash && stored.state_root() == point.state_root,
        "archive does not bind executed result"
    );
    ensure!(
        (1..=1024).contains(&sequence),
        "first-height audit revision bound"
    );
    // No public generic historical-outbox decoder exists: decode_replay is
    // private to the journal and requires its snapshot replay-reference pin.
    // Inspect only the bounded NVOUT002 envelope + first signed message here,
    // then reuse the product wire decoders/signature verifiers. This does NOT
    // validate historical replay evidence, snapshot lineage or timing.
    let inspect = |bytes: &[u8], revision: u64| -> Result<Value> {
        ensure!(
            (81..=512 * 1024).contains(&bytes.len()),
            "audit outbox size"
        );
        let (body, checksum) = bytes.split_at(bytes.len() - 32);
        let mut hash = Sha256::new();
        hash.update(b"novovm/replacement/signing-journal/v1\0");
        hash.update(body);
        ensure!(
            body.starts_with(b"NVOUT002") && hash.finalize().as_slice() == checksum,
            "audit outbox version/checksum"
        );
        ensure!(
            u64::from_be_bytes(body[8..16].try_into()?) == revision,
            "audit outbox sequence mismatch"
        );
        let kind = body[48];
        let mut summary = json!({"sequence":revision,"kind":kind,
            "envelope_verified":true,"message_signature_verified":false,
            "full_replay_evidence_revalidated":false});
        if kind == 0 {
            summary["kind"] = json!("state_only");
            return Ok(summary);
        }
        ensure!(body.len() >= 53, "audit truncated message frame");
        let size = u32::from_be_bytes(body[49..53].try_into()?) as usize;
        ensure!(
            size <= wire::MAX_WIRE_BYTES && size <= body.len() - 53,
            "audit message frame bound"
        );
        let frame = &body[53..53 + size];
        match kind {
            1 => {
                let p = wire::decode_proposal(frame)?;
                p.verify(&set)?;
                ensure!(
                    p.context == record.context() && p.proposer_id == validator,
                    "audit proposal owner/context"
                );
                summary["kind"] = json!("proposal");
                summary["round"] = json!(p.round);
                summary["value"] = json!(p.value);
            }
            2 => {
                let vote = wire::decode_vote(frame)?;
                vote.verify(&set)?;
                ensure!(
                    vote.context == record.context() && vote.validator_id == validator,
                    "audit vote owner/context"
                );
                summary["kind"] = json!("vote");
                summary["round"] = json!(vote.round);
                summary["phase"] = json!(vote.phase);
                summary["value"] = json!(vote.value);
            }
            3 => {
                let (p, qc) = decode_archived_decision(bytes, revision)?;
                VerifiedDecision::verify(&p, &qc, &set, record.context(), point.block_hash)?;
                summary["kind"] = json!("decision");
                summary["round"] = json!(p.round);
                summary["value"] = json!(p.value);
                summary["qc_signatures"] = json!(qc.votes.len());
            }
            _ => bail!("unknown audit outbox kind"),
        }
        summary["message_signature_verified"] = json!(true);
        Ok(summary)
    };
    let mut history = Vec::new();
    for revision in 1..=sequence {
        let bytes = read(MetaKey::ConsensusOutbox {
            validator,
            sequence: revision,
        })?;
        history.push(inspect(&bytes, revision)?);
    }
    println!(
        "{}",
        json!({"height":1,"round":proposal.round,
        "qc_signatures":certificate.votes.len(),"transactions":stored.raw_transactions().len(),
        "verified_decision":true,"validator":validator,
        "round_zero_leader":set.leader(1,0)?,"outbox_history":history,
        "scope":"offline copy; bounded envelope and message verification, not full replay validation or timing attribution"})
    );
    Ok(())
}

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
            key: transport_key(index).verifying_key().to_bytes(),
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
    let request = client.prepare_query(selector.clone())?;
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
    observation_rows_received: usize,
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
    count: usize,
    sent: bool,
    deadline: Instant,
}

// Every row remains independently checked against all four nodes, including
// repeated rows in a partially-final batch. A batch response is not a finality
// shortcut; it only amortizes the authenticated request/response envelope.
fn observe_receipt(
    row: &mut Observation,
    node: usize,
    index: usize,
    value: ReceiptState,
    elapsed_us: u64,
    counts: &mut Counts,
) -> Result<bool> {
    counts.observation_rows_received += 1;
    match value {
        ReceiptState::Finalized {
            success,
            nonce_after,
            charged_fee,
            height,
            block_hash,
        } => {
            if let Some(expected) = row.receipt {
                ensure!(
                    expected == value,
                    "four-node/cold receipt mismatch at {index}"
                );
            }
            ensure!(
                nonce_after == row.expected_nonce
                    && charged_fee == row.expected_fee
                    && height > 0
                    && block_hash != [0; 32],
                "receipt economics/nonce mismatch at {index}"
            );
            row.receipt = Some(value);
            if row.finalized_us[node].is_some() {
                return Ok(false);
            }
            row.finalized_us[node] = Some(elapsed_us);
            if row.complete() {
                if success {
                    counts.successful_transactions += 1;
                } else {
                    counts.business_failed_transactions += 1;
                }
                return Ok(true);
            }
        }
        ReceiptState::RejectedNonce => {
            counts.rejected_nonce += 1;
            bail!("valid workload nonce rejected at {index}");
        }
        _ => {
            ensure!(
                row.finalized_us[node].is_none(),
                "finalized receipt regressed at {index}"
            );
            counts.observation_not_final += 1;
        }
    }
    Ok(false)
}

/// Four existing native sessions are driven together. One outstanding read per
/// session is the real SDK contract, not HTTP batching or a privileged readback.
/// Supply uses the historical 8192 window and only four final replies free it.
#[allow(clippy::too_many_arguments)] // Keep process, SDK, workload and observation owners explicit.
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
    let query_batch = env_count("NOVOVM_NATIVE_QUERY_BATCH", 1, MAX_RECEIPTS)?;
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
                let count = query_batch.min(dispatched - index);
                let selector = if query_batch == 1 {
                    Query::Receipt {
                        hash: observations[index].hash,
                    }
                } else {
                    Query::Receipts {
                        hashes: observations[index..index + count]
                            .iter()
                            .map(|row| row.hash)
                            .collect::<Vec<_>>()
                            .try_into()
                            .map_err(anyhow::Error::msg)?,
                    }
                };
                reads[node] = Some(PendingRead {
                    request: clients[node].prepare_query(selector)?,
                    index,
                    count,
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
            let count = read.count;
            reads[node] = None;
            let states = match response.answer {
                Answer::Receipt { state, .. } if query_batch == 1 => vec![state],
                Answer::Receipts { states, .. } if query_batch > 1 => states.into_vec(),
                Answer::Error(_) => {
                    counts.observation_service_busy += 1;
                    continue;
                }
                other => bail!("unexpected receipt answer: {other:?}"),
            };
            ensure!(states.len() == count, "native receipt batch shape mismatch");
            let elapsed_us = start.elapsed().as_micros() as u64;
            for (offset, state) in states.into_iter().enumerate() {
                complete += usize::from(observe_receipt(
                    &mut observations[index + offset],
                    node,
                    index + offset,
                    state,
                    elapsed_us,
                    counts,
                )?);
            }
            while cursors[node] < dispatched
                && observations[cursors[node]].finalized_us[node].is_some()
            {
                cursors[node] += 1;
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
    let query_batch = env_count("NOVOVM_NATIVE_QUERY_BATCH", 1, MAX_RECEIPTS)?;
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
        "timeout_seconds":timeout.as_secs(),"query_batch":query_batch,"cold_recovery_pass":false,"full_fee_state_audited":false,
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
        // Existing readiness replies only: no extra observation traffic and no
        // timer reset. These precede APFL preparation, not the first submission.
        evidence["startup_status_before_apfl_preparation"] = json!(initial);
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
    evidence["first_height_all_receipts_observed_us"] = json!(observations
        .iter()
        .filter(|row| matches!(row.receipt, Some(ReceiptState::Finalized { height: 1, .. })))
        .flat_map(|row| row.finalized_us.iter().flatten())
        .max());
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

#[test]
fn native_load_batch_observer_requires_every_node_and_does_not_count_retries() -> Result<()> {
    let mut row = Observation {
        hash: [1; 32],
        first_submit_us: Some(0),
        finalized_us: [None; 4],
        receipt: None,
        expected_nonce: 7,
        expected_fee: 45,
    };
    let good = ReceiptState::Finalized {
        success: true,
        nonce_after: 7,
        charged_fee: 45,
        height: 2,
        block_hash: [3; 32],
    };
    let mut counts = Counts::default();
    assert!(!observe_receipt(
        &mut row,
        0,
        0,
        ReceiptState::Received,
        1,
        &mut counts
    )?);
    for node in 0..3 {
        assert!(!observe_receipt(&mut row, node, 0, good, 2, &mut counts)?);
        assert!(!observe_receipt(&mut row, node, 0, good, 3, &mut counts)?);
    }
    assert_eq!(counts.successful_transactions, 0);
    assert!(observe_receipt(&mut row, 3, 0, good, 4, &mut counts)?);
    assert!(!observe_receipt(&mut row, 3, 0, good, 5, &mut counts)?);
    assert_eq!(counts.successful_transactions, 1);
    assert_eq!(row.finalized_us, [Some(2), Some(2), Some(2), Some(4)]);
    let wrong = ReceiptState::Finalized {
        success: false,
        nonce_after: 7,
        charged_fee: 45,
        height: 2,
        block_hash: [3; 32],
    };
    assert!(observe_receipt(&mut row, 3, 0, wrong, 6, &mut counts).is_err());
    assert!(observe_receipt(&mut row, 3, 0, ReceiptState::NotRecent, 6, &mut counts).is_err());
    let mut failed = row.clone();
    failed.finalized_us = [None; 4];
    failed.receipt = None;
    for node in 0..4 {
        assert_eq!(
            observe_receipt(&mut failed, node, 1, wrong, 7, &mut counts)?,
            node == 3
        );
    }
    assert_eq!(counts.business_failed_transactions, 1);
    assert_eq!(counts.successful_transactions, 1);
    Ok(())
}
