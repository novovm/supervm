//! Bounded asynchronous observations on the ORIGINAL immutable-tree I/O owner.
//! Reading a grant does not reserve it or authenticate a spend; execution still
//! rechecks and consumes at its exact parent/prefix in the original plan.
use super::super::{Account, Hash, IoTicket};
use crate::native_pipeline::business::uca_spend_state::{
    AssetSpendStateAccessV2, CapturedAssetSpendStateV2,
};
use crate::native_pipeline::execution::plan::BatchContext;
use crate::native_pipeline::persistence::io::NodeReadReply;
use crate::native_pipeline::pipeline::{CandidatePipeline, ReadSeedLease};
use crate::native_pipeline::state::frontier::{
    BulkCapture, CaptureBudget, CaptureStep, DeclaredAccess, OwnedStateInput,
};
use anyhow::{ensure, Context, Result};
use novovm_consensus::round_bft::journal::ParentPoint;
use novovm_protocol::native_query::{Answer, AssetSpendStateViewV2, AssetState, Query, QueryError};
use novovm_protocol::uca_spend::AssetSpendIntentV2;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

pub(crate) enum UcaReadResult {
    Missing,
    Present(Box<CapturedAssetSpendStateV2>),
}

const MAX_BATCH_ROWS: usize = 1024;
const KEYS_PER_ROW: usize = 80;
const CAPTURE_QUANTUM: usize = 64;
const DECODE_QUANTUM: usize = 8;
const LOCAL_STEPS_PER_TURN: usize = 8;
const LOCAL_TURN_BUDGET: Duration = Duration::from_micros(250);

/// Spend only bounded local progress in this cooperative turn. I/O admission
/// and completion are yield boundaries so another owner gets the next slot.
/// The clock is checked between original steps, not a hard real-time deadline.
fn poll_local_turn<T>(
    mut step: impl FnMut() -> (Result<Option<T>>, bool, bool),
    mut elapsed: impl FnMut() -> Duration,
) -> (Result<Option<T>>, bool, usize) {
    let mut progressed = false;
    let mut local_steps = 0;
    for index in 0..LOCAL_STEPS_PER_TURN {
        let (result, step_progress, io_boundary) = step();
        progressed |= step_progress;
        local_steps += usize::from(step_progress && !io_boundary);
        if !matches!(&result, Ok(None))
            || !step_progress
            || io_boundary
            || index + 1 == LOCAL_STEPS_PER_TURN
            || elapsed() >= LOCAL_TURN_BUDGET
        {
            return (result, progressed, local_steps);
        }
    }
    unreachable!("nonzero local step bound")
}

/// Logical request work, not physical disk reads or unique transactions.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UcaCaptureStats {
    pub(crate) declaration_keys: usize,
    pub(crate) unique_keys: usize,
    pub(crate) io_requests: usize,
    pub(crate) io_nodes: usize,
    pub(crate) seed_nodes: usize,
    pub(crate) poll_turns: usize,
    /// Original steps making local progress, excluding I/O handoff boundaries.
    pub(crate) local_steps: usize,
}

/// One exact-parent captured frontier for an ordered, bounded group of UCA
/// observations. Shared keys and tree nodes are read once, not once per row.
/// Nothing here reserves nonce/budget or grants authority to spend.
pub(crate) struct UcaBatchStateRead {
    root: Hash,
    accesses: Vec<Option<Result<AssetSpendStateAccessV2>>>,
    capture: Option<BulkCapture>,
    input: Option<OwnedStateInput>,
    locality: Option<(ParentPoint, BatchContext, Option<ReadSeedLease>)>,
    keys: Vec<Vec<u8>>,
    values: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    ticket: Option<IoTicket<NodeReadReply>>,
    rows: Vec<Result<UcaReadResult>>,
    stats: UcaCaptureStats,
    made_progress: bool,
    failed: bool,
    completed: bool,
}
impl UcaBatchStateRead {
    pub(crate) fn new(
        root: Hash,
        inputs: Vec<(AssetSpendIntentV2, Account, Account)>,
        budget: CaptureBudget,
    ) -> Result<Self> {
        ensure!(
            !inputs.is_empty() && inputs.len() <= MAX_BATCH_ROWS,
            "UCA read row bound"
        );
        let mut accesses = Vec::with_capacity(inputs.len());
        let mut access_bytes = 0usize;
        for (intent, payer, recipient) in inputs {
            let access = AssetSpendStateAccessV2::new(intent, payer, recipient);
            if let Ok(access) = &access {
                for declaration in access.declarations() {
                    access_bytes = access_bytes
                        .checked_add(declaration.key.len() + std::mem::size_of::<DeclaredAccess>())
                        .context("UCA access retention overflow")?;
                    ensure!(
                        access_bytes <= budget.bytes,
                        "UCA access declaration retention budget"
                    );
                }
            }
            accesses.push(access);
        }
        Self::from_access(root, accesses, budget)
    }

