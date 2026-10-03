//! Cold-process assertions for the signed-load fixture, never an execution or
//! publication path. All state comes from the existing AOEM-owned ledger.

use super::*;
use crate::native_pipeline::business::direct_nov_fee::{
    quote_and_settle, quote_transfer, TransferFeeRequest,
};
use crate::native_pipeline::business::nov_transfer_batch::NovTransferReceipt;
use crate::native_pipeline::business::quoted_transfer::{
    compute_outcome, TransferIntent, TransferSnapshot,
};
use crate::native_pipeline::consensus::tests::controller_workload::{
    ExpectedState, Workload, AMOUNT, CHAIN_ID,
};
use crate::native_pipeline::ingress::authentication::check_nonce_sequence;
use std::fmt::Write as _;

/// This dedicated recovery process can wait; it is not a controller poll. Keep
/// absence distinct from a present zero so unused paged-record tails are checked.
fn read_optional(
    pipeline: &CandidatePipeline,
    root: Hash,
    key: Vec<u8>,
) -> Result<Option<Vec<u8>>> {
    let deadline = Instant::now() + DEADLINE;
    let mut ticket = loop {
        ensure!(
            Instant::now() < deadline,
            "load recovery query admission timed out"
        );
        if let Some(ticket) = pipeline.try_read_value(root, key.clone())? {
            break ticket;
        }
        std::thread::yield_now();
    };
    loop {
        ensure!(
            Instant::now() < deadline,
            "load recovery query completion timed out"
        );
        if let Some(value) = ticket.try_take()? {
            return Ok(value);
        }
        std::thread::yield_now();
    }
}

