//! Real HTTP workload against the original executable. This is explicitly
//! four-way client fanout by default, or explicit single-ingress submission.
//! Both modes require four-node finality; neither bypasses the product RPC.
//! All signing/oracle work is outside the timed window; failures retain reports.
use super::super::controller_workload::{ExpectedState, Workload, AMOUNT};
use super::*;
use crate::native_pipeline::business::direct_nov_fee::{
    quote_and_settle, quote_transfer, TransferFeeRequest,
};
use crate::native_pipeline::business::quoted_transfer::{
    compute_outcome, TransferIntent, TransferSnapshot,
};
use crate::native_pipeline::consensus::chain::ChainRecord;
use crate::native_pipeline::state::tree::read_state_value;
use novovm_consensus::round_bft::{journal::codec::decode_archived_decision, VerifiedDecision};
use serde::Serialize;
use std::collections::BTreeSet;
use std::io::{BufWriter, Write};

const SENDERS: usize = 1024;
const NONCES: u64 = 64;
const TRANSACTIONS: usize = SENDERS * NONCES as usize;
const WINDOW: usize = SENDERS * 8;
const HTTP_BATCH: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubmissionMode {
    Fanout,
    SingleIngress(usize),
}

impl SubmissionMode {
    fn parse(mode: Option<&str>, ingress: Option<&str>) -> Result<Self> {
        match mode.unwrap_or("fanout") {
            "fanout" => {
                ensure!(
                    ingress.is_none(),
                    "ingress node only applies to single_ingress"
                );
                Ok(Self::Fanout)
            }
            "single_ingress" => {
                let node = ingress.unwrap_or("0").parse::<usize>()?;
                ensure!(node < 4, "single ingress node must be in 0..4");
                Ok(Self::SingleIngress(node))
            }
            _ => bail!("RPC load mode must be fanout or single_ingress"),
        }
    }

    fn targets(self, node: usize) -> bool {
        match self {
            Self::Fanout => true,
            Self::SingleIngress(ingress) => ingress == node,
        }
    }

    fn indices(self, indices: std::ops::Range<usize>) -> [Vec<usize>; 4] {
        std::array::from_fn(|node| {
            if self.targets(node) {
                indices.clone().collect()
            } else {
                Vec::new()
            }
        })
    }

    fn retry(self, observations: &[Observation], dispatched: usize) -> [Vec<usize>; 4] {
        std::array::from_fn(|node| {
            if self.targets(node) {
                (0..dispatched)
                    .filter(|&i| !observations[i].admitted[node])
                    .take(HTTP_BATCH)
                    .collect()
            } else {
                Vec::new()
            }
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Fanout => "fanout",
            Self::SingleIngress(_) => "single_ingress",
        }
    }

    fn entry(self) -> Option<usize> {
        match self {
            Self::Fanout => None,
            Self::SingleIngress(node) => Some(node),
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Fanout => "explicit identical signed JSON-RPC fanout to all four nodes; does not prove single-ingress propagation",
            Self::SingleIngress(_) => "one fixed HTTP submission node; peer propagation and four-node final receipt queries; no client transaction fanout",
        }
    }
}

#[derive(Serialize)]
struct Observation {
    hash: String,
    first_submit_us: Option<u64>,
    admission_attempts: [u32; 4],
    admitted: [bool; 4],
    permanent_rejected: [bool; 4],
    finalized_us: [Option<u64>; 4],
    // All four independently read RPC receipts must equal this complete value.
    receipt: Option<Value>,
}

impl Observation {
    fn complete(&self) -> bool {
        self.finalized_us.iter().all(Option::is_some)
    }
}

#[derive(Default, Serialize)]
struct Counts {
    admission_attempts: u64,
    admission_error_responses: u64,
    retryable_admission_responses: u64,
    permanent_admission_responses: u64,
    retry_attempts: u64,
    transport_errors: u64,
    observation_requests: u64,
    finalized_successful: usize,
    finalized_business_failed: usize,
    max_outstanding: usize,
    dispatch_rounds: usize,
    observation_rounds: usize,
    deadline_expirations: u64,
}

struct HttpReply {
    at: Instant,
    response: std::result::Result<Value, String>,
}

struct Events {
    writer: BufWriter<fs::File>,
    count: u64,
}
impl Events {
    fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            writer: BufWriter::new(fs::File::create(path)?),
            count: 0,
        })
    }
    fn push(&mut self, value: Value) -> Result<()> {
        serde_json::to_writer(&mut self.writer, &value)?;
        self.writer.write_all(b"\n")?;
        self.count += 1;
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        Ok(self.writer.flush()?)
    }
}

// One HTTP request per node, four concurrent requests total. The caller waits
// for HTTP completion only, never a per-batch finality barrier. Response time is
// recorded in each thread, not after the slowest peer has joined.
fn fanout(nodes: &ProductNodes, requests: &[Option<Value>; 4]) -> Result<Vec<Option<HttpReply>>> {
    std::thread::scope(|scope| {
        let mut tasks = Vec::new();
        for (index, request) in requests.iter().enumerate() {
            let Some(request) = request else {
                continue;
            };
            let agent = nodes.agent.clone();
            let endpoint = nodes.endpoints[index].clone();
            let body = serde_json::to_string(request)?;
            tasks.push((
                index,
                scope.spawn(move || {
                    let response = (|| -> Result<Value> {
                        let bytes = agent
                            .post(&endpoint)
                            .set("Content-Type", "application/json")
                            .send_string(&body)?
                            .into_string()?;
                        Ok(serde_json::from_str(&bytes)?)
                    })()
                    .map_err(|error| format!("{error:#}"));
                    HttpReply {
                        at: Instant::now(),
                        response,
                    }
                }),
            ));
        }
        let mut replies: Vec<_> = (0..4).map(|_| None).collect();
        for (index, task) in tasks {
            replies[index] = Some(
                task.join()
                    .map_err(|_| anyhow::anyhow!("RPC worker panicked"))?,
            );
        }
        Ok(replies)
    })
}

