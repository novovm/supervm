//! Actual AOEM storage across independent processes. These are immutable local
//! candidates, NOT canonical blocks, finality, a business proof, or a TPS test.
//! The manifest records byte-for-byte expectations before the writer exits;
//! independent arithmetic below additionally checks money, fees and nonce.

use anyhow::{bail, ensure, Context, Result};
use ed25519_dalek::{Signer, SigningKey};
use novovm_aoem::{ComputeSession, StorageConfig, StorageSession, StorageWrite};
use novovm_host::business::direct_nov_fee::{DirectNovFeePolicy, FeeState};
use novovm_host::business::nov_transfer_batch::{
    balance_key, effect_contract, fee_record_changes, nonce_key, program_id, receipt_codec,
    ExecutedNovBatch, NovTransferPlan, SEMANTIC_VERSION,
};
use novovm_host::business::quoted_transfer::{Account, TransferError, TransferFailure};
use novovm_host::execution::plan::{BatchContext, PlanBudget};
use novovm_host::ingress::batch::{authenticate_batch, AuthenticationBudget};
use novovm_host::ingress::wire::{encode_transfer_v3, signing_message, FeePolicy, TransferV3};
use novovm_host::persistence::io::{IoBudget, IoService, IoTicket};
use novovm_host::persistence::{
    CandidateStore, OpenMode, PacketBudget, PersistedCandidate, PreparedCandidate, StorageDomain,
    StoreConfig, StoredCandidate,
};
use novovm_host::state::frontier::CaptureBudget;
use novovm_host::state::tree::{
    empty_root, read_state_value, stage_state_update, NodeHash, StagedStateUpdate, StateChange,
    StateNodeReader,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TEST: &str = "real_aoem_candidates_survive_process_exit_and_reject_corruption";
const PHASE: &str = "NOVOVM_DURABLE_CANDIDATE_TEST_PHASE";
const DIRECTORY: &str = "NOVOVM_DURABLE_CANDIDATE_TEST_DIRECTORY";
const CHAIN: u64 = 191;
const TIME: u64 = 172_800_123;
const GENESIS: NodeHash = [0x11; 32];
const PROTOCOL: NodeHash = [0x22; 32];
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
        .context("explicit real NOVOVM_AOEM_TEST_LIBRARY is required")?
        .into())
}

fn config(directory: &Path) -> Result<StoreConfig> {
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

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15_000,
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
    let key = SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes();
    let mut hash = Sha256::new();
    hash.update(b"novovm-native-auth-nonce-identity-v1");
    hash.update(CHAIN.to_be_bytes());
    hash.update(b"novovm-native-auth/ed25519-public-key/v2\0");
    hash.update(key);
    hash.finalize().into()
}

