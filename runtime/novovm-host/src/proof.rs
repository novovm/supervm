//! Complete direct-NOV business relation for a separately trusted zkVM image.
//!
//! This is NOT a second native executor or a proof/finality certificate. The
//! native path retains AOEM component scheduling. The guest checks signatures,
//! derives access declarations, authenticates the parent frontier and runs the
//! SAME economic reducer/tree update, exporting only public commitments.
//!
//! The verifier must independently choose both its trusted image and expected
//! journal (exact raw plan/config/authoritative parent and candidate outputs).
//! Proving a transition from a claimed parent cannot make that parent canonical.
//! This relation does not bind the physical candidate document or its location,
//! and does not by itself enable a mainchain proof policy.

use crate::business::direct_nov_fee::DirectNovFeePolicy;
use crate::business::nov_transfer_batch::{ExecutedNovBatch, NovTransferPlan};
use crate::execution::plan::{BatchContext, BatchPlan, PlanBudget};
use crate::ingress::batch::{authenticate_batch_for_proof, AuthenticationBudget};
use crate::state::frontier::CaptureBudget;
use crate::state::tree::NodeHash;
use anyhow::{ensure, Context, Result};

mod wire;

/// Guest resource profile, NOT a production block-size or proof-mandatory rule.
/// Larger native batches are not silently split, accepted or claimed proven.
// Four bytes of guest stdin length framing also count toward the AOEM ABI cap.
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024 - 4;
pub const MAX_TRANSACTIONS: usize = 1024;
pub const MAX_TRANSACTION_BYTES: usize = 64 * 1024;
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_POLICY_BYTES: usize = 2048;
pub const JOURNAL_BYTES: usize = 8 + 4 * 32 + 2 * 8;
const JOURNAL_MAGIC: &[u8; 8] = b"NVEXEC01";

pub(crate) fn capture_budget() -> CaptureBudget {
    CaptureBudget {
        keys: 4096,
        nodes: 16_384,
        bytes: 5 * 1024 * 1024,
    }
}

fn auth_budget() -> AuthenticationBudget {
    AuthenticationBudget {
        transactions: MAX_TRANSACTIONS,
        transaction_bytes: MAX_TRANSACTION_BYTES,
        body_bytes: MAX_BODY_BYTES,
    }
}

fn plan_budget() -> PlanBudget {
    PlanBudget {
        transactions: MAX_TRANSACTIONS,
        transaction_bytes: MAX_TRANSACTION_BYTES,
        body_bytes: MAX_BODY_BYTES,
        access_keys: capture_budget().keys,
    }
}

/// Expected public bytes, NOT evidence that a prover or AOEM was called.
/// No constructor accepts arbitrary caller-provided output roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionJournalV1 {
    plan: NodeHash,
    post_state: NodeHash,
    receipts: NodeHash,
    execution: NodeHash,
    transactions: u64,
    state_version: u64,
}

impl ExecutionJournalV1 {
    pub fn from_executed(batch: &ExecutedNovBatch) -> Result<Self> {
        let transactions = u64::try_from(batch.receipts().len())?;
        let state_version = batch
            .effects()
            .context()
            .parent_state_version
            .checked_add(transactions)
            .context("execution proof state version overflow")?;
        Ok(Self {
            plan: batch.effects().plan_commitment(),
            post_state: batch.effects().update().root(),
            receipts: batch.receipt_batch_commitment(),
            execution: batch.statement_commitment(),
            transactions,
            state_version,
        })
    }

    pub fn encode(&self) -> [u8; JOURNAL_BYTES] {
        let mut out = [0; JOURNAL_BYTES];
        out[..8].copy_from_slice(JOURNAL_MAGIC);
        for (index, hash) in [self.plan, self.post_state, self.receipts, self.execution]
            .iter()
            .enumerate()
        {
            out[8 + index * 32..8 + (index + 1) * 32].copy_from_slice(hash);
        }
        out[136..144].copy_from_slice(&self.transactions.to_be_bytes());
        out[144..152].copy_from_slice(&self.state_version.to_be_bytes());
        out
    }
}

pub(crate) fn encode_input(
    plan: &BatchPlan,
    policy: &DirectNovFeePolicy,
    witness: &[u8],
) -> Result<Vec<u8>> {
    wire::encode(plan, policy, witness)
}

/// Run in the guest/proof worker only. It returns bytes, never an ExecutedNovBatch
/// that could masquerade as native AOEM execution in candidate persistence.
pub fn execute_to_journal(input: &[u8]) -> Result<ExecutionJournalV1> {
    let wire::Input {
        context,
        raw,
        policy,
        witness,
    } = wire::decode(input)?;
    let authenticated = authenticate_batch_for_proof(context.chain_id, raw, auth_budget())?;
    let plan = NovTransferPlan::compile(authenticated, context, policy, plan_budget())?;
    let batch = plan
        .capture_witness(witness, capture_budget())?
        .execute_for_proof()?;
    ExecutionJournalV1::from_executed(&batch)
}

#[cfg(test)]
mod tests;