fn elapsed_us(start: Instant, at: Instant) -> u64 {
    at.duration_since(start)
        .as_micros()
        .try_into()
        .expect("bounded run duration")
}

fn request_batch(indices: &[usize], method: &str, params: impl Fn(usize) -> Value) -> Value {
    Value::Array(
        indices
            .iter()
            .map(
                |index| json!({"jsonrpc":"2.0","id":index,"method":method,"params":params(*index)}),
            )
            .collect(),
    )
}

fn checked_results(response: Value, requested: &[usize]) -> Result<Vec<(usize, Value)>> {
    let responses = response
        .as_array()
        .context("batch RPC response is not an array")?;
    ensure!(
        responses.len() == requested.len(),
        "batch response count mismatch"
    );
    let expected: BTreeSet<_> = requested.iter().copied().collect();
    let mut seen = BTreeSet::new();
    let mut results = Vec::with_capacity(responses.len());
    for response in responses {
        let index: usize = response["id"]
            .as_u64()
            .context("missing integer RPC id")?
            .try_into()?;
        ensure!(
            expected.contains(&index) && seen.insert(index),
            "foreign or repeated batch RPC id"
        );
        ensure!(response["jsonrpc"] == "2.0", "invalid RPC response version");
        results.push((index, response.clone()));
    }
    Ok(results)
}

fn submit_round(
    nodes: &ProductNodes,
    indices: &[Vec<usize>; 4],
    raw_hex: &[String],
    observations: &mut [Observation],
    counts: &mut Counts,
    events: &mut Events,
    started: Instant,
) -> Result<()> {
    let requests = std::array::from_fn(|node| {
        (!indices[node].is_empty()).then(|| {
            request_batch(&indices[node], "nov_sendRawTransaction", |i| {
                json!([raw_hex[i]])
            })
        })
    });
    let first = elapsed_us(started, Instant::now());
    for (node, indices) in indices.iter().enumerate() {
        for &index in indices {
            let observed = &mut observations[index];
            observed.first_submit_us.get_or_insert(first);
            if observed.admission_attempts[node] > 0 {
                counts.retry_attempts += 1;
            }
            observed.admission_attempts[node] += 1;
            counts.admission_attempts += 1;
        }
    }
    counts.dispatch_rounds += 1;
    let mut permanent_rejection = false;
    for (node, reply) in fanout(nodes, &requests)?.into_iter().enumerate() {
        let Some(reply) = reply else {
            continue;
        };
        let response = match reply.response {
            Ok(response) => response,
            Err(error) => {
                counts.transport_errors += 1;
                events.push(json!({"phase":"submit","node":node,"at_us":elapsed_us(started,reply.at),
                    "indices":indices[node],"transport_error":error,"outcome":"unknown; retry same signed bytes"}))?;
                continue;
            }
        };
        for (index, response) in checked_results(response, &indices[node])? {
            if let Some(error) = response.get("error") {
                counts.admission_error_responses += 1;
                let retryable = error["message"].as_str().is_some_and(|message| {
                    message.contains("projection catching up")
                        || message.contains("pool backpressure")
                });
                if retryable {
                    counts.retryable_admission_responses += 1;
                } else {
                    counts.permanent_admission_responses += 1;
                    observations[index].permanent_rejected[node] = true;
                    permanent_rejection = true;
                }
                events.push(json!({"phase":"submit","node":node,"index":index,
                    "at_us":elapsed_us(started,reply.at),"retryable":retryable,"response":response}))?;
            } else {
                let result = response.get("result").context("missing admission result")?;
                ensure!(
                    result["tx_hash"] == observations[index].hash,
                    "admission hash differs from signed input"
                );
                ensure!(
                    result["state"] == "received"
                        || result["state"] == "finalized_success"
                        || result["state"] == "finalized_business_failure",
                    "unexpected admission state"
                );
                if result["state"] == "received" {
                    ensure!(
                        result["finalized"] == false
                            && result["executed"] == false
                            && result["admission_durable"] == false,
                        "admission claimed false finality"
                    );
                }
                observations[index].admitted[node] = true;
            }
        }
    }
    events.flush()?;
    ensure!(
        !permanent_rejection,
        "funded signed input permanently rejected; all responses retained"
    );
    Ok(())
}

fn observe_round(
    nodes: &ProductNodes,
    indices: &[Vec<usize>; 4],
    observations: &mut [Observation],
    counts: &mut Counts,
    events: &mut Events,
    started: Instant,
) -> Result<()> {
    let requests = std::array::from_fn(|node| {
        (!indices[node].is_empty()).then(|| {
            request_batch(&indices[node], "nov_getTransactionStatus", |i| {
                json!([observations[i].hash])
            })
        })
    });
    counts.observation_rounds += 1;
    counts.observation_requests += indices.iter().map(Vec::len).sum::<usize>() as u64;
    for (node, reply) in fanout(nodes, &requests)?.into_iter().enumerate() {
        let Some(reply) = reply else {
            continue;
        };
        let response = match reply.response {
            Ok(response) => response,
            Err(error) => {
                counts.transport_errors += 1;
                events.push(
                    json!({"phase":"observe","node":node,"at_us":elapsed_us(started,reply.at),
                    "indices":indices[node],"transport_error":error}),
                )?;
                continue;
            }
        };
        for (index, response) in checked_results(response, &indices[node])? {
            ensure!(
                response.get("error").is_none(),
                "status RPC error: {response}"
            );
            let result = response.get("result").context("missing status result")?;
            ensure!(
                result["tx_hash"] == observations[index].hash,
                "status hash mismatch"
            );
            if result["finalized"] != true {
                ensure!(
                    result["state"] != "rejected_nonce",
                    "accepted input lost to conflicting nonce"
                );
                continue;
            }
            ensure!(
                result["executed"] == true
                    && result["proof_verified"] == false
                    && result["finality_kind"] == "BFT_durable"
                    && result["success"].is_boolean(),
                "unverified or incomplete final receipt"
            );
            let observed = &mut observations[index];
            if let Some(previous) = &observed.receipt {
                ensure!(previous == result, "four-node receipt disagreement");
            } else {
                observed.receipt = Some(result.clone());
            }
            ensure!(
                observed.first_submit_us.is_some(),
                "unsubmitted input was counted"
            );
            ensure!(
                observed.finalized_us[node].is_none(),
                "receipt observed twice"
            );
            observed.finalized_us[node] = Some(elapsed_us(started, reply.at));
            if observed.complete() {
                if result["success"] == true {
                    counts.finalized_successful += 1;
                } else {
                    counts.finalized_business_failed += 1;
                }
            }
        }
    }
    events.flush()?;
    Ok(())
}

