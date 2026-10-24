use super::*;
use crate::native_pipeline::business::direct_nov_fee::{
    FeeFailure, FeeJournalEntry, FeeQuotationContext, FeeQuote, PreparedTransferFee,
};
use crate::native_pipeline::business::quoted_transfer::{
    compute_outcome, TransferDelta, TransferFailure, TransferIntent, TransferOutcome,
    TransferSnapshot,
};
use crate::native_pipeline::execution::plan::UnpublishedBatchEffects;
use novovm_exec::resident::{ComputeSession, ComputeTask};
use std::sync::{Arc, Mutex};
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
    /// Actual verified structural rows used by this completed business batch.
    /// Diagnostic only, not a statement field or a finalized success count.
    pub apfl_view_transactions: usize,
    pub components: usize,
    pub credit_only_accounts: usize,
    pub recomputed_transactions: usize,
    /// Business rows completed by the resident Vulkan integer asset; excludes
    /// padding. This count is not physical GPU device attestation.
    pub gpu_rows: usize,
    /// Complete composition delegations (legacy field name). Not physical GPU
    /// dispatch count, callback count or finalized transaction count.
    pub gpu_waves: usize,
    /// Natural overlap, not a benchmark or mainchain transaction throughput.
    pub peak_callbacks: usize,
}

/// Only the complete compiler/executor constructs this, not `stage(arbitrary
/// patch)`. Still not a cryptographic proof or authorization to publish a root.
pub struct ExecutedNovBatch {
    spend_outputs: Option<Vec<SpendOutput>>,
    effects: UnpublishedBatchEffects,
    receipts: Vec<NovTransferReceipt>,
    fees: FeeState,
    receipt_batch_commitment: NodeHash,
    statement_commitment: NodeHash,
    pub(super) observation: ExecutionObservation,
}

impl ExecutedNovBatch {
    pub fn spend_outputs(&self) -> Option<&[SpendOutput]> {
        self.spend_outputs.as_deref()
    }
    pub(crate) fn into_poststate_seed(
        self,
        budget: CaptureBudget,
    ) -> Result<Option<crate::native_pipeline::state::frontier::PostStateSeed>> {
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
    outcome: Option<TransferOutcome>,
    fee: PreparedTransferFee,
}

struct Completion {
    input: Option<ExecutionInput>,
    remaining: usize,
    components: Vec<Option<Vec<Prediction>>>,
}

impl NovTransferInput {
    /// One complete typed composition on the resident owner. No node-side
    /// economic repair or second business callback. The retained session and
    /// timeout parameters belong to the common caller, not GPU cancellation.
    #[cfg(test)]
    pub(crate) fn execute_with_integer_resources(
        self,
        _session: &mut ComputeSession,
        resources: &mut ResidentTransferResources,
        _timeout: Duration,
    ) -> Result<ExecutedNovBatch> {
        self.execute_integer_plan(resources)
    }

    /// The complete semantic plan does not use the CPU callback session. Keep
    /// its owner-thread resources local; only captured inputs/results cross lanes.
    pub(crate) fn execute_integer_plan(
        self,
        resources: &mut ResidentTransferResources,
    ) -> Result<ExecutedNovBatch> {
        let _span = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::ExecuteInteger,
        );
        let now = u128::from(self.input.plan().context().timestamp_unix_ms);
        let result = super::full_effects::execute(self.input, &self.prepared, resources, now);
        if result.is_err() {
            // Includes projection/identity failures after native completion:
            // the owner must be explicitly recovered, never reuse stale output.
            resources.poison();
        }
        result
    }

    /// Deterministic proof-side scheduling of the SAME component computation
    /// and ordered settlement used by native execution. No AOEM callback or
    /// publication authority is fabricated; this remains crate-internal.
    pub(crate) fn execute_for_proof(self) -> Result<ExecutedNovBatch> {
        ensure!(
            self.prepared.spend.is_none(),
            "UCA economic contract requires its own approved proof relation"
        );
        let now = u128::from(self.input.plan().context().timestamp_unix_ms);
        let count = self.prepared.components.len();
        ensure!(count > 0, "empty NOV component plan");
        let quotation = FeeQuotationContext::new(&self.prepared.policy, now)?;
        let predictions = (0..count)
            .map(|component| speculate(&self.prepared, component, &quotation).map(Some))
            .collect::<Result<Vec<_>>>()?;
        finish(self.input, &self.prepared, predictions, now)
    }

