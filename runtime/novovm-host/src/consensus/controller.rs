//! Bounded autonomous host controller for the explicit development protocol.
//! Assembly/hash/decoding belongs to HostChannel; business execution belongs
//! to the SAME resident CandidatePipeline. Only journal readback publishes a
//! signature, adopted round, or head. Transport admission is never an ACK.
//!
//! Cold replay re-executes referenced undecided bodies on this resident owner
//! before enabling new signing or timers. Stored content is never a capability.

#[path = "controller_recovery.rs"]
mod recovery;
use recovery::Recovery;
mod successor;
use successor::Successor;
mod early_body;
use early_body::{EarlyOrigin, EarlyWork};

use super::channel::{
    ChannelEvent, HostChannel, Outbound, PrepareAdmission, PrepareInput, PrepareRequest,
    PreparedMessage, Ready, RetireAdmission, Retirement, SendAdmission, VerifiedEvidence,
};
use super::collector::{CollectorLimits, VoteCollector, VoteInsert};
use super::journal::CandidateLocator;
use super::pacemaker::{Pacemaker, TimeoutPolicy};
use super::statement::{BlockStatement, ParentPoint};
use super::transport::Message;
use super::wire::{
    Context, Hash, Phase, Proposal, Quorum, ValidatorSet, VerifiedProposal, VerifiedQuorum,
};
use super::{ArchiveRead, DurableMessage, TimeoutStep, ValidatorJournal};
use crate::business::nov_transfer_batch::ExecutionObservation;
use crate::execution::plan::BatchContext;
use crate::pipeline::{
    AuthenticatedRequest, AuthenticatedSubmission, AuthenticationTicket, BatchRequest,
    CandidatePipeline, DurableCandidate, PipelineTicket, Submission,
};
use anyhow::{ensure, Context as _, Result};
use novovm_network::product_overlay::peer_id_from_ed25519_public_key_v1;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct ControllerLimits {
    pub max_bodies: usize,
    /// Retained prepared body content, independently of channel/pipeline quotas.
    pub max_body_bytes: usize,
    pub max_proposals: usize,
    pub max_inflight: usize,
    pub events_per_poll: usize,
    pub sends_per_poll: usize,
}

impl Default for ControllerLimits {
    fn default() -> Self {
        Self {
            max_bodies: 32,
            max_body_bytes: 64 * 1024 * 1024,
            max_proposals: 32,
            max_inflight: 4,
            events_per_poll: 32,
            sends_per_poll: 16,
        }
    }
}

pub struct ControllerConfig {
    pub validators: Arc<ValidatorSet>,
    pub local_validator: Hash,
    /// Exact authenticated remote peer IDs, one per validator (not including us).
    pub peers: BTreeMap<Hash, String>,
    /// Immutable business/domain pins. Parent/height are derived from journal;
    /// slot/time are proposed input, then bound by the executed BlockStatement.
    pub execution: BatchContext,
    pub collector: CollectorLimits,
    pub timeouts: TimeoutPolicy,
    pub limits: ControllerLimits,
    pub retransmit: Duration,
}

#[derive(Clone, Debug, Default)]
pub struct ControllerStats {
    pub executed_batches: u64,
    pub successor_started: u64,
    pub successor_completed_before_parent: u64,
    pub successor_reused: u64,
    pub successor_promoted_inflight: u64,
    pub successor_discarded: u64,
    pub early_authentication_started: u64,
    pub early_authentication_completed: u64,
    /// Real authentication reply observed before this controller holds the
    /// matching durable-parent capability. Not a disk/native completion clock,
    /// business-execution overlap, enqueue count or cross-process clock delta.
    pub early_authentication_completed_before_parent: u64,
    pub early_bind_reused: u64,
    pub early_discarded: u64,
    /// Sum of component counts from successful DurableBatch observations,
    /// including stale completions. Not finalized transactions or parallelism.
    pub execution_components_total: u64,
    /// Sum per execution, not distinct accounts over the controller lifetime.
    pub execution_credit_only_accounts_total: u64,
    /// Actual business reducer repairs, not consensus/pipeline retry counts.
    pub execution_recomputed_transactions_total: u64,
    /// Maximum observed simultaneous AOEM business callbacks in a batch. This
    /// is neither the configured worker count nor a unique-thread measurement.
    pub execution_peak_callbacks: usize,
    /// Fixed-size scalar copy of the last successful execution observation;
    /// it can describe a stale candidate and confers no finality authority.
    pub last_execution_observation: Option<ExecutionObservation>,
    /// Saturation is explicit and never changes consensus progress or timing.
    pub execution_observation_saturated: bool,
    pub execution_failures: u64,
    pub stale_results: u64,
    pub durable_votes: u64,
    pub durable_decisions: u64,
    pub rejected_messages: u64,
    pub received_votes: u64,
    pub head_advances: u64,
    pub prevote_weight: u64,
    pub precommit_weight: u64,
    pub retained_bodies: usize,
    pub retained_body_bytes: usize,
    pub inflight: usize,
    pub last_error: Option<String>,
}

impl ControllerStats {
    /// O(1): copy/accumulate only the executor's existing four scalar fields.
    /// No controller wall time or configured worker value enters this report.
    fn observe_execution(&mut self, observation: ExecutionObservation) {
        self.execution_observation_saturated |=
            accumulate_observed(&mut self.execution_components_total, observation.components);
        self.execution_observation_saturated |= accumulate_observed(
            &mut self.execution_credit_only_accounts_total,
            observation.credit_only_accounts,
        );
        self.execution_observation_saturated |= accumulate_observed(
            &mut self.execution_recomputed_transactions_total,
            observation.recomputed_transactions,
        );
        self.execution_peak_callbacks = self
            .execution_peak_callbacks
            .max(observation.peak_callbacks);
        self.last_execution_observation = Some(observation);
    }
}

fn accumulate_observed(total: &mut u64, observed: usize) -> bool {
    let Some(sum) = u64::try_from(observed)
        .ok()
        .and_then(|value| total.checked_add(value))
    else {
        *total = u64::MAX;
        return true;
    };
    *total = sum;
    false
}

struct Body {
    source: String,
    prepared: PreparedMessage,
    request: Option<BatchRequest>,
    authenticated: Option<AuthenticatedRequest>,
    early_origin: Option<EarlyOrigin>,
    candidate: Option<(Hash, DurableCandidate)>,
    failed: bool,
    local_round: Option<u64>,
    /// At most the bounded cold replay roots; retain until this height retires.
    recovered: bool,
}

struct Execution {
    id: Hash,
    context: Context,
    parent: ParentPoint,
    requester: String,
    ticket: PipelineTicket,
    recovery: Option<CandidateLocator>,
}

struct Offered {
    proposal: VerifiedProposal,
    valid: Option<VerifiedQuorum>,
    decision: Option<VerifiedQuorum>,
    body_id: Hash,
}

enum Purpose {
    EarlyBody(u64),
    BindEarly(u64),
    EarlyBinding(EarlyOrigin),
    SuccessorBody(successor::Pin),
    LocalBody {
        context: Context,
        round: u64,
    },
    Broadcast,
    /// Already-durable local bytes. A failed preparation must fail recovery.
    Replay,
    RecoveryBody(CandidateLocator),
    ArchiveDecision(String),
    ArchiveBody {
        peer: String,
        proposal: Proposal,
        certificate: Quorum,
    },
}

struct Preparation {
    request: Option<PrepareRequest>,
    purpose: Purpose,
}

struct Fixed {
    prepared: PreparedMessage,
    destination: Option<String>,
    schedule: RetrySchedule,
    body_fanout: Option<BodyFanout>,
}

