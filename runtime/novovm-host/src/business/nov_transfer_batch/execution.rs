use super::*;
use crate::business::direct_nov_fee::{
    quote_and_settle, quote_transfer, FeeFailure, FeeJournalEntry, FeeQuote,
};
use crate::business::quoted_transfer::{
    compute_outcome, TransferDelta, TransferFailure, TransferIntent, TransferOutcome,
    TransferSnapshot,
};
use crate::execution::plan::UnpublishedBatchEffects;
#[cfg(feature = "native")]
use novovm_aoem::{ComputeSession, ComputeTask};
#[cfg(feature = "native")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "native")]
use std::time::Duration;

/// A new ordered receipt encoding. The final state binds diagnostics; this
/// receipt binds each quote, rejection and full successful fee journal entry.
/// It deliberately has no `safe`, `finalized`, old prev-seal or fake proof flag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NovTransferReceipt {
    pub tx_hash: NodeHash,
    pub signer_identity: [u8; 32],
    pub delta: TransferDelta,
    pub failure: Option<TransferFailure>,
    pub quote: Option<FeeQuote>,
    pub fee_failure: Option<FeeFailure>,
    pub journal: Option<FeeJournalEntry>,
    pub clear_clearing_candidates: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionObservation {
    pub components: usize,
    pub credit_only_accounts: usize,
    pub recomputed_transactions: usize,
    /// Natural overlap, not a benchmark or mainchain transaction throughput.
    pub peak_callbacks: usize,
}

/// Only the complete compiler/executor constructs this, not `stage(arbitrary
/// patch)`. Still not a cryptographic proof or authorization to publish a root.
pub struct ExecutedNovBatch {
    effects: UnpublishedBatchEffects,
    receipts: Vec<NovTransferReceipt>,
    fees: FeeState,
    receipt_batch_commitment: NodeHash,
    statement_commitment: NodeHash,
    observation: ExecutionObservation,
}

impl ExecutedNovBatch {
    #[cfg(feature = "native")]
    pub(crate) fn into_poststate_seed(
        self,
        budget: CaptureBudget,
    ) -> Result<Option<crate::state::frontier::PostStateSeed>> {
        self.effects.into_poststate_seed(budget)
    }

    pub fn effects(&self) -> &UnpublishedBatchEffects {
        &self.effects
    }
    pub fn receipts(&self) -> &[NovTransferReceipt] {
        &self.receipts
    }
    pub fn fees(&self) -> &FeeState {
        &self.fees
    }
    pub fn receipt_batch_commitment(&self) -> NodeHash {
        self.receipt_batch_commitment
    }
    pub fn statement_commitment(&self) -> NodeHash {
        self.statement_commitment
    }
    pub fn observation(&self) -> &ExecutionObservation {
        &self.observation
    }
}

struct Prediction {
    index: usize,
    snapshot: TransferSnapshot,
    outcome: TransferOutcome,
}

#[cfg(feature = "native")]
struct Completion {
    input: Option<SignatureCheckedInput>,
    remaining: usize,
    components: Vec<Option<Vec<Prediction>>>,
}

impl NovTransferInput {
    /// Deterministic proof-side scheduling of the SAME component computation
    /// and ordered settlement used by native execution. No AOEM callback or
    /// publication authority is fabricated; this remains crate-internal.
    pub(crate) fn execute_for_proof(self) -> Result<ExecutedNovBatch> {
        let now = u128::from(self.input.plan().context().timestamp_unix_ms);
        let count = self.prepared.components.len();
        ensure!(count > 0, "empty NOV component plan");
        let predictions = (0..count)
            .map(|component| speculate(&self.prepared, component, now).map(Some))
            .collect::<Result<Vec<_>>>()?;
        finish(self.input, &self.prepared, predictions, now)
    }