/// Serialize the real receipt type from the existing checked primitives and
/// compare every byte. Do not add a permissive receipt decoder or infer success
/// from a receipt count/hash. This oracle is outside all measured execution.
fn verify_receipts(
    block: &ArchiveBlock,
    workload: &Workload,
    expected: &mut ExpectedState,
    now: u128,
) -> Result<()> {
    let height = block.point().height;
    ensure!(
        expected.height.checked_add(1) == Some(height),
        "receipt oracle skipped a decided height"
    );
    let stored = block.stored();
    ensure!(
        stored.receipt_bytes().len() == workload.batch_size(),
        "height {height} has the wrong number of durable receipts"
    );
    let authenticated = stored
        .raw_transactions()
        .iter()
        .map(|raw| authenticate_transfer_v3(raw, CHAIN_ID, 1024))
        .collect::<Result<Vec<_>>>()?;
    let mut parent_nonces = BTreeMap::new();
    for tx in &authenticated {
        let identity = tx.nonce_identity();
        let before = if height == 1 {
            ensure!(
                expected.nonces.is_empty(),
                "initial nonce state is not empty"
            );
            0
        } else {
            *expected
                .nonces
                .get(&identity)
                .context("receipt oracle lost an earlier signer nonce")?
        };
        parent_nonces.insert(identity, before);
    }
    let transitions = check_nonce_sequence(&authenticated, &parent_nonces)?;
    let policy = policy();
    for (index, ((tx, saved), transition)) in authenticated
        .iter()
        .zip(stored.receipt_bytes())
        .zip(transitions)
        .enumerate()
    {
        let sender = &workload.senders()[index];
        let transfer = tx.transfer();
        ensure!(
            tx.public_key() == sender.public_key
                && transfer.from == sender.account.as_bytes()
                && transfer.to == workload.recipient().as_bytes()
                && transfer.amount == AMOUNT
                && transition.before == height - 1
                && transition.after == height,
            "height {height} receipt {index} has the wrong signed intent or nonce"
        );
        let request = TransferFeeRequest {
            tx_hash: tx.tx_hash(),
            payer: sender.account.clone(),
            recipient: workload.recipient().clone(),
            asset: transfer.asset.to_owned(),
            amount: transfer.amount,
            pay_asset: transfer.fee_policy.pay_asset.to_owned(),
            max_pay_amount: transfer.fee_policy.max_pay_amount,
            slippage_bps: transfer.fee_policy.slippage_bps,
        };
        let snapshot = TransferSnapshot {
            payer_balance: *expected
                .balances
                .get(&sender.account)
                .context("receipt oracle lost a funded sender")?,
            recipient_balance: expected
                .balances
                .get(workload.recipient())
                .copied()
                .unwrap_or(0),
            next_nonce: transition.before,
        };
        let quote = quote_transfer(&request, &policy, now)??;
        let mut nonce_identity = String::with_capacity(64);
        for byte in tx.nonce_identity() {
            write!(&mut nonce_identity, "{byte:02x}")?;
        }
        let outcome = compute_outcome(
            &TransferIntent {
                tx_hash: tx.tx_hash(),
                from: sender.account.clone(),
                to: workload.recipient().clone(),
                nonce_identity,
                nonce: transfer.nonce,
                amount: transfer.amount,
                approved_fee: quote.nov_amount,
                fee_cap: quote.max_pay_amount,
            },
            &snapshot,
            None,
        )?;
        let settled = quote_and_settle(
            &request,
            &policy,
            &expected.fees,
            snapshot.payer_balance,
            now,
        )?;
        ensure!(
            outcome.is_success() && settled.failure.is_none(),
            "funded receipt oracle failed at height {height}, transaction {index}"
        );
        let delta = outcome.delta();
        ensure!(
            delta.nonce_after == transition.after
                && delta.fee_funding_delta == settled.charged_fee()
                && delta.payer.after.checked_add(AMOUNT) == Some(settled.payer_after),
            "height {height} receipt {index} did not conserve fee funding"
        );
        let receipt = NovTransferReceipt {
            tx_hash: tx.tx_hash(),
            signer_identity: tx.nonce_identity(),
            delta: delta.clone(),
            failure: outcome.failure().cloned(),
            quote: settled.quote,
            fee_failure: settled.failure,
            journal: settled.journal,
            clear_clearing_candidates: settled.clear_clearing_candidates,
        };
        ensure!(
            postcard::to_allocvec(&receipt)?.as_slice() == saved.as_slice(),
            "height {height} receipt {index} differs from the full successful nonce/fee oracle"
        );
        expected
            .balances
            .insert(sender.account.clone(), delta.payer.after);
        expected
            .balances
            .insert(workload.recipient().clone(), delta.recipient.after);
        expected
            .nonces
            .insert(tx.nonce_identity(), transition.after);
        expected.fees = settled.after_fee_state;
        expected.transactions = expected
            .transactions
            .checked_add(1)
            .context("recovery oracle transaction count overflow")?;
    }
    expected.height = height;
    expected.fees.validate()?;
    Ok(())
}

