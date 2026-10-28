//! One optional local successor on the ORIGINAL body/pipeline owner.
//! Adapted from controller/{early_body,successor}; no old controller, sender,
//! journal or authority is activated. A durable parent is a computation basis,
//! not a decided parent. Only the existing journal ACK permits promotion.
use super::*;
use crate::native_pipeline::consensus::{
    chain::decision_id,
    statement::{BlockStatement, ParentPoint},
    wire::{Context as VoteContext, ValidatorSet},
};
use crate::native_pipeline::pipeline::{
    AuthenticatedRequest, AuthenticatedSubmission, AuthenticationRequest, AuthenticationSubmission,
    AuthenticationTicket,
};

/// Local selection hint. Its nonce overrides are read from real executed
/// receipts, never projected from pending transactions. Not a signing permit.
#[derive(Clone, Debug)]
pub struct SuccessorSelection {
    pub source: VoteContext,
    pub source_round: u64,
    pub parent: ParentPoint,
    pub nonce_after: BTreeMap<Hash, u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pin {
    source: VoteContext,
    source_round: u64,
    parent: ParentPoint,
}
impl Pin {
    fn from_selection(selection: &SuccessorSelection) -> Self {
        Self {
            source: selection.source,
            source_round: selection.source_round,
            parent: selection.parent,
        }
    }
    fn next(self) -> Result<VoteContext> {
        Ok(VoteContext {
            height: self
                .parent
                .height
                .checked_add(1)
                .context("successor height overflow")?,
            parent_block_hash: self.parent.block_hash,
            parent_decision_hash: self.parent.decision_hash,
            ..self.source
        })
    }
    fn live(self, context: VoteContext, round: u64, recovering: bool) -> bool {
        !recovering && context == self.source && round == self.source_round
    }
    fn promoted(
        self,
        context: VoteContext,
        round: u64,
        parent: ParentPoint,
        head: Option<ParentPoint>,
        recovering: bool,
    ) -> bool {
        !recovering
            && round == 0
            && self.next().ok() == Some(context)
            && parent == self.parent
            && head == Some(self.parent)
    }
}

enum Phase {
    Authenticate(Option<AuthenticationRequest>),
    Authenticating(AuthenticationTicket),
    Bind(Option<AuthenticatedRequest>),
    Running(PipelineTicket),
    Ready(DurableCandidate),
}
pub(super) struct Lookahead {
    selection: SuccessorSelection,
    id: Hash,
    message: Arc<Message>,
    context: BatchContext,
    charge: usize,
    phase: Phase,
    retired: bool,
}

impl CandidateBodies {
    pub(super) fn successor_inflight(&self) -> bool {
        self.successor
            .as_ref()
            .is_some_and(|w| matches!(w.phase, Phase::Authenticating(_) | Phase::Running(_)))
    }
    pub(super) fn retains_promoting_successor(&self, id: Hash, progress: &RoundProgress) -> bool {
        self.successor.as_ref().is_some_and(|work| {
            !work.retired
                && work.id == id
                && Pin::from_selection(&work.selection).promoted(
                    progress.context(),
                    progress.round(),
                    progress.parent(),
                    progress.head(),
                    progress.is_recovering(),
                )
        })
    }
    pub(crate) fn successor_metrics(&self) -> [u64; 5] {
        self.successor_metrics
    }

    pub(crate) fn successor_selection(
        &self,
        progress: &RoundProgress,
        validators: &ValidatorSet,
    ) -> Result<Option<SuccessorSelection>> {
        if self.frozen
            || progress.is_frozen()
            || progress.is_recovering()
            || self.limits.inflight < 2
            || self.limits.bodies < 2
        {
            return Ok(None);
        }
        let Some(height) = progress.context().height.checked_add(1) else {
            return Ok(None);
        };
        if validators.leader(height, 0)? != progress.local_validator() {
            return Ok(None);
        }
        if let Some(work) = &self.successor {
            return Ok((!work.retired
                && Pin::from_selection(&work.selection).live(
                    progress.context(),
                    progress.round(),
                    progress.is_recovering(),
                ))
            .then(|| work.selection.clone()));
        }
        if self.successor_attempt.is_some_and(|pin| {
            pin.live(
                progress.context(),
                progress.round(),
                progress.is_recovering(),
            )
        }) {
            return Ok(None);
        }
        for (id, body) in &self.bodies {
            // Only an already registered actual execution, not arbitrary peer
            // roots or a merely prepared/failed body, supplies the next basis.
            let Work::Registered(candidate) = &body.work else {
                continue;
            };
            if !matches_context(&body.context, &self.pin, progress) || !progress.retains_body(*id) {
                continue;
            }
            let packet = candidate.bind_to(&self.owner)?;
            let statement = BlockStatement::from_executed(
                packet,
                progress.context(),
                validators,
                &progress.parent(),
            )?;
            let parent = ParentPoint {
                height: progress.context().height,
                block_hash: statement.hash(),
                state_root: packet.state_root(),
                receipt_batch_commitment: packet.receipt_batch_commitment(),
                state_version: statement.state_version(),
                decision_hash: decision_id(&progress.context(), statement.hash()),
            };
            return Ok(Some(SuccessorSelection {
                source: progress.context(),
                source_round: progress.round(),
                parent,
                // Repeated signer entries intentionally keep the LAST actual
                // receipt nonce; failure need not mean unchanged nonce.
                // UCA stages nonce writes only for these executed subjects;
                // rejected authority rows retain the current prefix nonce.
                // Unmentioned published nonces are unchanged. This remains a
                // local selection hint, never a grant/balance state overlay.
                nonce_after: packet
                    .receipt_projection()
                    .iter()
                    .map(|receipt| (receipt.signer_identity, receipt.nonce_after))
                    .collect(),
            }));
        }
        Ok(None)
    }

