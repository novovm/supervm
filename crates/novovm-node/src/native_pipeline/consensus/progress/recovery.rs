//! Existing cold replay sequence, adapted from controller_recovery at 58e3aade.
//! Run on the node's assembly/execution owner, separate from the small control
//! poll. Reads and execution are async; no network wait or new signing rule.
use super::*;
use crate::native_pipeline::{
    business::{direct_nov_fee::DirectNovFeePolicy, nov_transfer_batch::effect_contract},
    consensus::{statement::BlockStatement, transport},
    persistence::{io::IoTicket, StoredCandidate},
    pipeline::{BatchRequest, PipelineTicket, Submission},
};

pub(super) enum Work {
    Read {
        locator: CandidateLocator,
        ticket: Option<IoTicket<Option<StoredCandidate>>>,
    },
    Execute {
        locator: CandidateLocator,
        body_id: Hash,
        body: Arc<Message>,
        request: Option<BatchRequest>,
        ticket: Option<PipelineTicket>,
    },
}

impl RoundProgress {
    /// At most one bounded cold body is read/prepared/executed at a time. The
    /// same pipeline revalidates signatures, exact parent and all business
    /// effects; stored bytes alone never become a DurableCandidate. Call this
    /// on the existing assembly role: body hashing/allocation is NOT I/O poll.
    /// The caller supplies its pinned business policy and decoder budget, not
    /// a peer policy. Any mismatch keeps recovery/signing closed until reopen.
    pub fn poll_recovery(
        &mut self,
        pipeline: &CandidatePipeline,
        policy: &DirectNovFeePolicy,
        limits: transport::DecodeLimits,
    ) -> Result<()> {
        ensure!(!self.is_frozen(), "progress frozen");
        let result = self.recover_inner(pipeline, policy, limits);
        if result.is_err() {
            self.frozen = true;
        }
        result
    }

    fn recover_inner(
        &mut self,
        pipeline: &CandidatePipeline,
        policy: &DirectNovFeePolicy,
        limits: transport::DecodeLimits,
    ) -> Result<()> {
        ensure!(
            Arc::ptr_eq(&self.owner, &pipeline.owner_identity()),
            "recovery owner changed"
        );
        if !self.recovering {
            return Ok(());
        }
        if self.recovery_work.is_none() {
            let Some(locator) = self.recovery_roots.values().next().copied() else {
                return Ok(());
            };
            self.recovery_work = Some(Work::Read {
                locator,
                ticket: None,
            });
        }
        match self.recovery_work.as_mut().expect("recovery work") {
            Work::Read { locator, ticket } => {
                if ticket.is_none() {
                    *ticket = pipeline.try_recover_consensus_candidate(locator.candidate_id)?;
                    return Ok(());
                }
                let Some(stored) = ticket.as_mut().expect("read ticket").try_take()? else {
                    return Ok(());
                };
                let stored = stored.context("cold candidate marker missing")?;
                let locator = *locator;
                ensure!(
                    stored.candidate_id() == locator.candidate_id
                        && stored.document_digest() == locator.document_digest
                        && stored.context().effect_contract == effect_contract(policy)?,
                    "cold locator/business policy mismatch"
                );
                ensure!(
                    BlockStatement::from_stored(
                        &stored,
                        self.context(),
                        &self.domain.validators,
                        &self.parent()
                    )?
                    .hash()
                        == locator.value,
                    "cold stored statement differs from journal"
                );
                ensure!(
                    stored.record_bytes()
                        <= self
                            .limits
                            .candidate_bytes
                            .saturating_sub(self.candidate_bytes),
                    "cold candidate byte budget"
                );
                let body_id =
                    transport::body_id(stored.context(), stored.raw_transactions(), limits)?;
                let request = BatchRequest::new(
                    stored.raw_transactions().to_vec(),
                    *stored.context(),
                    policy.clone(),
                )?;
                self.recovery_work = Some(Work::Execute {
                    locator,
                    body_id,
                    body: Arc::new(Message::Body {
                        context: *stored.context(),
                        raw_transactions: stored.raw_transactions().to_vec(),
                    }),
                    request: Some(request),
                    ticket: None,
                });
            }
            Work::Execute {
                locator,
                body_id,
                body,
                request,
                ticket,
            } => {
                if ticket.is_none() {
                    let owned = request
                        .take()
                        .context("cold request lost on backpressure")?;
                    match pipeline.try_submit_owned(owned) {
                        Ok(Submission::Accepted(accepted)) => *ticket = Some(accepted),
                        Ok(Submission::Backpressured(returned)) => *request = Some(returned),
                        Err(rejected) => {
                            *request = Some(rejected.request);
                            return Err(rejected.error.context("cold execution rejected"));
                        }
                    }
                    return Ok(());
                }
                let Some(done) = ticket.as_mut().expect("execution ticket").try_take()? else {
                    return Ok(());
                };
                let candidate = done.candidate().clone();
                ensure!(
                    CandidatePipeline::candidate_locator(&candidate, locator.value) == *locator,
                    "cold execution output differs from journal"
                );
                let body_id = *body_id;
                let body = body.clone();
                ensure!(
                    self.register_candidate(body_id, &candidate, false)?,
                    "cold candidate registration budget"
                );
                self.recovered_bodies.insert(body_id, body);
                self.recovery_work = None;
            }
        }
        Ok(())
    }
}