pub(super) fn recover(fixture: &Fixture, spec: LoadSpec) -> Result<()> {
    let workload = Workload::new(spec.batch_size, spec.heights, policy())?;
    let base_timestamp = batch_context(fixture.root).timestamp_unix_ms;
    base_timestamp
        .checked_add(spec.heights)
        .context("load recovery timestamp overflow")?;
    let timestamp = |height| u128::from(base_timestamp + height);
    let set = validator_set()?;
    let (context, parent) = anchor(fixture.root, &set);
    // Services also drains on unwinding, unlike merely dropping the pipeline.
    let mut services = Services(vec![CandidatePipeline::start(
        pipeline_config_for(fixture)?,
        OpenMode::Existing,
    )?]);
    let result = (|| -> Result<Recovered> {
        let pipeline = &services.0[0];
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
        let head = journal
            .head()
            .context("reopened load process lacks a durable head")?;
        ensure!(
            head.height == spec.heights,
            "wrong reopened load chain height"
        );
        let mut candidates = Vec::new();
        let mut receipts = Vec::new();
        let mut previous = parent;
        let mut receipt_oracle = workload.expected_through(0, timestamp)?;
        for height in 1..=spec.heights {
            let block = read_archive(pipeline, journal.context(), head, height, set.clone())?;
            ensure!(
                block.parent() == previous
                    && block.point().height == height
                    && block.stored().context().height == height
                    && u128::from(block.stored().context().timestamp_unix_ms) == timestamp(height),
                "load archive parent/height/timestamp changed at height {height}"
            );
            ensure!(
                block.stored().raw_transactions() == workload.raw_height(height)?,
                "load archive changed the original signed body at height {height}"
            );
            block.proposal().verify(&set)?;
            let qc = block.certificate().verify(&set)?;
            ensure!(
                qc.signed_weight() >= 3 && qc.value() == Some(block.point().block_hash),
                "load archive lacks an independently verified quorum at height {height}"
            );
            verify_receipts(&block, &workload, &mut receipt_oracle, timestamp(height))?;
            candidates.push(block.stored().candidate_id());
            let mut hash = Sha256::new();
            for receipt in block.stored().receipt_bytes() {
                hash.update((receipt.len() as u64).to_be_bytes());
                hash.update(receipt);
            }
            receipts.push(hash.finalize().into());
            previous = block.point();
        }
        ensure!(
            previous == head,
            "load archive tip differs from durable head"
        );
        let expected = workload.expected_through(spec.heights, timestamp)?;
        ensure!(
            receipt_oracle == expected,
            "load receipt effects differ from workload state oracle"
        );
        let account_total = expected.balances.values().try_fold(0u128, |sum, balance| {
            sum.checked_add(*balance)
                .context("recovered balance sum overflow")
        })?;
        let initial_total = workload
            .initial_sender_balance()
            .checked_mul(spec.batch_size as u128)
            .context("load initial money overflow")?;
        let accounting = &expected.fees.accounting;
        let buckets = accounting
            .reserve_bucket_nov
            .checked_add(accounting.fee_bucket_nov)
            .and_then(|value| value.checked_add(accounting.risk_buffer_nov));
        ensure!(
            expected.transactions == spec.heights * spec.batch_size as u64
                && accounting.settlements == expected.transactions
                && accounting.journal_next_seq == expected.transactions
                && expected.nonces.len() == spec.batch_size
                && expected.nonces.values().all(|nonce| *nonce == spec.heights)
                && buckets == Some(accounting.settled_nov_total)
                && accounting.treasury_reserve_nov == Some(accounting.settled_nov_total)
                && accounting.settled_by_asset_nov == Some(accounting.settled_nov_total)
                && account_total.checked_add(accounting.settled_nov_total) == Some(initial_total),
            "recovered load fees, nonces or total money were not conserved"
        );
        let expected_records = expected.record_changes(&policy())?;
        // This fixture starts with exactly Workload::initial_changes. Rebuild
        // the final logical snapshot from empty using the canonical tree, so
        // matching known keys cannot hide an unexpected extra live state key.
        // It is a pure assertion oracle, never installed in the AOEM ledger.
        let expected_root =
            stage_state_update(&Memory::default(), empty_root(), &expected_records)?.root();
        ensure!(
            head.state_root == expected_root,
            "recovered load state has unexpected keys or differs from the full canonical oracle"
        );
        let mut recipient = None;
        let recipient_key = balance_key(workload.recipient());
        for change in expected_records {
            match change {
                StateChange::Put { key, value } => {
                    let actual = read_optional(pipeline, head.state_root, key.clone())?;
                    ensure!(
                        actual.as_ref() == Some(&value),
                        "recovered load value differs at key {key:02x?}"
                    );
                    if key == recipient_key {
                        recipient = Some(u128::from_le_bytes(
                            actual
                                .context("recovered load recipient absent")?
                                .try_into()
                                .map_err(|_| anyhow::anyhow!("bad recovered load balance width"))?,
                        ));
                    }
                }
                StateChange::Delete { key } => {
                    ensure!(
                        read_optional(pipeline, head.state_root, key.clone())?.is_none(),
                        "recovered load retained an unused record tail at key {key:02x?}"
                    );
                }
            }
        }
        let recipient = recipient.context("load recipient was not independently read back")?;
        ensure!(
            recipient == u128::from(expected.transactions) * AMOUNT,
            "recovered shared recipient credit differs"
        );
        Ok(Recovered {
            pid: std::process::id(),
            head,
            candidates,
            receipts,
            recipient,
        })
    })();
    // Drain even on an assertion/error, and do not emit a success artifact if
    // either readback or shutdown failed. Panic unwinding uses Services::drop.
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
