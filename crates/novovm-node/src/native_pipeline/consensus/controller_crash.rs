//! Kill the only holder of an undecided block, then recover four actual node
//! processes without any wallet/body reinjection. Not an electrical power-loss
//! or four-machine test. All files are isolated under the repository target.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::{
    quote_and_settle, quote_transfer, TransferFeeRequest,
};
use crate::native_pipeline::business::nov_transfer_batch::NovTransferReceipt;
use crate::native_pipeline::business::quoted_transfer::{
    compute_outcome, TransferIntent, TransferSnapshot,
};
use crate::native_pipeline::consensus::journal::ReplayEvidence;
use crate::native_pipeline::persistence::packet::marker_key;
use novovm_exec::resident::{StorageSession, StorageWrite};
use std::fmt::Write as _;

const CRASH_TEST: &str = "native_pipeline::consensus::tests::controller_integration::crash::real_undecided_process_kill_recovers_original_body_and_finalizes_without_wallet_reinjection";

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    pid: u32,
    validator: Hash,
    snapshot: Vec<u8>,
    outboxes: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
struct Finished {
    pid: u32,
    head: ParentPoint,
    executed_batches: u64,
    execution_failures: u64,
}

fn run(fixture: &Fixture, crash: bool) -> Result<()> {
    let mut services = Services(vec![CandidatePipeline::start(
        pipeline_config(&fixture.ledger)?,
        OpenMode::Existing,
    )?]);
    let pipeline = &services.0[0];
    let mut controller = open_controller(fixture, pipeline)?;
    let result = (|| -> Result<()> {
        let checkpoint = if crash {
            None
        } else {
            Some(serde_json::from_slice::<Checkpoint>(&fs::read(
                fixture.directory.join("kill-checkpoint.json"),
            )?)?)
        };
        let local =
            Validator::new(validator_key(fixture.index).verifying_key().to_bytes(), 1)?.id();
        let recovery_expected = checkpoint
            .as_ref()
            .is_some_and(|saved| saved.validator == local);
        ensure!(
            controller.is_recovering() == recovery_expected,
            "startup skipped or invented the cold-recovery gate"
        );
        let mut recovery_verified = !recovery_expected;
        if recovery_expected {
            // An empty probe is never accepted or executed; real wallet bytes
            // are not reinjected anywhere in the restart process.
            let probe = Arc::new(Message::Body {
                context: batch_context(fixture.root),
                raw_transactions: Vec::new(),
            });
            ensure!(
                !controller.try_submit_body(&probe)?,
                "recovery accepted external data work before reconciliation"
            );
        }
        let deadline = Instant::now() + RUN_BUDGET;
        let mut offered = false;
        let mut reported = false;
        let input = if crash {
            Some(Arc::new(Message::Body {
                context: batch_context(fixture.root),
                raw_transactions: raw_height(1)?,
            }))
        } else {
            None
        };
        loop {
            let now = Instant::now();
            ensure!(
                now < deadline,
                "crash/restart controller timed out; head={:?}, stats={:?}",
                controller.head(),
                controller.stats()
            );
            controller.poll(pipeline, now)?;
            if !recovery_verified && !controller.is_recovering() {
                let original = checkpoint.as_ref().unwrap();
                let saved = read_metadata(
                    pipeline,
                    vec![
                        MetaKey::ConsensusState(local),
                        MetaKey::ConsensusOutbox {
                            validator: local,
                            sequence: 3,
                        },
                        MetaKey::ChainHead,
                    ],
                )?;
                ensure!(
                    saved.values == vec![Some(original.snapshot.clone()), None, None]
                        && controller.stats().executed_batches == 1,
                    "replay changed safety revision or bypassed real reexecution"
                );
                fs::write(
                    fixture.directory.join("replay-read-only-verified"),
                    b"one actual reexecution; unchanged snapshot; no new outbox/head",
                )?;
                recovery_verified = true;
            }
            if crash && !offered {
                ensure!(
                    controller.is_local_leader()?,
                    "crash fixture started wrong leader"
                );
                offered = controller.try_submit_body(input.as_ref().unwrap())?;
            }
            let stats = controller.stats();
            if crash && stats.durable_votes == 1 && !controller.is_pending() {
                ensure!(
                    controller.round() == 0
                        && controller.step() == TimeoutStep::Prevote
                        && controller.head().is_none()
                        && stats.executed_batches == 1,
                    "not the exact undecided proposal/prevote checkpoint"
                );
                let validator =
                    Validator::new(validator_key(fixture.index).verifying_key().to_bytes(), 1)?
                        .id();
                let saved = read_metadata(
                    pipeline,
                    vec![
                        MetaKey::ConsensusState(validator),
                        MetaKey::ConsensusOutbox {
                            validator,
                            sequence: 1,
                        },
                        MetaKey::ConsensusOutbox {
                            validator,
                            sequence: 2,
                        },
                        MetaKey::ConsensusOutbox {
                            validator,
                            sequence: 3,
                        },
                        MetaKey::ChainHead,
                    ],
                )?;
                ensure!(
                    saved.values.len() == 5
                        && saved.values[3].is_none()
                        && saved.values[4].is_none(),
                    "checkpoint advanced beyond the two original messages"
                );
                let checkpoint = Checkpoint {
                    pid: std::process::id(),
                    validator,
                    snapshot: saved.values[0]
                        .clone()
                        .context("snapshot was not persisted")?,
                    outboxes: saved.values[1..3]
                        .iter()
                        .cloned()
                        .map(|v| v.context("original outbox missing"))
                        .collect::<Result<Vec<_>>>()?,
                };
                fs::write(
                    fixture.directory.join("kill-checkpoint.json"),
                    serde_json::to_vec(&checkpoint)?,
                )?;
                // Stop the controller AFTER actual readback, not at a guessed
                // sleep duration. Parent kills this process; no orderly drain.
                loop {
                    ensure!(
                        Instant::now() < deadline,
                        "parent did not kill checkpoint process"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            if !crash && !reported {
                if let Some(head) = controller.head() {
                    ensure!(
                        head.height == 1
                            && stats.executed_batches >= 1
                            && stats.execution_failures == 0,
                        "restart did not execute original block"
                    );
                    fs::write(
                        fixture
                            .directory
                            .join(format!("finished-{}.json", fixture.index)),
                        serde_json::to_vec(&Finished {
                            pid: std::process::id(),
                            head,
                            executed_batches: stats.executed_batches,
                            execution_failures: stats.execution_failures,
                        })?,
                    )?;
                    reported = true;
                }
            }
            if !crash && fixture.directory.join("stop").exists() {
                ensure!(
                    reported && recovery_verified,
                    "restart stopped before recovery/finality"
                );
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    })();
    let channel_shutdown = controller.shutdown();
    let pipeline_shutdown = services.shutdown();
    result?;
    channel_shutdown?;
    pipeline_shutdown
}

/// Pure serial oracle: final roots and every receipt byte, including all fee
/// buckets and next-nonce records. It does not install any execution results.
fn verify_once(block: &ArchiveBlock) -> Result<()> {
    ensure!(
        block.stored().raw_transactions() == raw_height(1)?,
        "restarted block changed signed originals"
    );
    let mut balances = BTreeMap::from([(account(1), 1_000_000u128), (account(3), 1_000_000u128)]);
    let mut fees = FeeState::default();
    let mut nonces = BTreeMap::new();
    let policy = policy();
    let now = u128::from(block.stored().context().timestamp_unix_ms);
    ensure!(
        block.stored().receipt_bytes().len() == 2,
        "restart lost or duplicated receipts"
    );
    for (raw, receipt_bytes) in block
        .stored()
        .raw_transactions()
        .iter()
        .zip(block.stored().receipt_bytes())
    {
        let authenticated = authenticate_transfer_v3(raw, CHAIN, 1024)?;
        let tx = authenticated.transfer();
        ensure!(
            tx.nonce == 0 && !nonces.contains_key(&authenticated.nonce_identity()),
            "oracle expected one transaction per signer"
        );
        let from = Account::try_from(tx.from.clone()).map_err(anyhow::Error::msg)?;
        let to = Account::try_from(tx.to.clone()).map_err(anyhow::Error::msg)?;
        let snapshot = TransferSnapshot {
            payer_balance: *balances.get(&from).context("unfunded crash fixture")?,
            recipient_balance: balances.get(&to).copied().unwrap_or(0),
            next_nonce: 0,
        };
        let request = TransferFeeRequest {
            tx_hash: authenticated.tx_hash(),
            payer: from.clone(),
            recipient: to.clone(),
            asset: tx.asset.clone(),
            amount: tx.amount,
            pay_asset: tx.fee_policy.pay_asset.clone(),
            max_pay_amount: tx.fee_policy.max_pay_amount,
            slippage_bps: tx.fee_policy.slippage_bps,
        };
        let quote = quote_transfer(&request, &policy, now)??;
        let mut identity = String::new();
        for byte in authenticated.nonce_identity() {
            write!(&mut identity, "{byte:02x}")?;
        }
        let outcome = compute_outcome(
            &TransferIntent {
                tx_hash: authenticated.tx_hash(),
                from: from.clone(),
                to: to.clone(),
                nonce_identity: identity,
                nonce: 0,
                amount: tx.amount,
                approved_fee: quote.nov_amount,
                fee_cap: quote.max_pay_amount,
            },
            &snapshot,
            None,
        )?;
        let settled = quote_and_settle(&request, &policy, &fees, snapshot.payer_balance, now)?;
        ensure!(
            outcome.is_success()
                && settled.failure.is_none()
                && outcome.delta().nonce_after == 1
                && outcome.delta().fee_funding_delta == settled.charged_fee(),
            "restart oracle business/fee/nonce mismatch"
        );
        let receipt = NovTransferReceipt {
            tx_hash: authenticated.tx_hash(),
            signer_identity: authenticated.nonce_identity(),
            delta: outcome.delta().clone(),
            failure: outcome.failure().cloned(),
            quote: settled.quote,
            fee_failure: settled.failure,
            journal: settled.journal,
            clear_clearing_candidates: settled.clear_clearing_candidates,
        };
        ensure!(
            postcard::to_allocvec(&receipt)? == *receipt_bytes,
            "restart changed exact business receipt"
        );
        balances.insert(from, outcome.delta().payer.after);
        balances.insert(to, outcome.delta().recipient.after);
        nonces.insert(authenticated.nonce_identity(), 1u64);
        fees = settled.after_fee_state;
    }
    let accounting = &fees.accounting;
    ensure!(
        balances
            .values()
            .sum::<u128>()
            .checked_add(accounting.settled_nov_total)
            == Some(2_000_000)
            && accounting.settlements == 2
            && accounting.journal_next_seq == 2
            && balances[&account(2)] == 150,
        "restart duplicated a fee/nonce/credit or violated conservation"
    );
    let mut changes = fee_record_changes(&policy, &fees)?;
    changes.extend(
        balances
            .into_iter()
            .map(|(account, value)| StateChange::Put {
                key: balance_key(&account),
                value: value.to_le_bytes().to_vec(),
            }),
    );
    changes.extend(
        nonces
            .into_iter()
            .map(|(identity, value)| StateChange::Put {
                key: nonce_key(&identity),
                value: value.to_le_bytes().to_vec(),
            }),
    );
    let expected = stage_state_update(&Memory::default(), empty_root(), &changes)?.root();
    ensure!(
        block.point().state_root == expected,
        "cold recovered canonical state differs from full once-only oracle"
    );
    Ok(())
}

fn recover_once(fixture: &Fixture) -> Result<()> {
    let mut services = Services(vec![CandidatePipeline::start(
        pipeline_config(&fixture.ledger)?,
        OpenMode::Existing,
    )?]);
    let result = (|| -> Result<Recovered> {
        let pipeline = &services.0[0];
        let set = validator_set()?;
        let (context, parent) = anchor(fixture.root, &set);
        let journal = open_journal(
            ValidatorJournal::open(
                pipeline,
                context,
                parent,
                set.clone(),
                validator_key(fixture.index),
            )?,
            pipeline,
        )?;
        let head = journal.head().context("cold recovered head absent")?;
        ensure!(
            head.height == 1 && head.state_version == 2,
            "restart published a different state version"
        );
        let block = read_archive(pipeline, context, head, 1, set.clone())?;
        ensure!(
            block.certificate().verify(&set)?.signed_weight() >= 3,
            "restart lacks a quorum"
        );
        verify_once(&block)?;
        let checkpoint: Checkpoint =
            serde_json::from_slice(&fs::read(fixture.directory.join("kill-checkpoint.json"))?)?;
        let local =
            Validator::new(validator_key(fixture.index).verifying_key().to_bytes(), 1)?.id();
        if local == checkpoint.validator {
            let saved = read_metadata(
                pipeline,
                (1..=2)
                    .map(|sequence| MetaKey::ConsensusOutbox {
                        validator: local,
                        sequence,
                    })
                    .collect(),
            )?;
            ensure!(
                saved.values
                    == checkpoint
                        .outboxes
                        .iter()
                        .cloned()
                        .map(Some)
                        .collect::<Vec<_>>(),
                "restart overwrote original proposal or prevote"
            );
        }
        for raw in block.stored().raw_transactions() {
            let tx = authenticate_transfer_v3(raw, CHAIN, 1024)?;
            ensure!(
                read_value(pipeline, head.state_root, nonce_key(&tx.nonce_identity()))?
                    == 1u64.to_le_bytes(),
                "canonical nonce consumed twice"
            );
        }
        ensure!(
            read_value(pipeline, head.state_root, balance_key(&account(2)))?
                == 150u128.to_le_bytes(),
            "canonical credit applied twice"
        );
        let receipt_digest: Hash = Sha256::digest(block.stored().receipt_bytes().concat()).into();
        Ok(Recovered {
            pid: std::process::id(),
            head,
            candidates: vec![block.stored().candidate_id()],
            receipts: vec![receipt_digest],
            recipient: 150,
        })
    })();
    let shutdown = services.shutdown();
    let recovered = result?;
    shutdown?;
    fs::write(
        fixture
            .directory
            .join(format!("recovered-{}.json", fixture.index)),
        serde_json::to_vec(&recovered)?,
    )?;
    Ok(())
}

fn copy_stopped_ledger(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "fixture refuses copying a ledger symlink"
        );
        if kind.is_dir() {
            copy_stopped_ledger(&entry.path(), &to.join(entry.file_name()))?;
        } else {
            ensure!(kind.is_file(), "unexpected fixture ledger entry");
            fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn reject_missing_body(fixture: &Fixture) -> Result<()> {
    let set = validator_set()?;
    let (context, parent) = anchor(fixture.root, &set);
    let checkpoint: Checkpoint =
        serde_json::from_slice(&fs::read(fixture.directory.join("kill-checkpoint.json"))?)?;
    let mut services = Services(vec![CandidatePipeline::start(
        pipeline_config(&fixture.ledger)?,
        OpenMode::Existing,
    )?]);
    let mut journal = open_journal(
        ValidatorJournal::open(
            &services.0[0],
            context,
            parent,
            set,
            validator_key(fixture.index),
        )?,
        &services.0[0],
    )?;
    let candidate = journal
        .take_replay_records()
        .into_iter()
        .find_map(|record| match record.evidence {
            ReplayEvidence::Proposal { candidate, .. }
            | ReplayEvidence::Certified { candidate, .. } => Some(candidate),
            ReplayEvidence::None => None,
        })
        .context("crash fixture has no body locator")?;
    drop(journal);
    services.shutdown()?;
    let key = {
        let store = CandidateStore::open(store_config(&fixture.ledger)?, OpenMode::Existing)?;
        store.scoped_key(&marker_key(candidate.candidate_id))
    };
    let config = store_config(&fixture.ledger)?;
    {
        let mut storage = StorageSession::open(&config.library, &config.database, config.storage)?;
        ensure!(
            storage.get(&key)?.is_some(),
            "missing-body negative case had no original marker"
        );
        storage.atomic_write_batch(&[StorageWrite::Delete { key }])?;
    }
    services.0.push(CandidatePipeline::start(
        pipeline_config(&fixture.ledger)?,
        OpenMode::Existing,
    )?);
    let pipeline = &services.0[0];
    let mut controller = open_controller(fixture, pipeline)?;
    let deadline = Instant::now() + DEADLINE;
    let rejected = loop {
        ensure!(
            Instant::now() < deadline,
            "missing-body recovery never failed closed"
        );
        match controller.poll(pipeline, Instant::now()) {
            Err(error) => break error,
            Ok(()) => ensure!(
                controller.head().is_none() && controller.stats().durable_votes == 0,
                "missing-body recovery signed or finalized"
            ),
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    ensure!(
        format!("{rejected:#}").contains("candidate"),
        "wrong missing-body rejection: {rejected:#}"
    );
    let saved = read_metadata(
        pipeline,
        vec![
            MetaKey::ConsensusState(checkpoint.validator),
            MetaKey::ConsensusOutbox {
                validator: checkpoint.validator,
                sequence: 3,
            },
            MetaKey::ChainHead,
        ],
    )?;
    ensure!(
        saved.values == vec![Some(checkpoint.snapshot), None, None],
        "failed body recovery changed safety state or emitted a new vote"
    );
    controller.shutdown()?;
    services.shutdown()?;
    let store = CandidateStore::open(store_config(&fixture.ledger)?, OpenMode::Existing)?;
    ensure!(
        store.recover(candidate.candidate_id)?.is_none(),
        "recovery recreated a missing completion marker"
    );
    Ok(())
}

#[test]
#[ignore = "requires real AOEM; killed undecided process, four WSS validators and four cold readers"]
fn real_undecided_process_kill_recovers_original_body_and_finalizes_without_wallet_reinjection(
) -> Result<()> {
    let _ = library()?;
    if let Some(path) = std::env::var_os(CHILD_CONFIG) {
        let fixture: Fixture = serde_json::from_slice(&fs::read(path)?)?;
        return match std::env::var(CHILD_MODE)?.as_str() {
            "crash" => run(&fixture, true),
            "resume" => run(&fixture, false),
            "recover" => recover_once(&fixture),
            "reject-missing" => reject_missing_body(&fixture),
            mode => bail!("unknown crash fixture mode {mode}"),
        };
    }
    let artifacts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/runtime-rebuild");
    fs::create_dir_all(&artifacts)?;
    ensure!(
        artifacts.canonicalize()?.starts_with(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()?
        ),
        "crash artifacts escaped repository"
    );
    let directory = artifacts.join(format!(
        "undecided-crash-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&directory)?;
    eprintln!("undecided crash artifacts={}", directory.display());
    let mut relay = Relay::start(&directory.join("relay"))?;
    let mut fixtures = Vec::new();
    for index in 0..4 {
        let ledger = directory.join(format!("validator-{index}"));
        fixtures.push(Fixture {
            index,
            root: initialize(&ledger)?,
            ledger,
            directory: directory.clone(),
            endpoint: relay.endpoint.clone(),
            certificate: relay.certificate.clone(),
            load: None,
        });
    }
    let set = validator_set()?;
    let leader = (0..4)
        .find(|i| {
            Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                .is_ok_and(|v| v.id() == set.leader(1, 0).unwrap())
        })
        .context("no initial leader")?;
    let mut dying = Children(vec![spawn_test(&fixtures[leader], "crash", CRASH_TEST)?]);
    let deadline = Instant::now() + RUN_BUDGET;
    let mut checkpoint = None;
    wait_for(&mut dying, deadline, || {
        checkpoint = fs::read(directory.join("kill-checkpoint.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Checkpoint>(&bytes).ok());
        checkpoint.is_some()
    })?;
    let checkpoint = checkpoint.unwrap();
    ensure!(
        checkpoint.pid == dying.0[0].id(),
        "checkpoint belongs to another process"
    );
    dying.0[0].kill()?;
    ensure!(
        !dying.0[0].wait()?.success(),
        "fixture did not forcibly terminate undecided node"
    );
    dying.0.clear();
    // Damage only a copy made AFTER the original process has terminated. The
    // original ledger remains untouched and is used by the positive scenario.
    let mut damaged = fixtures[leader].clone();
    damaged.ledger = directory.join("missing-body-copy");
    copy_stopped_ledger(&fixtures[leader].ledger, &damaged.ledger)?;
    let mut negative = Children(vec![spawn_test(&damaged, "reject-missing", CRASH_TEST)?]);
    wait_exited(&mut negative)?;
    let mut resumed = Children::default();
    for fixture in &fixtures {
        resumed.0.push(spawn_test(fixture, "resume", CRASH_TEST)?);
    }
    let mut finished = Vec::new();
    wait_for(&mut resumed, deadline, || {
        let reports = fixtures
            .iter()
            .map(|f| {
                fs::read(directory.join(format!("finished-{}.json", f.index)))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Finished>(&b).ok())
            })
            .collect::<Option<Vec<_>>>();
        if let Some(reports) = reports {
            finished = reports;
            true
        } else {
            false
        }
    })?;
    let pids: std::collections::BTreeSet<_> = finished.iter().map(|r| r.pid).collect();
    ensure!(
        pids.len() == 4
            && !pids.contains(&checkpoint.pid)
            && finished.iter().all(|r| r.head == finished[0].head
                && r.execution_failures == 0
                && r.executed_batches >= 1),
        "new processes did not independently reexecute and finalize same block"
    );
    fs::write(directory.join("stop"), b"four resumed durable heads")?;
    wait_exited(&mut resumed)?;
    relay.shutdown()?;
    let mut readers = Children::default();
    for fixture in &fixtures {
        readers.0.push(spawn_test(fixture, "recover", CRASH_TEST)?);
    }
    wait_exited(&mut readers)?;
    let reopened = fixtures
        .iter()
        .map(|f| -> Result<Recovered> {
            Ok(serde_json::from_slice(&fs::read(
                directory.join(format!("recovered-{}.json", f.index)),
            )?)?)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        reopened.iter().all(|r| r.head == finished[0].head
            && r.candidates == reopened[0].candidates
            && r.receipts == reopened[0].receipts
            && r.recipient == 150
            && !pids.contains(&r.pid)),
        "cold processes changed canonical block, receipts or credit"
    );
    eprintln!("process kill + four restarted autonomous validators + four cold readers PASS; no reinjected body, exact original outbox retained; artifacts={}", directory.display());
    Ok(())
}
