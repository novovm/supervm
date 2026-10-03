//! One local successor computation may overlap its parent's consensus wait.
//! Neither a predicted decision identity nor durable candidate bytes authorize
//! a vote. Promotion rejoins the unchanged current-height journal checks.

use super::*;
use crate::native_pipeline::pipeline::DurableBatch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pin {
    source: Context,
    source_round: u64,
    next: Context,
    point: ParentPoint,
}

struct Basis {
    pin: Pin,
    parent: DurableCandidate,
}

pub(super) struct Successor {
    basis: Basis,
    body: Option<Body>,
    ticket: Option<PipelineTicket>,
    preparing: bool,
    attempted: bool,
}

impl Successor {
    fn new(basis: Basis) -> Self {
        Self {
            basis,
            body: None,
            ticket: None,
            preparing: false,
            attempted: false,
        }
    }

    pub(super) fn retained_body(&self) -> Option<(Hash, usize)> {
        self.body
            .as_ref()
            .map(|body| (body.prepared.fragment_id(), body.prepared.retained_bytes()))
    }
}

impl Controller {
    pub(super) fn preempt_successor_for_body(&mut self, charge: usize) -> Result<()> {
        let full = self.retained_bodies().len() >= self.config.limits.max_bodies
            || charge
                > self
                    .config
                    .limits
                    .max_body_bytes
                    .saturating_sub(self.body_bytes());
        if !full {
            return Ok(());
        }
        let Some(fragment) = self
            .successor
            .as_ref()
            .and_then(Successor::retained_body)
            .map(|(fragment, _)| fragment)
        else {
            return Ok(());
        };
        // A current-height body can displace optional speculation, including
        // its shared retransmission reference. Already accepted native work
        // remains charged and drains separately; retirement is NOT cancellation.
        let successor = self.successor.take().expect("retained successor body");
        self.prune_fixed(|fixed| fixed.prepared.fragment_id() != fragment);
        self.retire_successor(successor)
    }

    pub(super) fn inflight_count(&self) -> usize {
        self.inflight.len()
            + usize::from(self.successor.as_ref().is_some_and(|s| s.ticket.is_some()))
            + usize::from(self.successor_drain.is_some())
            + usize::from(self.early.as_ref().is_some_and(EarlyWork::has_ticket))
            + usize::from(self.early_drain.is_some())
    }

    /// An execution hint for the scheduled next-height leader, NOT a verified
    /// head. It comes only from this controller's actual durable parent work.
    /// Construct/clone the corresponding body on the existing assembly owner.
    pub fn successor_parent(&self) -> Result<Option<ParentPoint>> {
        if self.is_recovering() {
            return Ok(None);
        }
        let Some(height) = self.context().height.checked_add(1) else {
            return Ok(None);
        };
        if self.config.validators.leader(height, 0)? != self.config.local_validator {
            return Ok(None);
        }
        if let Some(successor) = &self.successor {
            return Ok(self
                .live_successor_pin(successor.basis.pin)
                .then_some(successor.basis.pin.point));
        }
        Ok(self.successor_basis(None)?.map(|basis| basis.pin.point))
    }

    /// Optional, bounded preannouncement. Ordinary try_submit_body remains
    /// exact-current-parent only. Admission grants no future signing rights.
    pub fn try_submit_successor_body(&mut self, message: &Arc<Message>) -> Result<bool> {
        let Message::Body { context, .. } = message.as_ref() else {
            anyhow::bail!("successor input is not a body");
        };
        if self.is_recovering()
            || self.early.is_some()
            || !self.retired.is_empty()
            || self.preparing.len() >= self.preparation_limit()
        {
            return Ok(false);
        }
        let Some(basis) = self.successor_basis(Some(context))? else {
            return Ok(false);
        };
        if self.config.validators.leader(basis.pin.next.height, 0)? != self.config.local_validator {
            return Ok(false);
        }
        if let Some(known) = &self.successor {
            if known.basis.pin != basis.pin
                || known.preparing
                || known.body.is_some()
                || known.attempted
            {
                return Ok(false);
            }
        }
        let pin = basis.pin;
        let token = self.allocate_token()?;
        match self.channel.try_prepare(PrepareRequest {
            token,
            input: PrepareInput::New(message.clone()),
        })? {
            PrepareAdmission::Accepted => {
                self.preparing.insert(
                    token,
                    Preparation {
                        request: None,
                        purpose: Purpose::SuccessorBody(pin),
                    },
                );
                let successor = self.successor.get_or_insert_with(|| Successor::new(basis));
                successor.preparing = true;
                Ok(true)
            }
            PrepareAdmission::Backpressure(_) => Ok(false),
            PrepareAdmission::Rejected { reason, .. } => anyhow::bail!(reason),
        }
    }