fn timed_load(
    nodes: &mut ProductNodes,
    mode: SubmissionMode,
    raw_hex: &[String],
    observations: &mut [Observation],
    counts: &mut Counts,
    events: &mut Events,
    timeout: Duration,
) -> Result<f64> {
    let started = Instant::now();
    let mut dispatched = 0usize;
    let mut cursor = 0usize;
    while counts.finalized_successful + counts.finalized_business_failed < TRANSACTIONS {
        nodes.alive()?;
        if started.elapsed() >= timeout {
            counts.deadline_expirations += 1;
            bail!("real RPC workload deadline expired");
        }
        // Retry only unresolved node admissions. HTTP transport failure means
        // UNKNOWN, not rejection; exact signed bytes and first-submit time survive.
        let retry = mode.retry(observations, dispatched);
        if retry.iter().any(|indices| !indices.is_empty()) {
            submit_round(
                nodes,
                &retry,
                raw_hex,
                observations,
                counts,
                events,
                started,
            )?;
            // A persistent retry does not skip process liveness/deadline checks.
            if retry
                .iter()
                .flatten()
                .any(|&i| observations[i].admission_attempts.iter().any(|&n| n > 1))
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            continue;
        }
        let completed = counts.finalized_successful + counts.finalized_business_failed;
        let credit = WINDOW - (dispatched - completed);
        if dispatched < TRANSACTIONS && credit > 0 {
            let end = (dispatched + credit.min(HTTP_BATCH)).min(TRANSACTIONS);
            let indices = mode.indices(dispatched..end);
            dispatched = end;
            counts.max_outstanding = counts.max_outstanding.max(dispatched - completed);
            submit_round(
                nodes,
                &indices,
                raw_hex,
                observations,
                counts,
                events,
                started,
            )?;
            continue;
        }
        // Fair bounded observations release only four-node finality credit.
        let mut selected = Vec::new();
        for offset in 0..dispatched {
            let index = (cursor + offset) % dispatched;
            if !observations[index].complete() {
                selected.push(index);
            }
            if selected.len() == HTTP_BATCH {
                break;
            }
        }
        ensure!(!selected.is_empty(), "no observable outstanding work");
        cursor = (selected.last().copied().unwrap() + 1) % dispatched;
        let indices = std::array::from_fn(|node| {
            selected
                .iter()
                .copied()
                .filter(|&i| observations[i].finalized_us[node].is_none())
                .collect()
        });
        observe_round(nodes, &indices, observations, counts, events, started)?;
    }
    let last = observations
        .iter()
        .flat_map(|o| o.finalized_us.iter().flatten())
        .copied()
        .max()
        .context("no final receipts")?;
    let first = observations
        .iter()
        .filter_map(|o| o.first_submit_us)
        .min()
        .context("no submissions")?;
    Ok((last - first) as f64 / 1_000_000.0)
}

fn verify_cold_receipts(nodes: &mut ProductNodes, observations: &[Observation]) -> Result<()> {
    for first in (0..TRANSACTIONS).step_by(HTTP_BATCH) {
        nodes.alive()?;
        let indices: Vec<_> = (first..(first + HTTP_BATCH).min(TRANSACTIONS)).collect();
        let requests = std::array::from_fn(|_| {
            Some(request_batch(&indices, "nov_getTransactionStatus", |i| {
                json!([observations[i].hash])
            }))
        });
        for (node, reply) in fanout(nodes, &requests)?.into_iter().enumerate() {
            let reply = reply.context("cold RPC fanout missing")?;
            let response = reply.response.map_err(anyhow::Error::msg)?;
            for (index, response) in checked_results(response, &indices)? {
                ensure!(
                    response.get("error").is_none(),
                    "cold status error on {node}: {response}"
                );
                ensure!(
                    response.get("result") == observations[index].receipt.as_ref(),
                    "cold RPC receipt differs on node {node}, index {index}"
                );
            }
        }
    }
    Ok(())
}

fn metadata_value(store: &CandidateStore, key: MetaKey) -> Result<Vec<u8>> {
    let mut reply = store.read_metadata(&[key])?;
    ensure!(reply.values.len() == 1, "archive metadata count mismatch");
    reply
        .values
        .pop()
        .flatten()
        .context("archive metadata missing")
}

