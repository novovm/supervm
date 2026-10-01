//! Real signed batches through the resident replacement pipeline. The three
//! candidates intentionally share one trusted fixture parent: they are isolated
//! siblings, NOT a linear canonical/finalized chain or a throughput benchmark.
//! No sleeps/barriers manufacture callback concurrency in this integration.

use anyhow::{bail, ensure, Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use novovm_aoem::{ComputeSession, StorageConfig};
use novovm_host::business::direct_nov_fee::{DirectNovFeePolicy, FeeState};
use novovm_host::business::nov_transfer_batch::{
    balance_key, effect_contract, fee_record_changes, nonce_key, program_id, receipt_codec,
    ExecutedNovBatch, NovTransferPlan, SEMANTIC_VERSION,
};
use novovm_host::business::quoted_transfer::Account;
use novovm_host::execution::plan::{BatchContext, PlanBudget};
use novovm_host::ingress::batch::{authenticate_batch, AuthenticationBudget};
use novovm_host::ingress::wire::{encode_transfer_v3, signing_message, FeePolicy, TransferV3};
use novovm_host::persistence::io::IoBudget;
use novovm_host::persistence::{
    CandidateStore, OpenMode, PacketBudget, PreparedCandidate, StorageDomain, StoreConfig,
};
use novovm_host::pipeline::{
    BatchRequest, CandidatePipeline, DurableBatch, PipelineConfig, PipelineTicket, Submission,
};
use novovm_host::state::frontier::CaptureBudget;
use novovm_host::state::tree::{
    empty_root, read_state_value, stage_state_update, NodeHash, StateChange, StateNodeReader,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TEST: &str = "real_resident_signed_pipeline_preserves_isolation_and_recovers";
const PHASE: &str = "NOVOVM_PIPELINE_TEST_PHASE";
const DIRECTORY: &str = "NOVOVM_PIPELINE_TEST_DIRECTORY";
const CHAIN: u64 = 291;
const GENESIS: NodeHash = [0x61; 32];
const PROTOCOL: NodeHash = [0x62; 32];
const TIME: u64 = 172_800_500;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn library() -> Result<PathBuf> {
    Ok(std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit real NOVOVM_AOEM_TEST_LIBRARY required")?
        .into())
}

fn store_config(directory: &Path) -> Result<StoreConfig> {
    Ok(StoreConfig {
        library: library()?,
        database: directory.join("provider.rocksdb"),
        domain: StorageDomain {
            chain_id: CHAIN,
            genesis_config_commitment: GENESIS,
            protocol_commitment: PROTOCOL,
        },
        storage: StorageConfig::default(),
        packet_budget: PacketBudget::default(),
    })
}

fn authentication_budget() -> AuthenticationBudget {
    AuthenticationBudget {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 8192,
    }
}
fn plan_budget() -> PlanBudget {
    PlanBudget {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 8192,
        access_keys: 128,
    }
}
fn capture_budget() -> CaptureBudget {
    CaptureBudget {
        keys: 128,
        nodes: 4096,
        bytes: 2 * 1024 * 1024,
    }
}
fn pipeline_config(directory: &Path) -> Result<PipelineConfig> {
    Ok(PipelineConfig {
        store: store_config(directory)?,
        workers: 4,
        max_batches: 8,
        max_retained_bytes: 512 * 1024 * 1024,
        authentication: authentication_budget(),
        plan: plan_budget(),
        capture: capture_budget(),
        compute_timeout: TIMEOUT,
        io: IoBudget {
            requests: 1,
            ..IoBudget::default()
        },
        capture_edge_quantum: 64,
    })
}

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn account(seed: u8) -> Account {
    let key = SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes();
    Account::try_from(Sha256::digest(key)[12..].to_vec()).unwrap()
}
fn identity(seed: u8) -> NodeHash {
    let mut hash = Sha256::new();
    hash.update(b"novovm-native-auth-nonce-identity-v1");
    hash.update(CHAIN.to_be_bytes());
    hash.update(b"novovm-native-auth/ed25519-public-key/v2\0");
    hash.update(
        SigningKey::from_bytes(&[seed; 32])
            .verifying_key()
            .to_bytes(),
    );
    hash.finalize().into()
}
fn transaction(seed: u8, recipient: u8, amount: u128, nonce: u64) -> TransferV3 {
    let mut tx = TransferV3 {
        chain_id: CHAIN,
        from: account(seed).as_bytes().to_vec(),
        to: account(recipient).as_bytes().to_vec(),
        asset: "NOV".into(),
        amount,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 0,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    };
    let signer = SigningKey::from_bytes(&[seed; 32]);
    let signature = signer.sign(&signing_message(&tx).unwrap());
    tx.signature = signer.verifying_key().to_bytes().to_vec();
    tx.signature.extend_from_slice(&signature.to_bytes());
    tx
}
fn transactions(variant: usize) -> Vec<TransferV3> {
    match variant {
        0 => vec![transaction(1, 2, 100, 0), transaction(3, 2, 50, 0)],
        1 => vec![transaction(1, 2, 100, 0), transaction(2, 3, 20, 0)],
        2 => vec![transaction(3, 4, 2_000_000, 0), transaction(1, 4, 1, 0)],
        _ => panic!("unexpected fixture variant"),
    }
}
fn raw(variant: usize) -> Result<Vec<Vec<u8>>> {
    transactions(variant)
        .iter()
        .map(encode_transfer_v3)
        .collect()
}
fn context(root: NodeHash, slot: u64) -> BatchContext {
    BatchContext {
        chain_id: CHAIN,
        genesis_config_commitment: GENESIS,
        protocol_commitment: PROTOCOL,
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(&policy()).unwrap(),
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: root,
        parent_receipt_root: empty_root(),
        parent_state_version: 0,
        receipt_codec: receipt_codec(),
        height: 1,
        slot,
        timestamp_unix_ms: TIME + slot,
    }
}
fn request(root: NodeHash, variant: usize) -> Result<BatchRequest> {
    BatchRequest::new(raw(variant)?, context(root, variant as u64), policy())
}

fn initialize(directory: &Path) -> Result<(Memory, NodeHash)> {
    let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
    for seed in [1, 3] {
        changes.push(StateChange::Put {
            key: balance_key(&account(seed)),
            value: 1_000_000u128.to_le_bytes().to_vec(),
        });
    }
    let update = stage_state_update(&Memory::default(), empty_root(), &changes)?;
    let store = CandidateStore::open(store_config(directory)?, OpenMode::CreateNew)?;
    store.install_unpublished_state(&update)?;
    Ok((Memory(update.nodes().clone()), update.root()))
}

#[derive(Serialize, Deserialize)]
struct Expected {
    id: NodeHash,
    root: NodeHash,
    statement: NodeHash,
    receipt_commitment: NodeHash,
    document_digest: NodeHash,
    receipt_bytes: Vec<Vec<u8>>,
    fees: FeeState,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    writer: u32,
    parent: NodeHash,
    rejected_nonce_id: NodeHash,
    candidates: Vec<Expected>,
}

fn direct_reference(
    session: &mut ComputeSession,
    memory: &Memory,
    root: NodeHash,
    variant: usize,
) -> Result<(PreparedCandidate, Expected)> {
    // Independent scheduling baseline, using the SAME already-qualified business
    // semantics. This is not called by the resident pipeline or recovery path.
    let authenticated = authenticate_batch(
        session,
        CHAIN,
        raw(variant)?,
        authentication_budget(),
        TIMEOUT,
    )?;
    let plan = NovTransferPlan::compile(
        authenticated,
        context(root, variant as u64),
        policy(),
        plan_budget(),
    )?;
    let output = plan
        .capture(memory, capture_budget())?
        .execute(session, TIMEOUT)?;
    check_execution_money(&output, variant)?;
    let fees = output.fees().clone();
    let receipt_bytes = output
        .receipts()
        .iter()
        .map(postcard::to_allocvec)
        .collect::<Result<Vec<_>, _>>()?;
    let packet = PreparedCandidate::from_executed(output, PacketBudget::default())?;
    let expected = Expected {
        id: packet.candidate_id(),
        root: packet.state_root(),
        statement: packet.statement_commitment(),
        receipt_commitment: packet.receipt_batch_commitment(),
        document_digest: packet.document_digest(),
        receipt_bytes,
        fees,
    };
    Ok((packet, expected))
}

fn base_fee(tx: &TransferV3) -> u128 {
    let address: String = tx.to.iter().map(|byte| format!("{byte:02x}")).collect();
    let args = format!(
        "{{\"asset\":\"NOV\",\"to\":\"0x{address}\",\"amount\":\"{}\"}}",
        tx.amount
    );
    40 + (args.len().div_ceil(16) as u128).min(64)
}
fn expected_balances(variant: usize) -> [(u8, Option<u128>); 4] {
    let fees: Vec<_> = transactions(variant).iter().map(base_fee).collect();
    match variant {
        0 => [
            (1, Some(1_000_000 - 100 - fees[0])),
            (2, Some(150)),
            (3, Some(1_000_000 - 50 - fees[1])),
            (4, None),
        ],
        1 => [
            (1, Some(1_000_000 - 100 - fees[0])),
            (2, Some(100 - 20 - fees[1])),
            (3, Some(1_000_020)),
            (4, None),
        ],
        2 => [
            (1, Some(1_000_000 - 1 - fees[1])),
            (2, None),
            (3, Some(1_000_000 - fees[0])),
            (4, Some(1)),
        ],
        _ => panic!("unexpected fixture variant"),
    }
}
fn check_fees(fees: &FeeState, variant: usize) -> Result<()> {
    let amounts: Vec<_> = transactions(variant).iter().map(base_fee).collect();
    let total: u128 = amounts.iter().sum();
    let reserve: u128 = amounts.iter().map(|fee| fee * 7000 / 10000).sum();
    let fee_bucket: u128 = amounts.iter().map(|fee| fee * 2000 / 10000).sum();
    let a = &fees.accounting;
    ensure!(
        a.treasury_reserve_nov == Some(total)
            && a.settled_nov_total == total
            && a.settled_by_asset_nov == Some(total),
        "fee totals differ"
    );
    ensure!(
        a.reserve_bucket_nov == reserve
            && a.fee_bucket_nov == fee_bucket
            && a.risk_buffer_nov == total - reserve - fee_bucket,
        "per-tx fee split differs"
    );
    ensure!(
        a.settlements == 2
            && a.journal_next_seq == 2
            && a.daily_window_day == 2
            && a.daily_nov_used == 0,
        "business failure lost fee/sequence"
    );
    let balance_sum = expected_balances(variant)
        .iter()
        .try_fold(0u128, |sum, (_, amount)| {
            sum.checked_add(amount.unwrap_or(0))
        })
        .context("fixture sum overflow")?;
    ensure!(
        balance_sum.checked_add(total) == Some(2_000_000),
        "fixture conservation differs"
    );
    Ok(())
}
fn check_execution_money(output: &ExecutedNovBatch, variant: usize) -> Result<()> {
    check_fees(output.fees(), variant)?;
    ensure!(output.receipts().len() == 2, "receipt count differs");
    for (index, (receipt, tx)) in output
        .receipts()
        .iter()
        .zip(transactions(variant))
        .enumerate()
    {
        ensure!(
            receipt.delta.fee_funding_delta == base_fee(&tx),
            "paid fee differs"
        );
        ensure!(
            receipt.delta.nonce_before == 0 && receipt.delta.nonce_after == 1,
            "nonce transition differs"
        );
        ensure!(
            receipt.failure.is_some() == (variant == 2 && index == 0),
            "unexpected business status"
        );
        ensure!(
            receipt.fee_failure.is_none() && receipt.journal.is_some(),
            "fee/journal dropped"
        );
    }
    Ok(())
}

fn admit(pipeline: &CandidatePipeline, mut request: BatchRequest) -> Result<PipelineTicket> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        ensure!(
            Instant::now() < deadline,
            "bounded pipeline admission exhausted"
        );
        match pipeline.try_submit(request)? {
            Submission::Accepted(ticket) => return Ok(ticket),
            Submission::Backpressured(returned) => request = returned,
        }
        // Only explicitly unaccepted input is retried; no accepted/unknown
        // outcome is re-submitted by this helper.
        std::thread::yield_now();
    }
}
fn check_durable(done: &DurableBatch, reference: &PreparedCandidate) -> Result<()> {
    ensure!(
        done.packet.records() == reference.records(),
        "pipeline packet differs from direct execution"
    );
    ensure!(
        done.persisted.candidate_id == reference.candidate_id()
            && done.persisted.state_root == reference.state_root()
            && done.persisted.statement_commitment == reference.statement_commitment()
            && done.persisted.document_digest == reference.document_digest(),
        "persisted completion binding differs"
    );
    ensure!(
        done.observation.peak_callbacks >= 1,
        "no real business callback observed"
    );
    Ok(())
}