    /// Blocking on the designated compute owner, never the node control loop.
    /// Last-completing callback reduces on the AOEM thread; no callback waits
    /// for other callbacks and no per-key message or live DB handle is carried.
    pub fn execute(
        self,
        session: &mut ComputeSession,
        timeout: Duration,
    ) -> Result<ExecutedNovBatch> {
        let _span = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::ExecuteCpu,
        );
        let now = u128::from(self.input.plan().context().timestamp_unix_ms);
        let commitment = self.input.plan().commitment();
        let prepared = Arc::new(self.prepared);
        let quotation = FeeQuotationContext::new(&prepared.policy, now)?;
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
                let quotation = Arc::clone(&quotation);
                Box::new(move || {
                    let predictions = speculate(&prepared, component_index, &quotation)?;
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

pub(super) fn intent(
    prepared: &Prepared,
    index: usize,
    fee: &PreparedTransferFee,
) -> Result<(TransferIntent, Option<String>)> {
    let request = &prepared.requests[index];
    let (approved_fee, fee_cap, rejection) = fee.intent_terms();
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

fn speculate(
    prepared: &Prepared,
    component: usize,
    quotation: &Arc<FeeQuotationContext>,
) -> Result<Vec<Prediction>> {
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
        let fee = if prepared.spend.is_some() {
            quotation.prepare_strict_cap(request)?
        } else {
            quotation.prepare(request)?
        };
        let (intent, rejection) = intent(prepared, index, &fee)?;
        let snapshot = TransferSnapshot {
            payer_balance: balances[&request.payer],
            recipient_balance: balances[&request.recipient],
            next_nonce: nonces[&prepared.identities[index]],
        };
        // UCA nonce/budget depend on effective authorized predecessors. Never
        // pre-increment or reject a later row using the parent's nonce here.
        let outcome = if prepared.spend.is_some() {
            None
        } else {
            let outcome = compute_outcome(&intent, &snapshot, rejection.as_deref())?;
            balances.insert(request.payer.clone(), outcome.delta().payer.after);
            balances.insert(request.recipient.clone(), outcome.delta().recipient.after);
            nonces.insert(prepared.identities[index], outcome.delta().nonce_after);
            Some(outcome)
        };
        predictions.push(Prediction {
            index,
            snapshot,
            outcome,
            fee,
        });
    }
    Ok(predictions)
}

fn finish(
    input: ExecutionInput,
    prepared: &Prepared,
    components: Vec<Option<Vec<Prediction>>>,
    now: u128,
) -> Result<ExecutedNovBatch> {
    let _span =
        crate::native_pipeline::profile::span(crate::native_pipeline::profile::Phase::Reduction);
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
    let mut budgets: BTreeMap<_, _> = prepared
        .spend
        .as_ref()
        .map(|s| {
            s.grants
                .iter()
                .map(|(id, g)| (*id, g.record.remaining_debit_budget))
                .collect()
        })
        .unwrap_or_default();
    let mut spend_outputs = prepared
        .spend
        .as_ref()
        .map(|_| Vec::with_capacity(ordered.len()));
    for (index, prediction) in ordered.into_iter().enumerate() {
        let request = &prepared.requests[index];
        let identity = prepared.identities[index];
        let prediction = prediction.context("missing ordered NOV result")?;
        let actual = TransferSnapshot {
            payer_balance: balances[&request.payer].unwrap_or(0),
            recipient_balance: balances[&request.recipient].unwrap_or(0),
            next_nonce: nonces[&identity],
        };
        if let Some(spend) = &prepared.spend {
            use novovm_adapter_api::uca_spend_policy::SpendPolicyError;
            let e = &spend.entries[index];
            let failure = e.static_failure.or_else(|| {
                if prepared.tx_nonces[index] != actual.next_nonce {
                    Some(SpendPolicyError::NonceMismatch)
                } else if e.reserve > budgets[&e.grant_id]
                    || e.reserve > spend.grants[&e.grant_id].record.max_debit_per_intent
                {
                    Some(SpendPolicyError::LimitExceeded)
                } else {
                    None
                }
            });
            if let Some(failure) = failure {
                receipts.push(super::uca_effects::rejected_receipt(
                    prepared, index, &actual, failure,
                ));
                spend_outputs.as_mut().unwrap().push(SpendOutput {
                    failure: Some(failure),
                    remaining: budgets[&e.grant_id],
                });
                continue;
            }
        }
        let credit_only = prepared.credit_only.contains(&request.recipient);
        let compatible = prediction.snapshot.payer_balance == actual.payer_balance
            && prediction.snapshot.next_nonce == actual.next_nonce
            && (credit_only || prediction.snapshot.recipient_balance == actual.recipient_balance);
        let mut outcome = match prediction.outcome.filter(|_| compatible) {
            Some(outcome) => outcome,
            None => {
                // One bounded repair per transaction, never replay whole suffixes.
                // This covers earlier global fee rejection erasing predicted money.
                recomputed += 1;
                let (intent, rejection) = intent(prepared, index, &prediction.fee)?;
                compute_outcome(&intent, &actual, rejection.as_deref())?
            }
        };
        let fee =
            prediction
                .fee
                .settle(request, &prepared.policy, &fees, actual.payer_balance, now)?;
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
        if let Some(spend) = &prepared.spend {
            let id = spend.entries[index].grant_id;
            let debit = if outcome.is_success() {
                request
                    .amount
                    .checked_add(delta.fee_funding_delta)
                    .context("UCA actual debit overflow")?
            } else {
                delta.fee_funding_delta
            };
            let remaining = budgets[&id]
                .checked_sub(debit)
                .context("UCA approved budget underflow")?;
            budgets.insert(id, remaining);
            spend_outputs.as_mut().unwrap().push(SpendOutput {
                failure: None,
                remaining,
            });
        }
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
    stage_results(
        input,
        prepared,
        balances,
        nonces,
        fees,
        receipts,
        recomputed,
        spend_outputs,
    )
}

/// Encoding, commitment and original tree staging only. Business decisions are
/// already complete in either the CPU reference or the AOEM composition.
#[allow(clippy::too_many_arguments)]
pub(super) fn stage_results(
    input: ExecutionInput,
    prepared: &Prepared,
    balances: BTreeMap<Account, Option<u128>>,
    nonces: BTreeMap<[u8; 32], u64>,
    fees: FeeState,
    receipts: Vec<NovTransferReceipt>,
    recomputed: usize,
    spend_outputs: Option<Vec<SpendOutput>>,
) -> Result<ExecutedNovBatch> {
    fees.validate()?;
    let mut changes =
        record_pages::record_changes(FEE_PREFIX, &postcard::to_allocvec(&fees)?, FEE_BYTES)?;
    ensure!(
        prepared.spend.is_some() == spend_outputs.is_some(),
        "UCA effect output missing/unexpected"
    );
    if let (Some(spend), Some(outputs)) = (&prepared.spend, &spend_outputs) {
        ensure!(
            outputs.len() == spend.entries.len(),
            "UCA effect output shape"
        );
        let mut final_budgets = BTreeMap::new();
        for (entry, output) in spend.entries.iter().zip(outputs) {
            final_budgets.insert(entry.grant_id, output.remaining);
        }
        for (id, remaining) in final_budgets {
            let grant = &spend.grants[&id];
            if remaining != grant.record.remaining_debit_budget {
                let mut record = grant.record.clone();
                record.remaining_debit_budget = remaining;
                changes.extend(
                    crate::native_pipeline::business::uca_spend_state::grant_record_changes(
                        &grant.intent,
                        &record,
                    )?,
                );
            }
        }
    }
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
    digest.update(input.plan().context().receipt_codec);
    digest.update((receipts.len() as u64).to_be_bytes());
    for receipt in &receipts {
        let bytes = postcard::to_allocvec(receipt)?;
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    if let Some(outputs) = &spend_outputs {
        digest.update(b"uca-authorized-effects/v1\0");
        for output in outputs {
            digest.update(super::uca_effects::authorization_code(output.failure).to_be_bytes());
            digest.update(output.remaining.to_be_bytes());
        }
    }
    let receipt_batch_commitment: NodeHash = digest.finalize().into();
    let apfl_view_transactions = input.apfl_count();
    let effects = {
        let _span = crate::native_pipeline::profile::span(
            crate::native_pipeline::profile::Phase::StateTree,
        );
        input.stage(&changes)?
    };
    let mut digest = Sha256::new();
    digest.update(b"novovm/replacement/nov-executed-batch/v1\0");
    digest.update(effects.plan_commitment());
    digest.update(effects.update().root());
    digest.update(receipt_batch_commitment);
    Ok(ExecutedNovBatch {
        spend_outputs,
        effects,
        receipts,
        fees,
        receipt_batch_commitment,
        statement_commitment: digest.finalize().into(),
        observation: ExecutionObservation {
            apfl_view_transactions,
            components: prepared.components.len(),
            credit_only_accounts: prepared.credit_only.len(),
            recomputed_transactions: recomputed,
            gpu_rows: 0,
            gpu_waves: 0,
            peak_callbacks: 0,
        },
    })
}