fn transaction(seed: u8, recipient: u8, amount: u128, nonce: u64) -> TransferV3 {
    let mut transaction = TransferV3 {
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
    let signature = signer.sign(&signing_message(&transaction).unwrap());
    transaction.signature = signer.verifying_key().to_bytes().to_vec();
    transaction
        .signature
        .extend_from_slice(&signature.to_bytes());
    transaction
}

fn transactions(child: bool) -> Vec<TransferV3> {
    if child {
        vec![
            transaction(2, 4, 20, 0),
            transaction(1, 3, 200, 1),
            transaction(3, 4, 2_000_000, 1),
        ]
    } else {
        vec![transaction(1, 2, 100, 0), transaction(3, 2, 50, 0)]
    }
}

fn raw_transactions(child: bool) -> Result<Vec<Vec<u8>>> {
    transactions(child).iter().map(encode_transfer_v3).collect()
}

fn base_fee(transaction: &TransferV3) -> u128 {
    // Independent old canonical Transfer JSON length formula, not a call to
    // quote_transfer, quote_and_settle, compute_outcome or the batch reducer.
    let to: String = transaction
        .to
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let json = format!(
        "{{\"asset\":\"NOV\",\"to\":\"0x{to}\",\"amount\":\"{}\"}}",
        transaction.amount
    );
    40 + (json.len().div_ceil(16) as u128).min(64)
}

fn initial_state() -> Result<StagedStateUpdate> {
    let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
    for seed in [1, 3] {
        changes.push(StateChange::Put {
            key: balance_key(&account(seed)),
            value: 1_000_000u128.to_le_bytes().to_vec(),
        });
    }
    stage_state_update(&Memory::default(), empty_root(), &changes)
}

fn context(parent: NodeHash, child: bool) -> BatchContext {
    BatchContext {
        chain_id: CHAIN,
        genesis_config_commitment: GENESIS,
        protocol_commitment: PROTOCOL,
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(&policy()).unwrap(),
        // Synthetic claimed block/receipt pins: this fixture has no canonical
        // block or cumulative receipt ledger. Only state ancestry is real here.
        parent_block_hash: if child { [0x33; 32] } else { [0; 32] },
        parent_height: u64::from(child),
        parent_state_root: parent,
        parent_receipt_root: empty_root(),
        parent_state_version: if child { 2 } else { 0 },
        receipt_codec: receipt_codec(),
        height: if child { 2 } else { 1 },
        slot: u64::from(child),
        timestamp_unix_ms: TIME + u64::from(child),
    }
}

fn execute(store: &CandidateStore, parent: NodeHash, child: bool) -> Result<ExecutedNovBatch> {
    let mut session = ComputeSession::open(&library()?, 4)?;
    let batch = authenticate_batch(
        &mut session,
        CHAIN,
        raw_transactions(child)?,
        AuthenticationBudget {
            transactions: 8,
            transaction_bytes: 1024,
            body_bytes: 8192,
        },
        TIMEOUT,
    )?;
    let plan = NovTransferPlan::compile(
        batch,
        context(parent, child),
        policy(),
        PlanBudget {
            transactions: 8,
            transaction_bytes: 1024,
            body_bytes: 8192,
            access_keys: 128,
        },
    )?;
    let input = plan.capture(
        store,
        CaptureBudget {
            keys: 128,
            nodes: 4096,
            bytes: 2 * 1024 * 1024,
        },
    )?;
    // The source is an actual AOEM database reader. The detached input is the
    // only thing supplied to computation; no storage handle enters a callback.
    input.execute(&mut session, TIMEOUT)
}

#[derive(Serialize, Deserialize)]
struct ExpectedCandidate {
    id: NodeHash,
    state_root: NodeHash,
    parent_root: NodeHash,
    receipt_commitment: NodeHash,
    statement: NodeHash,
    document_digest: NodeHash,
    receipts: Vec<Vec<u8>>,
    fees: FeeState,
    records: Vec<(Vec<u8>, Vec<u8>)>,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    writer_pid: u32,
    genesis_root: NodeHash,
    parent: ExpectedCandidate,
    child: ExpectedCandidate,
    document_fault_key: Vec<u8>,
    document_original: Vec<u8>,
    node_fault_key: Vec<u8>,
    node_original: Vec<u8>,
}

fn prepare(output: ExecutedNovBatch) -> Result<(PreparedCandidate, ExpectedCandidate)> {
    let receipts = output
        .receipts()
        .iter()
        .map(postcard::to_allocvec)
        .collect::<Result<Vec<_>, _>>()?;
    let fees = output.fees().clone();
    let packet = PreparedCandidate::from_executed(output, PacketBudget::default())?;
    let expected = ExpectedCandidate {
        id: packet.candidate_id(),
        state_root: packet.state_root(),
        parent_root: packet.parent_state_root(),
        receipt_commitment: packet.receipt_batch_commitment(),
        statement: packet.statement_commitment(),
        document_digest: packet.document_digest(),
        receipts,
        fees,
        records: packet
            .records()
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    };
    Ok((packet, expected))
}

fn write_manifest(directory: &Path, manifest: &Manifest) -> Result<()> {
    let bytes = postcard::to_allocvec(manifest)?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.join("expected.postcard"))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn manifest(directory: &Path) -> Result<Manifest> {
    let bytes = fs::read(directory.join("expected.postcard"))?;
    ensure!(
        bytes.len() < 4 * 1024 * 1024,
        "test manifest unexpectedly large"
    );
    let (manifest, rest): (Manifest, &[u8]) = postcard::take_from_bytes(&bytes)?;
    ensure!(rest.is_empty(), "test manifest trailing bytes");
    ensure!(
        manifest.writer_pid != std::process::id(),
        "not an independent recovery process"
    );
    Ok(manifest)
}

fn admit_for_test(
    io: &IoService,
    packet: &Arc<PreparedCandidate>,
) -> Result<IoTicket<PersistedCandidate>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        ensure!(
            Instant::now() < deadline,
            "test I/O admission deadline exceeded"
        );
        // None is explicitly NOT accepted, including short permit-lock
        // contention. Errors are never retried, and an accepted ticket returns
        // immediately: this helper cannot re-submit an unknown/lost reply.
        if let Some(ticket) = io.try_persist(Arc::clone(packet))? {
            return Ok(ticket);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn write_phase(directory: &Path) -> Result<()> {
    let mut missing = config(directory)?;
    missing.database = directory.join("must-not-be-created");
    ensure!(
        CandidateStore::open(missing.clone(), OpenMode::Existing).is_err(),
        "Existing accepted a missing database"
    );
    ensure!(
        !missing.database.exists(),
        "Existing created a missing database"
    );
    let store = CandidateStore::open(config(directory)?, OpenMode::CreateNew)?;
    ensure!(
        CandidateStore::open(config(directory)?, OpenMode::CreateNew).is_err(),
        "CreateNew accepted an existing database"
    );
    let initial = initial_state()?;
    store.install_unpublished_state(&initial)?;
    let parent_output = execute(&store, initial.root(), false)?;
    ensure!(
        parent_output
            .receipts()
            .iter()
            .all(|receipt| receipt.failure.is_none()),
        "parent fixture failed"
    );
    let (parent_packet, parent) = prepare(parent_output)?;
    ensure!(
        !store.persist(&parent_packet)?.already_present,
        "first parent persist not new"
    );
    ensure!(
        parent_packet.matches(
            &store
                .recover(parent.id)?
                .context("persisted parent missing")?
        ),
        "parent readback mismatch"
    );
    let child_output = execute(&store, parent.state_root, true)?;
    ensure!(
        child_output.receipts()[..2]
            .iter()
            .all(|receipt| receipt.failure.is_none()),
        "child successes failed"
    );
    ensure!(
        matches!(
            child_output.receipts()[2].failure,
            Some(TransferFailure::Business(
                TransferError::InsufficientFunds { .. }
            ))
        ),
        "child must charge a failed transfer"
    );
    let (child_packet, child) = prepare(child_output)?;
    let child_packet = Arc::new(child_packet);
    drop(store);
    let io = IoService::start(config(directory)?, OpenMode::Existing, IoBudget::default())?;
    let first = admit_for_test(&io, &child_packet)?;
    // Lose the VERY FIRST reply, not only an already-durable duplicate reply.
    // The later same-owner replay must report already_present=true, proving
    // that dropping the first ticket did not cancel its accepted new write.
    drop(first);
    let replay = admit_for_test(&io, &child_packet)?;
    ensure!(
        replay.wait()?.already_present,
        "dropping first ticket cancelled accepted persistence"
    );
    let final_reply = admit_for_test(&io, &child_packet)?;
    drop(final_reply);
    io.shutdown()?;
    let store = CandidateStore::open(config(directory)?, OpenMode::Existing)?;
    // Exact idempotent replay remains valid after the explicit I/O-owner drain.
    ensure!(
        store.persist(&child_packet)?.already_present,
        "exact second persist not idempotent"
    );
    ensure!(
        child_packet.matches(
            &store
                .recover(child.id)?
                .context("persisted child missing")?
        ),
        "child readback mismatch"
    );
    let (doc_relative, document_original) = child
        .records
        .iter()
        .find(|(key, _)| key.first() == Some(&b'd'))
        .context("child document missing")?;
    let node_relative = [b"n".as_slice(), &child.state_root].concat();
    ensure!(
        !parent.records.iter().any(|(key, _)| key == &node_relative),
        "fault must be child-only"
    );
    let node_original = child_packet
        .records()
        .get(&node_relative)
        .context("new child root node absent")?
        .clone();
    let manifest = Manifest {
        writer_pid: std::process::id(),
        genesis_root: initial.root(),
        document_fault_key: store.scoped_key(doc_relative),
        document_original: document_original.clone(),
        node_fault_key: store.scoped_key(&node_relative),
        node_original,
        parent,
        child,
    };
    check_economics(&store, &manifest.parent, false)?;
    check_economics(&store, &manifest.child, true)?;
    write_manifest(directory, &manifest)?;
    // Deliberately bypass Rust drops after acknowledged synchronous batches.
    // This is process-exit recovery, not power-loss or an in-flight kill test.
    println!("writer complete: acknowledged AOEM atomic batches; exiting without Rust destructors");
    std::io::stdout().flush()?;
    std::process::exit(0);
}

fn check_candidate(
    stored: &StoredCandidate,
    expected: &ExpectedCandidate,
    child: bool,
) -> Result<()> {
    ensure!(
        stored.candidate_id() == expected.id,
        "candidate id mismatch"
    );
    ensure!(stored.plan_commitment() == expected.id, "plan id mismatch");
    ensure!(
        *stored.context() == context(expected.parent_root, child),
        "candidate context changed"
    );
    ensure!(
        stored.parent_state_root() == expected.parent_root,
        "parent root changed"
    );
    ensure!(
        stored.state_root() == expected.state_root,
        "state root changed"
    );
    ensure!(
        stored.raw_transactions() == raw_transactions(child)?,
        "signed bytes/order changed"
    );
    ensure!(
        stored.receipt_bytes() == expected.receipts,
        "complete receipt bytes changed"
    );
    ensure!(
        stored.receipt_batch_commitment() == expected.receipt_commitment,
        "receipt commitment changed"
    );
    ensure!(
        stored.statement_commitment() == expected.statement,
        "statement changed"
    );
    ensure!(
        stored.document_digest() == expected.document_digest,
        "document changed"
    );
    for (hash, bytes) in stored.nodes() {
        let key = [b"n".as_slice(), hash].concat();
        ensure!(
            expected
                .records
                .iter()
                .any(|(original_key, original)| original_key == &key && original == bytes),
            "recovered node bytes differ from executed packet"
        );
    }
    Ok(())
}

fn check_economics(
    store: &CandidateStore,
    candidate: &ExpectedCandidate,
    child: bool,
) -> Result<()> {
    let parent_txs = transactions(false);
    let child_txs = transactions(true);
    let parent_fees: Vec<_> = parent_txs.iter().map(base_fee).collect();
    let child_fees: Vec<_> = child_txs.iter().map(base_fee).collect();
    let a = 1_000_000 - 100 - parent_fees[0];
    let c = 1_000_000 - 50 - parent_fees[1];
    let balances = if child {
        [
            (1, a - 200 - child_fees[1]),
            (2, 150 - 20 - child_fees[0]),
            (3, c + 200 - child_fees[2]),
            (4, 20),
        ]
    } else {
        [(1, a), (2, 150), (3, c), (4, 0)]
    };
    let mut fees = parent_fees;
    if child {
        fees.extend(child_fees);
    }
    let total: u128 = fees.iter().sum();
    let reserve: u128 = fees.iter().map(|fee| fee * 7000 / 10000).sum();
    let fee_bucket: u128 = fees.iter().map(|fee| fee * 2000 / 10000).sum();
    let state = &candidate.fees;
    let accounting = &state.accounting;
    ensure!(
        accounting.treasury_reserve_nov == Some(total)
            && accounting.settled_nov_total == total
            && accounting.settled_by_asset_nov == Some(total),
        "fee totals mismatch"
    );
    ensure!(
        accounting.reserve_bucket_nov == reserve
            && accounting.fee_bucket_nov == fee_bucket
            && accounting.risk_buffer_nov == total - reserve - fee_bucket,
        "per-transaction fee buckets mismatch"
    );
    ensure!(
        accounting.settlements == fees.len() as u64
            && accounting.journal_next_seq == fees.len() as u64,
        "failed business transaction lost settlement/journal sequence"
    );
    ensure!(
        accounting.daily_window_day == 2 && accounting.daily_nov_used == 0,
        "direct NOV changed daily usage"
    );
    ensure!(
        state.diagnostics.quote_max_pay_exceeded == 0
            && state.diagnostics.settlement_policy_fallback == 0
            && state.diagnostics.settlement_paused == 0
            && state.diagnostics.settlement_amount_overflow == 0
            && state.diagnostics.clearing_quote_expired == 0
            && state.diagnostics.clearing_insufficient_user_balance == 0,
        "unexpected fee rejection"
    );
    ensure!(
        state.diagnostics.last_quote_failure.is_none()
            && state.diagnostics.last_clearing_failure.is_none(),
        "unexpected retained fee failure"
    );
    let mut expected_changes = fee_record_changes(&policy(), state)?;
    let mut sum = 0u128;
    for (seed, balance) in balances {
        let expected = if seed == 4 && !child {
            None
        } else {
            Some(balance.to_le_bytes().to_vec())
        };
        ensure!(
            read_state_value(store, candidate.state_root, &balance_key(&account(seed)))?
                == expected,
            "independent balance expectation failed for seed {seed}"
        );
        sum = sum.checked_add(balance).context("balance sum overflow")?;
        if let Some(value) = expected {
            expected_changes.push(StateChange::Put {
                key: balance_key(&account(seed)),
                value,
            });
        }
    }
    ensure!(
        sum.checked_add(total) == Some(2_000_000),
        "supply/fee conservation failed"
    );
    for seed in 1..=4 {
        let expected_nonce = match (seed, child) {
            (1 | 3, false) => Some(1u64),
            (1 | 3, true) => Some(2),
            (2, true) => Some(1),
            _ => None,
        };
        let expected = expected_nonce.map(|nonce| nonce.to_le_bytes().to_vec());
        ensure!(
            read_state_value(store, candidate.state_root, &nonce_key(&identity(seed)))? == expected,
            "failed business nonce or nonce absence differs"
        );
        if let Some(value) = expected {
            expected_changes.push(StateChange::Put {
                key: nonce_key(&identity(seed)),
                value,
            });
        }
    }
    // Full root reconstruction here is a small TEST oracle, never recovery's
    // implementation. It detects an unintended extra account/record in output.
    let rebuilt = stage_state_update(&Memory::default(), empty_root(), &expected_changes)?;
    ensure!(
        rebuilt.root() == candidate.state_root,
        "unexpected extra/missing state records"
    );
    for change in fee_record_changes(&policy(), state)? {
        match change {
            StateChange::Put { key, value } => ensure!(
                read_state_value(store, candidate.state_root, &key)? == Some(value),
                "fee page mismatch"
            ),
            StateChange::Delete { key } => ensure!(
                read_state_value(store, candidate.state_root, &key)?.is_none(),
                "hidden fee tail page"
            ),
        }
    }
    Ok(())
}

fn read_phase(directory: &Path) -> Result<()> {
    let expected = manifest(directory)?;
    for field in ["chain", "genesis", "protocol"] {
        let mut wrong = config(directory)?;
        match field {
            "chain" => wrong.domain.chain_id += 1,
            "genesis" => wrong.domain.genesis_config_commitment[0] ^= 1,
            _ => wrong.domain.protocol_commitment[0] ^= 1,
        }
        match CandidateStore::open(wrong, OpenMode::Existing) {
            Ok(_) => bail!("foreign {field} database accepted"),
            Err(error) => ensure!(
                format!("{error:#}").contains("format/domain"),
                "wrong-domain failure was not a domain check: {error:#}"
            ),
        }
    }
    let store = CandidateStore::open(config(directory)?, OpenMode::Existing)?;
    let parent = store
        .recover(expected.parent.id)?
        .context("parent lost on restart")?;
    let child = store
        .recover(expected.child.id)?
        .context("child lost on restart")?;
    check_candidate(&parent, &expected.parent, false)?;
    check_candidate(&child, &expected.child, true)?;
    ensure!(
        child.parent_state_root() == parent.state_root(),
        "durable ancestry mismatch"
    );
    check_economics(&store, &expected.parent, false)?;
    check_economics(&store, &expected.child, true)?;
    ensure!(
        initial_state()?.root() == expected.genesis_root,
        "seed root changed"
    );
    for seed in [1, 3] {
        ensure!(
            read_state_value(&store, expected.genesis_root, &balance_key(&account(seed)))?
                == Some(1_000_000u128.to_le_bytes().to_vec()),
            "original seed no longer readable"
        );
    }
    ensure!(
        store.recover([0xee; 32])?.is_none(),
        "unknown candidate falsely recovered"
    );
    println!("independent process: exact signed body/receipts/roots/fee pages/nonce and historical parent PASS");
    Ok(())
}

fn corrupt_document(directory: &Path) -> Result<()> {
    let expected = manifest(directory)?;
    let mut raw = StorageSession::open(
        &library()?,
        &directory.join("provider.rocksdb"),
        StorageConfig::default(),
    )?;
    ensure!(
        raw.get(&expected.document_fault_key)? == Some(expected.document_original.clone()),
        "fault target differs"
    );
    let mut bad = expected.document_original;
    *bad.last_mut().context("empty candidate document")? ^= 1;
    raw.atomic_write_batch(&[StorageWrite::Put {
        key: expected.document_fault_key,
        value: bad,
    }])
}

fn reject_phase(directory: &Path, document: bool) -> Result<()> {
    let expected = manifest(directory)?;
    {
        let store = CandidateStore::open(config(directory)?, OpenMode::Existing)?;
        ensure!(
            store.recover(expected.child.id).is_err(),
            "completed corrupt candidate was accepted"
        );
        let parent = store
            .recover(expected.parent.id)?
            .context("unrelated parent became unavailable")?;
        check_candidate(&parent, &expected.parent, false)?;
        check_economics(&store, &expected.parent, false)?;
        if !document {
            // Deliberate test caller replay, NOT automatic recovery/re-execution:
            // rebuild the same private packet via genuine AOEM computation from
            // the still-valid parent, then demand rejection rather than repair.
            let (packet, recomputed) = prepare(execute(&store, expected.parent.state_root, true)?)?;
            ensure!(
                recomputed.id == expected.child.id
                    && recomputed.statement == expected.child.statement
                    && recomputed.receipts == expected.child.receipts,
                "explicit replay did not produce the original exact candidate"
            );
            let rejected = store.persist(&packet);
            ensure!(
                rejected.is_err(),
                "completed content was silently repaired on re-persist"
            );
            ensure!(
                store.recover(expected.child.id).is_err(),
                "re-persist restored damaged candidate"
            );
        }
    }
    let mut raw = StorageSession::open(
        &library()?,
        &directory.join("provider.rocksdb"),
        StorageConfig::default(),
    )?;
    if document {
        let mut bad = expected.document_original.clone();
        *bad.last_mut().unwrap() ^= 1;
        ensure!(
            raw.get(&expected.document_fault_key)? == Some(bad),
            "recovery repaired tampered document"
        );
        // Explicit TEST-only restoration after the rejection/no-repair check,
        // then a different fault. Production recovery never performs repair.
        ensure!(
            raw.get(&expected.node_fault_key)? == Some(expected.node_original),
            "node fault target differs"
        );
        raw.atomic_write_batch(&[
            StorageWrite::Put {
                key: expected.document_fault_key,
                value: expected.document_original,
            },
            StorageWrite::Delete {
                key: expected.node_fault_key,
            },
        ])?;
    } else {
        ensure!(
            raw.get(&expected.node_fault_key)?.is_none(),
            "recovery repaired missing completed node"
        );
        ensure!(
            raw.get(&expected.document_fault_key)? == Some(expected.document_original),
            "recovery changed restored document"
        );
    }
    println!(
        "independent process: corrupt completed {} refused without repair; parent still readable",
        if document { "document" } else { "node" }
    );
    Ok(())
}

fn run_phase(directory: &Path, phase: &str) -> Result<()> {
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PHASE, phase)
        .env(DIRECTORY, directory)
        .output()?;
    ensure!(
        output.status.success(),
        "phase {phase} failed: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

#[test]
#[ignore = "requires explicit trusted AOEM library; actual isolated process persistence, not finality"]
fn real_aoem_candidates_survive_process_exit_and_reject_corruption() -> Result<()> {
    let _ = library()?; // No missing-library skip, including --include-ignored.
    if let Some(phase) = std::env::var_os(PHASE) {
        let directory =
            PathBuf::from(std::env::var_os(DIRECTORY).context("child test directory missing")?);
        return match phase.to_str().context("invalid test phase")? {
            "write" => write_phase(&directory),
            "read" => read_phase(&directory),
            "corrupt_document" => corrupt_document(&directory),
            "reject_document_then_corrupt_node" => reject_phase(&directory, true),
            "reject_node" => reject_phase(&directory, false),
            other => bail!("unknown test phase {other}"),
        };
    }
    ensure!(
        std::env::var_os(DIRECTORY).is_none(),
        "unpaired test-directory environment"
    );
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/durable-candidate-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    for phase in [
        "write",
        "read",
        "corrupt_document",
        "reject_document_then_corrupt_node",
        "reject_node",
    ] {
        run_phase(&directory, phase)?;
    }
    println!(
        "real AOEM candidate process/fault evidence retained: {}",
        directory.display()
    );
    Ok(())
}
