//! Batch signature work runs on AOEM, then binds unchanged raw bytes and verified
//! identities to a structural plan. It does not reserve nonce, quote fees, admit
//! a mempool entry or mint a state/finality certificate.

use super::authentication::{authenticate_transfer_v3, SignatureCheckedTransfer};
use crate::execution::plan::{
    BatchContext, BatchPlan, BoundBatchCapture, OwnedBatchInput, PlanBudget,
    UnpublishedBatchEffects,
};
use crate::state::frontier::{CaptureBudget, CaptureStep, DeclaredAccess};
use crate::state::tree::{NodeHash, StateChange, StateNodeReader};
use anyhow::{ensure, Context, Result};
#[cfg(feature = "native")]
use novovm_aoem::{ComputeSession, ComputeTask};
use std::collections::BTreeSet;
#[cfg(feature = "native")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "native")]
use std::time::Duration;

#[derive(Clone, Copy)]
pub struct AuthenticationBudget {
    pub transactions: usize,
    pub transaction_bytes: usize,
    pub body_bytes: usize,
}

pub struct SignatureCheckedBatch {
    chain_id: u64,
    raw_transactions: Vec<Vec<u8>>,
    transactions: Vec<SignatureCheckedTransfer>,
    peak_callbacks: usize,
}

impl SignatureCheckedBatch {
    /// Immutable authenticated bytes; business preparation may check its own
    /// bounds before a parent context exists. This grants no state authority.
    pub(crate) fn raw_transactions(&self) -> &[Vec<u8>] {
        &self.raw_transactions
    }

    pub fn transactions(&self) -> &[SignatureCheckedTransfer] {
        &self.transactions
    }

    /// Observed authentication callback overlap, never a mainchain TPS measure.
    pub fn peak_callbacks(&self) -> usize {
        self.peak_callbacks
    }

    /// The business compiler still must derive COMPLETE access/effect declarations
    /// and validate the pinned program. Supplied declarations are not verified here.
    pub fn bind(
        self,
        context: BatchContext,
        declarations: Vec<DeclaredAccess>,
        budget: PlanBudget,
    ) -> Result<SignatureCheckedPlan> {
        ensure!(
            context.chain_id == self.chain_id,
            "authenticated batch chain cannot be replaced"
        );
        let plan = BatchPlan::new(context, self.raw_transactions, declarations, budget)?;
        Ok(SignatureCheckedPlan {
            plan,
            transactions: self.transactions,
        })
    }
}

pub struct SignatureCheckedPlan {
    plan: BatchPlan,
    transactions: Vec<SignatureCheckedTransfer>,
}

impl SignatureCheckedPlan {
    pub fn plan(&self) -> &BatchPlan {
        &self.plan
    }

    pub fn capture(
        self,
        reader: &dyn StateNodeReader,
        budget: CaptureBudget,
    ) -> Result<SignatureCheckedInput> {
        Ok(SignatureCheckedInput {
            input: self.plan.capture(reader, budget)?,
            transactions: self.transactions,
        })
    }

    pub(crate) fn capture_witness(
        self,
        wire: &[u8],
        budget: CaptureBudget,
    ) -> Result<SignatureCheckedInput> {
        Ok(SignatureCheckedInput {
            input: self.plan.capture_witness(wire, budget)?,
            transactions: self.transactions,
        })
    }

    /// Incremental capture owns this exact plan and authenticated metadata.
    /// There is no API for attaching an unrelated prebuilt state witness.
    pub fn begin_capture(self, budget: CaptureBudget) -> Result<SignatureCheckedCapture> {
        Ok(SignatureCheckedCapture {
            capture: self.plan.begin_capture(budget)?,
            transactions: self.transactions,
        })
    }
}

pub struct SignatureCheckedCapture {
    capture: BoundBatchCapture,
    transactions: Vec<SignatureCheckedTransfer>,
}

impl SignatureCheckedCapture {
    #[cfg(feature = "native")]
    pub(crate) fn seed_hits(&self) -> usize {
        self.capture.seed_hits()
    }

    #[cfg(feature = "native")]
    pub(crate) fn context(&self) -> &BatchContext {
        self.capture.context()
    }

    #[cfg(feature = "native")]
    pub(crate) fn advance_with_seed(
        &mut self,
        max_edge_steps: usize,
        seed: Option<&crate::state::frontier::PostStateSeed>,
    ) -> Result<CaptureStep> {
        self.capture.advance_with_seed(max_edge_steps, seed)
    }

    pub fn advance(&mut self, max_edge_steps: usize) -> Result<CaptureStep> {
        self.capture.advance(max_edge_steps)
    }

    pub fn next_request(&mut self) -> Result<Option<Vec<NodeHash>>> {
        self.capture.next_request()
    }

    pub fn accept(&mut self, values: Vec<Option<Vec<u8>>>) -> Result<()> {
        self.capture.accept(values)
    }

    pub fn finish(self) -> Result<SignatureCheckedInput> {
        Ok(SignatureCheckedInput {
            input: self.capture.finish()?,
            transactions: self.transactions,
        })
    }
}

/// Owns the exact verified body, metadata, declared inputs and claimed parent.
/// Signature verification does not make the claimed parent authoritative.
pub struct SignatureCheckedInput {
    input: OwnedBatchInput,
    transactions: Vec<SignatureCheckedTransfer>,
}

impl SignatureCheckedInput {
    pub fn plan(&self) -> &BatchPlan {
        self.input.plan()
    }
    pub fn transactions(&self) -> &[SignatureCheckedTransfer] {
        &self.transactions
    }
    pub fn read(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.input.read(key)
    }