    pub(crate) fn offer_successor(
        &mut self,
        message: &Arc<Message>,
        selection: &SuccessorSelection,
        pipeline: &CandidatePipeline,
        progress: &RoundProgress,
        validators: &ValidatorSet,
    ) -> Result<bool> {
        self.offer_successor_inner(message, selection, pipeline, progress, validators, None)
    }

    pub(crate) fn offer_successor_admitted(
        &mut self,
        message: &Arc<Message>,
        selection: &SuccessorSelection,
        pipeline: &CandidatePipeline,
        progress: &RoundProgress,
        validators: &ValidatorSet,
        admitted: &[&AdmittedTransfer],
    ) -> Result<bool> {
        self.offer_successor_inner(
            message,
            selection,
            pipeline,
            progress,
            validators,
            Some(admitted),
        )
    }

    fn offer_successor_inner(
        &mut self,
        message: &Arc<Message>,
        selection: &SuccessorSelection,
        pipeline: &CandidatePipeline,
        progress: &RoundProgress,
        validators: &ValidatorSet,
        admitted: Option<&[&AdmittedTransfer]>,
    ) -> Result<bool> {
        ensure!(
            Arc::ptr_eq(&self.owner, &pipeline.owner_identity()),
            "successor execution owner changed"
        );
        ensure!(
            !self.frozen && !progress.is_frozen(),
            "successor owner frozen"
        );
        let Some(actual) = self.successor_selection(progress, validators)? else {
            return Ok(false);
        };
        let pin = Pin::from_selection(&actual);
        ensure!(
            pin == Pin::from_selection(selection),
            "stale successor selection pin"
        );
        let context = match message.as_ref() {
            Message::ApflBody { context, .. } | Message::AssetBody { context, .. } => context,
            _ => anyhow::bail!("successor requires an original structured APFL/NUAS body"),
        };
        ensure!(
            matches!(message.as_ref(), Message::AssetBody { .. })
                == (self.pin.business_program == uca_program_id()),
            "successor body wire/profile mismatch"
        );
        ensure!(
            matches_successor_context(context, &self.pin, pin)?,
            "successor parent/business pins mismatch"
        );
        let id = transport::candidate_body_id(message, self.codec)?;
        if let Some(work) = &self.successor {
            return Ok(!work.retired
                && work.id == id
                && Pin::from_selection(&work.selection) == pin);
        }
        let (request, metadata) = match (message.as_ref(), admitted) {
            (Message::ApflBody { batch, .. }, Some(rows)) => {
                let checked = pipeline.rebind_admitted_apfl(batch.clone(), rows)?;
                let bytes = checked.retained_metadata_bytes();
                (
                    AuthenticationRequest::from_authenticated(checked, self.policy.clone())?,
                    bytes,
                )
            }
            (Message::ApflBody { batch, .. }, None) => (
                AuthenticationRequest::from_apfl(batch.clone(), self.policy.clone())?,
                0,
            ),
            (Message::AssetBody { batch, .. }, Some(rows)) => {
                let checked = pipeline.rebind_admitted_uca(batch.clone(), rows)?;
                let bytes = checked.retained_metadata_bytes();
                (
                    AuthenticationRequest::from_authenticated(checked, self.policy.clone())?,
                    bytes,
                )
            }
            (Message::AssetBody { batch, .. }, None) => (
                AuthenticationRequest::from_uca_batch(batch.clone(), self.policy.clone())?,
                0,
            ),
            _ => unreachable!("structured successor profile checked above"),
        };
        if !pipeline.can_authenticate_background(&request)? {
            return Ok(false);
        }
        let mut charge = transport::encode(message, self.codec)?
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(actual.nonce_after.len().checked_mul(96)?))
            .and_then(|n| n.checked_add(metadata))
            .context("successor retained charge overflow")?;
        // Match ordinary NUAS retention: the original structural dictionary and
        // expanded locator pairs remain owned while authentication/bind runs.
        if let Message::AssetBody { batch, .. } = message.as_ref() {
            charge = charge
                .checked_add(batch.canonical_bytes())
                .and_then(|n| batch.len().checked_mul(64).and_then(|h| n.checked_add(h)))
                .context("NUAS successor retained charge overflow")?;
        }
        // Reserve one worst-case ordinary body. Optional work must never occupy
        // the last current-height input slot or its logical retained bytes.
        let reserve = self
            .codec
            .message_bytes
            .checked_mul(2)
            .context("body reserve overflow")?;
        if self.bodies.len() + 1 >= self.limits.bodies
            || charge
                .checked_add(reserve)
                .is_none_or(|n| n > self.limits.bytes.saturating_sub(self.bytes))
        {
            return Ok(false);
        }
        self.bytes += charge;
        self.successor_attempt = Some(pin);
        self.successor = Some(Lookahead {
            selection: actual,
            id,
            message: message.clone(),
            context: *context,
            charge,
            phase: Phase::Authenticate(Some(request)),
            retired: false,
        });
        Ok(true)
    }