/// A large body's initial fanout is admitted once per configured peer. This is
/// NOT delivery evidence: repeated signed offers let a missing receiver ask for
/// the exact body again. Direct RequestBody replies retain their existing retry
/// and request-driven wake-up rules. Control evidence still retransmits.
struct BodyFanout {
    pending: Vec<bool>,
    remaining: usize,
}

impl BodyFanout {
    fn new(peers: usize) -> Self {
        Self {
            pending: vec![true; peers],
            remaining: peers,
        }
    }
    fn needs(&self, peer: usize) -> bool {
        self.pending.get(peer).copied().unwrap_or(false)
    }
    fn accepted(&mut self, peer: usize) {
        if let Some(pending) = self.pending.get_mut(peer) {
            if *pending {
                *pending = false;
                self.remaining -= 1;
            }
        }
    }
}

struct RetrySchedule {
    next_peer: usize,
    due: Option<Instant>,
    active: bool,
}

impl RetrySchedule {
    fn new() -> Self {
        Self {
            next_peer: 0,
            due: None,
            active: true,
        }
    }
    fn wake(&mut self) {
        self.active = true;
        self.due = None;
    }
    fn attempted(
        &mut self,
        direct: bool,
        admitted: bool,
        peers: usize,
        now: Instant,
        interval: Duration,
    ) -> Result<()> {
        self.next_peer += 1;
        if direct && admitted {
            self.active = false;
        }
        if direct || self.next_peer >= peers {
            self.next_peer = 0;
            self.due = Some(
                now.checked_add(interval)
                    .context("retransmission time overflow")?,
            );
        }
        Ok(())
    }
}

struct ArchiveJob {
    context: Context,
    read: ArchiveRead,
}

pub struct Controller {
    config: ControllerConfig,
    journal: ValidatorJournal,
    channel: HostChannel,
    collector: VoteCollector,
    pacemaker: Pacemaker,
    local_peer: String,
    bodies: BTreeMap<Hash, Body>,
    offers: BTreeMap<(String, bool), Offered>,
    inflight: Vec<Execution>,
    successor: Option<Successor>,
    successor_drain: Option<PipelineTicket>,
    early: Option<EarlyWork>,
    early_drain: Option<AuthenticationTicket>,
    early_attempted_scope: Option<super::transport::EarlyBodyScope>,
    preparing: BTreeMap<u64, Preparation>,
    fixed: VecDeque<Fixed>,
    archives: BTreeMap<String, ArchiveJob>,
    archive_requests: BTreeMap<String, Context>,
    peer_requests: BTreeMap<String, Context>,
    next_token: u64,
    local_preparing: bool,
    proposed_round: Option<u64>,
    pending_value: Option<Hash>,
    execution_turn: usize,
    archive_turn: usize,
    retired: VecDeque<Retirement>,
    request_due: Option<Instant>,
    now: Option<Instant>,
    stats: ControllerStats,
    recovery: Option<Recovery>,
    recovery_failure: Option<String>,
    #[cfg(test)]
    recovery_test_ingress: Option<super::channel::Received>,
}

impl Controller {
    pub fn new(
        config: ControllerConfig,
        mut journal: ValidatorJournal,
        channel: HostChannel,
    ) -> Result<Self> {
        validate_config(&config, &journal)?;
        let reserve = config.validators.members().len() * 2 + config.limits.max_inflight + 2;
        ensure!(
            channel
                .preparation_charge()
                .checked_mul(reserve)
                .is_some_and(|bytes| bytes <= config.limits.max_body_bytes),
            "controller bytes cannot reserve independent peer/current/archive body slots"
        );
        let local_peer = peer_id_from_ed25519_public_key_v1(
            config
                .validators
                .member(&config.local_validator)
                .context("local validator missing")?
                .public_key(),
        );
        let collector = VoteCollector::new(
            journal.context(),
            config.validators.clone(),
            journal.round(),
            config.collector,
        )?;
        let pacemaker = Pacemaker::new(config.timeouts)?;
        let replay = journal.take_replay_records();
        let last = if replay.is_empty() {
            journal.last_durable_message().cloned()
        } else {
            None
        };
        let mut this = Self {
            config,
            journal,
            channel,
            collector,
            pacemaker,
            local_peer,
            bodies: BTreeMap::new(),
            offers: BTreeMap::new(),
            inflight: Vec::new(),
            successor: None,
            successor_drain: None,
            early: None,
            early_drain: None,
            early_attempted_scope: None,
            preparing: BTreeMap::new(),
            fixed: VecDeque::new(),
            archives: BTreeMap::new(),
            archive_requests: BTreeMap::new(),
            peer_requests: BTreeMap::new(),
            next_token: 0,
            local_preparing: false,
            proposed_round: None,
            pending_value: None,
            execution_turn: 0,
            archive_turn: 0,
            retired: VecDeque::new(),
            request_due: None,
            now: None,
            stats: ControllerStats::default(),
            recovery: None,
            recovery_failure: None,
            #[cfg(test)]
            recovery_test_ingress: None,
        };
        this.initialize_recovery(replay)?;
        // Compatibility for an acknowledged in-memory nil vote. Non-nil warm
        // journals need explicit reopening to supply their complete replay roots.
        match last {
            Some(DurableMessage::Vote(vote)) => {
                ensure!(
                    vote.value.is_none() && this.journal.valid_certificate().is_none(),
                    "undecided journal replay dependencies not loaded; reopen explicitly"
                );
                this.prepare(
                    PrepareInput::New(Arc::new(Message::Vote(vote))),
                    Purpose::Broadcast,
                )?;
            }
            Some(DurableMessage::Proposal(_)) => {
                anyhow::bail!(
                    "undecided proposal replay dependencies not loaded; reopen explicitly"
                )
            }
            _ => {}
        }
        Ok(this)
    }

    pub fn context(&self) -> Context {
        self.journal.context()
    }
    pub fn parent(&self) -> ParentPoint {
        self.journal.parent()
    }
    pub fn head(&self) -> Option<ParentPoint> {
        self.journal.head()
    }
    pub fn round(&self) -> u64 {
        self.journal.round()
    }
    pub fn step(&self) -> TimeoutStep {
        self.journal.step()
    }
    pub fn is_pending(&self) -> bool {
        self.journal.is_pending()
    }
    pub fn is_recovering(&self) -> bool {
        self.recovery.is_some() || self.recovery_failure.is_some()
    }
    pub fn stats(&self) -> &ControllerStats {
        &self.stats
    }
    pub fn is_local_leader(&self) -> Result<bool> {
        Ok(self
            .config
            .validators
            .leader(self.context().height, self.round())?
            == self.config.local_validator)
    }
    pub fn shutdown(&mut self) -> Result<()> {
        self.channel.shutdown()
    }

    /// Only O(1) domain/parent checks and bounded owner admission here. Caller
    /// retains its Arc on backpressure. No raw cloning/hash/BatchRequest work.
    pub fn try_submit_body(&mut self, message: &Arc<Message>) -> Result<bool> {
        let Message::Body { context, .. } = message.as_ref() else {
            anyhow::bail!("local input is not a body");
        };
        ensure!(
            self.matches_context(context),
            "local body does not extend exact configured parent/business"
        );
        if self.is_recovering()
            || !self.retired.is_empty()
            || !self.is_local_leader()?
            || self.journal.decided().is_some()
            || self.journal.step() != TimeoutStep::Propose
            || self.local_preparing
            || self.proposed_round == Some(self.round())
            || self.journal.valid_certificate().is_some()
            || self
                .bodies
                .values()
                .any(|body| body.local_round == Some(self.round()) && !body.failed)
        {
            return Ok(false);
        }
        if self.preparing.len() >= self.preparation_limit() {
            return Ok(false);
        }
        let token = self.allocate_token()?;
        let request = PrepareRequest {
            token,
            input: PrepareInput::New(message.clone()),
        };
        match self.channel.try_prepare(request)? {
            PrepareAdmission::Accepted => {
                self.preparing.insert(
                    token,
                    Preparation {
                        request: None,
                        purpose: Purpose::LocalBody {
                            context: self.context(),
                            round: self.round(),
                        },
                    },
                );
                self.local_preparing = true;
                Ok(true)
            }
            PrepareAdmission::Backpressure(_) => Ok(false),
            PrepareAdmission::Rejected { reason, .. } => anyhow::bail!(reason),
        }
    }

