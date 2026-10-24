//! Assembly/execution handoff for the original native node. Adapted from
//! keep_body/submit_executions/poll_executions at 58e3aade, without its Relay
//! controller. RoundProgress owns consensus; CandidatePipeline owns actual
//! authentication, algebraic execution and persistence. No business loop here.
use super::{
    progress::RoundProgress,
    transport::{self, DecodeLimits, Message},
    wire::Hash,
};
use crate::native_pipeline::{
    business::{
        direct_nov_fee::DirectNovFeePolicy,
        nov_transfer_batch::{
            effect_contract, program_id, receipt_codec, uca_effect_contract, uca_program_id,
            uca_receipt_codec, SEMANTIC_VERSION,
        },
    },
    execution::plan::BatchContext,
    pipeline::{
        AdmittedTransfer, BatchRequest, CandidatePipeline, DurableCandidate, FailureStage,
        PipelineFailure, PipelineTicket, Submission,
    },
};
use anyhow::{ensure, Context, Result};
use std::{collections::BTreeMap, sync::Arc};

mod successor;
use successor::Lookahead;
pub use successor::SuccessorSelection;

#[derive(Clone, Copy)]
pub struct BodyLimits {
    pub bodies: usize,
    pub bytes: usize,
    pub inflight: usize,
}

enum Work {
    Pending(Option<BatchRequest>),
    Authenticated(Option<crate::native_pipeline::pipeline::AuthenticatedRequest>),
    Running(PipelineTicket),
    Ready(DurableCandidate),
    Registered(DurableCandidate),
    Failed,
}
struct Body {
    message: Arc<Message>,
    context: BatchContext,
    charge: usize,
    local_round: Option<u64>,
    work: Work,
}

/// Bounded input ownership only. A peer body is accepted only for an existing
/// verified proposal/decision hint, with the exact requested body ID and parent.
/// No arbitrary peer can fill the queue with unsolicited speculative candidates.
pub struct CandidateBodies {
    owner: Arc<()>,
    pin: BatchContext,
    policy: DirectNovFeePolicy,
    codec: DecodeLimits,
    limits: BodyLimits,
    bodies: BTreeMap<Hash, Body>,
    bytes: usize,
    frozen: bool,
    successor: Option<Lookahead>,
    successor_attempt: Option<successor::Pin>,
    successor_metrics: [u64; 5],
    pub executed: u64,
    pub rejected_execution: u64,
    pub integer_rows: u64,
    pub integer_plans: u64,
    pub apfl_rows: u64,
}
impl CandidateBodies {
    pub fn new(
        pipeline: &CandidatePipeline,
        pin: BatchContext,
        policy: DirectNovFeePolicy,
        codec: DecodeLimits,
        limits: BodyLimits,
    ) -> Result<Self> {
        ensure!(
            limits.bodies > 0
                && limits.bodies <= 64
                && limits.bytes > 0
                && limits.inflight > 0
                && limits.inflight <= limits.bodies,
            "invalid body budget"
        );
        ensure!(
            pin.semantic_version == SEMANTIC_VERSION
                && ((pin.business_program == program_id()
                    && pin.receipt_codec == receipt_codec()
                    && pin.effect_contract == effect_contract(&policy)?)
                    || (pin.business_program == uca_program_id()
                        && pin.receipt_codec == uca_receipt_codec()
                        && pin.effect_contract == uca_effect_contract(&policy)?)),
            "body program/receipt/policy pin mismatch"
        );
        transport::encode(&Message::RequestBody { body_id: [1; 32] }, codec)?;
        let domain = pipeline.storage_domain();
        ensure!(
            pin.chain_id == domain.chain_id
                && pin.genesis_config_commitment == domain.genesis_config_commitment
                && pin.protocol_commitment == domain.protocol_commitment,
            "body pipeline domain mismatch"
        );
        Ok(Self {
            owner: pipeline.owner_identity(),
            pin,
            policy,
            codec,
            limits,
            bodies: BTreeMap::new(),
            bytes: 0,
            frozen: false,
            successor: None,
            successor_attempt: None,
            successor_metrics: [0; 5],
            executed: 0,
            rejected_execution: 0,
            integer_rows: 0,
            integer_plans: 0,
            apfl_rows: 0,
        })
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }
    pub fn retained(&self) -> (usize, usize) {
        (
            self.bodies.len() + usize::from(self.successor.is_some()),
            self.bytes,
        )
    }