// The serial oracle is the SAME quote/transfer/settlement procedure used by
// controller_workload::Workload::expected_through, applied to actual archived
// block order/timestamps. Real RPC batching is not assumed to be 64 full blocks.
fn apply_oracle(
    expected: &mut ExpectedState,
    checked: &crate::native_pipeline::ingress::authentication::SignatureCheckedTransfer,
    policy: &DirectNovFeePolicy,
    now: u128,
    receipt: &crate::native_pipeline::persistence::packet::ReceiptView,
) -> Result<()> {
    let tx = checked.transfer();
    let payer = Account::try_from(tx.from).map_err(anyhow::Error::msg)?;
    let recipient = Account::try_from(tx.to).map_err(anyhow::Error::msg)?;
    let identity = checked.nonce_identity();
    let before = expected.nonces.get(&identity).copied().unwrap_or(0);
    ensure!(
        tx.nonce == before,
        "archive signer nonce order differs from serial oracle"
    );
    let request = TransferFeeRequest {
        tx_hash: checked.tx_hash(),
        payer: payer.clone(),
        recipient: recipient.clone(),
        asset: tx.asset.to_owned(),
        amount: tx.amount,
        pay_asset: tx.fee_policy.pay_asset.to_owned(),
        max_pay_amount: tx.fee_policy.max_pay_amount,
        slippage_bps: tx.fee_policy.slippage_bps,
    };
    let snapshot = TransferSnapshot {
        payer_balance: *expected
            .balances
            .get(&payer)
            .context("unknown workload payer")?,
        recipient_balance: expected.balances.get(&recipient).copied().unwrap_or(0),
        next_nonce: before,
    };
    let quote = quote_transfer(&request, policy, now)??;
    let intent = TransferIntent {
        tx_hash: checked.tx_hash(),
        from: payer.clone(),
        to: recipient.clone(),
        nonce_identity: hex(&identity)[2..].to_owned(),
        nonce: tx.nonce,
        amount: tx.amount,
        approved_fee: quote.nov_amount,
        fee_cap: quote.max_pay_amount,
    };
    let outcome = compute_outcome(&intent, &snapshot, None)?;
    let settled = quote_and_settle(
        &request,
        policy,
        &expected.fees,
        snapshot.payer_balance,
        now,
    )?;
    let delta = outcome.delta();
    ensure!(
        outcome.is_success() && settled.failure.is_none(),
        "funded load failed in serial oracle"
    );
    ensure!(
        delta.nonce_after == before.checked_add(1).context("oracle nonce overflow")?
            && delta.fee_funding_delta == settled.charged_fee()
            && delta.payer.after.checked_add(tx.amount) == Some(settled.payer_after),
        "existing transfer/fee primitives disagree"
    );
    ensure!(
        receipt.tx_hash == checked.tx_hash()
            && receipt.signer_identity == identity
            && receipt.success
            && receipt.nonce_after == delta.nonce_after
            && receipt.charged_fee == delta.fee_funding_delta.to_string(),
        "archive receipt differs from full economic oracle"
    );
    expected.balances.insert(payer, delta.payer.after);
    expected.balances.insert(recipient, delta.recipient.after);
    expected.nonces.insert(identity, delta.nonce_after);
    expected.fees = settled.after_fee_state;
    expected.transactions += 1;
    Ok(())
}