    /// Bounded scheduling only. No wait/sleep/socket/full-body operation. The
    /// caller keeps servicing ingress/query paths while AOEM or storage runs.
    pub fn poll(&mut self, pipeline: &CandidatePipeline, now: Instant) -> Result<()> {
        ensure!(
            self.recovery_failure.is_none(),
            "controller cold recovery failed; explicit restart required: {:?}",
            self.recovery_failure
        );
        let recovering = self.recovery.is_some();
        let result = self.poll_inner(pipeline, now);
        if recovering {
            if let Err(error) = &result {
                self.recovery_failure = Some(error.to_string().chars().take(256).collect());
            }
        }
        result
    }

    fn poll_inner(&mut self, pipeline: &CandidatePipeline, now: Instant) -> Result<()> {
        ensure!(
            self.now.is_none_or(|previous| now >= previous),
            "controller clock moved backwards"
        );
        self.now = Some(now);
        let recovering = self.recovery.is_some();
        self.flush_retired()?;
        self.poll_journal(pipeline)?;
        self.poll_executions()?;
        if recovering {
            // Service one bounded cold step BEFORE this poll's ingress can
            // append fresh retirement. Otherwise even one control message per
            // poll could indefinitely starve recovery without owner congestion.
            self.poll_recovery(pipeline)?;
        } else if self.retired.is_empty() {
            // Give already-ready data work a bounded turn BEFORE fresh control
            // ingress can append retirement. A received vote/request awaiting
            // its first owner handoff is not evidence of owner congestion; if
            // it gated this work later in poll, steady ingress could starve
            // execution/archive progress forever. Genuine pending retirement
            // still backpressures admission, and new bodies wait one poll.
            self.submit_executions(pipeline)?;
            // Accepted lookahead may now target the current height. Give it
            // one bounded turn before fresh control ingress adds retirement;
            // otherwise even a healthy owner could starve its exact binding.
            self.poll_early(pipeline)?;
            self.poll_archives(pipeline)?;
            if self.retired.is_empty() {
                self.request_missing(now)?;
            }
        }
        for _ in 0..self.config.limits.events_per_poll {
            if !self.retired.is_empty() {
                break;
            }
            let event = self.channel.try_recv()?;
            // Do not let the test's synthetic ingress timing bypass the real
            // channel's independent local-reply progress. The queued control
            // remains present on every poll until its ordinary slot is served.
            #[cfg(test)]
            let event = event.or_else(|| {
                self.recovery_test_ingress
                    .take()
                    .map(ChannelEvent::Received)
            });
            let Some(event) = event else {
                break;
            };
            match event {
                ChannelEvent::Prepared { token, result } => self.prepared(token, result)?,
                ChannelEvent::Received(received) => self.receive(received.peer, received.ready)?,
            }
        }
        if !recovering {
            // Even the poll that completes recovery must not start a timer or
            // sign: normal consensus resumes on the following poll only.
            // Eligible execution/proposal/QC always wins over a failure timer.
            self.drive_consensus()?;
            self.pacemaker
                .poll(&mut self.journal, &self.collector, now)?;
            self.poll_successor(pipeline)?;
        }
        self.flush_preparations()?;
        self.flush_sends(now)?;
        self.flush_retired()?;
        self.stats.prevote_weight = self.collector.phase_weight(self.round(), Phase::Prevote);
        self.stats.precommit_weight = self.collector.phase_weight(self.round(), Phase::Precommit);
        self.stats.retained_bodies = self.bodies.len();
        self.stats.retained_body_bytes = self.body_bytes();
        self.stats.inflight = self.inflight_count();
        Ok(())
    }

    fn matches_context(&self, body: &BatchContext) -> bool {
        matches_context(body, &self.config.execution, self.context(), self.parent())
    }

    fn preparation_limit(&self) -> usize {
        self.config.limits.max_proposals * 4 + self.config.peers.len() * 3 + 4
    }
    fn fixed_limit(&self) -> usize {
        self.preparation_limit() + self.config.limits.max_bodies
    }
    fn allocate_token(&mut self) -> Result<u64> {
        self.next_token = self
            .next_token
            .checked_add(1)
            .context("controller token exhausted")?;
        Ok(self.next_token)
    }
    fn prepare(&mut self, input: PrepareInput, purpose: Purpose) -> Result<bool> {
        if self.preparing.len() >= self.preparation_limit() {
            self.stats.rejected_messages += 1;
            self.retire(Retirement::Input(input));
            return Ok(false);
        }
        let token = self.allocate_token()?;
        self.preparing.insert(
            token,
            Preparation {
                request: Some(PrepareRequest { token, input }),
                purpose,
            },
        );
        Ok(true)
    }
    fn flush_preparations(&mut self) -> Result<()> {
        for pending in self
            .preparing
            .values_mut()
            .take(self.config.limits.events_per_poll)
        {
            let Some(request) = pending.request.take() else {
                continue;
            };
            match self.channel.try_prepare(request)? {
                PrepareAdmission::Accepted => {}
                PrepareAdmission::Backpressure(request) => pending.request = Some(request),
                PrepareAdmission::Rejected { request, reason } => {
                    pending.request = Some(request);
                    anyhow::bail!("controller preparation rejected: {reason}")
                }
            }
        }
        Ok(())
    }