    /// Borrowed admission: backpressure never consumes the caller's input.
    /// Local selection is still not proposal authority; RoundProgress checks
    /// leader/lock/QC and an acknowledged durable execution before signing.
    pub fn offer(
        &mut self,
        message: &Arc<Message>,
        local: bool,
        progress: &RoundProgress,
    ) -> Result<bool> {
        self.offer_inner(message, local, progress, None)
    }

    /// Local, process-only signature evidence. Remote bodies never carry these
    /// capabilities; all parent/execution/consensus qualifications remain below.
    pub(crate) fn offer_admitted(
        &mut self,
        message: &Arc<Message>,
        admitted: &[&AdmittedTransfer],
        pipeline: &CandidatePipeline,
        progress: &RoundProgress,
    ) -> Result<bool> {
        ensure!(
            Arc::ptr_eq(&self.owner, &pipeline.owner_identity()),
            "admitted body execution owner changed"
        );
        self.offer_inner(message, true, progress, Some((pipeline, admitted)))
    }

    fn offer_inner(
        &mut self,
        message: &Arc<Message>,
        local: bool,
        progress: &RoundProgress,
        admitted: Option<(&CandidatePipeline, &[&AdmittedTransfer])>,
    ) -> Result<bool> {
        ensure!(!self.frozen, "body owner frozen");
        ensure!(!progress.is_frozen(), "consensus owner frozen");
        ensure!(
            self.pin.business_program == program_id(),
            "UCA native asset body admission is not active; legacy V3 bodies are forbidden"
        );
        if progress.is_recovering() {
            return Ok(false);
        }
        let context = match message.as_ref() {
            Message::Body { context, .. } | Message::ApflBody { context, .. } => context,
            _ => anyhow::bail!("candidate input must be an exact body"),
        };
        ensure!(
            matches_context(context, &self.pin, progress),
            "body parent/business pins mismatch"
        );
        let id = match message.as_ref() {
            Message::Body {
                raw_transactions, ..
            } => transport::body_id(context, raw_transactions, self.codec)?,
            Message::ApflBody { batch, .. } => transport::apfl_body_id(context, batch, self.codec)?,
            _ => unreachable!(),
        };
        ensure!(
            local || progress.missing_bodies().any(|(_, wanted)| wanted == id),
            "unsolicited body or different ordered input"
        );
        // A parent ACK can arrive during the same lifecycle turn before the
        // background owner is polled again. Retain its one existing input;
        // do not create a second execution for an exact demanded child.
        if self.retains_promoting_successor(id, progress) {
            return Ok(true);
        }
        if let Some(known) = self.bodies.get_mut(&id) {
            if local {
                known.local_round = Some(progress.round());
            }
            return Ok(true);
        }
        // Charge canonical input plus a request copy. The pipeline separately
        // reserves its own execution/capture/output memory before taking work.
        let mut charge = transport::encode(message, self.codec)?
            .len()
            .checked_mul(2)
            .context("body charge overflow")?;
        if self.retained().0 >= self.limits.bodies
            || charge > self.limits.bytes.saturating_sub(self.bytes)
        {
            return Ok(false);
        }
        let request = match message.as_ref() {
            Message::Body {
                raw_transactions, ..
            } => BatchRequest::new(raw_transactions.clone(), *context, self.policy.clone())?,
            Message::ApflBody { batch, .. } => match admitted {
                Some((pipeline, rows)) => {
                    let checked = pipeline.rebind_admitted_apfl(batch.clone(), rows)?;
                    charge = charge
                        .checked_add(checked.retained_metadata_bytes())
                        .context("admitted body charge overflow")?;
                    BatchRequest::from_authenticated(checked, *context, self.policy.clone())?
                }
                None => BatchRequest::from_apfl(batch.clone(), *context, self.policy.clone())?,
            },
            _ => unreachable!(),
        };
        if charge > self.limits.bytes.saturating_sub(self.bytes) {
            return Ok(false);
        }
        self.bodies.insert(
            id,
            Body {
                message: message.clone(),
                context: *context,
                charge,
                local_round: local.then_some(progress.round()),
                work: Work::Pending(Some(request)),
            },
        );
        self.bytes += charge;
        Ok(true)
    }