fn audit_database(
    nodes: &ProductNodes,
    node: usize,
    workload: &Workload,
    genesis: &GenesisConfig,
    set: &ValidatorSet,
    final_head: ParentPoint,
    observations: &[Observation],
) -> Result<Value> {
    let initial = stage_state_update(
        &Memory::default(),
        empty_root(),
        &workload.initial_changes()?,
    )?;
    let mut parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: initial.root(),
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    let store = CandidateStore::open(
        StoreConfig {
            library: library()?,
            database: nodes.directory.join(format!("validator-{node}.rocksdb")),
            domain: StorageDomain {
                chain_id: CHAIN,
                genesis_config_commitment: genesis.genesis_config_commitment,
                protocol_commitment: genesis.protocol_commitment,
            },
            storage: StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        },
        OpenMode::Existing,
    )?;
    let mut expected = ExpectedState {
        height: 0,
        transactions: 0,
        balances: workload
            .senders()
            .iter()
            .map(|sender| (sender.account.clone(), workload.initial_sender_balance()))
            .collect(),
        nonces: BTreeMap::new(),
        fees: FeeState::default(),
    };
    let by_hash: BTreeMap<_, _> = observations.iter().map(|o| (o.hash.clone(), o)).collect();
    ensure!(
        by_hash.len() == TRANSACTIONS,
        "duplicate signed workload hash"
    );
    let mut seen = BTreeSet::new();
    let mut batches = Vec::new();
    let mut last_record = None;
    for height in 1..=final_head.height {
        let record = ChainRecord::decode(&metadata_value(&store, MetaKey::ChainBlock { height })?)?;
        let context = ConsensusContext {
            chain_id: CHAIN,
            genesis_config_commitment: genesis.genesis_config_commitment,
            protocol_commitment: genesis.protocol_commitment,
            epoch: genesis.validator_epoch,
            validator_set_hash: set.hash(),
            height,
            parent_block_hash: parent.block_hash,
            parent_decision_hash: parent.decision_hash,
        };
        ensure!(
            record.parent() == parent && record.context() == context,
            "archive has nonconsecutive parent/domain"
        );
        let stored = store
            .recover(record.candidate_id())?
            .context("decided candidate missing")?;
        let statement = BlockStatement::from_stored(&stored, context, set, &parent)?;
        let point = record.point();
        ensure!(
            statement.hash() == point.block_hash
                && stored.state_root() == point.state_root
                && stored.receipt_batch_commitment() == point.receipt_batch_commitment
                && statement.state_version() == point.state_version,
            "archive output roots are not signed value"
        );
        let outbox_key = record.outbox_key();
        let MetaKey::ConsensusOutbox { sequence, .. } = &outbox_key else {
            bail!("invalid outbox locator")
        };
        let (proposal, certificate) =
            decode_archived_decision(&metadata_value(&store, outbox_key.clone())?, *sequence)?;
        VerifiedDecision::verify(&proposal, &certificate, set, context, point.block_hash)?;
        ensure!(
            certificate.votes.len() >= 3,
            "equal-weight four-node QC missing threshold"
        );
        let now = u128::from(stored.context().timestamp_unix_ms);
        ensure!(
            stored.raw_transactions().len() == stored.receipt_bytes().len(),
            "archive body/receipt count mismatch"
        );
        for (index, raw) in stored.raw_transactions().iter().enumerate() {
            let checked = authenticate_transfer_v3(raw, CHAIN, 1024)?;
            let hash = hex(&checked.tx_hash());
            let rpc = by_hash
                .get(&hash)
                .context("archive contains transaction outside requested workload")?;
            ensure!(
                seen.insert(hash),
                "transaction executed twice in durable chain"
            );
            let receipt = stored.receipt_view(index)?;
            apply_oracle(&mut expected, &checked, &genesis.policy, now, &receipt)?;
            let rpc = rpc
                .receipt
                .as_ref()
                .context("missing timed four-node receipt")?;
            ensure!(
                rpc["block_height"] == height
                    && rpc["block_hash"] == hex(&point.block_hash)
                    && rpc["nonce_after"] == receipt.nonce_after
                    && rpc["charged_fee"] == receipt.charged_fee
                    && rpc["success"] == receipt.success,
                "RPC receipt differs from durable archive"
            );
        }
        expected.height = height;
        batches.push(json!({"height":height,"transactions":stored.raw_transactions().len(),
            "qc_signatures":certificate.votes.len(),"round":proposal.round,"block_hash":hex(&point.block_hash)}));
        parent = point;
        last_record = Some(record);
    }
    ensure!(
        parent == final_head
            && seen.len() == TRANSACTIONS
            && expected.transactions == TRANSACTIONS as u64,
        "incomplete durable workload prefix"
    );
    ensure!(
        metadata_value(&store, MetaKey::ChainHead)?
            == last_record.context("empty chain")?.head_bytes()?,
        "durable head pointer differs from verified archive"
    );
    ensure!(
        expected.nonces.len() == SENDERS && expected.nonces.values().all(|nonce| *nonce == NONCES),
        "complete signer nonce set differs"
    );
    ensure!(
        expected.balances[workload.recipient()] == TRANSACTIONS as u128 * AMOUNT,
        "shared recipient lost credit"
    );
    expected.fees.validate()?;
    let records = expected.record_changes(&genesis.policy)?;
    let oracle_root = stage_state_update(&Memory::default(), empty_root(), &records)?;
    ensure!(
        oracle_root.root() == final_head.state_root,
        "complete logical oracle state root mismatch"
    );
    for change in &records {
        let (key, value) = match change {
            StateChange::Put { key, value } => (key, Some(value.clone())),
            StateChange::Delete { key } => (key, None),
        };
        ensure!(
            read_state_value(&store, final_head.state_root, key)? == value,
            "durable balance/nonce/fee record differs from serial oracle"
        );
    }
    Ok(
        json!({"node":node,"head":final_head,"transactions":seen.len(),"signers":expected.nonces.len(),
        "shared_recipient_balance":expected.balances[workload.recipient()].to_string(),
        "fees":expected.fees,"full_state_root_equals_oracle":true,"batches":batches}),
    )
}

fn percentile(sorted: &[u64], percent: usize) -> Option<u64> {
    (!sorted.is_empty()).then(|| sorted[(sorted.len() * percent).div_ceil(100).saturating_sub(1)])
}

fn early_reuse_evidence(statuses: &[Value], explicitly_required: bool) -> Result<Value> {
    let available = statuses
        .iter()
        .any(|status| status.get("early_bind_reused").is_some());
    let total = if available {
        statuses.iter().try_fold(0u64, |total, status| {
            total
                .checked_add(
                    status["early_bind_reused"]
                        .as_u64()
                        .context("missing early reuse counter")?,
                )
                .context("early reuse counter overflow")
        })?
    } else {
        0
    };
    ensure!(
        !(available || explicitly_required) || total > 0,
        "large product RPC workload did not reuse any early authenticated body"
    );
    Ok(
        json!({"counters_available":available,"explicitly_required":explicitly_required,"total":total,
        "legacy_binary_without_counters_skipped":!available && !explicitly_required}),
    )
}