    fn live_successor_pin(&self, pin: Pin) -> bool {
        self.context() == pin.source && self.round() == pin.source_round && !self.is_recovering()
    }

    fn successor_basis(&self, expected: Option<&BatchContext>) -> Result<Option<Basis>> {
        if self.is_recovering() {
            return Ok(None);
        }
        for body in self.bodies.values() {
            let Some((value, candidate)) = &body.candidate else {
                continue;
            };
            if body.failed || !self.matches_context(candidate.packet().context()) {
                continue;
            }
            let statement = BlockStatement::from_executed(
                candidate.packet(),
                self.context(),
                &self.config.validators,
                &self.parent(),
            )?;
            ensure!(
                statement.hash() == *value,
                "successor parent statement changed"
            );
            let point = ParentPoint {
                height: self.context().height,
                block_hash: *value,
                state_root: candidate.packet().state_root(),
                receipt_batch_commitment: candidate.packet().receipt_batch_commitment(),
                state_version: statement.state_version(),
                decision_hash: super::super::chain::decision_id(&self.context(), *value),
            };
            let next = Context {
                height: point
                    .height
                    .checked_add(1)
                    .context("successor height exhausted")?,
                parent_block_hash: point.block_hash,
                parent_decision_hash: point.decision_hash,
                ..self.context()
            };
            if expected
                .is_some_and(|body| !matches_context(body, &self.config.execution, next, point))
            {
                continue;
            }
            return Ok(Some(Basis {
                pin: Pin {
                    source: self.context(),
                    source_round: self.round(),
                    next,
                    point,
                },
                parent: candidate.clone(),
            }));
        }
        Ok(None)
    }

    pub(super) fn early_parent(
        &self,
        expected: Option<&BatchContext>,
    ) -> Result<Option<ParentPoint>> {
        Ok(self.successor_basis(expected)?.map(|basis| basis.pin.point))
    }

    pub(super) fn successor_has_origin(&self, origin: EarlyOrigin) -> bool {
        self.successor
            .as_ref()
            .and_then(|s| s.body.as_ref())
            .is_some_and(|body| body.early_origin == Some(origin))
    }

    pub(super) fn successor_has_local_early_target(&self, height: u64) -> bool {
        self.successor.as_ref().and_then(|s| s.body.as_ref()).is_some_and(|body|
            body.source == self.local_peer && !body.failed && body.early_origin.is_some()
                && matches!(body.prepared.message().as_ref(), Message::Body { context, .. } if context.height == height))
    }

    pub(super) fn successor_body_mut(&mut self, id: Hash) -> Option<&mut Body> {
        self.successor
            .as_mut()
            .and_then(|s| s.body.as_mut())
            .filter(|body| body.prepared.body_id() == Some(id))
    }

    pub(super) fn is_successor_body(&self, ready: &Ready) -> bool {
        ready
            .body
            .as_ref()
            .is_some_and(|body| self.context().height.checked_add(1) == Some(body.context().height))
    }

    pub(super) fn successor_preparation_failed(&mut self, pin: Pin) {
        if let Some(successor) = self.successor.as_mut().filter(|s| s.basis.pin == pin) {
            successor.preparing = false;
            successor.attempted = true;
        }
    }