    fn from_access(
        root: Hash,
        accesses: Vec<Result<AssetSpendStateAccessV2>>,
        budget: CaptureBudget,
    ) -> Result<Self> {
        ensure!(root != [0; 32], "UCA read parent missing");
        ensure!(
            !accesses.is_empty() && accesses.len() <= MAX_BATCH_ROWS,
            "UCA read row bound"
        );
        let mut declarations = BTreeMap::<Vec<u8>, DeclaredAccess>::new();
        let mut stats = UcaCaptureStats::default();
        let mut key_bytes = 0usize;
        for access in &accesses {
            let Ok(access) = access else {
                continue;
            };
            ensure!(
                access.declarations().len() <= KEYS_PER_ROW,
                "UCA read declaration bound"
            );
            stats.declaration_keys += access.declarations().len();
            for declaration in access.declarations() {
                ensure!(
                    !declaration.may_put && !declaration.may_delete,
                    "UCA observation must be read-only"
                );
                if !declarations.contains_key(&declaration.key) {
                    key_bytes = key_bytes
                        .checked_add(declaration.key.len() + 257)
                        .context("UCA read retention overflow")?;
                    ensure!(
                        declarations.len() < budget.keys && key_bytes <= budget.bytes,
                        "UCA read declaration retention budget"
                    );
                    declarations.insert(declaration.key.clone(), declaration.clone());
                }
            }
        }
        let declarations: Vec<_> = declarations.into_values().collect();
        stats.unique_keys = declarations.len();
        let keys = declarations.iter().map(|d| d.key.clone()).collect();
        let capture = if declarations.is_empty() {
            None
        } else {
            Some(BulkCapture::new(root, &declarations, budget)?)
        };
        Ok(Self {
            root,
            accesses: accesses.into_iter().map(Some).collect(),
            capture,
            input: None,
            locality: None,
            keys,
            values: BTreeMap::new(),
            ticket: None,
            rows: Vec::new(),
            stats,
            made_progress: false,
            failed: false,
            completed: false,
        })
    }

    pub(crate) fn stats(&self) -> UcaCaptureStats {
        self.stats
    }

    /// The caller supplies the controller's full published parent, never an
    /// external query's claimed root. The seed carries no authorization result.
    pub(crate) fn with_read_locality(
        mut self,
        pipeline: &CandidatePipeline,
        parent: ParentPoint,
        mut context: BatchContext,
    ) -> Result<Self> {
        ensure!(
            self.root == parent.state_root,
            "UCA locality parent mismatch"
        );
        context.height = parent
            .height
            .checked_add(1)
            .context("UCA height overflow")?;
        context.parent_height = parent.height;
        context.parent_block_hash = parent.block_hash;
        context.parent_state_root = parent.state_root;
        context.parent_receipt_root = parent.receipt_batch_commitment;
        context.parent_state_version = parent.state_version;
        let seed = pipeline.capture_read_seed(context, parent);
        self.locality = Some((parent, context, seed));
        Ok(self)
    }

    pub(crate) fn made_progress(&self) -> bool {
        self.made_progress
    }