    /// Blocking on the designated compute owner, never the node control loop.
    /// Last-completing callback reduces on the AOEM thread; no callback waits
    /// for other callbacks and no per-key message or live DB handle is carried.
    #[cfg(feature = "native")]
    pub fn execute(
        self,
        session: &mut ComputeSession,
        timeout: Duration,
    ) -> Result<ExecutedNovBatch> {
        let now = u128::from(self.input.plan().context().timestamp_unix_ms);
        let commitment = self.input.plan().commitment();
        let prepared = Arc::new(self.prepared);
        let count = prepared.components.len();
        ensure!(count > 0, "empty NOV component plan");
        let completion = Arc::new(Mutex::new(Completion {
            input: Some(self.input),
            remaining: count,
            components: (0..count).map(|_| None).collect(),
        }));
        let output: Arc<Mutex<Option<ExecutedNovBatch>>> = Arc::new(Mutex::new(None));
        let tasks: Vec<ComputeTask> = (0..count)
            .map(|component_index| {
                let prepared = Arc::clone(&prepared);
                let completion = Arc::clone(&completion);
                let output = Arc::clone(&output);
                Box::new(move || {
                    let predictions = speculate(&prepared, component_index, now)?;
                    let reduce = {
                        let mut state = completion
                            .lock()
                            .map_err(|_| anyhow::anyhow!("NOV completion poisoned"))?;
                        ensure!(
                            state.components[component_index].is_none() && state.remaining > 0,
                            "duplicate component completion"
                        );
                        state.components[component_index] = Some(predictions);
                        state.remaining -= 1;
                        if state.remaining == 0 {
                            Some((
                                state.input.take().context("missing complete NOV input")?,
                                std::mem::take(&mut state.components),
                            ))
                        } else {
                            None
                        }
                    };
                    if let Some((input, predictions)) = reduce {
                        // All expensive reduction/tree work is outside the short
                        // completion lock, still inside this generic AOEM callback.
                        let result = finish(input, &prepared, predictions, now)?;
                        let digest = result.statement_commitment;
                        *output
                            .lock()
                            .map_err(|_| anyhow::anyhow!("NOV output poisoned"))? = Some(result);
                        Ok(digest.to_vec())
                    } else {
                        Ok(Vec::new())
                    }
                }) as ComputeTask
            })
            .collect();
        let report = session.execute(tasks, timeout)?;
        let mut result = output
            .lock()
            .map_err(|_| anyhow::anyhow!("NOV output poisoned"))?
            .take()
            .context("missing NOV business output")?;
        ensure!(
            result.effects.plan_commitment() == commitment,
            "NOV output bound to another plan"
        );
        ensure!(
            report.outputs.len() == count
                && report.processed as usize == count
                && report.failed == 0,
            "NOV graph output count mismatch"
        );
        let outputs: Vec<_> = report
            .outputs
            .iter()
            .filter(|bytes| !bytes.is_empty())
            .collect();
        ensure!(
            outputs.len() == 1 && outputs[0].as_slice() == result.statement_commitment,
            "NOV graph result commitment mismatch"
        );
        result.observation.peak_callbacks = report.peak_inflight;
        Ok(result)
    }
}

fn intent(
    prepared: &Prepared,
    index: usize,
    now: u128,
) -> Result<(TransferIntent, Option<String>)> {
    let request = &prepared.requests[index];
    let (approved_fee, fee_cap, rejection) = match quote_transfer(request, &prepared.policy, now)? {
        Ok(quote) => (quote.nov_amount, quote.max_pay_amount, None),
        Err(error) => (0, 0, Some(error.to_string())),
    };
    use std::fmt::Write;
    let mut identity = String::with_capacity(64);
    for byte in &prepared.identities[index] {
        write!(&mut identity, "{byte:02x}")?;
    }
    Ok((
        TransferIntent {
            tx_hash: request.tx_hash,
            from: request.payer.clone(),
            to: request.recipient.clone(),
            nonce_identity: identity,
            nonce: prepared.tx_nonces[index],
            amount: request.amount,
            approved_fee,
            fee_cap,
        },
        rejection,
    ))
}

fn speculate(prepared: &Prepared, component: usize, now: u128) -> Result<Vec<Prediction>> {
    // Only this component's accounts are copied, never the whole batch N times.
    let mut balances = BTreeMap::new();
    let mut nonces = BTreeMap::new();
    for &index in &prepared.components[component] {
        let request = &prepared.requests[index];
        for account in [&request.payer, &request.recipient] {
            balances.insert(account.clone(), prepared.balances[account].unwrap_or(0));
        }
        nonces.insert(
            prepared.identities[index],
            prepared.nonces[&prepared.identities[index]],
        );
    }
    let mut predictions = Vec::with_capacity(prepared.components[component].len());
    for &index in &prepared.components[component] {
        let request = &prepared.requests[index];
        let (intent, rejection) = intent(prepared, index, now)?;
        let snapshot = TransferSnapshot {
            payer_balance: balances[&request.payer],
            recipient_balance: balances[&request.recipient],
            next_nonce: nonces[&prepared.identities[index]],
        };
        let outcome = compute_outcome(&intent, &snapshot, rejection.as_deref())?;
        balances.insert(request.payer.clone(), outcome.delta().payer.after);
        balances.insert(request.recipient.clone(), outcome.delta().recipient.after);
        nonces.insert(prepared.identities[index], outcome.delta().nonce_after);
        predictions.push(Prediction {
            index,
            snapshot,
            outcome,
        });
    }
    Ok(predictions)
}

