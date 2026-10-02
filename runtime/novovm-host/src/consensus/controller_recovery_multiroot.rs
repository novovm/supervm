//! Real AOEM, two different undecided candidates over one parent. Consensus
//! QCs are explicitly signed fixtures, NOT autonomous network finality. Only
//! cold controller re-executions count toward the asserted two-batch total.
use super::*;
use crate::business::direct_nov_fee::{quote_and_settle, quote_transfer, TransferFeeRequest};
use crate::business::nov_transfer_batch::NovTransferReceipt;
use crate::business::quoted_transfer::{compute_outcome, TransferIntent, TransferSnapshot};
use crate::consensus::journal::ReplayEvidence;
use crate::persistence::StoredCandidate;
use std::fmt::Write as _;

fn body(amounts: [u128; 2]) -> Result<Vec<Vec<u8>>> {
    [1u8, 3]
        .into_iter()
        .zip(amounts)
        .map(|(seed, amount)| {
            let key = SigningKey::from_bytes(&[seed; 32]);
            let mut tx = TransferV3 {
                chain_id: CHAIN,
                from: account(seed).as_bytes().to_vec(),
                to: account(2).as_bytes().to_vec(),
                asset: "NOV".into(),
                amount,
                nonce: 0,
                fee_policy: FeePolicy {
                    pay_asset: "NOV".into(),
                    max_pay_amount: 0,
                    slippage_bps: 0,
                },
                signature: Vec::new(),
            };
            let signature = key.sign(&signing_message(&tx)?);
            tx.signature = key.verifying_key().to_bytes().to_vec();
            tx.signature.extend_from_slice(&signature.to_bytes());
            encode_transfer_v3(&tx)
        })
        .collect()
}

fn execute(
    pipeline: &CandidatePipeline,
    root: Hash,
    raw: Vec<Vec<u8>>,
) -> Result<DurableCandidate> {
    let deadline = Instant::now() + DEADLINE;
    let mut request = BatchRequest::new(raw, batch_context(root), policy())?;
    let mut ticket = loop {
        ensure!(Instant::now() < deadline, "multiroot admission timed out");
        match pipeline.try_submit_owned(request) {
            Ok(Submission::Accepted(ticket)) => break ticket,
            Ok(Submission::Backpressured(returned)) => request = returned,
            Err(rejected) => return Err(rejected.error),
        }
        std::thread::yield_now();
    };
    loop {
        ensure!(Instant::now() < deadline, "multiroot execution timed out");
        if let Some(batch) = ticket.try_take()? {
            return Ok(batch.candidate().clone());
        }
        std::thread::yield_now();
    }
}

fn read_candidate(pipeline: &CandidatePipeline, id: Hash) -> Result<StoredCandidate> {
    let deadline = Instant::now() + DEADLINE;
    let mut ticket = loop {
        ensure!(
            Instant::now() < deadline,
            "multiroot read admission timed out"
        );
        if let Some(ticket) = pipeline.try_recover_consensus_candidate(id)? {
            break ticket;
        }
        std::thread::yield_now();
    };
    loop {
        ensure!(
            Instant::now() < deadline,
            "multiroot candidate read timed out"
        );
        if let Some(stored) = ticket.try_take()? {
            return stored.context("multiroot durable candidate disappeared");
        }
        std::thread::yield_now();
    }
}

fn state_only(journal: &mut ValidatorJournal, pipeline: &CandidatePipeline) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "multiroot journal timed out");
        if let Some(message) = journal.poll(pipeline)? {
            ensure!(
                message.is_none(),
                "state-only transition signed another vote"
            );
            return Ok(());
        }
        std::thread::yield_now();
    }
}

fn leader_index(set: &ValidatorSet, round: u64) -> Result<usize> {
    let id = set.leader(1, round)?;
    (0..4)
        .find(|i| {
            Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                .is_ok_and(|validator| validator.id() == id)
        })
        .context("fixture leader missing")
}

fn quorum(
    set: &ValidatorSet,
    context: ConsensusContext,
    round: u64,
    value: Hash,
    excluded: usize,
) -> Result<wire::VerifiedQuorum> {
    Quorum::from_votes(
        set,
        (0..4)
            .filter(|index| *index != excluded)
            .map(|index| {
                wire::Vote::sign(
                    context,
                    round,
                    Phase::Prevote,
                    Some(value),
                    set,
                    &validator_key(index),
                )
            })
            .collect::<Result<Vec<_>>>()?,
    )?
    .verify(set)
}