    pub(crate) fn read_many(&self, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>> {
        self.input.read_many(keys)
    }

    pub(crate) fn encode_witness(&self, budget: CaptureBudget) -> Result<Vec<u8>> {
        self.input.encode_witness(budget)
    }

    /// Tentative effects only; signature-checked inputs do not prove arbitrary
    /// patches implement the business program, settlement, or nonce transitions.
    pub fn stage(self, changes: &[StateChange]) -> Result<UnpublishedBatchEffects> {
        self.input.stage(changes)
    }
}

/// Shared preflight bounds for native graph admission and proof execution.
fn validate_authentication_input(
    configured_chain_id: u64,
    raw_transactions: &[Vec<u8>],
    budget: AuthenticationBudget,
) -> Result<()> {
    ensure!(
        configured_chain_id != 0,
        "configured chain id must be nonzero"
    );
    ensure!(
        !raw_transactions.is_empty() && raw_transactions.len() <= budget.transactions,
        "authentication batch empty or transaction budget exceeded"
    );
    let mut bytes = 0usize;
    for raw in raw_transactions {
        ensure!(
            !raw.is_empty() && raw.len() <= budget.transaction_bytes,
            "authentication transaction byte budget exceeded"
        );
        bytes = bytes
            .checked_add(raw.len())
            .context("authentication body length overflow")?;
        ensure!(
            bytes <= budget.body_bytes,
            "authentication body byte budget exceeded"
        );
    }
    Ok(())
}

fn finish_authentication(
    configured_chain_id: u64,
    raw_transactions: Vec<Vec<u8>>,
    transactions: Vec<SignatureCheckedTransfer>,
    peak_callbacks: usize,
) -> Result<SignatureCheckedBatch> {
    ensure!(
        transactions.len() == raw_transactions.len(),
        "authentication result count mismatch"
    );
    let mut seen = BTreeSet::new();
    for transaction in &transactions {
        ensure!(
            seen.insert(transaction.tx_hash()),
            "duplicate canonical transaction in authenticated batch"
        );
    }
    Ok(SignatureCheckedBatch {
        chain_id: configured_chain_id,
        raw_transactions,
        transactions,
        peak_callbacks,
    })
}

/// Proof execution uses the same bounds, cryptographic admission and duplicate
/// rejection as native admission. This is not an AOEM execution observation.
pub(crate) fn authenticate_batch_for_proof(
    configured_chain_id: u64,
    raw_transactions: Vec<Vec<u8>>,
    budget: AuthenticationBudget,
) -> Result<SignatureCheckedBatch> {
    validate_authentication_input(configured_chain_id, &raw_transactions, budget)?;
    let transactions = raw_transactions
        .iter()
        .map(|raw| authenticate_transfer_v3(raw, configured_chain_id, budget.transaction_bytes))
        .collect::<Result<Vec<_>>>()?;
    finish_authentication(configured_chain_id, raw_transactions, transactions, 0)
}

/// Blocking work for the designated compute owner, not the network/control loop.
/// All input bounds are checked before graph admission. One failed signature
/// returns no accepted batch; it does not mutate state or consume a nonce.
#[cfg(feature = "native")]
pub fn authenticate_batch(
    session: &mut ComputeSession,
    configured_chain_id: u64,
    raw_transactions: Vec<Vec<u8>>,
    budget: AuthenticationBudget,
    timeout: Duration,
) -> Result<SignatureCheckedBatch> {
    validate_authentication_input(configured_chain_id, &raw_transactions, budget)?;
    // Share immutable input allocation, not one copy of the whole body per task.
    let body = Arc::new(raw_transactions);
    let slots: Vec<_> = (0..body.len())
        .map(|_| Arc::new(Mutex::new(None)))
        .collect();
    let tasks: Vec<ComputeTask> = slots
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            let slot = Arc::clone(slot);
            let body = Arc::clone(&body);
            Box::new(move || {
                // Invalid external input is a normal admission decision, not an
                // AOEM infrastructure failure. Returning it as a task error would
                // let any bad signature permanently poison the compute owner.
                let authentication = authenticate_transfer_v3(
                    &body[index],
                    configured_chain_id,
                    budget.transaction_bytes,
                );
                let output = match &authentication {
                    Ok(transaction) => [vec![1], transaction.tx_hash().to_vec()].concat(),
                    Err(_) => vec![0],
                };
                *slot
                    .lock()
                    .map_err(|_| anyhow::anyhow!("authentication result lock poisoned"))? =
                    Some(authentication);
                Ok(output)
            }) as ComputeTask
        })
        .collect();
    let report = session.execute(tasks, timeout)?;
    ensure!(
        report.outputs.len() == body.len(),
        "authentication output count mismatch"
    );
    let mut transactions = Vec::with_capacity(body.len());
    for (slot, output) in slots.into_iter().zip(report.outputs) {
        let authentication = slot
            .lock()
            .map_err(|_| anyhow::anyhow!("authentication result lock poisoned"))?
            .take()
            .context("missing authentication output")?;
        let transaction = match authentication {
            Ok(transaction) => {
                ensure!(
                    output.len() == 33 && output[0] == 1 && output[1..] == transaction.tx_hash(),
                    "authentication task result mismatch"
                );
                transaction
            }
            Err(error) => {
                ensure!(output == [0], "authentication rejection result mismatch");
                return Err(error.context("signature batch rejected without state admission"));
            }
        };
        transactions.push(transaction);
    }
    let raw_transactions = Arc::try_unwrap(body).map_err(|_| {
        anyhow::anyhow!("authentication callbacks retained batch input after completion")
    })?;
    finish_authentication(
        configured_chain_id,
        raw_transactions,
        transactions,
        report.peak_inflight,
    )
}
