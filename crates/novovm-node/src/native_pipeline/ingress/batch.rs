//! Batch signature work runs on AOEM, then binds unchanged raw bytes and verified
//! identities to a structural plan. It does not reserve nonce, quote fees, admit
//! a mempool entry or mint a state/finality certificate.

use super::apfl::ApflTransferBatch;
use super::authentication::{
    authenticate_apfl_row, authenticate_transfer_v3, SignatureCheckedTransfer,
};
use crate::native_pipeline::execution::plan::{
    BatchContext, BatchPlan, BoundBatchCapture, OwnedBatchInput, PlanBudget,
    UnpublishedBatchEffects,
};
use crate::native_pipeline::state::frontier::{CaptureBudget, CaptureStep, DeclaredAccess};
use crate::native_pipeline::state::tree::{NodeHash, StateChange, StateNodeReader};
use anyhow::{ensure, Context, Result};
use novovm_exec::resident::{ComputeSession, ComputeTask};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy)]
pub struct AuthenticationBudget {
    pub transactions: usize,
    pub transaction_bytes: usize,
    pub body_bytes: usize,
}

/// Structured input is immutable but untrusted. Bounds are charged against the
/// expanded canonical body, not the compressed byte count.
#[derive(Debug)]
pub(crate) enum BatchSource {
    Raw(Vec<Vec<u8>>),
    Apfl(Arc<ApflTransferBatch>),
}

impl BatchSource {
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Raw(raw) => raw.len(),
            Self::Apfl(batch) => batch.len(),
        }
    }

    pub(crate) fn sizes(&self) -> Result<(usize, usize)> {
        match self {
            Self::Raw(raw) => {
                let mut total = 0usize;
                let mut max = 0usize;
                for row in raw {
                    ensure!(!row.is_empty(), "empty pipeline transaction");
                    total = total
                        .checked_add(row.len())
                        .context("pipeline body size overflow")?;
                    max = max.max(row.len());
                }
                Ok((total, max))
            }
            Self::Apfl(batch) => Ok((batch.canonical_bytes(), batch.max_transaction_bytes())),
        }
    }

    fn validate(&self, chain: u64, budget: AuthenticationBudget) -> Result<()> {
        ensure!(chain != 0, "configured chain id must be nonzero");
        let (bytes, max) = self.sizes()?;
        ensure!(
            !self.is_empty()
                && self.len() <= budget.transactions
                && max <= budget.transaction_bytes
                && bytes <= budget.body_bytes,
            "signature batch exceeds input budget"
        );
        Ok(())
    }

    fn authenticate(
        &self,
        index: usize,
        chain: u64,
        max: usize,
    ) -> Result<SignatureCheckedTransfer> {
        match self {
            Self::Raw(raw) => authenticate_transfer_v3(&raw[index], chain, max),
            Self::Apfl(batch) => authenticate_apfl_row(Arc::clone(batch), index, chain),
        }
    }

    pub(crate) fn into_raw(self) -> Result<Vec<Vec<u8>>> {
        match self {
            Self::Raw(raw) => Ok(raw),
            Self::Apfl(batch) => (0..batch.len()).map(|i| batch.canonical_raw(i)).collect(),
        }
    }
}

impl From<Vec<Vec<u8>>> for BatchSource {
    fn from(raw: Vec<Vec<u8>>) -> Self {
        Self::Raw(raw)
    }
}

/// Unverified admission row. Keeping a structured row does not confer the
/// signature-checked capability; only the shared verifier below creates it.
pub enum AdmissionInput {
    Raw(Vec<u8>),
    Apfl {
        batch: Arc<ApflTransferBatch>,
        index: usize,
    },
}

impl AdmissionInput {
    pub fn encoded_len(&self) -> Result<usize> {
        match self {
            Self::Raw(raw) => Ok(raw.len()),
            Self::Apfl { batch, index } => batch.row(*index)?.encoded_len(),
        }
    }