    fn prepared(&mut self, token: u64, result: std::result::Result<Ready, String>) -> Result<()> {
        let Some(pending) = self.preparing.remove(&token) else {
            self.stats.rejected_messages += 1;
            if let Ok(ready) = result {
                self.retire(Retirement::Ready(ready));
            }
            return Ok(());
        };
        if matches!(pending.purpose, Purpose::LocalBody { .. }) {
            self.local_preparing = false;
        }
        let ready = match result {
            Ok(ready) => ready,
            Err(error) => {
                if let Purpose::EarlyBody(generation) | Purpose::BindEarly(generation) =
                    &pending.purpose
                {
                    self.early_preparation_failed(*generation);
                }
                if let Purpose::SuccessorBody(pin) = &pending.purpose {
                    self.successor_preparation_failed(*pin);
                }
                if matches!(pending.purpose, Purpose::RecoveryBody(_) | Purpose::Replay) {
                    anyhow::bail!("cold replay preparation failed: {error}");
                }
                match &pending.purpose {
                    Purpose::ArchiveBody { peer, .. } | Purpose::ArchiveDecision(peer) => {
                        self.archive_requests.remove(peer);
                    }
                    _ => {}
                }
                self.reject(error);
                return Ok(());
            }
        };
        match pending.purpose {
            Purpose::EarlyBody(generation) => self.early_prepared(generation, ready)?,
            Purpose::BindEarly(generation) => self.early_bound_prepared(generation, ready)?,
            Purpose::EarlyBinding(origin) => {
                if self.live_early_origin(origin) {
                    self.cache(ready.prepared.clone(), None);
                }
                self.retire(Retirement::Ready(ready));
            }
            Purpose::SuccessorBody(pin) => self.successor_prepared(pin, ready)?,
            Purpose::RecoveryBody(locator) => self.recovered_body(locator, ready)?,
            Purpose::LocalBody { context, round } => {
                if context != self.context() || round != self.round() {
                    self.stats.stale_results += 1;
                    self.retire(Retirement::Ready(ready));
                    return Ok(());
                }
                self.keep_body(self.local_peer.clone(), ready, Some(round))?;
            }
            Purpose::Broadcast | Purpose::Replay => {
                if message_context(&ready.message).is_some_and(|context| context != self.context())
                {
                    self.retire(Retirement::Ready(ready));
                    return Ok(());
                }
                if let Message::RequestBody { body_id } = ready.message.as_ref() {
                    if !self.offers.values().any(|offer| offer.body_id == *body_id) {
                        self.retire(Retirement::Ready(ready));
                        return Ok(());
                    }
                }
                let prepared = ready.prepared.clone();
                self.receive(self.local_peer.clone(), ready)?;
                self.cache(prepared, None);
            }
            Purpose::ArchiveDecision(peer) => {
                if message_context(&ready.message).is_some_and(|context| {
                    self.peer_requests
                        .get(&peer)
                        .is_some_and(|requested| *requested != context)
                }) {
                    self.archive_requests.remove(&peer);
                    self.retire(Retirement::Ready(ready));
                    return Ok(());
                }
                if !self.cache(ready.prepared.clone(), Some(peer.clone())) {
                    self.archive_requests.remove(&peer);
                }
                self.retire(Retirement::Ready(ready));
            }
            Purpose::ArchiveBody {
                peer,
                proposal,
                certificate,
            } => {
                let Some(id) = ready.prepared.body_id() else {
                    self.retire(Retirement::Ready(ready));
                    anyhow::bail!("archive owner did not return body id");
                };
                if self
                    .peer_requests
                    .get(&peer)
                    .is_some_and(|requested| *requested != proposal.context)
                {
                    self.archive_requests.remove(&peer);
                    self.retire(Retirement::Ready(ready));
                    return Ok(());
                }
                if !self.cache_archive_body(ready.prepared.clone(), peer.clone())
                    || !self.prepare(
                        PrepareInput::New(Arc::new(Message::Decision {
                            proposal,
                            certificate,
                            body_id: id,
                        })),
                        Purpose::ArchiveDecision(peer.clone()),
                    )?
                {
                    self.archive_requests.remove(&peer);
                }
                self.retire(Retirement::Ready(ready));
            }
        }
        Ok(())
    }