#[test]
#[ignore = "real product binary/AOEM required; 65,536 signed HTTP transfers, explicit submission mode, four-node finality and cold restart; run alone"]
fn actual_product_rpc_65536_signed_transfers_finality_and_cold_economics() -> Result<()> {
    let mode = SubmissionMode::parse(
        std::env::var("NOVOVM_RESIDENT_RPC_LOAD_MODE")
            .ok()
            .as_deref(),
        std::env::var("NOVOVM_RESIDENT_RPC_LOAD_INGRESS_NODE")
            .ok()
            .as_deref(),
    )?;
    let require_early = match std::env::var("NOVOVM_RESIDENT_RPC_REQUIRE_EARLY_REUSE").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => false,
        Ok("1") => true,
        _ => bail!("NOVOVM_RESIDENT_RPC_REQUIRE_EARLY_REUSE must be 0 or 1"),
    };
    let require_async_auth =
        match std::env::var("NOVOVM_RESIDENT_RPC_REQUIRE_ASYNC_AUTH").as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("0") => false,
            Ok("1") => true,
            _ => bail!("NOVOVM_RESIDENT_RPC_REQUIRE_ASYNC_AUTH must be 0 or 1"),
        };
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
            .context("explicit NOVOVM_RESIDENT_NODE_BINARY required")?,
    )
    .canonicalize()?;
    ensure!(binary.is_file(), "actual product binary missing");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/audit/resident-rpc-load")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    eprintln!("real 65,536 RPC workload artifacts={}", directory.display());
    let mut counts = Counts::default();
    let mut events = Events::new(&directory.join("events.jsonl"))?;
    let mut observations = Vec::new();
    let mut evidence = json!({"async_authentication_required":require_async_auth});
    let mut elapsed = None;
    let mut timed_wall_seconds = None;
    let mut phase = "prepare signatures/configuration";
    let result = (|| -> Result<()> {
        let workload = Workload::new(SENDERS, NONCES, policy())?;
        let raw = (1..=NONCES)
            .map(|height| workload.raw_height(height))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        ensure!(raw.len() == TRANSACTIONS, "workload count changed");
        let raw_hex: Vec<_> = raw.iter().map(|raw| hex(raw)).collect();
        for raw in &raw {
            let checked = authenticate_transfer_v3(raw, CHAIN, 1024)?;
            observations.push(Observation {
                hash: hex(&checked.tx_hash()),
                first_submit_us: None,
                admission_attempts: [0; 4],
                admitted: [false; 4],
                permanent_rejected: [false; 4],
                finalized_us: [None; 4],
                receipt: None,
            });
        }
        let mut genesis = genesis()?;
        genesis.allocations = workload
            .senders()
            .iter()
            .map(|sender| GenesisAllocation {
                account_hex: hex(sender.account.as_bytes()),
                amount: workload.initial_sender_balance().to_string(),
            })
            .collect();
        genesis.genesis_config_commitment = genesis.derive_commitment()?;
        genesis.validate()?;
        let timeout = std::env::var("NOVOVM_RESIDENT_RPC_LOAD_TIMEOUT_SECS")
            .ok()
            .map(|s| s.parse::<u64>())
            .transpose()?
            .unwrap_or(600);
        ensure!(
            (1..=3600).contains(&timeout),
            "bounded RPC test timeout required"
        );
        evidence = json!({"binary":binary,"binary_sha256":hex(&Sha256::digest(fs::read(&binary)?)),
            "aoem_sha256":hex(&Sha256::digest(fs::read(library()?)?)),"timeout_seconds":timeout,
            "genesis_commitment":hex(&genesis.genesis_config_commitment),
            "async_authentication_required":require_async_auth});
        let mut relay = Relay::start(&directory.join("relay"))?;
        let (mut nodes, set) =
            setup_with_genesis(&directory, &relay, binary.clone(), genesis.clone(), SENDERS)?;
        phase = "startup";
        for node in 0..4 {
            nodes.start(node, "create")?;
        }
        let startup = nodes.wait_ready(&[0, 1, 2, 3])?;
        ensure!(
            startup.iter().all(|s| s["head"].is_null()),
            "load must begin on fresh genesis"
        );
        phase = "timed HTTP admission and four-node finality observation";
        let timing_start = Instant::now();
        let measured = timed_load(
            &mut nodes,
            mode,
            &raw_hex,
            &mut observations,
            &mut counts,
            &mut events,
            Duration::from_secs(timeout),
        );
        timed_wall_seconds = Some(timing_start.elapsed().as_secs_f64());
        if measured.is_err() {
            evidence["last_rpc_status"] = Value::Array((0..4).map(|node| {
                nodes.rpc(node, "nov_chainStatus", json!([]))
                    .unwrap_or_else(|error| json!({"node":node,"status_read_error":format!("{error:#}")}))
            }).collect());
        }
        elapsed = Some(measured?);
        ensure!(
            counts.finalized_successful == TRANSACTIONS && counts.finalized_business_failed == 0,
            "success-only workload was not fully successful"
        );
        phase = "pre-restart full receipt verification";
        verify_cold_receipts(&mut nodes, &observations)?;
        let before = nodes.wait_ready(&[0, 1, 2, 3])?;
        // Preserve real counters even when the additional early-reuse gate fails.
        evidence["live_status"] = json!(before);
        evidence["async_authentication"] = json!({
            "explicitly_required":require_async_auth,
            "per_node":before.iter().map(|status|status["rpc_authentication"].clone()).collect::<Vec<_>>(),
            "counter_scope":"owner authentication work; may include duplicates/retries and is not the finalized TPS numerator",
            "control_polls_while_checking":"recorded only; zero is permitted for work completing between control polls",
        });
        if require_async_auth {
            check_async_authentication(&before)?;
        }
        let require_apfl = std::env::var("NOVOVM_RESIDENT_RPC_REQUIRE_APFL").as_deref() == Ok("1");
        evidence["apfl_views_required"] = json!(require_apfl);
        if require_apfl {
            ensure!(
                before.iter().all(|s| s["apfl_view_transactions_total"]
                    .as_u64()
                    .is_some_and(|count| count >= TRANSACTIONS as u64)),
                "four-node execution did not all consume APFL authenticated views"
            );
        }
        ensure!(
            before.iter().all(|s| s["head"] == before[0]["head"]),
            "four heads disagree"
        );
        for (node, _) in nodes.endpoints.iter().enumerate() {
            ensure!(
                observations.iter().all(|observation| {
                    if mode.targets(node) {
                        observation.admitted[node] && observation.admission_attempts[node] > 0
                    } else {
                        !observation.admitted[node] && observation.admission_attempts[node] == 0
                    }
                }),
                "HTTP submission escaped the selected mode"
            );
        }
        let head: ParentPoint = serde_json::from_value(before[0]["head"].clone())?;
        evidence["early_body_reuse"] = early_reuse_evidence(&before, require_early)?;
        ensure!(
            head.state_version == TRANSACTIONS as u64,
            "final state version differs from transaction count"
        );
        let live_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        nodes.stop_all()?;
        phase = "four-process cold restart and full receipt verification";
        for node in 0..4 {
            nodes.start(node, "existing")?;
        }
        let cold = nodes.wait_ready(&[0, 1, 2, 3])?;
        ensure!(
            cold.iter().all(|s| s["head"] == before[0]["head"]),
            "cold head changed"
        );
        verify_cold_receipts(&mut nodes, &observations)?;
        let cold_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        ensure!(
            cold_pids.iter().all(|pid| !live_pids.contains(pid)),
            "four nodes were not restarted"
        );
        nodes.stop_all()?;
        phase = "four offline AOEM archives, QC and full economic oracle";
        let audits = (0..4)
            .map(|node| {
                audit_database(&nodes, node, &workload, &genesis, &set, head, &observations)
            })
            .collect::<Result<Vec<_>>>()?;
        evidence["live_pids"] = json!(live_pids);
        evidence["cold_pids"] = json!(cold_pids);
        evidence["live_status"] = json!(before);
        evidence["cold_status"] = json!(cold);
        evidence["database_audits"] = json!(audits);
        relay.shutdown()?;
        phase = "complete";
        Ok(())
    })();
    let mut latencies: Vec<_> = observations
        .iter()
        .filter(|o| o.complete())
        .map(|o| o.finalized_us.iter().flatten().max().unwrap() - o.first_submit_us.unwrap())
        .collect();
    latencies.sort_unstable();
    let submitted = observations
        .iter()
        .filter(|o| o.first_submit_us.is_some())
        .count();
    let report = json!({"schema":"novovm/resident-product-rpc-load/v1","passed":result.is_ok(),
        "failure":result.as_ref().err().map(|e|format!("{e:#}")),"phase":phase,"evidence":evidence,
        "topology":"one host; four actual novovm-node processes and independent AOEM stores; WSS/E2E relay",
        "distribution":mode.description(),"submission_mode":mode.name(),"ingress_node":mode.entry(),
        "http_admission_targets":(0..4).filter(|&node|mode.targets(node)).collect::<Vec<_>>(),
        "http_admissions_by_node":(0..4).map(|node|observations.iter().filter(|o|o.admitted[node]).count()).collect::<Vec<_>>(),
        "workload":{"transactions":TRANSACTIONS,"senders":SENDERS,"nonces_per_sender":NONCES,
            "shared_recipient":true,"max_outstanding":WINDOW,"http_batch_max":HTTP_BATCH,
            "http_concurrency":4,"per_node_http_inflight":1,
            "submission_http_concurrency":if mode == SubmissionMode::Fanout {4} else {1},
            "observation_http_concurrency":4,
            "submission_mode":if mode == SubmissionMode::Fanout {"credit_first_bounded_fanout_v1"} else {"credit_first_bounded_single_ingress_v1"},
            "signatures_pregenerated":true,"expected_business_failures":0,"actual_block_count_is_not_assumed":true},
        "counts":counts,"unique_submitted":submitted,"unsubmitted":TRANSACTIONS-submitted,
        "unique_permanently_rejected":observations.iter().filter(|o|o.permanent_rejected.iter().any(|v|*v)).count(),
        "unresolved_submitted":submitted-counts.finalized_successful-counts.finalized_business_failed,
        "elapsed_seconds":elapsed,"timed_loop_wall_seconds_including_failed_attempt":timed_wall_seconds,
        "successful_tps":elapsed.map(|s|counts.finalized_successful as f64/s),
        "latency_us":{"p95":percentile(&latencies,95),"p99":percentile(&latencies,99),
            "scope":"first client submission dispatch to receipt HTTP response completion on all four nodes; nearest-rank; includes HTTP and bounded observation scheduling"},
        "timing":"first HTTP submission to all unique four-node finalized receipts; signing/startup/cold oracle excluded",
        "business_failure_load_exercised":false,"production_acceptance":false,"four_machine_test":false,
        "events_file":"events.jsonl","event_count":events.count,
        "failure_event_logging":"bounded-memory JSONL flushed per HTTP round; logging overhead included in timer",
        "all_transaction_observations_file":"observations.json"});
    events.flush()?;
    fs::write(
        directory.join("observations.json"),
        serde_json::to_vec(&observations)?,
    )?;
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    eprintln!(
        "real RPC load report={}",
        directory.join("result.json").display()
    );
    result
}