    pub(crate) fn has_successor_target(&self, height: u64, progress: &RoundProgress) -> bool {
        if self.frozen || progress.is_frozen() || progress.is_recovering() {
            return false;
        }
        let active = self.successor.as_ref().is_some_and(|work| {
            let pin = Pin::from_selection(&work.selection);
            !work.retired
                && work.context.height == height
                && (pin.live(progress.context(), progress.round(), false)
                    || pin.promoted(
                        progress.context(),
                        progress.round(),
                        progress.parent(),
                        progress.head(),
                        false,
                    ))
        });
        active
            || self.bodies.values().any(|body| {
                body.context.height == height
                    && body.local_round == Some(progress.round())
                    && matches_context(&body.context, &self.pin, progress)
                    && !matches!(body.work, Work::Failed)
            })
    }

    pub(super) fn poll_successor(
        &mut self,
        pipeline: &CandidatePipeline,
        progress: &RoundProgress,
        allow_submit: bool,
    ) -> Result<()> {
        let Some(mut work) = self.successor.take() else {
            return Ok(());
        };
        let pin = Pin::from_selection(&work.selection);
        let live = pin.live(
            progress.context(),
            progress.round(),
            progress.is_recovering(),
        );
        let promoted = pin.promoted(
            progress.context(),
            progress.round(),
            progress.parent(),
            progress.head(),
            progress.is_recovering(),
        );
        if !work.retired && (!live && !promoted || progress.is_frozen()) {
            work.retired = true;
            self.successor_metrics[4] += 1;
        }
        let result = self.advance_successor(&mut work, pipeline, live, promoted, allow_submit);
        match result {
            Ok(true) => {
                self.bytes -= work.charge;
            }
            Ok(false) => {
                self.successor = Some(work);
            }
            Err(error) => {
                // Preserve any ticket/owner on an invariant or unknown write
                // outcome; the existing frozen lifecycle performs shutdown.
                self.successor = Some(work);
                return Err(error);
            }
        }
        Ok(())
    }