    pub(crate) fn poll(
        &mut self,
        pipeline: &CandidatePipeline,
    ) -> Result<Option<Vec<Result<UcaReadResult>>>> {
        ensure!(
            !self.completed && !self.failed,
            "UCA capture already consumed or failed"
        );
        let started = Instant::now();
        let (result, progressed, local_steps) = poll_local_turn(
            || {
                let had_ticket = self.ticket.is_some();
                self.made_progress = false;
                let result = self.poll_inner(pipeline);
                (
                    result,
                    self.made_progress,
                    had_ticket || self.ticket.is_some(),
                )
            },
            || started.elapsed(),
        );
        self.made_progress = progressed;
        self.stats.poll_turns = self.stats.poll_turns.saturating_add(1);
        self.stats.local_steps = self.stats.local_steps.saturating_add(local_steps);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn poll_inner(
        &mut self,
        pipeline: &CandidatePipeline,
    ) -> Result<Option<Vec<Result<UcaReadResult>>>> {
        if let Some(capture) = self.capture.as_mut() {
            if let Some(ticket) = self.ticket.as_mut() {
                let Some(values) = ticket.try_take()? else {
                    return Ok(None);
                };
                capture.accept(values)?;
                self.ticket = None;
                self.made_progress = true;
                return Ok(None);
            }
            let seed = self.locality.as_ref().and_then(|(parent, context, lease)| {
                lease
                    .as_ref()
                    .and_then(|lease| pipeline.read_seed_material(lease, context, parent))
            });
            let step = capture.advance_with_seed(CAPTURE_QUANTUM, seed)?;
            self.stats.seed_nodes = capture.seed_hits();
            match step {
                CaptureStep::More => {
                    self.made_progress = true;
                    return Ok(None);
                }
                CaptureStep::NeedRead => {
                    let hashes = capture
                        .next_request()?
                        .context("UCA capture needs read without hashes")?;
                    let count = hashes.len();
                    if let Some(ticket) = pipeline.try_read_state_nodes(hashes)? {
                        self.stats.io_requests += 1;
                        self.stats.io_nodes += count;
                        self.ticket = Some(ticket);
                        self.made_progress = true;
                    }
                    return Ok(None);
                }
                CaptureStep::Complete => {
                    let input = self.capture.take().expect("capture present").finish()?;
                    ensure!(
                        input.parent_root() == self.root,
                        "UCA capture root mismatch"
                    );
                    self.input = Some(input);
                    self.made_progress = true;
                }
            }
        }
        self.poll_captured(Some(pipeline))
    }

    fn poll_captured(
        &mut self,
        pipeline: Option<&CandidatePipeline>,
    ) -> Result<Option<Vec<Result<UcaReadResult>>>> {
        // State-tree traversal here is memory-only, bounded per poll. Every
        // logical key is resolved once from the already authenticated frontier.
        if self.values.len() < self.keys.len() {
            let start = self.values.len();
            let end = (start + CAPTURE_QUANTUM).min(self.keys.len());
            let keys = &self.keys[start..end];
            let values = self
                .input
                .as_ref()
                .context("UCA capture input missing")?
                .read_many(keys)?;
            ensure!(values.len() == keys.len(), "UCA capture value count");
            for (key, value) in keys.iter().zip(values) {
                ensure!(
                    value.as_ref().is_none_or(|v| v.len() <= 256),
                    "UCA read value bound"
                );
                self.values.insert(key.clone(), value);
            }
            self.made_progress = true;
            if end == self.keys.len() {
                // Release the borrowed old seed before optionally moving these
                // authenticated nodes into the SAME pipeline locality slot.
                // No old read permissions or per-row approval is retained.
                let input = self.input.take();
                if let (Some(pipeline), Some(input), Some((parent, context, lease))) =
                    (pipeline, input, self.locality.as_mut())
                {
                    *lease = None;
                    pipeline.retain_capture_read_seed(input, *context, *parent)?;
                }
            }
            return Ok(None);
        }
        // Preserve row-local record errors and exact row order. Tail-page,
        // canonical encoding and binding checks remain in the original reader.
        let end = (self.rows.len() + DECODE_QUANTUM).min(self.accesses.len());
        for index in self.rows.len()..end {
            let access = self.accesses[index]
                .take()
                .context("UCA row already decoded")?;
            self.rows.push(access.and_then(|access| {
                access
                    .read_optional_with(self.root, |key| {
                        self.values.get(key).cloned().context("UCA read incomplete")
                    })
                    .map(|state| match state {
                        Some(state) => UcaReadResult::Present(Box::new(state)),
                        None => UcaReadResult::Missing,
                    })
            }));
            self.made_progress = true;
        }
        if self.rows.len() != self.accesses.len() {
            return Ok(None);
        }
        self.input = None;
        self.locality = None;
        self.completed = true;
        Ok(Some(std::mem::take(&mut self.rows)))
    }
}

/// Single public-query observation uses the exact same captured implementation,
/// not a second point-read path or an authoritative state cache.
pub(crate) struct AssetStateRead {
    root: Hash,
    access: Option<AssetSpendStateAccessV2>,
    batch: Option<UcaBatchStateRead>,
    completed: bool,
}
impl AssetStateRead {
    pub(crate) fn new(
        root: Hash,
        intent: AssetSpendIntentV2,
        payer: Account,
        recipient: Account,
    ) -> Result<Self> {
        ensure!(root != [0; 32], "UCA read parent missing");
        Ok(Self {
            root,
            access: Some(AssetSpendStateAccessV2::new(intent, payer, recipient)?),
            batch: None,
            completed: false,
        })
    }