fn finish(
    input: SignatureCheckedInput,
    prepared: &Prepared,
    components: Vec<Option<Vec<Prediction>>>,
    now: u128,
) -> Result<ExecutedNovBatch> {
    let mut ordered: Vec<Option<Prediction>> = (0..prepared.requests.len()).map(|_| None).collect();
    for component in components {
        for prediction in component.context("missing NOV component results")? {
            let index = prediction.index;
            ensure!(
                index < ordered.len() && ordered[index].is_none(),
                "duplicate or out of range NOV prediction"
            );
            ordered[index] = Some(prediction);
        }
    }
    let mut balances = prepared.balances.clone();
    let mut nonces = prepared.nonces.clone();
    let mut fees = prepared.fees.clone();
    let mut receipts = Vec::with_capacity(ordered.len());
    let mut recomputed = 0;
    for (index, prediction) in ordered.into_iter().enumerate() {
        let request = &prepared.requests[index];
        let identity = prepared.identities[index];
        let prediction = prediction.context("missing ordered NOV result")?;
        let actual = TransferSnapshot {
            payer_balance: balances[&request.payer].unwrap_or(0),
            recipient_balance: balances[&request.recipient].unwrap_or(0),
            next_nonce: nonces[&identity],
        };
        let credit_only = prepared.credit_only.contains(&request.recipient);
        let compatible = prediction.snapshot.payer_balance == actual.payer_balance
            && prediction.snapshot.next_nonce == actual.next_nonce
            && (credit_only || prediction.snapshot.recipient_balance == actual.recipient_balance);
        let mut outcome = if compatible {
            prediction.outcome
        } else {
            // One bounded repair per transaction, never replay whole suffixes.
            // This covers earlier global fee rejection erasing predicted money.
            recomputed += 1;
            let (intent, rejection) = intent(prepared, index, now)?;
            compute_outcome(&intent, &actual, rejection.as_deref())?
        };
        let fee = quote_and_settle(request, &prepared.policy, &fees, actual.payer_balance, now)?;
        if let Some(failure) = &fee.failure {
            ensure!(
                fee.payer_after == actual.payer_balance && fee.charged_fee() == 0,
                "failed fee mutated money"
            );
            outcome = outcome.reject_fee(failure.to_string());
        } else {
            ensure!(
                !matches!(outcome.failure(), Some(TransferFailure::Fee(_))),
                "successful settlement disagrees with business fee outcome"
            );
            ensure!(
                outcome.delta().fee_funding_delta == fee.charged_fee()
                    && actual.payer_balance.checked_sub(fee.charged_fee()) == Some(fee.payer_after),
                "fee funding not conserved"
            );
        }
        let mut delta = outcome.delta().clone();
        if credit_only {
            // Guarded pure credit is rebased on the ACTUAL successful prefix;
            // the worker's partial recipient prefix is not a conflict or money.
            delta.recipient.before = actual.recipient_balance;
            delta.recipient.after = if outcome.is_success() {
                actual
                    .recipient_balance
                    .checked_add(request.amount)
                    .context("checked pure credit headroom violated")?
            } else {
                actual.recipient_balance
            };
        }
        ensure!(
            delta.payer.before == actual.payer_balance
                && delta.recipient.before == actual.recipient_balance
                && delta.nonce_before == actual.next_nonce,
            "NOV reduced before-state mismatch"
        );
        if fee.failure.is_none() {
            // Absolute AOEM after-values already include fee: do NOT debit twice.
            balances.insert(request.payer.clone(), Some(delta.payer.after));
            if request.payer != request.recipient && outcome.is_success() {
                balances.insert(request.recipient.clone(), Some(delta.recipient.after));
            }
        }
        nonces.insert(identity, delta.nonce_after);
        receipts.push(NovTransferReceipt {
            tx_hash: request.tx_hash,
            signer_identity: identity,
            delta,
            failure: outcome.failure().cloned(),
            quote: fee.quote,
            fee_failure: fee.failure,
            journal: fee.journal,
            clear_clearing_candidates: fee.clear_clearing_candidates,
        });
        fees = fee.after_fee_state;
    }
    fees.validate()?;
    let mut changes =
        record_pages::record_changes(FEE_PREFIX, &postcard::to_allocvec(&fees)?, FEE_BYTES)?;
    for (account, after) in balances {
        if after != prepared.balances[&account] {
            changes.push(StateChange::Put {
                key: balance_key(&account),
                value: after
                    .context("balance unexpectedly removed")?
                    .to_le_bytes()
                    .to_vec(),
            });
        }
    }
    for (identity, after) in nonces {
        if after != prepared.nonces[&identity] {
            changes.push(StateChange::Put {
                key: nonce_key(&identity),
                value: after.to_le_bytes().to_vec(),
            });
        }
    }
    let mut digest = Sha256::new();
    digest.update(b"novovm/replacement/nov-receipt-batch/v1\0");
    digest.update(receipt_codec());
    digest.update((receipts.len() as u64).to_be_bytes());
    for receipt in &receipts {
        let bytes = postcard::to_allocvec(receipt)?;
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    let receipt_batch_commitment: NodeHash = digest.finalize().into();
    let effects = input.stage(&changes)?;
    let mut digest = Sha256::new();
    digest.update(b"novovm/replacement/nov-executed-batch/v1\0");
    digest.update(effects.plan_commitment());
    digest.update(effects.update().root());
    digest.update(receipt_batch_commitment);
    Ok(ExecutedNovBatch {
        effects,
        receipts,
        fees,
        receipt_batch_commitment,
        statement_commitment: digest.finalize().into(),
        observation: ExecutionObservation {
            components: prepared.components.len(),
            credit_only_accounts: prepared.credit_only.len(),
            recomputed_transactions: recomputed,
            peak_callbacks: 0,
        },
    })
}