// These work counters are extra post-timer evidence, never the TPS numerator.
// Do not lower the four-node count to compensate for a missing gossip route or
// receipt projection winning a race: such a run must retain the unmet gate.
fn check_async_authentication(statuses: &[Value]) -> Result<()> {
    ensure!(
        statuses.len() == 4,
        "async authentication requires all four node statuses"
    );
    for (node, status) in statuses.iter().enumerate() {
        let observed = &status["rpc_authentication"];
        ensure!(
            observed["owner"] == "same_resident_aoem_compute_session",
            "node {node} RPC authentication used a missing or different owner"
        );
        for name in ["submitted_batches", "completed_batches"] {
            ensure!(
                observed[name].as_u64().is_some_and(|count| count > 0),
                "node {node} async authentication {name} is missing or zero"
            );
        }
        ensure!(
            observed["checked_rows"].as_u64().is_some_and(|rows| rows >= TRANSACTIONS as u64),
            "node {node} async authentication did not check all {TRANSACTIONS} signed rows: {observed}"
        );
    }
    Ok(())
}

#[test]
fn load_async_authentication_requires_same_owner_and_full_four_node_work_without_claiming_tps() {
    let good = json!({"rpc_authentication":{
        "owner":"same_resident_aoem_compute_session",
        "submitted_batches":64,"completed_batches":64,"checked_rows":TRANSACTIONS,
        "control_polls_while_checking":0,
    }});
    let statuses = vec![good; 4];
    check_async_authentication(&statuses).unwrap();
    assert!(check_async_authentication(&statuses[..3]).is_err());
    for (field, value) in [
        ("owner", json!("host_thread_pool")),
        ("owner", Value::Null),
        ("submitted_batches", json!(0)),
        ("completed_batches", json!(0)),
        ("checked_rows", json!(TRANSACTIONS - 1)),
        ("checked_rows", json!(TRANSACTIONS.to_string())),
    ] {
        for node in 0..4 {
            let mut changed = statuses.clone();
            changed[node]["rpc_authentication"][field] = value.clone();
            assert!(
                check_async_authentication(&changed).is_err(),
                "node {node} field {field} escaped gate"
            );
        }
    }
    let mut missing = statuses;
    missing[3] = json!({});
    assert!(check_async_authentication(&missing).is_err());
}