    /// true releases this optional owner. Accepted tickets are always drained;
    /// a stale completion cannot become input for a new generation/body ID.
    fn advance_successor(
        &mut self,
        work: &mut Lookahead,
        pipeline: &CandidatePipeline,
        live: bool,
        promoted: bool,
        allow_submit: bool,
    ) -> Result<bool> {
        if let Phase::Authenticating(ticket) = &mut work.phase {
            match ticket.try_take() {
                Ok(None) => return Ok(false),
                Ok(Some(body)) => {
                    if work.retired {
                        return Ok(true);
                    }
                    work.phase = Phase::Bind(Some(body.bind(work.context)));
                }
                Err(_) => {
                    if !work.retired {
                        self.successor_metrics[4] += 1;
                    }
                    return Ok(true);
                }
            }
        }
        if let Phase::Running(ticket) = &mut work.phase {
            match ticket.try_take() {
                Ok(None) => {
                    if !work.retired && promoted {
                        return self.promote_successor(work);
                    }
                    return Ok(false);
                }
                Ok(Some(done)) => {
                    self.executed += 1;
                    self.integer_rows += done.observation.gpu_rows as u64;
                    self.integer_plans += done.observation.gpu_waves as u64;
                    self.apfl_rows += done.observation.apfl_view_transactions as u64;
                    if work.retired {
                        return Ok(true);
                    }
                    if live {
                        self.successor_metrics[2] += 1;
                    }
                    // A future result is not yet in RoundProgress's candidate
                    // budget. Charge its actual retained records until promoted.
                    let extra = done.candidate().packet().retained_bytes();
                    let reserve = self.codec.message_bytes.saturating_mul(2);
                    if extra
                        .checked_add(reserve)
                        .is_none_or(|n| n > self.limits.bytes.saturating_sub(self.bytes))
                    {
                        self.successor_metrics[4] += 1;
                        return Ok(true);
                    }
                    self.bytes += extra;
                    work.charge += extra;
                    work.phase = Phase::Ready(done.candidate().clone());
                }
                Err(error) => {
                    if error
                        .downcast_ref::<PipelineFailure>()
                        .is_none_or(|f| !matches!(f.stage, FailureStage::Prepare))
                    {
                        return Err(error.context("successor execution/storage owner failed"));
                    }
                    self.rejected_execution += 1;
                    if !work.retired {
                        self.successor_metrics[4] += 1;
                    }
                    return Ok(true);
                }
            }
        }
        if work.retired {
            return Ok(true);
        }
        if promoted && !matches!(work.phase, Phase::Authenticate(_)) {
            return self.promote_successor(work);
        }
        if !allow_submit {
            return Ok(false);
        }
        let running = self
            .bodies
            .values()
            .filter(|b| matches!(b.work, Work::Running(_)))
            .count();
        // Authentication is real queued work too. Both optional submissions
        // leave one local slot for current-height work, in addition to the
        // pipeline's independent atomic byte/batch reservation.
        if !background_slot_available(running, self.limits.inflight) {
            return Ok(false);
        }
        if let Phase::Authenticate(request) = &mut work.phase {
            let request = request
                .take()
                .context("successor authentication input lost")?;
            match pipeline.try_authenticate_owned(request) {
                Ok(AuthenticationSubmission::Accepted(ticket)) => {
                    self.successor_metrics[0] += 1;
                    work.phase = Phase::Authenticating(ticket);
                }
                Ok(AuthenticationSubmission::Backpressured(returned)) => {
                    work.phase = Phase::Authenticate(Some(returned));
                }
                Err(rejected) => {
                    work.phase = Phase::Authenticate(Some(rejected.request));
                    return Err(rejected.error);
                }
            }
        } else if let Phase::Bind(request) = &mut work.phase {
            let request = request.take().context("successor bound input lost")?;
            match pipeline.try_submit_authenticated_background_owned(request) {
                Ok(AuthenticatedSubmission::Accepted(ticket)) => {
                    self.successor_metrics[1] += 1;
                    work.phase = Phase::Running(ticket);
                }
                Ok(AuthenticatedSubmission::Backpressured(returned)) => {
                    work.phase = Phase::Bind(Some(returned));
                }
                Err(rejected) => {
                    work.phase = Phase::Bind(Some(rejected.request));
                    return Err(rejected.error);
                }
            }
        }
        Ok(false)
    }

    fn promote_successor(&mut self, work: &mut Lookahead) -> Result<bool> {
        ensure!(
            !self.bodies.contains_key(&work.id),
            "successor would replace existing body ownership"
        );
        let old = std::mem::replace(&mut work.phase, Phase::Authenticate(None));
        let promoted = match old {
            Phase::Bind(request) => Work::Authenticated(request),
            Phase::Running(ticket) => Work::Running(ticket),
            Phase::Ready(candidate) => Work::Ready(candidate),
            other => {
                work.phase = other;
                anyhow::bail!("successor promotion before authentication");
            }
        };
        self.bodies.insert(
            work.id,
            Body {
                message: work.message.clone(),
                context: work.context,
                charge: work.charge,
                local_round: Some(0),
                work: promoted,
            },
        );
        // Transfer the same retained charge; poll_successor releases only zero.
        work.charge = 0;
        self.successor_metrics[3] += 1;
        Ok(true)
    }
}

fn background_slot_available(current: usize, limit: usize) -> bool {
    current < limit.saturating_sub(1)
}

fn matches_successor_context(body: &BatchContext, pin: &BatchContext, basis: Pin) -> Result<bool> {
    let next = basis.next()?;
    let parent = basis.parent;
    Ok(body.chain_id == next.chain_id
        && body.genesis_config_commitment == next.genesis_config_commitment
        && body.protocol_commitment == next.protocol_commitment
        && body.height == next.height
        && body.parent_height == parent.height
        && body.parent_block_hash == parent.block_hash
        && body.parent_state_root == parent.state_root
        && body.parent_receipt_root == parent.receipt_batch_commitment
        && body.parent_state_version == parent.state_version
        && body.business_program == pin.business_program
        && body.semantic_version == pin.semantic_version
        && body.effect_contract == pin.effect_contract
        && body.receipt_codec == pin.receipt_codec)
}

#[cfg(test)]
mod tests;
