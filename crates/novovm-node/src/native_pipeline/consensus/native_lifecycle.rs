//! Original-node wiring of native transport, exact-body execution and existing
//! durable consensus. This is not a second safety machine or a new executor.
//! Socket I/O is independently pollable; no ACK waits, synchronous AOEM calls,
//! body production by a fixture, or business calculation in the control loop.
use super::{
    bodies::CandidateBodies,
    native::{NativeChannel, PreparedMessage, ReceiveEvent},
    progress::RoundProgress,
    transport::Message,
    wire::{Hash, ValidatorSet},
};
use crate::native_pipeline::{
    business::direct_nov_fee::DirectNovFeePolicy, consensus::transport::DecodeLimits,
    pipeline::CandidatePipeline,
};
use anyhow::{ensure, Context, Result};
use novovm_network::{
    novorudp::object_sender::ObjectSendState, product_overlay::peer_id_from_ed25519_public_key_v1,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub struct DeliveryLimits {
    pub messages_per_peer: usize,
    pub bytes_per_peer: usize,
}
struct Delivery {
    prepared: PreparedMessage,
    admitted: bool,
    retiring: bool,
    archive: bool,
    height: u64,
}
struct Fanout {
    prepared: PreparedMessage,
    pending: Vec<bool>,
}
struct Incoming {
    peer: usize,
    message: Arc<Message>,
}

/// Owns prepared messages until delivery or explicit expiry/retry. Per-peer
/// queues and native lane budgets both apply. Queue pressure retains the durable
/// outbox instead of dropping a signature. Superseded realtime obligations are
/// retired only after durable context/round advancement; decided history stays
/// available through the same owner's checked archive, not an unbounded queue.
pub struct NativeLifecycle {
    pub progress: RoundProgress,
    bodies: CandidateBodies,
    channel: NativeChannel,
    peers: Vec<Hash>,
    peer_names: BTreeMap<String, usize>,
    limits: DeliveryLimits,
    deliveries: Vec<VecDeque<Delivery>>,
    bytes: Vec<usize>,
    send_turn: usize,
    pending: [Option<Incoming>; 3],
    transactions: Vec<VecDeque<Arc<Message>>>,
    client_transactions: VecDeque<Arc<Message>>,
    transaction_turn: usize,
    transaction_session: Hash,
    transaction_sequence: u64,
    outbox: Option<Fanout>,
    requested: BTreeSet<(Hash, Hash)>,
    height: u64,
    round: u64,
    archives: Vec<recovery::ArchivePeer>,
    archive_turn: usize,
    decision_due: Option<Instant>,
    policy: DirectNovFeePolicy,
    validators: ValidatorSet,
    codec: DecodeLimits,
    frozen: bool,
    pub rejected_messages: u64,
    pub requested_bodies: u64,
    pub served_bodies: u64,
    pub expired_sends: u64,
    pub retired_sends: u64,
    pub archive_decisions: u64,
}
impl NativeLifecycle {
    #[cfg(test)]
    pub(crate) fn retain_control_for_deadline_test(
        &mut self,
        peer: Hash,
        message: Message,
    ) -> Result<()> {
        ensure!(
            matches!(
                &message,
                Message::Proposal { .. } | Message::Vote(_) | Message::Decision { .. }
            ),
            "deadline fixture requires control"
        );
        let peer = self
            .peers
            .iter()
            .position(|id| *id == peer)
            .context("unknown fixture peer")?;
        ensure!(self.pending[0].is_none(), "fixture control slot occupied");
        self.pending[0] = Some(Incoming {
            peer,
            message: Arc::new(message),
        });
        Ok(())
    }

    pub fn new(
        progress: RoundProgress,
        bodies: CandidateBodies,
        channel: NativeChannel,
        validators: &ValidatorSet,
        limits: DeliveryLimits,
        policy: DirectNovFeePolicy,
        codec: DecodeLimits,
    ) -> Result<Self> {
        ensure!(
            (8..=4096).contains(&limits.messages_per_peer)
                && limits.bytes_per_peer >= codec.message_bytes.saturating_mul(5),
            "invalid native delivery budget"
        );
        progress.context().validate(validators)?;
        ensure!(
            channel.peer_count() + 1 == validators.members().len(),
            "missing native validator route"
        );
        let mut peers = Vec::new();
        let mut peer_names = BTreeMap::new();
        for peer in 0..channel.peer_count() {
            let name = channel.peer_id(peer)?;
            let member = validators
                .members()
                .iter()
                .find(|v| peer_id_from_ed25519_public_key_v1(v.public_key()) == name)
                .context("unknown native validator route")?;
            ensure!(
                member.id() != progress.local_validator(),
                "native route targets local signer"
            );
            peers.push(member.id());
            peer_names.insert(name.to_owned(), peer);
        }
        ensure!(
            peers.iter().copied().collect::<BTreeSet<_>>().len() == peers.len(),
            "duplicate native validator route"
        );
        let count = peers.len();
        let height = progress.context().height;
        let round = progress.round();
        Ok(Self {
            progress,
            bodies,
            channel,
            peers,
            peer_names,
            limits,
            deliveries: (0..count).map(|_| VecDeque::new()).collect(),
            bytes: vec![0; count],
            send_turn: 0,
            pending: std::array::from_fn(|_| None),
            transactions: (0..count).map(|_| VecDeque::new()).collect(),
            client_transactions: VecDeque::new(),
            transaction_turn: 0,
            transaction_session: rand::random(),
            transaction_sequence: 0,
            outbox: None,
            requested: BTreeSet::new(),
            height,
            round,
            archives: (0..count)
                .map(|_| recovery::ArchivePeer::default())
                .collect(),
            archive_turn: 0,
            decision_due: None,
            policy,
            validators: validators.clone(),
            codec,
            frozen: false,
            rejected_messages: 0,
            requested_bodies: 0,
            served_bodies: 0,
            expired_sends: 0,
            retired_sends: 0,
            archive_decisions: 0,
        })
    }

    pub fn offer(&mut self, body: &Arc<Message>) -> Result<bool> {
        ensure!(!self.frozen, "native lifecycle frozen");
        self.bodies.offer(body, true, &self.progress)
    }
    pub(crate) fn offer_admitted(
        &mut self,
        body: &Arc<Message>,
        admitted: &[&crate::native_pipeline::pipeline::AdmittedTransfer],
        pipeline: &CandidatePipeline,
    ) -> Result<bool> {
        ensure!(!self.frozen, "native lifecycle frozen");
        self.bodies
            .offer_admitted(body, admitted, pipeline, &self.progress)
    }
    pub fn executed(&self) -> u64 {
        self.bodies.executed
    }
    pub(crate) fn published_packet(
        &self,
        expected: super::statement::ParentPoint,
    ) -> Option<&crate::native_pipeline::persistence::PreparedCandidate> {
        if self.is_frozen() {
            return None;
        }
        self.progress.published_packet(expected)
    }
    pub fn rejected_execution(&self) -> u64 {
        self.bodies.rejected_execution
    }
    pub fn integer_execution(&self) -> (u64, u64) {
        (self.bodies.integer_rows, self.bodies.integer_plans)
    }
    pub fn apfl_execution_rows(&self) -> u64 {
        self.bodies.apfl_rows
    }
    pub fn pending_deliveries(&self) -> usize {
        self.deliveries.iter().map(VecDeque::len).sum()
    }
    /// Scheduling-only change token; no transaction or durable decision meaning.
    pub fn io_progress_generation(&self) -> u64 {
        self.channel.io_progress_generation()
    }
    pub fn is_frozen(&self) -> bool {
        self.frozen || self.progress.is_frozen() || self.bodies.is_frozen()
    }

    /// Only framed I/O and delivery ownership. Even while assembly/execution
    /// waits on a body or durable storage, sessions/ACK/repair keep progressing.
    pub fn poll_io(&mut self, wall_ms: u64, now: Instant) -> Result<()> {
        ensure!(!self.is_frozen(), "native lifecycle frozen");
        let result = self.io_inner(wall_ms, now);
        if result.is_err() {
            self.frozen = true;
        }
        result
    }
    fn io_inner(&mut self, wall_ms: u64, now: Instant) -> Result<()> {
        let report = self.channel.poll_io(wall_ms, now, 64)?;
        self.rejected_messages += report.rejected as u64;
        for completed in report.completed {
            let queue = self
                .deliveries
                .get_mut(completed.peer)
                .context("completion peer out of range")?;
            let index = queue
                .iter()
                .position(|d| d.prepared.fragment_id() == completed.body.id())
                .context("native completion lost its retained input")?;
            ensure!(queue[index].admitted, "completion without native admission");
            if queue[index].retiring {
                let done = queue.remove(index).expect("retired delivery");
                self.bytes[completed.peer] -= done.prepared.byte_len();
                continue;
            }
            match completed.state {
                ObjectSendState::Delivered => {
                    let done = queue.remove(index).expect("completed delivery");
                    self.bytes[completed.peer] -= done.prepared.byte_len();
                }
                ObjectSendState::Expired => {
                    // Retry the SAME immutable bytes. This is not a transaction
                    // retry and cannot create a new signature/nonce/decision.
                    queue[index].admitted = false;
                    self.expired_sends += 1;
                }
                ObjectSendState::Sending => anyhow::bail!("nonterminal native completion"),
                ObjectSendState::SessionReplaced => {
                    // The endpoint has authenticated the SAME configured key
                    // and confirmed the fresh secret. Retry immutable original
                    // bytes; transport replacement grants no new signing right.
                    queue[index].admitted = false;
                }
                // Explicit retirement was handled above. An unrequested
                // cancellation is not an ACK or a safe retry authorization.
                ObjectSendState::Cancelled => {
                    anyhow::bail!("validator delivery unexpectedly cancelled")
                }
            }
        }
        if self.peers.is_empty() {
            return Ok(());
        }
        for _ in 0..self.peers.len() {
            let peer = self.send_turn;
            self.send_turn = (peer + 1) % self.peers.len();
            // One pending attempt per native lane. A full BODY lane must not
            // block a control message queued behind it for this same peer.
            let mut attempted = [false; 3];
            for delivery in self.deliveries[peer]
                .iter_mut()
                .filter(|d| !d.admitted && !d.retiring)
            {
                let lane = delivery.prepared.lane();
                if attempted[lane] {
                    continue;
                }
                attempted[lane] = true;
                if self.channel.try_send(peer, &delivery.prepared, now)? {
                    delivery.admitted = true;
                }
                if attempted.iter().all(|attempt| *attempt) {
                    break;
                }
            }
        }
        Ok(())
    }

    fn fits(&self, peer: usize, prepared: &PreparedMessage, archive: bool) -> bool {
        if self.deliveries[peer]
            .iter()
            .any(|d| d.retiring && d.prepared.fragment_id() == prepared.fragment_id())
        {
            return false;
        }
        if self.deliveries[peer]
            .iter()
            .any(|d| d.prepared.fragment_id() == prepared.fragment_id())
        {
            return true;
        }
        if self.deliveries[peer].len() >= self.limits.messages_per_peer
            || prepared.byte_len() > self.limits.bytes_per_peer.saturating_sub(self.bytes[peer])
        {
            return false;
        }
        if !archive && prepared.is_durable_control() {
            return true;
        }
        // Preserve four durable control slots (current proposal/prevote/
        // precommit plus an immediately preceding decision) inside the existing
        // peer budget. Requests, input,
        // body/archive replies cannot saturate the journal's publication path.
        // Superseded controls are locally cancelled/drained on durable advance.
        let mut auxiliary_count = 0;
        let mut auxiliary_bytes = 0usize;
        for delivery in &self.deliveries[peer] {
            if delivery.archive || !delivery.prepared.is_durable_control() {
                auxiliary_count += 1;
                auxiliary_bytes += delivery.prepared.byte_len();
            }
        }
        auxiliary_count < self.limits.messages_per_peer - 4
            && prepared.byte_len()
                <= self
                    .limits
                    .bytes_per_peer
                    .saturating_sub(self.codec.message_bytes.saturating_mul(4))
                    .saturating_sub(auxiliary_bytes)
    }
    fn enqueue(&mut self, peer: usize, prepared: PreparedMessage) -> bool {
        self.enqueue_kind(peer, prepared, false)
    }
    fn enqueue_kind(&mut self, peer: usize, prepared: PreparedMessage, archive: bool) -> bool {
        if !self.fits(peer, &prepared, archive) {
            return false;
        }
        if !self.deliveries[peer]
            .iter()
            .any(|d| d.prepared.fragment_id() == prepared.fragment_id())
        {
            self.bytes[peer] += prepared.byte_len();
            self.deliveries[peer].push_back(Delivery {
                prepared,
                admitted: false,
                retiring: false,
                archive,
                height: self.progress.context().height,
            });
        }
        true
    }

    /// Assembly-side poll: strict body identities, async execution completions,
    /// exact requests and original journal ACK/outbox. Never constructs a vote
    /// or invokes Host balance/fee/nonce logic. No input receipt is finality.
    pub fn poll(&mut self, pipeline: &CandidatePipeline, now: Instant) -> Result<()> {
        ensure!(!self.is_frozen(), "native lifecycle frozen");
        let result = self.poll_inner(pipeline, now);
        if result.is_err() {
            self.frozen = true;
        }
        result
    }
    fn poll_inner(&mut self, pipeline: &CandidatePipeline, now: Instant) -> Result<()> {
        self.progress
            .poll_recovery(pipeline, &self.policy, self.codec)?;
        self.bodies.poll(pipeline, &mut self.progress)?;
        self.progress.poll_acknowledgements(pipeline)?;
        if self.height != self.progress.context().height || self.round != self.progress.round() {
            self.height = self.progress.context().height;
            self.round = self.progress.round();
            self.requested.clear();
            // Genuine local advance postpones archive discovery. Do not read
            // old candidates on every healthy fast-path height just because
            // peers' latest context announcements are still in flight.
            self.decision_due = Some(
                now.checked_add(Duration::from_millis(250))
                    .context("request deadline overflow")?,
            );
            self.retire_realtime()?;
        }
        // Reserve the application handoff before taking transport custody. It
        // neither enters RoundProgress nor participates in validator fanout.
        if self.client_transactions.len() < 2 {
            match self.channel.poll_application(now, 64)? {
                Some(ReceiveEvent::Message(message)) => self
                    .client_transactions
                    .push_back(Arc::new(message.message)),
                Some(ReceiveEvent::Rejected { .. }) => self.rejected_messages += 1,
                None => {}
            }
        }
        // Reserve the one bounded application handoff before completing another
        // transport object. Backpressured bodies stay owned here, not discarded
        // after transport DONE or re-generated by a testing wallet.
        for lane in 0..3 {
            // Reuse the original controller's bounded control-event turn (32).
            // A rejected/late first object must not hide an already-arrived
            // eligible proposal/QC behind the deadline. This is not an inbox
            // capacity increase: use the same single retained slot, stop on
            // backpressure, and preserve one turn for each data lane.
            let turns = if lane == 0 { 32 } else { 1 };
            for _ in 0..turns {
                if self.pending[lane].is_none() {
                    match self.channel.poll_receive_lane(lane, now, 64)? {
                        Some(ReceiveEvent::Message(message)) => {
                            let peer = *self
                                .peer_names
                                .get(&message.peer)
                                .context("unmapped authenticated peer")?;
                            self.pending[lane] = Some(Incoming {
                                peer,
                                message: Arc::new(message.message),
                            });
                        }
                        Some(ReceiveEvent::Rejected { .. }) => {
                            self.rejected_messages += 1;
                            continue;
                        }
                        None => break,
                    }
                }
                if let Some(received) = self.pending[lane].take() {
                    let peer = received.peer;
                    match received.message.as_ref() {
                        Message::Body { .. } | Message::ApflBody { .. } => {
                            match self.bodies.offer(&received.message, false, &self.progress) {
                                Ok(true) => {}
                                Ok(false) => self.pending[lane] = Some(received),
                                Err(_) => self.rejected_messages += 1,
                            }
                        }
                        Message::RequestBody { body_id } => {
                            // A slow requester's reply cannot occupy the shared
                            // evidence slot and hide healthy proposals/votes.
                            // Coalesce its idempotent exact hint per peer; the
                            // original body remains owned by live/archive state.
                            self.remember_body_request(peer, *body_id);
                        }
                        Message::RequestDecision { context } => {
                            self.request_archive(peer, *context)?;
                        }
                        Message::Proposal { .. } | Message::Vote(_) | Message::Decision { .. } => {
                            if self
                                .progress
                                .receive(self.peers[peer], &received.message)
                                .is_err()
                            {
                                self.rejected_messages += 1;
                            }
                        }
                        Message::Transactions { .. } | Message::ApflTransactions { .. } => {
                            // Native DONE means bounded custody, never signature admission.
                            // RPC consumes these originals via the resident auth owner.
                            if self.transactions[peer].len() < 2 {
                                self.transactions[peer].push_back(received.message);
                            } else {
                                self.pending[lane] = Some(received);
                            }
                        }
                        // Unsupported optional announcement/credit extensions
                        // are not another consensus or execution entrance.
                        _ => self.rejected_messages += 1,
                    }
                }
                if self.pending[lane].is_some() {
                    break;
                }
            }
        }
        // ACK/context first, then the bounded native ingress above, then ready
        // evidence and deadlines. A control message already in custody must
        // not lose to the timeout of the same poll and force an avoidable round.
        self.progress.poll_after_receive(now)?;
        if let Some(message) = self.progress.pending_message() {
            if self.outbox.is_none() {
                self.outbox = Some(Fanout {
                    prepared: self.channel.prepare(message)?,
                    pending: vec![true; self.peers.len()],
                });
            }
            let mut fanout = self.outbox.take().expect("prepared outbox");
            for peer in 0..self.peers.len() {
                if fanout.pending[peer] && self.enqueue(peer, fanout.prepared.clone()) {
                    fanout.pending[peer] = false;
                }
            }
            if fanout.pending.iter().any(|pending| *pending) {
                self.outbox = Some(fanout);
            } else {
                self.progress.take_message();
            }
        }
        self.poll_archives(pipeline)?;
        self.request_decisions(now)?;
        let missing = self.bodies.requests(&self.progress);
        self.requested.retain(|key| missing.contains(key));
        for (source, id) in missing {
            if self.requested.contains(&(source, id)) {
                continue;
            }
            let Some(peer) = self.peers.iter().position(|p| *p == source) else {
                continue;
            };
            let request = self
                .channel
                .prepare(&Message::RequestBody { body_id: id })?;
            if self.enqueue(peer, request) {
                self.requested.insert((source, id));
                self.requested_bodies += 1;
            }
        }
        #[cfg(test)]
        for peer in 0..self.peers.len() {
            ensure!(
                self.deliveries[peer].len() <= self.limits.messages_per_peer,
                "delivery count overflow"
            );
            ensure!(
                self.bytes[peer] <= self.limits.bytes_per_peer,
                "delivery byte overflow"
            );
            ensure!(
                self.archives[peer].charge() <= self.codec.message_bytes.saturating_mul(2),
                "archive cache overflow"
            );
        }
        Ok(())
    }
}

mod ingress;
mod recovery;
mod successor;
pub use successor::SuccessorSelection;
