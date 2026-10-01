//! Batch signature work runs on AOEM, then binds unchanged raw bytes and verified
//! identities to a structural plan. It does not reserve nonce, quote fees, admit
//! a mempool entry or mint a state/finality certificate.

use super::authentication::{authenticate_transfer_v3, SignatureCheckedTransfer};
use crate::execution::plan::{
    BatchContext, BatchPlan, OwnedBatchInput, PlanBudget, UnpublishedBatchEffects,
};
use crate::state::frontier::{CaptureBudget, DeclaredAccess};
use crate::state::tree::{StateChange, StateNodeReader};
use anyhow::{ensure, Context, Result};
use novovm_aoem::{ComputeSession, ComputeTask};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
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

    /// Tentative effects only; signature-checked inputs do not prove arbitrary
    /// patches implement the business program, settlement, or nonce transitions.
    pub fn stage(self, changes: &[StateChange]) -> Result<UnpublishedBatchEffects> {
        self.input.stage(changes)
    }
}

/// Blocking work for the designated compute owner, not the network/control loop.
/// All input bounds are checked before graph admission. One failed signature
/// returns no accepted batch; it does not mutate state or consume a nonce.
pub fn authenticate_batch(
    session: &mut ComputeSession,
    configured_chain_id: u64,
    raw_transactions: Vec<Vec<u8>>,
    budget: AuthenticationBudget,
    timeout: Duration,
) -> Result<SignatureCheckedBatch> {
    ensure!(
        configured_chain_id != 0,
        "configured chain id must be nonzero"
    );
    ensure!(
        !raw_transactions.is_empty() && raw_transactions.len() <= budget.transactions,
        "authentication batch empty or transaction budget exceeded"
    );
    let mut bytes = 0usize;
    for raw in &raw_transactions {
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
    let mut seen = BTreeSet::new();
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
        ensure!(
            seen.insert(transaction.tx_hash()),
            "duplicate canonical transaction in authenticated batch"
        );
        transactions.push(transaction);
    }
    let raw_transactions = Arc::try_unwrap(body).map_err(|_| {
        anyhow::anyhow!("authentication callbacks retained batch input after completion")
    })?;
    Ok(SignatureCheckedBatch {
        chain_id: configured_chain_id,
        raw_transactions,
        transactions,
        peak_callbacks: report.peak_inflight,
    })
}