    /// Exact missing IDs and their authenticated proposer. The native delivery
    /// owner deduplicates/retries prepared requests; no full-body blind gossip.
    pub fn requests(&self, progress: &RoundProgress) -> Vec<(Hash, Hash)> {
        progress
            .missing_bodies()
            .filter(|(_, id)| !self.bodies.contains_key(id))
            .collect()
    }

    /// Original canonical input is reused, not rebuilt from balances/receipts.
    /// Returning it is not evidence that the peer may vote or publish it.
    pub fn response(&self, id: Hash, progress: &RoundProgress) -> Option<Arc<Message>> {
        self.bodies
            .get(&id)
            .map(|b| b.message.clone())
            .or_else(|| progress.candidate_body(id))
    }

    /// Nonblocking AOEM handoff, separate from native socket/control polling.
    /// Completed stale work is drained before retirement; no cancelled tickets
    /// or unknown persistence result is silently retried.
    pub fn poll(
        &mut self,
        pipeline: &CandidatePipeline,
        progress: &mut RoundProgress,
    ) -> Result<()> {
        ensure!(!self.frozen, "body owner frozen");
        let result = self.poll_inner(pipeline, progress);
        if result.is_err() {
            self.frozen = true;
        }
        result
    }

    fn poll_inner(
        &mut self,
        pipeline: &CandidatePipeline,
        progress: &mut RoundProgress,
    ) -> Result<()> {
        ensure!(
            Arc::ptr_eq(&self.owner, &pipeline.owner_identity()),
            "body execution owner changed"
        );
        // Observe/drain/promotion first; new optional submissions only after
        // current-height requests have received their admission turn below.
        self.poll_successor(pipeline, progress, false)?;
        let mut retired = Vec::new();
        for (id, body) in &mut self.bodies {
            let current = matches_context(&body.context, &self.pin, progress);
            if let Work::Running(ticket) = &mut body.work {
                match ticket.try_take() {
                    Ok(None) => continue,
                    Ok(Some(done)) => {
                        self.executed += 1;
                        self.integer_rows += done.observation.gpu_rows as u64;
                        self.integer_plans += done.observation.gpu_waves as u64;
                        self.apfl_rows += done.observation.apfl_view_transactions as u64;
                        body.work = Work::Ready(done.candidate().clone());
                    }
                    Err(error) => {
                        // Authentication/domain failures are input rejection.
                        // Persist failures (including unknown commit outcome)
                        // and unclassified owner failures stop this lifecycle.
                        if error
                            .downcast_ref::<PipelineFailure>()
                            .is_none_or(|failure| !matches!(failure.stage, FailureStage::Prepare))
                        {
                            return Err(error.context("body execution/storage owner failed"));
                        }
                        self.rejected_execution += 1;
                        body.work = Work::Failed;
                    }
                }
            }
            if !current {
                // Keep the immediately preceding decided body's ORIGINAL input
                // for an honest peer whose finality lags this node. Older
                // catch-up belongs to the verified archive, not an unbounded
                // in-memory body cache.
                if !matches!(body.work, Work::Registered(_))
                    || body.context.height.saturating_add(1) < progress.context().height
                {
                    retired.push(*id);
                }
                continue;
            }
            if body.local_round != Some(progress.round())
                && !progress.retains_body(*id)
                && !progress.missing_bodies().any(|(_, needed)| needed == *id)
            {
                retired.push(*id);
                continue;
            }
            if let Work::Ready(candidate) = &body.work {
                // Optional follower preparation stays on this bounded body
                // owner until a verified proposal demands it. It must not fill
                // the consensus candidate budget with unrelated local results.
                if !progress.is_local_proposer()?
                    && !progress.missing_bodies().any(|(_, needed)| needed == *id)
                {
                    continue;
                }
                if progress.register_candidate(
                    *id,
                    candidate,
                    body.local_round == Some(progress.round()),
                )? {
                    body.work = Work::Registered(candidate.clone());
                }
            }
            if let Work::Registered(candidate) = &body.work {
                let local = body.local_round == Some(progress.round());
                if local || progress.missing_bodies().any(|(_, needed)| needed == *id) {
                    // Reuse the same owner-bound completion across a permitted
                    // new round. No re-execution, signature replay or lock reset.
                    progress.register_candidate(*id, candidate, local)?;
                }
            }
        }
        for id in retired {
            let body = self.bodies.remove(&id).expect("retired body");
            self.bytes -= body.charge;
        }
        if progress.is_recovering() || progress.is_frozen() {
            return Ok(());
        }
        let mut running = self
            .bodies
            .values()
            .filter(|b| matches!(b.work, Work::Running(_)))
            .count()
            + usize::from(self.successor_inflight());
        let demanded = progress
            .missing_bodies()
            .map(|(_, id)| id)
            .collect::<Vec<_>>();
        // A local selection can execute before the leader's proposal arrives.
        // Only exact matching durable results qualify for that proposal in
        // RoundProgress; an unrelated local body cannot authorize a vote.
        // Serve verified demand first so optional work cannot win merely by
        // lexicographic body ID. All work retains the original owner/budgets.
        let mut order = self.bodies.keys().copied().collect::<Vec<_>>();
        order.sort_by_key(|id| !demanded.contains(id));
        for id in &order {
            let body = self.bodies.get_mut(id).expect("retained body");
            if running >= self.limits.inflight {
                break;
            }
            if !(demanded.contains(id) || body.local_round == Some(progress.round())) {
                continue;
            }
            if let Work::Authenticated(request) = &mut body.work {
                use crate::native_pipeline::pipeline::AuthenticatedSubmission;
                let owned = request
                    .take()
                    .context("authenticated body lost on backpressure")?;
                match pipeline.try_submit_authenticated_owned(owned) {
                    Ok(AuthenticatedSubmission::Accepted(ticket)) => {
                        body.work = Work::Running(ticket);
                        running += 1;
                    }
                    Ok(AuthenticatedSubmission::Backpressured(returned)) => {
                        *request = Some(returned);
                        break;
                    }
                    Err(rejected) => {
                        *request = Some(rejected.request);
                        return Err(rejected.error);
                    }
                }
                continue;
            }
            let Work::Pending(request) = &mut body.work else {
                continue;
            };
            let owned = request
                .take()
                .context("body request lost on backpressure")?;
            match pipeline.try_submit_owned(owned) {
                Ok(Submission::Accepted(ticket)) => {
                    body.work = Work::Running(ticket);
                    running += 1;
                }
                Ok(Submission::Backpressured(returned)) => {
                    *request = Some(returned);
                    break;
                }
                Err(rejected) => {
                    *request = Some(rejected.request);
                    return Err(rejected.error);
                }
            }
        }
        self.poll_successor(pipeline, progress, true)?;
        Ok(())
    }
}

// Original controller parent/business checks, unchanged. Slot/time are part of
// the body identity and execution statement, never inferred from arrival time.
fn matches_context(body: &BatchContext, pin: &BatchContext, progress: &RoundProgress) -> bool {
    let context = progress.context();
    let parent = progress.parent();
    body.chain_id == context.chain_id
        && body.genesis_config_commitment == context.genesis_config_commitment
        && body.protocol_commitment == context.protocol_commitment
        && body.height == context.height
        && body.parent_height == parent.height
        && body.parent_block_hash == parent.block_hash
        && body.parent_state_root == parent.state_root
        && body.parent_receipt_root == parent.receipt_batch_commitment
        && body.parent_state_version == parent.state_version
        && body.business_program == pin.business_program
        && body.semantic_version == pin.semantic_version
        && body.effect_contract == pin.effect_contract
        && body.receipt_codec == pin.receipt_codec
}