    pub(super) fn successor_prepared(&mut self, pin: Pin, ready: Ready) -> Result<()> {
        if self.context() == pin.next && self.parent() == pin.point && self.round() == 0 {
            // Parent advanced while body encoding was in flight. This is now
            // ordinary current input, not permission cached by the old hint.
            return self.keep_body(self.local_peer.clone(), ready, Some(0));
        }
        if let Some(successor) = self.successor.as_mut().filter(|s| s.basis.pin == pin) {
            successor.preparing = false;
        } else {
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        self.keep_successor_body(self.local_peer.clone(), ready, true)
    }

    pub(super) fn keep_successor_body(
        &mut self,
        source: String,
        mut ready: Ready,
        local: bool,
    ) -> Result<()> {
        let Some(request) = ready.body.as_ref() else {
            self.retire(Retirement::Ready(ready));
            return Ok(());
        };
        let Some(basis) = self.successor_basis(Some(request.context()))? else {
            self.reject("successor body lacks exact locally executed durable parent");
            self.retire(Retirement::Ready(ready));
            return Ok(());
        };
        let leader = self.config.validators.leader(basis.pin.next.height, 0)?;
        if self.peer_for(&leader) != Some(source.as_str())
            || self
                .successor
                .as_ref()
                .is_some_and(|s| s.basis.pin != basis.pin || s.body.is_some() || s.attempted)
            || self.retained_bodies().len() >= self.config.limits.max_bodies
            || ready.prepared.retained_bytes()
                > self
                    .config
                    .limits
                    .max_body_bytes
                    .saturating_sub(self.body_bytes())
        {
            self.reject("successor source, slot or retained-content budget");
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        let mut request = ready.body.take().expect("checked successor body");
        let prepared = ready.prepared;
        let successor = self.successor.get_or_insert_with(|| Successor::new(basis));
        successor.body = Some(Body {
            source,
            prepared: prepared.clone(),
            request: request.take_request(),
            authenticated: None,
            early_origin: None,
            candidate: None,
            failed: false,
            local_round: local.then_some(0),
            recovered: false,
        });
        if local {
            // The normal bounded body fanout carries the unchanged signed raw
            // transactions; there is no speculative proposal, vote or QC.
            self.cache(prepared, None);
        }
        Ok(())
    }

    /// Called after normal submissions and consensus progress. Native work
    /// already accepted is drained even if its parent later loses.
    pub(super) fn poll_successor(&mut self, pipeline: &CandidatePipeline) -> Result<()> {
        self.drain_successor()?;
        let Some(mut successor) = self.successor.take() else {
            return Ok(());
        };
        if !self.live_successor_pin(successor.basis.pin) {
            return self.retire_successor(successor);
        }
        // Retain ownership even on an invariant/owner error. In particular an
        // error must not drop a live ticket or a large body on the poll thread.
        let result = self.advance_successor(&mut successor, pipeline);
        self.successor = Some(successor);
        result
    }

    fn advance_successor(
        &mut self,
        successor: &mut Successor,
        pipeline: &CandidatePipeline,
    ) -> Result<()> {
        if let Some(mut ticket) = successor.ticket.take() {
            match ticket.try_take() {
                Ok(None) => successor.ticket = Some(ticket),
                Ok(Some(batch)) => self.finish_successor(successor, batch)?,
                Err(error) => {
                    self.stats.execution_failures += 1;
                    self.stats.last_error = Some(error.to_string().chars().take(256).collect());
                    if let Some(body) = &mut successor.body {
                        body.failed = true;
                    }
                }
            }
        }
        if !successor.attempted
            && self.retired.is_empty()
            && self.successor_drain.is_none()
            && self.inflight.len() < self.config.limits.max_inflight.saturating_sub(1)
        {
            successor.basis.parent.bind_to(&pipeline.owner_identity())?;
            if let Some(body) = &mut successor.body {
                if let Some(request) = body.authenticated.take() {
                    ensure!(body.request.is_none(), "successor has two execution inputs");
                    match pipeline.try_submit_authenticated_background_owned(request) {
                        Ok(AuthenticatedSubmission::Accepted(ticket)) => {
                            successor.ticket = Some(ticket);
                            successor.attempted = true;
                            self.stats.successor_started += 1;
                        }
                        Ok(AuthenticatedSubmission::Backpressured(request)) => {
                            body.authenticated = Some(request)
                        }
                        Err(rejected) => {
                            body.failed = true;
                            successor.attempted = true;
                            self.retire(Retirement::AuthenticatedRequest(rejected.request));
                            self.reject(rejected.error);
                        }
                    }
                    return Ok(());
                }
                if let Some(request) = body.request.take() {
                    match pipeline.try_submit_background_owned(request) {
                        Ok(Submission::Accepted(ticket)) => {
                            successor.ticket = Some(ticket);
                            successor.attempted = true;
                            self.stats.successor_started += 1;
                        }
                        Ok(Submission::Backpressured(request)) => body.request = Some(request),
                        Err(rejected) => {
                            body.failed = true;
                            successor.attempted = true;
                            self.retire(Retirement::Request(rejected.request));
                            self.reject(rejected.error);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn finish_successor(&mut self, successor: &mut Successor, batch: DurableBatch) -> Result<()> {
        self.stats.executed_batches += 1;
        self.stats.observe_execution(batch.observation);
        self.stats.observe_capture(batch.capture);
        let pin = successor.basis.pin;
        let statement = BlockStatement::from_executed(
            batch.candidate().packet(),
            pin.next,
            &self.config.validators,
            &pin.point,
        );
        match statement {
            Ok(statement) => {
                if self.head() != Some(pin.point) {
                    self.stats.successor_completed_before_parent += 1;
                }
                if let Some(body) = &mut successor.body {
                    body.candidate = Some((statement.hash(), batch.candidate().clone()));
                }
            }
            Err(error) => {
                if let Some(body) = &mut successor.body {
                    body.failed = true;
                }
                self.retire(Retirement::Batch(batch));
                return Err(error);
            }
        }
        self.retire(Retirement::Batch(batch));
        Ok(())
    }

    /// Only the acknowledged journal transition reaches this promotion hook.
    /// All six parent fields and the complete consensus context must match;
    /// the unchanged journal will recheck its live head again before signing.
    pub(super) fn reconcile_successor(&mut self) -> Result<()> {
        let Some(mut successor) = self.successor.take() else {
            return Ok(());
        };
        let pin = successor.basis.pin;
        if self.live_successor_pin(pin) {
            self.successor = Some(successor);
            return Ok(());
        }
        if self.context() != pin.next
            || self.parent() != pin.point
            || self.head() != Some(pin.point)
            || self.round() != 0
        {
            return self.retire_successor(successor);
        }
        let checked = successor
            .body
            .as_ref()
            .map(|body| -> Result<Hash> {
                let id = body
                    .prepared
                    .body_id()
                    .context("successor lost body identity")?;
                if let Some((value, candidate)) = &body.candidate {
                    let statement = BlockStatement::from_executed(
                        candidate.packet(),
                        self.context(),
                        &self.config.validators,
                        &self.parent(),
                    )?;
                    ensure!(
                        *value == statement.hash(),
                        "promoted successor statement differs"
                    );
                }
                Ok(id)
            })
            .transpose();
        let id = match checked {
            Ok(id) => id,
            Err(error) => {
                self.retire_successor(successor)?;
                return Err(error);
            }
        };
        if let Some(body) = successor.body.take() {
            let id = id.expect("checked successor body identity");
            self.stats.successor_reused += u64::from(body.candidate.is_some());
            if let Some(ticket) = successor.ticket.take() {
                self.stats.successor_promoted_inflight += 1;
                self.inflight.push(Execution {
                    id,
                    context: pin.next,
                    parent: pin.point,
                    requester: body.source.clone(),
                    ticket,
                    recovery: None,
                });
            }
            self.bodies.insert(id, body);
        }
        self.retire(Retirement::Candidate(successor.basis.parent));
        Ok(())
    }

    fn retire_successor(&mut self, mut successor: Successor) -> Result<()> {
        self.stats.successor_discarded += 1;
        if let Some(ticket) = successor.ticket.take() {
            ensure!(
                self.successor_drain.is_none(),
                "multiple abandoned background jobs"
            );
            // The service's single background Permit remains charged until
            // this reply is consumed. Keep it outside requester single-flight
            // slots, and NEVER attach its late result to a later matching body.
            self.successor_drain = Some(ticket);
        }
        if let Some(body) = successor.body.take() {
            self.retire_body(body);
        }
        self.retire(Retirement::Candidate(successor.basis.parent));
        Ok(())
    }

    fn drain_successor(&mut self) -> Result<()> {
        let Some(mut ticket) = self.successor_drain.take() else {
            return Ok(());
        };
        match ticket.try_take() {
            Ok(None) => self.successor_drain = Some(ticket),
            Ok(Some(batch)) => {
                self.stats.executed_batches += 1;
                self.stats.observe_execution(batch.observation);
                self.stats.observe_capture(batch.capture);
                self.stats.stale_results += 1;
                self.retire(Retirement::Batch(batch));
            }
            Err(error) => {
                self.stats.execution_failures += 1;
                self.reject(error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