fn write_phase(directory: &Path) -> Result<()> {
    let (memory, root) = initialize(directory)?;
    let mut oracle_session = ComputeSession::open(&library()?, 4)?;
    let mut references = Vec::new();
    let mut expectations = Vec::new();
    for variant in 0..3 {
        let (packet, expected) = direct_reference(&mut oracle_session, &memory, root, variant)?;
        references.push(packet);
        expectations.push(expected);
    }
    let bad_nonce_raw = vec![encode_transfer_v3(&transaction(1, 2, 1, 1))?];
    let bad_nonce_auth = authenticate_batch(
        &mut oracle_session,
        CHAIN,
        bad_nonce_raw.clone(),
        authentication_budget(),
        TIMEOUT,
    )?;
    let bad_nonce_plan =
        NovTransferPlan::compile(bad_nonce_auth, context(root, 4), policy(), plan_budget())?;
    let rejected_nonce_id = bad_nonce_plan.commitment();
    ensure!(
        bad_nonce_plan.capture(&memory, capture_budget()).is_err(),
        "nonce fixture unexpectedly valid"
    );
    drop(oracle_session);
    let pipeline = CandidatePipeline::start(pipeline_config(directory)?, OpenMode::Existing)?;
    // Retain the only public-query permit, including its completed response,
    // until AFTER administrative drain. This must not take the internal capture
    // or persistence quota, and a ticket must not keep an I/O sender alive.
    let query_deadline = Instant::now() + Duration::from_secs(5);
    let held_query = loop {
        ensure!(
            Instant::now() < query_deadline,
            "query admission deadline exceeded"
        );
        if let Some(ticket) = pipeline.try_read_value(root, balance_key(&account(1)))? {
            break ticket;
        }
        std::thread::yield_now();
    };
    drop(admit(&pipeline, request(root, 0)?)?);
    let mut bad_signature = raw(0)?;
    *bad_signature[0].last_mut().unwrap() ^= 1;
    let bad_signature = admit(
        &pipeline,
        BatchRequest::new(bad_signature, context(root, 3), policy())?,
    )?;
    let good_one = admit(&pipeline, request(root, 1)?)?;
    let bad_nonce = admit(
        &pipeline,
        BatchRequest::new(bad_nonce_raw, context(root, 4), policy())?,
    )?;
    let good_two = admit(&pipeline, request(root, 2)?)?;
    let mut pending = [
        (Some(bad_signature), None),
        (Some(good_one), Some(1)),
        (Some(bad_nonce), None),
        (Some(good_two), Some(2)),
    ];
    let mut control_ticks = 0usize;
    let deadline = Instant::now() + Duration::from_secs(60);
    while pending.iter().any(|(ticket, _)| ticket.is_some()) {
        ensure!(
            Instant::now() < deadline,
            "pipeline completion deadline exceeded"
        );
        control_ticks += 1;
        ensure!(
            pipeline
                .try_read_value(root, balance_key(&account(1)))?
                .is_none(),
            "unconsumed public reply stopped accounting against query budget"
        );
        for (index, (ticket, expected)) in pending.iter_mut().enumerate() {
            let Some(current) = ticket.as_mut() else {
                continue;
            };
            match current.try_take() {
                Ok(None) => {}
                Ok(Some(done)) => {
                    let variant = expected.context("invalid signature/nonce was persisted")?;
                    check_durable(&done, &references[variant])?;
                    ensure!(
                        !done.persisted.already_present,
                        "fresh independent candidate already existed"
                    );
                    ticket.take();
                }
                Err(error) => {
                    ensure!(expected.is_none(), "valid batch failed: {error:#}");
                    let text = format!("{error:#}");
                    ensure!(
                        text.contains(if index == 0 { "signature" } else { "nonce" }),
                        "unexpected rejection cause: {text}"
                    );
                    ticket.take();
                }
            }
        }
        std::thread::yield_now();
    }
    // Both bad requests were ordinary rejections, and later real execution and
    // durable completion succeeded on the same resident pipeline/session/DB.
    ensure!(control_ticks > 0, "control loop did not progress");
    pipeline.shutdown()?;
    ensure!(
        held_query.wait()? == Some(1_000_000u128.to_le_bytes().to_vec()),
        "unconsumed query lost immutable parent across drain"
    );
    // Explicit administrative drain, NOT a retry after an unknown outcome.
    // The ticket-less first candidate must have completed, so rebuilding the
    // exact request in a new resident service is a known idempotent replay.
    let replay = CandidatePipeline::start(pipeline_config(directory)?, OpenMode::Existing)?;
    let done = admit(&replay, request(root, 0)?)?.wait()?;
    check_durable(&done, &references[0])?;
    ensure!(
        done.persisted.already_present,
        "dropping accepted pipeline ticket cancelled its work"
    );
    replay.shutdown()?;
    let manifest = Manifest {
        writer: std::process::id(),
        parent: root,
        rejected_nonce_id,
        candidates: expectations,
    };
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.join("expected.postcard"))?;
    file.write_all(&postcard::to_allocvec(&manifest)?)?;
    file.sync_all()?;
    println!("resident pipeline: 3 isolated siblings, signature+nonce rejection, unconsumed query quota/drain and dropped ticket PASS; not finalized/TPS");
    Ok(())
}