    /// Preserve raw allocations; project just this structured row when needed.
    pub fn into_raw(self) -> Result<Vec<u8>> {
        match self {
            Self::Raw(raw) => Ok(raw),
            Self::Apfl { batch, index } => batch.canonical_raw(index),
        }
    }
}

pub struct AdmissionRow {
    pub input: AdmissionInput,
    pub result: Result<SignatureCheckedTransfer>,
}

pub(crate) struct AdmissionRows {
    pub rows: std::collections::VecDeque<AdmissionRow>,
    pub peak_callbacks: usize,
}

trait AuthenticationSource: Send + Sync + 'static {
    fn len(&self) -> usize;
    fn authenticate(
        &self,
        index: usize,
        chain: u64,
        max: usize,
    ) -> Result<SignatureCheckedTransfer>;
}

impl AuthenticationSource for BatchSource {
    fn len(&self) -> usize {
        self.len()
    }
    fn authenticate(
        &self,
        index: usize,
        chain: u64,
        max: usize,
    ) -> Result<SignatureCheckedTransfer> {
        self.authenticate(index, chain, max)
    }
}

impl AuthenticationSource for Vec<AdmissionInput> {
    fn len(&self) -> usize {
        self.len()
    }
    fn authenticate(
        &self,
        index: usize,
        chain: u64,
        max: usize,
    ) -> Result<SignatureCheckedTransfer> {
        match &self[index] {
            AdmissionInput::Raw(raw) => authenticate_transfer_v3(raw, chain, max),
            AdmissionInput::Apfl { batch, index } => {
                authenticate_apfl_row(batch.clone(), *index, chain)
            }
        }
    }
}

pub(crate) fn admission_sizes(inputs: &[AdmissionInput]) -> Result<(usize, usize)> {
    let mut bytes = 0usize;
    let mut max = 0usize;
    for input in inputs {
        let len = input.encoded_len()?;
        bytes = bytes
            .checked_add(len)
            .context("signature admission size overflow")?;
        max = max.max(len);
    }
    Ok((bytes, max))
}

/// A borrowed row retains its whole immutable dictionary owner. Charge each
/// distinct owner once, even if only one of its rows is offered for admission.
pub(crate) fn admission_retained_bytes(inputs: &[AdmissionInput]) -> Result<usize> {
    let mut owners = BTreeSet::new();
    let mut bytes = 0usize;
    for input in inputs {
        let retained = match input {
            AdmissionInput::Raw(raw) => raw.len(),
            AdmissionInput::Apfl { batch, .. } if owners.insert(Arc::as_ptr(batch)) => {
                batch.canonical_bytes()
            }
            AdmissionInput::Apfl { .. } => 0,
        };
        bytes = bytes
            .checked_add(retained)
            .context("signature retained input size overflow")?;
    }
    Ok(bytes)
}

/// Per-row external rejection is data, not owner failure. Duplicate hashes and
/// nonce reservations belong to the serialized RPC pool, not this graph.
pub(crate) fn authenticate_admission_rows(
    session: &mut ComputeSession,
    configured_chain_id: u64,
    inputs: Vec<AdmissionInput>,
    budget: AuthenticationBudget,
    timeout: Duration,
) -> Result<AdmissionRows> {
    let (bytes, max) = admission_sizes(&inputs)?;
    ensure!(
        configured_chain_id != 0,
        "configured chain id must be nonzero"
    );
    ensure!(
        !inputs.is_empty()
            && inputs.len() <= budget.transactions
            && max <= budget.transaction_bytes
            && bytes <= budget.body_bytes,
        "signature admission exceeds input budget"
    );
    let (inputs, results, peak_callbacks) =
        authenticate_graph(session, configured_chain_id, inputs, budget, timeout)?;
    Ok(AdmissionRows {
        rows: inputs
            .into_iter()
            .zip(results)
            .map(|(input, result)| AdmissionRow { input, result })
            .collect(),
        peak_callbacks,
    })
}