    fn receive(&mut self, source: String, ready: Ready) -> Result<()> {
        if source != self.local_peer && !self.config.peers.values().any(|peer| peer == &source) {
            self.reject("unconfigured authenticated peer");
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        if matches!(ready.message.as_ref(), Message::EarlyBody { .. }) {
            return self.keep_early(source, ready, None);
        }
        if matches!(ready.message.as_ref(), Message::Body { .. }) {
            if self.is_recovering() {
                // Replay roots have reserved precedence. Peers retain their
                // signed hints and can answer RequestBody after recovery.
                self.retire(Retirement::Ready(ready));
                return Ok(());
            }
            if self.is_successor_body(&ready) {
                return self.keep_successor_body(source, ready, false);
            }
            return self.keep_body(source, ready, None);
        }
        let result = self.receive_control(source, &ready);
        self.retire(Retirement::Ready(ready));
        result
    }

    fn receive_control(&mut self, source: String, ready: &Ready) -> Result<()> {
        match ready.message.as_ref() {
            Message::Body { .. } => unreachable!("body routed separately"),
            Message::EarlyBody { .. } => unreachable!("early body routed separately"),
            Message::BindBody {
                scope,
                announcement_id,
                context,
            } => {
                self.receive_early_binding(&source, *scope, *announcement_id, *context)?;
            }
            Message::Vote(vote) => {
                if !matches!(ready.evidence.as_ref(), VerifiedEvidence::Vote(_)) {
                    self.reject("vote missing owner evidence");
                    return Ok(());
                }
                match self.collector.insert(vote) {
                    Ok(VoteInsert::Inserted) => self.stats.received_votes += 1,
                    Ok(VoteInsert::Duplicate) => {}
                    Err(error) => self.reject(error),
                }
            }
            Message::Proposal { body_id, .. } => {
                let VerifiedEvidence::Proposal {
                    proposal,
                    valid_quorum,
                } = ready.evidence.as_ref()
                else {
                    self.reject("proposal evidence missing");
                    return Ok(());
                };
                let p = proposal.proposal();
                // The hint is NOT signed. Only the scheduled proposer's own
                // authenticated stream may supply a current proposal hint.
                let expected = self.peer_for(&p.proposer_id);
                if p.context != self.context()
                    || p.round != self.round()
                    || expected != Some(source.as_str())
                {
                    self.reject("proposal context/round/hint source mismatch");
                    return Ok(());
                }
                self.offers.insert(
                    (source, false),
                    Offered {
                        proposal: proposal.clone(),
                        valid: valid_quorum.clone(),
                        decision: None,
                        body_id: *body_id,
                    },
                );
            }
            Message::Decision { body_id, .. } => {
                let VerifiedEvidence::Decision {
                    proposal,
                    certificate,
                } = ready.evidence.as_ref()
                else {
                    self.reject("decision evidence missing");
                    return Ok(());
                };
                if proposal.proposal().context != self.context() {
                    self.reject("decision not for exact current parent");
                    return Ok(());
                }
                // Each peer has its own replaceable hint slot. A relayed valid
                // QC can fetch from ANY peer; a bad first hint cannot poison it.
                self.offers.insert(
                    (source, true),
                    Offered {
                        proposal: proposal.clone(),
                        valid: None,
                        decision: Some(certificate.clone()),
                        body_id: *body_id,
                    },
                );
            }
            Message::RequestBody { body_id } => {
                let prepared = self
                    .bodies
                    .get(body_id)
                    .map(|body| body.prepared.clone())
                    .or_else(|| {
                        self.fixed
                            .iter()
                            .find(|fixed| {
                                fixed.destination.as_ref() == Some(&source)
                                    && fixed.prepared.body_id() == Some(*body_id)
                            })
                            .map(|fixed| fixed.prepared.clone())
                    });
                if let Some(prepared) = prepared {
                    self.cache(prepared, Some(source));
                }
            }
            Message::RequestDecision { context } => {
                // Do not let remote archive reads occupy cold replay's sole
                // bounded I/O slot. The existing requester retries this hint.
                if !self.is_recovering() {
                    self.request_archive(source, *context)?;
                }
            }
        }
        Ok(())
    }

    fn peer_for(&self, validator: &Hash) -> Option<&str> {
        if validator == &self.config.local_validator {
            Some(&self.local_peer)
        } else {
            self.config.peers.get(validator).map(String::as_str)
        }
    }
    fn reject(&mut self, error: impl std::fmt::Display) {
        self.stats.rejected_messages += 1;
        self.stats.last_error = Some(error.to_string().chars().take(256).collect());
    }
    fn retained_bodies(&self) -> BTreeMap<Hash, usize> {
        self.bodies
            .values()
            .map(|body| (body.prepared.fragment_id(), body.prepared.retained_bytes()))
            .chain(self.successor.as_ref().and_then(Successor::retained_body))
            .chain(self.early.iter().flat_map(EarlyWork::retained_bodies))
            .chain(
                self.fixed
                    .iter()
                    .filter(|fixed| {
                        matches!(
                            fixed.prepared.message().as_ref(),
                            Message::Body { .. } | Message::EarlyBody { .. }
                        )
                    })
                    .map(|fixed| {
                        (
                            fixed.prepared.fragment_id(),
                            fixed.prepared.retained_bytes(),
                        )
                    }),
            )
            .collect()
    }
    fn body_bytes(&self) -> usize {
        self.retained_bodies().values().sum::<usize>()
            + self.recovery.as_ref().map_or(0, Recovery::retained_bytes)
    }
    fn pinned(&self, id: &Hash) -> bool {
        self.bodies.get(id).is_some_and(|body| body.recovered)
            || self.inflight.iter().any(|work| &work.id == id)
            || self
                .bodies
                .get(id)
                .and_then(|body| body.candidate.as_ref())
                .is_some_and(|(value, _)| {
                    self.pending_value == Some(*value)
                        || self
                            .offers
                            .values()
                            .any(|offer| offer.proposal.proposal().value == *value)
                        || self.journal.valid_certificate().is_some_and(|qc| {
                            qc.votes
                                .first()
                                .is_some_and(|vote| vote.value == Some(*value))
                        })
                })
    }
    fn keep_body(
        &mut self,
        source: String,
        mut ready: Ready,
        local_round: Option<u64>,
    ) -> Result<()> {
        let Some(body) = ready.body.as_ref() else {
            self.reject("body owner request missing");
            self.retire(Retirement::Ready(ready));
            return Ok(());
        };
        if !self.matches_context(body.context()) {
            self.reject("body parent/business pins mismatch");
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        let id = body.id();
        if let Some(existing) = self.bodies.get_mut(&id) {
            if local_round.is_some() {
                existing.local_round = local_round;
            }
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        // One unpinned body slot per authenticated peer, plus separately pinned
        // executing/valid candidates. Replacement does not cancel any ticket.
        let old: Vec<_> = self
            .bodies
            .iter()
            .filter(|(id, b)| b.source == source && !self.pinned(id))
            .map(|(id, _)| *id)
            .collect();
        for id in old {
            if let Some(body) = self.bodies.remove(&id) {
                self.retire_body(body);
            }
        }
        let charge = ready.prepared.retained_bytes();
        self.preempt_early_for_body(charge);
        self.preempt_successor_for_body(charge)?;
        if self.retained_bodies().len() >= self.config.limits.max_bodies
            || charge
                > self
                    .config
                    .limits
                    .max_body_bytes
                    .saturating_sub(self.body_bytes())
        {
            self.reject("controller retained body budget");
            self.retire(Retirement::Ready(ready));
            return Ok(());
        }
        let mut body = ready.body.take().expect("checked body");
        self.bodies.insert(
            id,
            Body {
                source,
                prepared: ready.prepared,
                request: body.take_request(),
                authenticated: None,
                early_origin: None,
                candidate: None,
                failed: false,
                local_round,
                recovered: false,
            },
        );
        Ok(())
    }

    fn poll_executions(&mut self) -> Result<()> {
        let mut index = 0;
        while index < self.inflight.len() {
            let result = self.inflight[index].ticket.try_take();
            if matches!(result, Ok(None)) {
                index += 1;
                continue;
            }
            let work = self.inflight.swap_remove(index);
            let batch = match result {
                Ok(Some(batch)) => batch,
                Err(error) => {
                    self.stats.execution_failures += 1;
                    if let Some(body) = self.bodies.get_mut(&work.id) {
                        body.failed = true;
                    }
                    self.stats.last_error = Some(error.to_string().chars().take(256).collect());
                    if work.recovery.is_some() {
                        return Err(error.context("cold candidate re-execution failed"));
                    }
                    continue;
                }
                Ok(None) => unreachable!(),
            };
            self.stats.executed_batches += 1;
            // Count every successful completion, even when it is drained as
            // stale below. These are execution costs, not finalized throughput.
            self.stats.observe_execution(batch.observation);
            if work.context != self.context()
                || work.parent != self.parent()
                || !self.matches_context(batch.packet.context())
            {
                self.stats.stale_results += 1;
                self.retire(Retirement::Batch(batch));
                ensure!(
                    work.recovery.is_none(),
                    "cold execution changed its pinned parent"
                );
                continue;
            }
            let statement = match BlockStatement::from_executed(
                &batch.packet,
                self.context(),
                &self.config.validators,
                &self.parent(),
            ) {
                Ok(statement) => statement,
                Err(error) => {
                    self.retire(Retirement::Batch(batch));
                    return Err(error);
                }
            };
            if let Some(locator) = work.recovery {
                if statement.hash() != locator.value
                    || batch.packet.candidate_id() != locator.candidate_id
                    || batch.packet.document_digest() != locator.document_digest
                    || !self.bodies.contains_key(&work.id)
                {
                    self.retire(Retirement::Batch(batch));
                    anyhow::bail!("cold re-execution differs from durable replay locator");
                }
            }
            if let Some(body) = self.bodies.get_mut(&work.id) {
                body.candidate = Some((statement.hash(), batch.candidate().clone()));
            } else {
                self.stats.stale_results += 1;
            }
            self.retire(Retirement::Batch(batch));
        }
        Ok(())
    }
    fn submit_executions(&mut self, pipeline: &CandidatePipeline) -> Result<()> {
        if self.journal.decided().is_some() {
            return Ok(());
        }
        let leader = self.is_local_leader()?;
        let round = self.round();
        let context = self.context();
        let parent = self.parent();
        let requesters: Vec<_> = std::iter::once(self.local_peer.clone())
            .chain(self.config.peers.values().cloned())
            .collect();
        for _ in 0..requesters.len() {
            if self.inflight_count() >= self.config.limits.max_inflight {
                break;
            }
            let requester = requesters[self.execution_turn % requesters.len()].clone();
            self.execution_turn = (self.execution_turn + 1) % requesters.len();
            if self.inflight.iter().any(|work| work.requester == requester) {
                continue;
            }
            let id = self.bodies.iter().find_map(|(id, body)| {
                let demanded =
                    (requester == self.local_peer && leader && body.local_round == Some(round))
                        || self
                            .offers
                            .iter()
                            .any(|((peer, _), offer)| peer == &requester && offer.body_id == *id);
                (demanded
                    && !body.failed
                    && body.candidate.is_none()
                    && (body.request.is_some() || body.authenticated.is_some())
                    && !self.inflight.iter().any(|work| work.id == *id))
                .then_some(*id)
            });
            let Some(id) = id else {
                continue;
            };
            let body = self.bodies.get_mut(&id).expect("selected body");
            if let Some(request) = body.authenticated.take() {
                ensure!(body.request.is_none(), "body has two execution inputs");
                match pipeline.try_submit_authenticated_owned(request) {
                    Ok(AuthenticatedSubmission::Accepted(ticket)) => {
                        self.inflight.push(Execution {
                            id,
                            context,
                            parent,
                            requester,
                            ticket,
                            recovery: None,
                        })
                    }
                    Ok(AuthenticatedSubmission::Backpressured(request)) => {
                        body.authenticated = Some(request);
                        break;
                    }
                    Err(rejected) => {
                        self.retire(Retirement::AuthenticatedRequest(rejected.request));
                        return Err(rejected.error);
                    }
                }
                continue;
            }
            let Some(request) = body.request.take() else {
                continue;
            };
            match pipeline.try_submit_owned(request) {
                Ok(Submission::Accepted(ticket)) => self.inflight.push(Execution {
                    id,
                    context,
                    parent,
                    requester,
                    ticket,
                    recovery: None,
                }),
                Ok(Submission::Backpressured(request)) => {
                    body.request = Some(request);
                    break;
                }
                Err(rejected) => {
                    self.retire(Retirement::Request(rejected.request));
                    return Err(rejected.error);
                }
            }
        }
        Ok(())
    }
    fn candidate(&self, value: Hash) -> Option<(Hash, DurableCandidate)> {
        self.bodies.iter().find_map(|(id, body)| {
            body.candidate
                .as_ref()
                .filter(|(found, _)| *found == value)
                .map(|(_, candidate)| (*id, candidate.clone()))
        })
    }

    fn drive_consensus(&mut self) -> Result<()> {
        // Pin a completed old-round decision certificate BEFORE retiring vote
        // buckets, even when the corresponding body has not executed yet.
        let mut proofs = Vec::new();
        for ((peer, decision), offer) in &self.offers {
            if !decision {
                let p = offer.proposal.proposal();
                if self.offers.get(&(peer.clone(), true)).is_some_and(|known| {
                    known.proposal.proposal().round == p.round
                        && known.proposal.proposal().value == p.value
                }) {
                    continue;
                }
                if let Some(qc) = self
                    .collector
                    .quorum(p.round, Phase::Precommit, Some(p.value))?
                {
                    proofs.push((
                        peer.clone(),
                        Offered {
                            proposal: offer.proposal.clone(),
                            valid: None,
                            decision: Some(qc),
                            body_id: offer.body_id,
                        },
                    ));
                }
            }
        }
        for (peer, proof) in proofs {
            self.offers.insert((peer, true), proof);
        }
        if self.journal.is_pending() {
            return Ok(());
        }
        if self.journal.decided().is_some() {
            self.journal.advance_height()?;
            return Ok(());
        }
        // An old-round decision remains eligible after a round change. No
        // body hint or QC bypasses the current local execution capability.
        for offer in self.offers.values() {
            let p = offer.proposal.proposal();
            let Some((_, candidate)) = self.candidate(p.value) else {
                continue;
            };
            let decision = match &offer.decision {
                Some(qc) => Some(qc.clone()),
                None => self
                    .collector
                    .quorum(p.round, Phase::Precommit, Some(p.value))?,
            };
            if let Some(qc) = decision {
                self.journal
                    .observe_decision(&offer.proposal, &candidate, &qc)?;
                self.pending_value = Some(p.value);
                return Ok(());
            }
        }
        if self.journal.step() == TimeoutStep::Propose {
            if let Some(offer) = self.offers.values().find(|offer| {
                offer.proposal.proposal().round == self.round()
                    && offer.decision.is_none()
                    && self.candidate(offer.proposal.proposal().value).is_some()
            }) {
                let candidate = self
                    .candidate(offer.proposal.proposal().value)
                    .expect("checked candidate")
                    .1;
                self.journal
                    .accept_proposal(&offer.proposal, &candidate, offer.valid.as_ref())?;
                self.pending_value = Some(offer.proposal.proposal().value);
                return Ok(());
            }
            if self.is_local_leader()? && self.proposed_round != Some(self.round()) {
                let valid = self.journal.verified_valid_certificate();
                let candidate = match &valid {
                    Some(qc) => qc.value().and_then(|value| self.candidate(value)),
                    None => self.bodies.iter().find_map(|(id, body)| {
                        (body.local_round == Some(self.round()))
                            .then_some(body.candidate.as_ref())
                            .flatten()
                            .map(|(_, candidate)| (*id, candidate.clone()))
                    }),
                };
                if let Some((_, candidate)) = candidate {
                    self.journal.propose(&candidate, valid.as_deref())?;
                    self.pending_value = Some(
                        BlockStatement::from_executed(
                            candidate.packet(),
                            self.context(),
                            &self.config.validators,
                            &self.parent(),
                        )?
                        .hash(),
                    );
                    return Ok(());
                }
            }
        } else {
            let round = self.round();
            let already_valid = self
                .journal
                .valid_certificate()
                .is_some_and(|qc| qc.votes.first().is_some_and(|vote| vote.round == round));
            if !already_valid {
                for offer in self
                    .offers
                    .values()
                    .filter(|offer| offer.proposal.proposal().round == round)
                {
                    let value = offer.proposal.proposal().value;
                    let Some((_, candidate)) = self.candidate(value) else {
                        continue;
                    };
                    if let Some(qc) = self.collector.quorum(round, Phase::Prevote, Some(value))? {
                        self.journal
                            .observe_prevotes(&qc, Some((&offer.proposal, &candidate)))?;
                        self.pending_value = Some(value);
                        return Ok(());
                    }
                }
            }
            if self.journal.step() == TimeoutStep::Prevote {
                if let Some(qc) = self.collector.quorum(round, Phase::Prevote, None)? {
                    self.journal.observe_prevotes(&qc, None)?;
                }
            }
        }
        Ok(())
    }

    fn poll_journal(&mut self, pipeline: &CandidatePipeline) -> Result<()> {
        let old_context = self.context();
        let old_round = self.round();
        let Some(message) = self.journal.poll(pipeline)? else {
            return Ok(());
        };
        if self.context() != old_context {
            self.collector.reset_context(self.context(), self.round())?;
            for (_, body) in std::mem::take(&mut self.bodies) {
                self.retire_body(body);
            }
            self.offers.clear();
            self.proposed_round = None;
            self.local_preparing = false;
            self.request_due = None;
            // Outstanding pipeline tickets are intentionally retained/drained.
            // Historical direct replay caches survive height advancement.
            let current = self.context();
            let parent = self.parent();
            self.prune_fixed(|fixed| {
                fixed.destination.is_some()
                    || early_body::survives_parent_ack(fixed.prepared.message(), current, parent)
            });
            self.stats.head_advances += 1;
        } else if self.round() != old_round {
            self.collector
                .advance_round_and_retire(self.round(), self.round())?;
            self.proposed_round = None;
            self.request_due = None;
            // Retain executed valid candidates and exact old decision evidence.
            self.offers.retain(|_, offer| {
                offer.decision.is_some() || offer.proposal.proposal().round == self.journal.round()
            });
            let keep: Vec<_> = self
                .bodies
                .keys()
                .copied()
                .filter(|id| self.pinned(id))
                .collect();
            let discard: Vec<_> = self
                .bodies
                .keys()
                .filter(|id| !keep.contains(id))
                .copied()
                .collect();
            for id in discard {
                if let Some(body) = self.bodies.remove(&id) {
                    self.retire_body(body);
                }
            }
            let round = self.round();
            self.prune_fixed(|fixed| {
                fixed.destination.is_some()
                    || message_round(fixed.prepared.message()) == Some(round)
            });
        }
        self.reconcile_successor()?;
        self.reconcile_early(old_context, old_round)?;
        if let Some(message) = message {
            let message = match message {
                DurableMessage::Vote(vote) => {
                    self.stats.durable_votes += 1;
                    Message::Vote(vote)
                }
                DurableMessage::Proposal(proposal) => {
                    self.proposed_round = Some(proposal.round);
                    let (id, _) = self
                        .candidate(proposal.value)
                        .context("durable proposal lost executed candidate")?;
                    if let Some(body) = self
                        .bodies
                        .get(&id)
                        .filter(|body| body.early_origin.is_none())
                    {
                        self.cache(body.prepared.clone(), None);
                    }
                    Message::Proposal {
                        proposal,
                        valid_quorum: self.journal.valid_certificate().cloned(),
                        body_id: id,
                    }
                }
                DurableMessage::Decision {
                    proposal,
                    certificate,
                } => {
                    self.stats.durable_decisions += 1;
                    let (id, _) = self
                        .candidate(proposal.value)
                        .context("durable decision lost executed candidate")?;
                    Message::Decision {
                        proposal,
                        certificate,
                        body_id: id,
                    }
                }
            };
            ensure!(
                self.prepare(PrepareInput::New(Arc::new(message)), Purpose::Broadcast)?,
                "durable outbox preparation budget exhausted"
            );
        }
        self.pending_value = None;
        Ok(())
    }

    fn cache_archive_body(&mut self, prepared: PreparedMessage, peer: String) -> bool {
        let id = prepared.fragment_id();
        if !self.cache(prepared, Some(peer.clone())) {
            return false;
        }
        // A decision query is not a body request. Cache the owner-prepared
        // archive so an exact RequestBody can wake it, but do not send the full
        // block on every small decision retry from a peer that may already have
        // executed it. Lost requests/replies use the normal request retry path.
        if let Some(fixed) = self.fixed.iter_mut().find(|fixed| {
            fixed.destination.as_ref() == Some(&peer) && fixed.prepared.fragment_id() == id
        }) {
            fixed.schedule.active = false;
        }
        true
    }

    fn cache(&mut self, prepared: PreparedMessage, destination: Option<String>) -> bool {
        let id = prepared.fragment_id();
        if let Some(old) = self
            .fixed
            .iter_mut()
            .find(|fixed| fixed.prepared.fragment_id() == id && fixed.destination == destination)
        {
            old.schedule.wake();
            self.retire(Retirement::Prepared(prepared));
            return true;
        }
        // A peer's archive response replaces only that peer's same-kind slot.
        if destination.is_some() {
            let body = matches!(prepared.message().as_ref(), Message::Body { .. });
            self.prune_fixed(|old| {
                old.destination != destination
                    || matches!(old.prepared.message().as_ref(), Message::Body { .. }) != body
            });
        }
        if matches!(
            prepared.message().as_ref(),
            Message::Body { .. } | Message::EarlyBody { .. }
        ) {
            let retained = self.retained_bodies();
            if !retained.contains_key(&id)
                && (retained.len() >= self.config.limits.max_bodies
                    || prepared.retained_bytes()
                        > self
                            .config
                            .limits
                            .max_body_bytes
                            .saturating_sub(retained.values().sum()))
            {
                self.reject("fixed body byte budget exhausted");
                self.retire(Retirement::Prepared(prepared));
                return false;
            }
        }
        if self.fixed.len() >= self.fixed_limit() {
            self.reject("fixed retransmission cache full");
            self.retire(Retirement::Prepared(prepared));
            return false;
        }
        let body_fanout = (destination.is_none()
            && matches!(
                prepared.message().as_ref(),
                Message::Body { .. } | Message::EarlyBody { .. }
            ))
        .then(|| BodyFanout::new(self.config.peers.len()));
        self.fixed.push_back(Fixed {
            prepared,
            destination,
            schedule: RetrySchedule::new(),
            body_fanout,
        });
        true
    }
    fn flush_sends(&mut self, now: Instant) -> Result<()> {
        let peers: Vec<_> = self.config.peers.values().cloned().collect();
        for _ in 0..self.config.limits.sends_per_poll.min(self.fixed.len()) {
            let Some(mut fixed) = self.fixed.pop_front() else {
                break;
            };
            if fixed
                .body_fanout
                .as_ref()
                .is_some_and(|fanout| fanout.remaining == 0)
            {
                fixed.schedule.active = false;
            }
            if fixed.schedule.active && fixed.schedule.due.is_none_or(|due| now >= due) {
                let peer = fixed
                    .destination
                    .clone()
                    .or_else(|| peers.get(fixed.schedule.next_peer).cloned());
                if let Some(peer) = peer {
                    let needed = fixed
                        .body_fanout
                        .as_ref()
                        .is_none_or(|fanout| fanout.needs(fixed.schedule.next_peer));
                    let admitted = if !needed {
                        false
                    } else {
                        match self.channel.try_send(Outbound {
                            peer,
                            message: fixed.prepared.clone(),
                        })? {
                            SendAdmission::Accepted => true,
                            SendAdmission::Backpressure(_) => false,
                            SendAdmission::Rejected { reason, .. } => {
                                self.reject(reason);
                                false
                            }
                        }
                    };
                    if admitted {
                        if let Some(fanout) = &mut fixed.body_fanout {
                            fanout.accepted(fixed.schedule.next_peer);
                        }
                    }
                    // An offline/full peer cannot stop this fixed broadcast
                    // from reaching the remaining healthy peers. All peers are
                    // retried next cycle; admission is not delivery evidence.
                    fixed.schedule.attempted(
                        fixed.destination.is_some(),
                        admitted,
                        peers.len(),
                        now,
                        self.config.retransmit,
                    )?;
                }
            }
            self.fixed.push_back(fixed);
        }
        Ok(())
    }

    fn request_missing(&mut self, now: Instant) -> Result<()> {
        if self.request_due.is_some_and(|due| now < due) {
            return Ok(());
        }
        self.request_due = Some(
            now.checked_add(self.config.retransmit)
                .context("request time overflow")?,
        );
        self.prepare(
            PrepareInput::New(Arc::new(Message::RequestDecision {
                context: self.context(),
            })),
            Purpose::Broadcast,
        )?;
        let missing: Vec<_> = self
            .offers
            .values()
            .filter(|offer| {
                self.candidate(offer.proposal.proposal().value).is_none()
                    && !self.bodies.contains_key(&offer.body_id)
            })
            .map(|offer| offer.body_id)
            .collect();
        // Unsigned hint replacement must not grow an unbounded catalogue of
        // obsolete body requests or evict the fixed durable vote/proposal slots.
        self.prune_fixed(|fixed| {
            fixed.destination.is_some()
                || match fixed.prepared.message().as_ref() {
                    Message::RequestBody { body_id } => missing.contains(body_id),
                    _ => true,
                }
        });
        for id in missing {
            self.prepare(
                PrepareInput::New(Arc::new(Message::RequestBody { body_id: id })),
                Purpose::Broadcast,
            )?;
        }
        Ok(())
    }
    fn request_archive(&mut self, peer: String, requested: Context) -> Result<()> {
        let current = self.context();
        if !same_domain(requested, current) {
            self.reject("archive request domain mismatch");
            return Ok(());
        }
        // This only schedules that peer's requested replay, never authorizes a
        // head. A restarted peer may legitimately request a LOWER height.
        self.peer_requests.insert(peer.clone(), requested);
        self.prune_fixed(|fixed| {
            fixed.destination.as_ref() != Some(&peer)
                || match fixed.prepared.message().as_ref() {
                    Message::Body { context, .. } => context.height == requested.height,
                    Message::Decision { proposal, .. } => proposal.context == requested,
                    _ => true,
                }
        });
        let Some(head) = self.head() else {
            return Ok(());
        };
        if requested.height > head.height || requested.height == 0 {
            return Ok(());
        }
        let preparing = self
            .preparing
            .values()
            .any(|pending| match &pending.purpose {
                Purpose::ArchiveBody { peer: owner, .. } | Purpose::ArchiveDecision(owner) => {
                    owner == &peer
                }
                _ => false,
            });
        let cached = self.fixed.iter().any(|fixed| {
            if fixed.destination.as_ref() != Some(&peer) {
                return false;
            }
            let Message::Decision {
                proposal, body_id, ..
            } = fixed.prepared.message().as_ref()
            else {
                return false;
            };
            proposal.context == requested
                && self.fixed.iter().any(|body| {
                    body.destination.as_ref() == Some(&peer)
                        && body.prepared.body_id() == Some(*body_id)
                })
        });
        if self.archive_requests.get(&peer) == Some(&requested) && cached {
            for fixed in &mut self.fixed {
                if fixed.destination.as_ref() == Some(&peer)
                    && matches!(fixed.prepared.message().as_ref(), Message::Decision { .. })
                {
                    fixed.schedule.wake();
                }
            }
            return Ok(());
        }
        if self.archives.contains_key(&peer) || preparing {
            return Ok(());
        }
        self.archive_requests.insert(peer.clone(), requested);
        self.archives.insert(
            peer,
            ArchiveJob {
                context: requested,
                read: ArchiveRead::new(
                    requested.height,
                    head,
                    current,
                    self.config.validators.clone(),
                )?,
            },
        );
        Ok(())
    }
    fn poll_archives(&mut self, pipeline: &CandidatePipeline) -> Result<()> {
        let peers: Vec<_> = self.archives.keys().cloned().collect();
        for _ in 0..self.config.limits.events_per_poll.min(peers.len()) {
            let peer = peers[self.archive_turn % peers.len()].clone();
            self.archive_turn = (self.archive_turn + 1) % peers.len();
            let result = self
                .archives
                .get_mut(&peer)
                .expect("archive peer")
                .read
                .poll(pipeline)?;
            let Some(block) = result else {
                continue;
            };
            let job = self.archives.remove(&peer).expect("archive job");
            if let Some(block) = block {
                if block.context() != job.context
                    || self.peer_requests.get(&peer) != Some(&job.context)
                {
                    self.reject("requested archive parent differs from local prefix");
                    self.retire(Retirement::Input(PrepareInput::StoredBody(
                        block.stored().clone(),
                    )));
                    continue;
                }
                if !self.prepare(
                    PrepareInput::StoredBody(block.stored().clone()),
                    Purpose::ArchiveBody {
                        peer: peer.clone(),
                        proposal: block.proposal().clone(),
                        certificate: block.certificate().clone(),
                    },
                )? {
                    self.archive_requests.remove(&peer);
                }
            } else {
                self.archive_requests.remove(&peer);
            }
        }
        Ok(())
    }

    fn retire_body(&mut self, body: Body) {
        if let Some(origin) = body.early_origin {
            self.retire_early_origin_cache(origin);
        }
        if let Some(request) = body.authenticated {
            self.retire(Retirement::AuthenticatedRequest(request));
        }
        self.retire(Retirement::Body {
            prepared: body.prepared,
            request: body.request,
            candidate: body.candidate.map(|(_, candidate)| candidate),
        });
    }
    fn retire(&mut self, value: Retirement) {
        // New receive/local admission stops while any returned item is queued.
        // The reserve also covers one journal cleanup + every in-flight reply.
        assert!(
            self.retired.len()
                < self.fixed_limit() * 3
                    + self.config.limits.max_bodies * 3
                    + self.config.limits.max_inflight
                    + self.config.limits.events_per_poll
                    + 8,
            "controller retirement reserve invariant"
        );
        self.retired.push_back(value);
    }
    fn flush_retired(&mut self) -> Result<()> {
        for _ in 0..self.config.limits.events_per_poll {
            let Some(value) = self.retired.pop_front() else {
                break;
            };
            match self.channel.try_retire(value)? {
                RetireAdmission::Accepted => {}
                RetireAdmission::Backpressure(value) => {
                    self.retired.push_front(value);
                    break;
                }
                RetireAdmission::Rejected { value, reason } => {
                    self.retired.push_front(value);
                    anyhow::bail!("controller retirement budget/configuration: {reason}");
                }
            }
        }
        Ok(())
    }
    fn prune_fixed(&mut self, mut keep: impl FnMut(&Fixed) -> bool) {
        let mut retained = VecDeque::new();
        for fixed in std::mem::take(&mut self.fixed) {
            if keep(&fixed) {
                retained.push_back(fixed);
            } else {
                self.retire(Retirement::Prepared(fixed.prepared));
            }
        }
        self.fixed = retained;
    }
}

fn same_domain(a: Context, b: Context) -> bool {
    a.chain_id == b.chain_id
        && a.genesis_config_commitment == b.genesis_config_commitment
        && a.protocol_commitment == b.protocol_commitment
        && a.epoch == b.epoch
        && a.validator_set_hash == b.validator_set_hash
}
fn matches_context(
    body: &BatchContext,
    pin: &BatchContext,
    context: Context,
    parent: ParentPoint,
) -> bool {
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
fn message_round(message: &Message) -> Option<u64> {
    match message {
        Message::Proposal { proposal, .. } | Message::Decision { proposal, .. } => {
            Some(proposal.round)
        }
        Message::Vote(vote) => Some(vote.round),
        _ => None,
    }
}
fn message_context(message: &Message) -> Option<Context> {
    match message {
        Message::Proposal { proposal, .. } | Message::Decision { proposal, .. } => {
            Some(proposal.context)
        }
        Message::Vote(vote) => Some(vote.context),
        Message::RequestDecision { context } => Some(*context),
        _ => None,
    }
}
fn validate_config(config: &ControllerConfig, journal: &ValidatorJournal) -> Result<()> {
    journal.context().validate(&config.validators)?;
    ensure!(
        !journal.is_frozen() && !journal.is_pending(),
        "controller requires an acknowledged unfrozen journal"
    );
    let members = config.validators.members();
    ensure!(
        config.local_validator == journal.local_validator(),
        "controller validator differs from durable journal signer"
    );
    ensure!(
        config.validators.member(&config.local_validator).is_some(),
        "local validator missing"
    );
    ensure!(
        config.peers.len() + 1 == members.len(),
        "controller requires explicit routes for every other validator"
    );
    for member in members {
        if member.id() != config.local_validator {
            ensure!(
                config.peers.get(&member.id())
                    == Some(&peer_id_from_ed25519_public_key_v1(member.public_key())),
                "validator peer route does not match authenticated key"
            );
        }
    }
    let limits = config.limits;
    let reserved_bodies = members
        .len()
        .checked_mul(2)
        .and_then(|count| count.checked_add(limits.max_inflight))
        .and_then(|count| count.checked_add(2))
        .context("controller body slot budget overflow")?;
    ensure!(
        limits.max_bodies >= reserved_bodies
            && limits.max_proposals >= members.len() * 2
            && limits.max_body_bytes > 0
            && limits.max_inflight > 0
            && limits.max_inflight <= limits.max_bodies
            && limits.events_per_poll > 0
            && limits.sends_per_poll > 0
            && !config.retransmit.is_zero(),
        "invalid controller budget"
    );
    ensure!(
        limits
            .max_proposals
            .checked_mul(4)
            .and_then(|n| n.checked_add(members.len() * 3 + 4))
            .and_then(|n| n.checked_add(limits.max_bodies))
            .is_some(),
        "controller count budget overflow"
    );
    ensure!(
        limits
            .max_proposals
            .checked_mul(4)
            .and_then(|count| count.checked_add(members.len() * 3 + 4))
            .and_then(|count| count.checked_add(limits.max_bodies))
            .and_then(|count| count.checked_mul(3))
            .and_then(|count| count.checked_add(limits.max_bodies.checked_mul(3)?))
            .and_then(|count| count.checked_add(limits.max_inflight))
            .and_then(|count| count.checked_add(limits.events_per_poll))
            .and_then(|count| count.checked_add(8))
            .is_some(),
        "controller retirement count budget overflow"
    );
    ensure!(
        config.execution.chain_id == journal.context().chain_id
            && config.execution.genesis_config_commitment
                == journal.context().genesis_config_commitment
            && config.execution.protocol_commitment == journal.context().protocol_commitment,
        "controller execution domain mismatch"
    );
    Ok(())
}

#[cfg(test)]
#[path = "controller/regressions.rs"]
mod regressions;