fn read_phase(directory: &Path) -> Result<()> {
    let bytes = fs::read(directory.join("expected.postcard"))?;
    ensure!(
        bytes.len() < 4 * 1024 * 1024,
        "fixture manifest unexpectedly large"
    );
    let (expected, rest): (Manifest, &[u8]) = postcard::take_from_bytes(&bytes)?;
    ensure!(
        rest.is_empty() && expected.writer != std::process::id(),
        "not independent recovery process"
    );
    let store = CandidateStore::open(store_config(directory)?, OpenMode::Existing)?;
    ensure!(expected.candidates.len() == 3, "missing expected candidate");
    for (variant, expected_candidate) in expected.candidates.iter().enumerate() {
        let stored = store
            .recover(expected_candidate.id)?
            .context("accepted candidate lost at restart")?;
        ensure!(
            *stored.context() == context(expected.parent, variant as u64),
            "candidate changed ancestry/domain"
        );
        ensure!(
            stored.raw_transactions() == raw(variant)?
                && stored.receipt_bytes() == expected_candidate.receipt_bytes,
            "body or complete receipt bytes changed"
        );
        ensure!(
            stored.state_root() == expected_candidate.root
                && stored.statement_commitment() == expected_candidate.statement
                && stored.receipt_batch_commitment() == expected_candidate.receipt_commitment
                && stored.document_digest() == expected_candidate.document_digest,
            "durable packet commitments differ"
        );
        check_fees(&expected_candidate.fees, variant)?;
        let mut changes = fee_record_changes(&policy(), &expected_candidate.fees)?;
        for (seed, amount) in expected_balances(variant) {
            let value = amount.map(|amount| amount.to_le_bytes().to_vec());
            ensure!(
                read_state_value(&store, stored.state_root(), &balance_key(&account(seed)))?
                    == value,
                "sibling balance contaminated"
            );
            if let Some(value) = value {
                changes.push(StateChange::Put {
                    key: balance_key(&account(seed)),
                    value,
                });
            }
        }
        for seed in 1..=4 {
            let touched = if variant == 1 {
                seed == 1 || seed == 2
            } else {
                seed == 1 || seed == 3
            };
            let value = touched.then(|| 1u64.to_le_bytes().to_vec());
            ensure!(
                read_state_value(&store, stored.state_root(), &nonce_key(&identity(seed)))?
                    == value,
                "candidate nonce/absence differs"
            );
            if let Some(value) = value {
                changes.push(StateChange::Put {
                    key: nonce_key(&identity(seed)),
                    value,
                });
            }
        }
        ensure!(
            stage_state_update(&Memory::default(), empty_root(), &changes)?.root()
                == stored.state_root(),
            "unexpected full-state difference"
        );
    }
    ensure!(
        store.recover(expected.rejected_nonce_id)?.is_none(),
        "rejected nonce candidate was persisted"
    );
    for seed in [1, 3] {
        ensure!(
            read_state_value(&store, expected.parent, &balance_key(&account(seed)))?
                == Some(1_000_000u128.to_le_bytes().to_vec()),
            "shared parent was mutated"
        );
        ensure!(
            read_state_value(&store, expected.parent, &nonce_key(&identity(seed)))?.is_none(),
            "shared parent nonce was consumed"
        );
    }
    println!("new process: all sibling raw/receipt/state/economic bindings recover; no canonical/finality claim");
    Ok(())
}

#[test]
#[ignore = "requires explicit real AOEM library; resident data pipeline, not a finalized chain"]
fn real_resident_signed_pipeline_preserves_isolation_and_recovers() -> Result<()> {
    let _ = library()?;
    if let Some(phase) = std::env::var_os(PHASE) {
        let directory =
            PathBuf::from(std::env::var_os(DIRECTORY).context("pipeline test directory required")?);
        return match phase.to_str().context("invalid pipeline test phase")? {
            "write" => write_phase(&directory),
            "read" => read_phase(&directory),
            other => bail!("unknown pipeline phase {other}"),
        };
    }
    ensure!(
        std::env::var_os(DIRECTORY).is_none(),
        "unpaired pipeline test environment"
    );
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/pipeline-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    for phase in ["write", "read"] {
        let output = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                TEST,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(PHASE, phase)
            .env(DIRECTORY, &directory)
            .output()?;
        ensure!(
            output.status.success(),
            "pipeline phase {phase} failed: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    println!(
        "actual pipeline restart artifacts retained: {}",
        directory.display()
    );
    Ok(())
}