/// Independent serial primitives only: reconstruct every receipt and the full
/// resulting state, never supply these oracle results to the actual executor.
fn verify_candidate(stored: &StoredCandidate, raw: &[Vec<u8>], root: Hash) -> Result<()> {
    ensure!(
        stored.context() == &batch_context(root)
            && stored.raw_transactions() == raw
            && raw.len() == 2
            && stored.receipt_bytes().len() == raw.len(),
        "multiroot recovery changed parent, signed originals or receipt count"
    );
    let mut balances = BTreeMap::from([(account(1), 1_000_000u128), (account(3), 1_000_000u128)]);
    let mut nonces = BTreeMap::new();
    let mut fees = FeeState::default();
    let policy = policy();
    let now = u128::from(stored.context().timestamp_unix_ms);
    let mut transferred = 0u128;
    for (raw, actual_receipt) in raw.iter().zip(stored.receipt_bytes()) {
        let authenticated = authenticate_transfer_v3(raw, CHAIN, 1024)?;
        let tx = authenticated.transfer();
        ensure!(
            tx.nonce == 0 && !nonces.contains_key(&authenticated.nonce_identity()),
            "each branch must independently consume its signer's first nonce"
        );
        let from = Account::try_from(tx.from.clone()).map_err(anyhow::Error::msg)?;
        let to = Account::try_from(tx.to.clone()).map_err(anyhow::Error::msg)?;
        let snapshot = TransferSnapshot {
            payer_balance: *balances.get(&from).context("unfunded multiroot sender")?,
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
            "multiroot branch bypassed nonce, fee or business execution"
        );
        let expected_receipt = NovTransferReceipt {
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
            postcard::to_allocvec(&expected_receipt)? == *actual_receipt,
            "multiroot exact original receipt differs from serial oracle"
        );
        balances.insert(from, outcome.delta().payer.after);
        balances.insert(to, outcome.delta().recipient.after);
        nonces.insert(authenticated.nonce_identity(), 1u64);
        transferred += tx.amount;
        fees = settled.after_fee_state;
    }
    ensure!(
        balances.values().sum::<u128>() + fees.accounting.settled_nov_total == 2_000_000
            && fees.accounting.settlements == 2
            && fees.accounting.journal_next_seq == 2
            && balances[&account(2)] == transferred,
        "branch recovery combined rival credits/nonces/fees"
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
    ensure!(
        stored.state_root()
            == stage_state_update(&Memory::default(), empty_root(), &changes)?.root(),
        "full candidate state includes wrong/missing/additional business or fee records"
    );
    Ok(())
}

#[test]
#[ignore = "requires real AOEM; signed QC fixtures and same-process cold recovery, not network finality"]
fn real_cold_controller_recovers_distinct_locked_and_late_valid_candidates() -> Result<()> {
    let _ = library()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/runtime-rebuild/consensus-tests")
        .join(format!(
            "multiroot-recovery-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    let ledger = directory.join("ledger");
    let root = initialize(&ledger)?;
    let set = validator_set()?;
    let (context, parent) = anchor(root, &set);
    let first_leader = leader_index(&set, 0)?;
    let second_leader = leader_index(&set, 1)?;
    let index = (0..4)
        .find(|i| *i != first_leader && *i != second_leader)
        .context("need a local nonleader for both fixture rounds")?;
    let validator = Validator::new(validator_key(index).verifying_key().to_bytes(), 1)?.id();
    let raw_a = body([100, 50])?;
    let raw_b = body([200, 75])?;
    ensure!(
        raw_a != raw_b,
        "fixture did not create different signed bodies"
    );
    let mut services = Services(vec![CandidatePipeline::start(
        pipeline_config(&ledger)?,
        OpenMode::Existing,
    )?]);
    let pipeline = &services.0[0];
    let a = execute(pipeline, root, raw_a.clone())?;
    let b = execute(pipeline, root, raw_b.clone())?;
    let value_a = BlockStatement::from_executed(a.packet(), context, &set, &parent)?.hash();
    let value_b = BlockStatement::from_executed(b.packet(), context, &set, &parent)?.hash();
    ensure!(
        value_a != value_b && a.packet().state_root() != b.packet().state_root(),
        "fixture values unexpectedly alias"
    );
    let ids = [a.packet().candidate_id(), b.packet().candidate_id()];
    let digests = [a.packet().document_digest(), b.packet().document_digest()];
    let p0 = wire::Proposal::sign(
        context,
        0,
        value_a,
        None,
        &set,
        &validator_key(first_leader),
    )?
    .verify(&set)?;
    let p1 = wire::Proposal::sign(
        context,
        1,
        value_b,
        None,
        &set,
        &validator_key(second_leader),
    )?
    .verify(&set)?;
    let qc0 = quorum(&set, context, 0, value_a, (index + 1) % 4)?;
    // The local node signs NIL at round 1; never forge its opposite prevote in
    // this fixture's QC. The other three keys supply the explicit late proof.
    let qc1 = quorum(&set, context, 1, value_b, index)?;
    let mut journal = open_journal(
        ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(index))?,
        pipeline,
    )?;
    journal.accept_proposal(&p0, &a, None)?;
    complete(&mut journal, pipeline)?;
    journal.observe_prevotes(&qc0, Some((&p0, &a)))?;
    complete(&mut journal, pipeline)?;
    journal.round_wait_elapsed(0)?;
    state_only(&mut journal, pipeline)?;
    journal.timeout(1, TimeoutStep::Propose)?;
    let prevote = complete(&mut journal, pipeline)?;
    journal.timeout(1, TimeoutStep::Prevote)?;
    let precommit = complete(&mut journal, pipeline)?;
    for (message, phase) in [(&prevote, Phase::Prevote), (&precommit, Phase::Precommit)] {
        ensure!(
            matches!(message, DurableMessage::Vote(vote) if vote.phase == phase && vote.value.is_none()),
            "fixture lost original nil vote"
        );
    }
    journal.observe_prevotes(&qc1, Some((&p1, &b)))?;
    state_only(&mut journal, pipeline)?;
    ensure!(
        journal.head().is_none() && journal.last_durable_message().is_none(),
        "late QC published a head or new vote"
    );
    let mut keys = vec![MetaKey::ConsensusState(validator), MetaKey::ChainHead];
    keys.extend((1..=6).map(|sequence| MetaKey::ConsensusOutbox {
        validator,
        sequence,
    }));
    let before = read_metadata(pipeline, keys.clone())?;
    ensure!(
        before.values[0].is_some()
            && before.values[1].is_none()
            && before.values[2..].iter().all(Option::is_some),
        "fixture did not persist exact six transitions"
    );
    drop(journal);
    drop(a);
    drop(b);
    services.shutdown()?;

    let mut relay = Relay::start(&directory.join("relay"))?;
    let fixture = Fixture {
        index,
        directory: directory.clone(),
        ledger: ledger.clone(),
        endpoint: relay.endpoint.clone(),
        certificate: relay.certificate.clone(),
        root,
        load: None,
    };
    services.0.push(CandidatePipeline::start(
        pipeline_config(&ledger)?,
        OpenMode::Existing,
    )?);
    let pipeline = &services.0[0];
    // Read the actual reopened role closure without giving the controller an
    // already-consumed journal. A second opening performs the same validation.
    let mut reopened = open_journal(
        ValidatorJournal::open(pipeline, context, parent, set.clone(), validator_key(index))?,
        pipeline,
    )?;
    let records = reopened.take_replay_records();
    let certified: Vec<_> = records
        .iter()
        .filter_map(|record| match &record.evidence {
            ReplayEvidence::Certified {
                certificate,
                candidate,
                ..
            } => Some((certificate.round(), candidate.value, candidate.candidate_id)),
            _ => None,
        })
        .collect();
    ensure!(
        records.len() == 4 && certified == [(0, value_a, ids[0]), (1, value_b, ids[1])],
        "reopen lost distinct old lock or late valid candidate"
    );
    drop(reopened);
    let mut controller = open_controller(&fixture, pipeline)?;
    let result = (|| -> Result<()> {
        ensure!(controller.is_recovering(), "cold multiroot gate not active");
        let probe = Arc::new(Message::Body {
            context: batch_context(root),
            raw_transactions: Vec::new(),
        });
        ensure!(
            !controller.try_submit_body(&probe)?,
            "recovery accepted an external body"
        );
        let deadline = Instant::now() + DEADLINE;
        loop {
            ensure!(
                Instant::now() < deadline,
                "cold multiroot recovery timed out: {:?}",
                controller.stats()
            );
            controller.poll(pipeline, Instant::now())?;
            ensure!(
                controller.head().is_none()
                    && !controller.is_pending()
                    && controller.round() == 1
                    && controller.step() == TimeoutStep::Precommit
                    && controller.stats().durable_votes == 0
                    && controller.stats().durable_decisions == 0,
                "cold recovery created a signature, transition or canonical head"
            );
            if !controller.is_recovering() {
                break;
            }
            std::thread::yield_now();
        }
        ensure!(
            controller.stats().executed_batches == 2
                && controller.stats().execution_failures == 0
                && controller.stats().retained_bodies == 2,
            "cold controller did not really reexecute and retain both distinct candidates"
        );
        ensure!(
            read_metadata(pipeline, keys.clone())? == before,
            "cold recovery rewrote original safety state or outboxes"
        );
        ensure!(
            read_metadata(
                pipeline,
                vec![MetaKey::ConsensusOutbox {
                    validator,
                    sequence: 7
                }]
            )?
            .values
                == [None],
            "cold recovery appended a signing revision"
        );
        for (i, raw) in [&raw_a, &raw_b].into_iter().enumerate() {
            let stored = read_candidate(pipeline, ids[i])?;
            ensure!(
                stored.document_digest() == digests[i],
                "reexecution changed exact candidate document"
            );
            verify_candidate(&stored, raw, root)?;
            for raw in raw {
                let authenticated = authenticate_transfer_v3(raw, CHAIN, 1024)?;
                ensure!(
                    read_value(
                        pipeline,
                        stored.state_root(),
                        nonce_key(&authenticated.nonce_identity())
                    )? == 1u64.to_le_bytes(),
                    "candidate nonce consumed twice or inherited from rival branch"
                );
            }
        }
        Ok(())
    })();
    let channel_shutdown = controller.shutdown();
    drop(controller);
    let pipeline_shutdown = services.shutdown();
    let relay_shutdown = relay.shutdown();
    result?;
    channel_shutdown?;
    pipeline_shutdown?;
    relay_shutdown?;
    eprintln!(
        "two distinct cold candidates reexecuted without new signatures: {}",
        directory.display()
    );
    Ok(())
}