// Existing raw-input allocation/ownership tests inspect the exact returned
// allocation. No production accessor can mutate or expand a structured batch.
#[cfg(test)]
impl std::ops::Deref for BatchSource {
    type Target = Vec<Vec<u8>>;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(raw) => raw,
            Self::Apfl(_) => panic!("raw-only test accessor"),
        }
    }
}

#[cfg(test)]
impl std::ops::DerefMut for BatchSource {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Raw(raw) => raw,
            Self::Apfl(_) => panic!("raw-only test accessor"),
        }
    }
}

#[cfg(test)]
impl PartialEq<Vec<Vec<u8>>> for BatchSource {
    fn eq(&self, other: &Vec<Vec<u8>>) -> bool {
        matches!(self, Self::Raw(raw) if raw == other)
    }
}

pub struct SignatureCheckedBatch {
    chain_id: u64,
    raw_transactions: BatchSource,
    transactions: Vec<SignatureCheckedTransfer>,
    peak_callbacks: usize,
}

impl SignatureCheckedBatch {
    /// Immutable authenticated bytes; business preparation may check its own
    /// bounds before a parent context exists. This grants no state authority.
    pub(crate) fn source(&self) -> &BatchSource {
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
        // Canonical projection for existing plan/packet/proof compatibility.
        // Checked rows retain their shared APFL source through compilation.
        let plan = BatchPlan::new(
            context,
            self.raw_transactions.into_raw()?,
            declarations,
            budget,
        )?;
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
    pub(crate) fn seed_hits(&self) -> usize {
        self.capture.seed_hits()
    }

    pub(crate) fn context(&self) -> &BatchContext {
        self.capture.context()
    }

    pub(crate) fn advance_with_seed(
        &mut self,
        max_edge_steps: usize,
        seed: Option<&crate::native_pipeline::state::frontier::PostStateSeed>,
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
    raw_transactions: BatchSource,
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
    finish_authentication(
        configured_chain_id,
        BatchSource::Raw(raw_transactions),
        transactions,
        0,
    )
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
    authenticate_source_batch(
        session,
        configured_chain_id,
        BatchSource::Raw(raw_transactions),
        budget,
        timeout,
    )
}

pub(crate) fn authenticate_source_batch(
    session: &mut ComputeSession,
    configured_chain_id: u64,
    raw_transactions: BatchSource,
    budget: AuthenticationBudget,
    timeout: Duration,
) -> Result<SignatureCheckedBatch> {
    raw_transactions.validate(configured_chain_id, budget)?;
    let (raw_transactions, results, peak_callbacks) = authenticate_graph(
        session,
        configured_chain_id,
        raw_transactions,
        budget,
        timeout,
    )?;
    let transactions = results
        .into_iter()
        .collect::<Result<Vec<_>>>()
        .context("signature batch rejected without state admission")?;
    finish_authentication(
        configured_chain_id,
        raw_transactions,
        transactions,
        peak_callbacks,
    )
}

fn authenticate_graph<S: AuthenticationSource>(
    session: &mut ComputeSession,
    configured_chain_id: u64,
    source: S,
    budget: AuthenticationBudget,
    timeout: Duration,
) -> Result<(S, Vec<Result<SignatureCheckedTransfer>>, usize)> {
    // Share immutable input allocation, not one copy of the whole body per task.
    let body = Arc::new(source);
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
                let authentication =
                    body.authenticate(index, configured_chain_id, budget.transaction_bytes);
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
                Ok(transaction)
            }
            Err(error) => {
                ensure!(output == [0], "authentication rejection result mismatch");
                Err(error)
            }
        };
        transactions.push(transaction);
    }
    let raw_transactions = Arc::try_unwrap(body).map_err(|_| {
        anyhow::anyhow!("authentication callbacks retained batch input after completion")
    })?;
    Ok((raw_transactions, transactions, report.peak_inflight))
}