    pub(crate) fn poll(&mut self, pipeline: &CandidatePipeline) -> Result<Option<UcaReadResult>> {
        ensure!(!self.completed, "UCA read already consumed");
        if self.batch.is_none() {
            self.batch = Some(UcaBatchStateRead::from_access(
                self.root,
                vec![Ok(self.access.take().context("UCA read access missing")?)],
                pipeline.capture_budget(),
            )?);
        }
        let Some(mut rows) = self.batch.as_mut().expect("batch present").poll(pipeline)? else {
            return Ok(None);
        };
        self.completed = true;
        ensure!(rows.len() == 1, "UCA single observation shape");
        rows.pop().expect("one row").map(Some)
    }
}

struct Entry {
    query: Query,
    root: Hash,
    height: u64,
    expires: Instant,
    read: AssetStateRead,
    answer: Option<Answer>,
}

#[derive(Default)]
pub(crate) struct UcaQueries(VecDeque<Entry>);
impl UcaQueries {
    pub(super) fn poll(&mut self, pipeline: &CandidatePipeline, now: Instant) {
        self.0.retain(|e| now < e.expires);
        // As with history reads, never reclaim a released I/O slot in this
        // turn: admission/nonce hydration must get the next opportunity.
        if let Some(index) = self.0.iter().position(|entry| entry.answer.is_none()) {
            let mut entry = self.0.remove(index).unwrap();
            entry.answer = match entry.read.poll(pipeline) {
                Ok(None) => None,
                Err(_) => Some(Answer::Error(QueryError::Unavailable)),
                Ok(Some(UcaReadResult::Missing)) => {
                    Some(Answer::AssetSpendState(AssetState::Missing {
                        state_root: entry.root,
                        height: entry.height,
                    }))
                }
                Ok(Some(UcaReadResult::Present(state))) => Some(Answer::AssetSpendState(
                    AssetState::Present(Box::new(view(&state, entry.height))),
                )),
            };
            self.0.push_back(entry);
        }
    }

    pub(super) fn read(&mut self, query: Query, root: Hash, height: u64, now: Instant) -> Answer {
        if let Some(index) = self.0.iter().position(|entry| entry.query == query) {
            return if self.0[index].answer.is_some() {
                self.0.remove(index).unwrap().answer.unwrap()
            } else {
                Answer::AssetSpendState(AssetState::Pending)
            };
        }
        if self.0.len() >= 8 {
            return Answer::Error(QueryError::Busy);
        }
        let Query::AssetSpendState { intent } = &query else {
            return Answer::Error(QueryError::Unavailable);
        };
        let read = (|| {
            let intent = intent.intent();
            let payer = Account::try_from(intent.payer.as_slice()).map_err(anyhow::Error::msg)?;
            let recipient =
                Account::try_from(intent.recipient.as_slice()).map_err(anyhow::Error::msg)?;
            AssetStateRead::new(root, intent, payer, recipient)
        })();
        match read {
            Err(_) => Answer::Error(QueryError::Unavailable),
            Ok(read) => {
                self.0.push_back(Entry {
                    query,
                    root,
                    height,
                    expires: now + Duration::from_secs(30),
                    read,
                    answer: None,
                });
                Answer::AssetSpendState(AssetState::Pending)
            }
        }
    }
}

fn view(state: &CapturedAssetSpendStateV2, height: u64) -> AssetSpendStateViewV2 {
    let grant = state.grant();
    let (payer_balance, recipient_balance) = state.balances();
    AssetSpendStateViewV2 {
        state_root: state.parent_root(),
        height,
        account_active: state.payer().active,
        current_spend_epoch: state.payer().current_spend_epoch,
        management_key_ref: state.payer().management_key_ref,
        recovery_key_ref: state.payer().recovery_key_ref,
        grant_present: grant.is_some(),
        grant_epoch: grant.map_or(0, |grant| grant.authorization_epoch),
        permits_nov_transfer: grant.is_some_and(|grant| grant.permits_nov_transfer),
        revoked: grant.is_some_and(|grant| grant.revoked),
        not_before_height: grant.map_or(0, |grant| grant.not_before_height),
        expires_at_height: grant.map_or(0, |grant| grant.expires_at_height),
        max_debit_per_intent: grant.map_or(0, |grant| grant.max_debit_per_intent),
        remaining_debit_budget: grant.map_or(0, |grant| grant.remaining_debit_budget),
        next_nonce: state.next_nonce(),
        payer_balance,
        recipient_balance,
    }
}

#[cfg(test)]
mod tests;