#[test]
fn load_batch_response_mapping_rejects_duplicate_missing_and_foreign_ids() {
    let response = |ids: &[u64]| {
        Value::Array(
            ids.iter()
                .map(|id| json!({"jsonrpc":"2.0","id":id,"result":{}}))
                .collect(),
        )
    };
    assert_eq!(
        checked_results(response(&[9, 7]), &[7, 9])
            .unwrap()
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        vec![9, 7]
    );
    for ids in [&[7, 7][..], &[7, 8][..], &[7][..]] {
        assert!(checked_results(response(ids), &[7, 9]).is_err());
    }
}

#[test]
fn load_counts_only_four_node_observation_and_uses_nearest_rank_latency() {
    let mut observed = Observation {
        hash: "test".into(),
        first_submit_us: Some(10),
        admission_attempts: [1; 4],
        admitted: [true; 4],
        permanent_rejected: [false; 4],
        finalized_us: [Some(20), Some(21), Some(22), None],
        receipt: Some(json!({"success":false})),
    };
    assert!(
        !observed.complete(),
        "admission and three final receipts do not release credit"
    );
    observed.finalized_us[3] = Some(35);
    assert!(observed.complete());
    assert_eq!(
        observed.finalized_us.iter().flatten().max().unwrap() - observed.first_submit_us.unwrap(),
        25
    );
    assert_eq!(percentile(&(1..=100).collect::<Vec<_>>(), 95), Some(95));
    assert_eq!(percentile(&(1..=100).collect::<Vec<_>>(), 99), Some(99));
    assert_eq!(percentile(&[], 99), None);
}

#[test]
fn load_submission_mode_defaults_to_four_node_fanout_and_rejects_ambiguous_configuration() {
    assert_eq!(
        SubmissionMode::parse(None, None).unwrap(),
        SubmissionMode::Fanout
    );
    assert_eq!(
        SubmissionMode::parse(Some("fanout"), None).unwrap(),
        SubmissionMode::Fanout
    );
    assert_eq!(
        SubmissionMode::Fanout.indices(7..9),
        [vec![7, 8], vec![7, 8], vec![7, 8], vec![7, 8]]
    );
    assert_eq!(
        SubmissionMode::parse(Some("single_ingress"), None).unwrap(),
        SubmissionMode::SingleIngress(0)
    );
    assert_eq!(
        SubmissionMode::parse(Some("single_ingress"), Some("3")).unwrap(),
        SubmissionMode::SingleIngress(3)
    );
    for (mode, ingress) in [
        (Some("fanout"), Some("0")),
        (None, Some("0")),
        (Some("single_ingress"), Some("4")),
        (Some("single_ingress"), Some("-1")),
        (Some("single"), None),
    ] {
        assert!(SubmissionMode::parse(mode, ingress).is_err());
    }
}

#[test]
fn load_single_ingress_never_retries_non_http_peers_or_releases_three_node_credit() {
    let mode = SubmissionMode::SingleIngress(2);
    assert_eq!(mode.indices(0..2), [vec![], vec![], vec![0, 1], vec![]]);
    let mut observed = Observation {
        hash: "test".into(),
        first_submit_us: Some(10),
        admission_attempts: [0, 0, 1, 0],
        admitted: [false, false, false, false],
        permanent_rejected: [false; 4],
        finalized_us: [Some(20), Some(21), Some(22), None],
        receipt: Some(json!({"success":true})),
    };
    assert_eq!(
        mode.retry(std::slice::from_ref(&observed), 1),
        [vec![], vec![], vec![0], vec![]]
    );
    observed.admitted[2] = true;
    assert!(mode
        .retry(std::slice::from_ref(&observed), 1)
        .iter()
        .all(Vec::is_empty));
    assert!(!observed.complete());
    observed.finalized_us[3] = Some(24);
    assert!(observed.complete());
    assert_eq!(
        observed.admitted,
        [false, false, true, false],
        "gossip is not HTTP admission"
    );
    assert_eq!(
        SubmissionMode::Fanout.retry(std::slice::from_ref(&observed), 1),
        [vec![0], vec![0], vec![], vec![0]]
    );
}

#[test]
fn load_early_reuse_requires_real_counter_but_keeps_frozen_legacy_comparison_runnable() {
    let old = vec![json!({}); 4];
    assert_eq!(
        early_reuse_evidence(&old, false).unwrap()["legacy_binary_without_counters_skipped"],
        true
    );
    assert!(early_reuse_evidence(&old, true).is_err());
    let mut statuses = vec![json!({"early_bind_reused":0}); 4];
    assert!(early_reuse_evidence(&statuses, false).is_err());
    statuses[2]["early_bind_reused"] = json!(3);
    assert_eq!(early_reuse_evidence(&statuses, true).unwrap()["total"], 3);
    statuses[1] = json!({});
    assert!(early_reuse_evidence(&statuses, false).is_err());
}
