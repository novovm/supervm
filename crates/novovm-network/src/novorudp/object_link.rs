//! One native socket with separately bounded object lanes. Reuses the original
//! handshake, frame, window repair, pacing and reassembly implementations.
//! No body encoding/decoding, business execution, consensus rules or worker.
use super::{
    endpoint::{NativeEndpoint, NativePoll, NativeSessionChange},
    fragments::{
        CompletedMessage, FragmentAdmission, FragmentProfile, OutgoingMessage, ReassemblyLimits,
    },
    object_ack::{encode_object_ack, OBJECT_WINDOW},
    object_receiver::NativeObjectReceiver,
    object_sender::{NativeObjectSender, ObjectSendState},
    NovoRudpPacingProfile, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
    NovoRudpWindowConfig,
};
use anyhow::{ensure, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct NativeSendLimits {
    pub messages: usize,
    pub bytes: usize,
    pub peer_messages: usize,
    pub peer_bytes: usize,
}
#[derive(Clone)]
pub struct NativeLaneConfig {
    pub stream: u64,
    pub domain: [u8; 32],
    pub receive: ReassemblyLimits,
    pub send: NativeSendLimits,
}
#[derive(Clone)]
pub struct NativeSendPolicy {
    pub window: NovoRudpWindowConfig,
    pub pacing: NovoRudpPacingProfile,
    pub ttl: Duration,
}

impl NativeSendPolicy {
    /// Only for NativeCarrierPump over an already reliable QUIC/TLS channel.
    /// Keep native framing, completion ACKs, deadlines and bounded queues; the
    /// carrier supplies congestion/backpressure, so do not duplicate every frame
    /// or insert a second fixed-rate pacing delay above it. Raw UDP is unchanged.
    pub fn reliable_carrier_v1(ttl: Duration) -> Self {
        Self {
            window: NovoRudpWindowConfig {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 64,
                batch_pause_ms: 0,
                no_progress_backoff: false,
                ..Default::default()
            },
            pacing: NovoRudpPacingProfile {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 64,
                batch_pause_ms: 0,
                no_progress_backoff: false,
                ..Default::default()
            },
            ttl,
        }
    }
}

#[derive(Default, Clone, Copy)]
struct Usage {
    messages: usize,
    bytes: usize,
}
impl Usage {
    fn fits(self, bytes: usize, messages_limit: usize, bytes_limit: usize) -> bool {
        self.messages < messages_limit
            && self
                .bytes
                .checked_add(bytes)
                .is_some_and(|n| n <= bytes_limit)
    }
    fn add(&mut self, bytes: usize) {
        self.messages += 1;
        self.bytes += bytes;
    }
    fn remove(&mut self, bytes: usize) {
        self.messages -= 1;
        self.bytes -= bytes;
    }
}
type JobKey = (usize, u64, [u8; 32]);
type Feedback = (usize, [u8; 32]);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativeApplicationRole {
    Client,
    Service,
}
/// Read-only local generation guard, not an identity/entitlement authority.
/// A pending operation can check this without holding the socket owner across
/// an await. Replacement or dropping the link permanently invalidates it.
#[derive(Clone)]
pub struct NativeSessionValidity(Arc<AtomicBool>);
impl NativeSessionValidity {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
    pub fn is_current(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    fn invalidate(&self) {
        self.0.store(false, Ordering::Release);
    }
}
/// Local cancellation only: stops future sends when the original owner polls.
/// Does not recall datagrams, roll back remote commits or authorize a retry.
#[derive(Clone, Default)]
pub struct NativeSendCancellation(Arc<AtomicBool>);
impl NativeSendCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
struct PendingFeedback {
    frame: NovoRudpTransportFrameV0,
    dirty: bool,
}
struct Lane {
    config: NativeLaneConfig,
    receiver: NativeObjectReceiver,
    usage: Usage,
    peers: Vec<Usage>,
    feedback: VecDeque<Feedback>,
    queued: BTreeSet<Feedback>,
    pending_feedback: BTreeMap<Feedback, PendingFeedback>,
}
impl Lane {
    fn feedback(&mut self, peer: usize, id: [u8; 32]) {
        if let Some(pending) = self.pending_feedback.get_mut(&(peer, id)) {
            pending.dirty = true;
        }
        self.requeue_feedback(peer, id);
    }
    fn requeue_feedback(&mut self, peer: usize, id: [u8; 32]) {
        // Coalesced hint only. No payload eviction or fabricated completion.
        if self.queued.len() < self.config.receive.messages * 2 && self.queued.insert((peer, id)) {
            self.feedback.push_back((peer, id));
        }
    }
}

/// The caller must retain/report expired work; expiry is NOT transaction failure.
#[must_use]
pub struct NativeSendCompletion {
    pub peer: usize,
    pub stream: u64,
    pub state: ObjectSendState,
    pub body: Arc<OutgoingMessage>,
}
#[must_use]
#[derive(Default)]
pub struct NativeLinkPoll {
    pub rejected: usize,
    pub backpressured: usize,
    pub accepted_fragments: usize,
    pub duplicate_fragments: usize,
    pub feedback_sent: usize,
    pub data_sent: usize,
    pub handshake_send_failures: usize,
    pub receive_errors: usize,
    pub send_failures: usize,
    /// At most the supplied poll quantum. Ownership transfers to the caller;
    /// it must budget retained completions before polling again.
    pub completed: Vec<NativeSendCompletion>,
    /// Process before receiving new application objects. Old business work may
    /// already have committed; this is NOT a rollback or automatic retry.
    pub session_changes: Vec<NativeSessionChange>,
    pub retired_inbound_messages: usize,
}

pub struct NativeObjectLink {
    endpoint: NativeEndpoint,
    lanes: BTreeMap<u64, Lane>,
    streams: Vec<u64>,
    peers: BTreeMap<String, usize>,
    jobs: BTreeMap<JobKey, NativeObjectSender>,
    order: VecDeque<JobKey>,
    cancellations: BTreeMap<JobKey, NativeSendCancellation>,
    feedback_turn: usize,
    epoch: u64,
    policy: NativeSendPolicy,
    // One application sequence owner per authenticated peer/stream. Bounded by
    // the existing peer and lane counts; not an admission/entitlement registry.
    application_owners: BTreeMap<(usize, u64, NativeApplicationRole), [u8; 16]>,
    session_validity: BTreeMap<usize, ([u8; 16], NativeSessionValidity)>,
}
impl NativeObjectLink {
    pub fn new(
        endpoint: NativeEndpoint,
        configs: Vec<NativeLaneConfig>,
        policy: NativeSendPolicy,
    ) -> Result<Self> {
        ensure!(
            (1..=16).contains(&configs.len()),
            "invalid native lane count"
        );
        ensure!(!policy.ttl.is_zero(), "zero native send TTL");
        let peers = (0..endpoint.peer_count())
            .map(|i| Ok((endpoint.peer_id(i)?.to_owned(), i)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let ids = peers.keys().cloned().collect::<Vec<_>>();
        let mut lanes = BTreeMap::new();
        for config in configs {
            ensure!(!lanes.contains_key(&config.stream), "duplicate native lane");
            let s = &config.send;
            ensure!(
                (1..=4096).contains(&s.messages)
                    && s.peer_messages > 0
                    && s.peer_messages <= s.messages
                    && s.peer_bytes > 0
                    && s.peer_bytes <= s.bytes,
                "invalid native send limits"
            );
            let receiver = if ids.is_empty() && endpoint.accepts_clients() {
                NativeObjectReceiver::awaiting_authenticated_peers(
                    config.stream,
                    config.domain,
                    config.receive.clone(),
                )?
            } else {
                NativeObjectReceiver::new(
                    config.stream,
                    config.domain,
                    ids.clone(),
                    config.receive.clone(),
                )?
            };
            lanes.insert(
                config.stream,
                Lane {
                    config,
                    receiver,
                    usage: Usage::default(),
                    peers: vec![Usage::default(); endpoint.peer_count()],
                    feedback: VecDeque::new(),
                    queued: BTreeSet::new(),
                    pending_feedback: BTreeMap::new(),
                },
            );
        }
        let streams = lanes.keys().copied().collect();
        Ok(Self {
            endpoint,
            lanes,
            streams,
            peers,
            jobs: BTreeMap::new(),
            order: VecDeque::new(),
            cancellations: BTreeMap::new(),
            feedback_turn: 0,
            epoch: 0,
            policy,
            application_owners: BTreeMap::new(),
            session_validity: BTreeMap::new(),
        })
    }
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.endpoint.local_addr()
    }
    pub fn local_identity(&self) -> [u8; 32] {
        self.endpoint.local_identity()
    }
    pub fn session_id(&self, peer: usize) -> Option<[u8; 16]> {
        self.endpoint.session_id(peer)
    }
    pub fn peer_id(&self, peer: usize) -> Result<&str> {
        self.endpoint.peer_id(peer)
    }
    pub fn peer_count(&self) -> usize {
        self.endpoint.peer_count()
    }
    pub fn session_validity(&mut self, peer: usize) -> Result<NativeSessionValidity> {
        let session = self.session_id(peer).context("peer not authenticated")?;
        let entry = self
            .session_validity
            .entry(peer)
            .or_insert_with(|| (session, NativeSessionValidity::new()));
        if entry.0 != session {
            entry.1.invalidate();
            *entry = (session, NativeSessionValidity::new());
        }
        Ok(entry.1.clone())
    }
    /// Prevent dropping/recreating an application adapter from resetting replay
    /// counters on the SAME authenticated session. Only a genuinely new session
    /// may acquire this slot again; there is deliberately no reset/unclaim API.
    pub fn claim_application_session(
        &mut self,
        peer: usize,
        stream: u64,
        domain: [u8; 32],
        role: NativeApplicationRole,
    ) -> Result<[u8; 16]> {
        ensure!(
            self.lanes
                .get(&stream)
                .is_some_and(|l| l.config.domain == domain),
            "application lane/domain mismatch"
        );
        let session = self
            .session_id(peer)
            .context("application peer not authenticated")?;
        ensure!(
            self.application_owners.get(&(peer, stream, role)) != Some(&session),
            "application session already owned"
        );
        self.application_owners
            .insert((peer, stream, role), session);
        Ok(session)
    }
    pub fn pending_messages(&self, stream: u64) -> Result<usize> {
        Ok(self
            .lanes
            .get(&stream)
            .context("unknown native lane")?
            .receiver
            .pending_messages())
    }
    pub fn reserved_bytes(&self, stream: u64) -> Result<usize> {
        Ok(self
            .lanes
            .get(&stream)
            .context("unknown native lane")?
            .receiver
            .reserved_bytes())
    }
    pub fn outbound_usage(&self, stream: u64) -> Result<(usize, usize)> {
        let u = self
            .lanes
            .get(&stream)
            .context("unknown native lane")?
            .usage;
        Ok((u.messages, u.bytes))
    }
    /// Encoding/hash belongs to the assembly caller. False leaves its original
    /// Arc untouched; duplicate in-flight work does not reset repair or TTL.
    pub fn try_send(
        &mut self,
        peer: usize,
        stream: u64,
        body: &Arc<OutgoingMessage>,
        now: Instant,
    ) -> Result<bool> {
        let deadline = now
            .checked_add(self.policy.ttl)
            .context("native send deadline overflow")?;
        self.try_send_until(peer, stream, body, now, deadline)
    }

    /// A short operation can finish before the transport's normal repair TTL.
    /// Reusing an in-flight object never restarts or extends its original TTL.
    pub fn try_send_until(
        &mut self,
        peer: usize,
        stream: u64,
        body: &Arc<OutgoingMessage>,
        now: Instant,
        deadline: Instant,
    ) -> Result<bool> {
        ensure!(now < deadline, "native send operation deadline");
        ensure!(peer < self.endpoint.peer_count(), "unknown native peer");
        let lane = self.lanes.get_mut(&stream).context("unknown native lane")?;
        ensure!(
            body.profile() == FragmentProfile::NativeUdpV2
                && body.domain() == lane.config.domain
                && body.byte_len() <= lane.config.receive.max_message_bytes,
            "native outgoing object scope/shape mismatch"
        );
        let key = (peer, stream, body.id());
        if let Some(sender) = self.jobs.get(&key) {
            // A retired job still owns its quota until its completion is handed
            // back. Identical bytes in a new session are not that old job.
            return Ok(self.endpoint.session_id(peer) == Some(sender.session_id()));
        }
        let Some(session) = self.endpoint.session_id(peer) else {
            return Ok(false);
        };
        let bytes = body.byte_len();
        let limits = &lane.config.send;
        if !lane.usage.fits(bytes, limits.messages, limits.bytes)
            || !lane.peers[peer].fits(bytes, limits.peer_messages, limits.peer_bytes)
        {
            return Ok(false);
        }
        let sender = NativeObjectSender::new(
            body.clone(),
            session,
            stream,
            self.policy.window.clone(),
            self.policy.pacing.clone(),
            now,
            self.policy.ttl.min(deadline.duration_since(now)),
        )?;
        lane.usage.add(bytes);
        lane.peers[peer].add(bytes);
        self.jobs.insert(key, sender);
        self.order.push_back(key);
        Ok(true)
    }
    /// Cancel only this retained object handle. An equal digest from another
    /// caller is not ownership of the queued send. Quota is returned by poll.
    pub fn cancel_send(&mut self, peer: usize, stream: u64, body: &Arc<OutgoingMessage>) -> bool {
        let Some(sender) = self.jobs.get_mut(&(peer, stream, body.id())) else {
            return false;
        };
        if !Arc::ptr_eq(sender.body(), body) {
            return false;
        }
        sender.cancel();
        true
    }

    /// Retain the caller's one-way cancellation signal with the SAME send job.
    /// Re-enqueue cannot replace its owner, restart its deadline or revive it.
    pub fn try_send_cancellable_until(
        &mut self,
        peer: usize,
        stream: u64,
        body: &Arc<OutgoingMessage>,
        now: Instant,
        deadline: Instant,
        cancellation: &NativeSendCancellation,
    ) -> Result<bool> {
        ensure!(!cancellation.is_cancelled(), "native send cancelled");
        let key = (peer, stream, body.id());
        if self.jobs.contains_key(&key) {
            ensure!(
                self.cancellations
                    .get(&key)
                    .is_some_and(|old| Arc::ptr_eq(&old.0, &cancellation.0)),
                "native send cancellation owner mismatch"
            );
        }
        let accepted = self.try_send_until(peer, stream, body, now, deadline)?;
        if accepted {
            self.cancellations.insert(key, cancellation.clone());
        }
        Ok(accepted)
    }
    pub fn poll(&mut self, wall_ms: u64, now: Instant, quantum: usize) -> Result<NativeLinkPoll> {
        let received = self.endpoint.poll(wall_ms, now, quantum)?;
        self.process_poll(received, now, quantum)
    }

    /// Reuse the same authenticated endpoint and object state machine when the
    /// original socket owner supplies datagrams. The receiver must be bounded
    /// and nonblocking; it preserves source addresses and encrypted wire bytes.
    /// `Ok(None)` consumes one poll turn used by the owner's other protocol.
    /// This does not accept pre-authenticated frames or bypass endpoint budgets.
    pub fn poll_with_receiver<F>(
        &mut self,
        wall_ms: u64,
        now: Instant,
        quantum: usize,
        receiver: F,
    ) -> Result<NativeLinkPoll>
    where
        F: FnMut(&mut [u8]) -> std::io::Result<Option<(usize, SocketAddr)>>,
    {
        let received = self
            .endpoint
            .poll_with_receiver(wall_ms, now, quantum, receiver)?;
        self.process_poll(received, now, quantum)
    }

    fn process_poll(
        &mut self,
        received: NativePoll,
        now: Instant,
        quantum: usize,
    ) -> Result<NativeLinkPoll> {
        let NativePoll {
            frames,
            rejected_datagrams,
            handshake_send_failures,
            receive_errors,
            session_changes,
            ..
        } = received;
        // Append only identities authenticated by this endpoint. Indices never
        // change, so retained jobs/ACKs cannot attach to a recycled client slot.
        while self.peers.len() < self.endpoint.peer_count() {
            let index = self.peers.len();
            let id = self.endpoint.peer_id(index)?.to_owned();
            for lane in self.lanes.values_mut() {
                lane.receiver.admit_authenticated_peer(id.clone())?;
                lane.peers.push(Usage::default());
            }
            self.peers.insert(id, index);
        }
        let mut report = NativeLinkPoll {
            rejected: rejected_datagrams,
            handshake_send_failures,
            receive_errors,
            session_changes,
            ..Default::default()
        };
        for change in &report.session_changes {
            if let Some((_, validity)) = self.session_validity.remove(&change.peer) {
                validity.invalidate();
            }
            let id = self.endpoint.peer_id(change.peer)?;
            for lane in self.lanes.values_mut() {
                report.retired_inbound_messages += lane.receiver.retire_peer_session(id);
                lane.feedback.retain(|(peer, _)| *peer != change.peer);
                lane.queued.retain(|(peer, _)| *peer != change.peer);
                // The endpoint already retired that exact session and its IO.
                lane.pending_feedback
                    .retain(|(peer, _), _| *peer != change.peer);
            }
            for ((peer, _, _), sender) in &mut self.jobs {
                if *peer == change.peer {
                    sender.retire_session();
                }
            }
        }
        for lane in self.lanes.values_mut() {
            lane.receiver.expire(now);
        }
        for received in frames {
            let frame = received.frame();
            let stream = frame.stream_id;
            let Some(lane) = self.lanes.get_mut(&stream) else {
                report.rejected += 1;
                continue;
            };
            if frame.kind == NovoRudpTransportFrameKindV0::Ack {
                let Some(id) = frame.payload.get(8..40) else {
                    report.rejected += 1;
                    continue;
                };
                let key = (received.peer(), stream, id.try_into()?);
                if let Some(sender) = self.jobs.get_mut(&key) {
                    if self
                        .cancellations
                        .get(&key)
                        .is_some_and(NativeSendCancellation::is_cancelled)
                    {
                        sender.cancel();
                    }
                    if sender.status() == ObjectSendState::Sending
                        && sender.on_ack(frame, now).is_err()
                    {
                        report.rejected += 1;
                    }
                }
            } else {
                match lane.receiver.accept_received(&received, now) {
                    Ok(FragmentAdmission::Backpressure) => report.backpressured += 1,
                    Ok(admission) => {
                        if admission == FragmentAdmission::Accepted {
                            report.accepted_fragments += 1;
                        } else {
                            report.duplicate_fragments += 1;
                        }
                        // Reliable bound carriers already deliver initial data in order.
                        // Report window progress without adding a second per-fragment
                        // ACK stream. Completion still ACKs only after poll_complete's
                        // whole-object verification; probes/repairs remain immediate.
                        // Deferred writes alone are not evidence of a reliable carrier.
                        if !self.endpoint.peer_uses_reliable_carrier(received.peer())
                            || admission != FragmentAdmission::Accepted
                            || frame.kind != NovoRudpTransportFrameKindV0::Data
                            || frame.sequence % OBJECT_WINDOW == OBJECT_WINDOW - 1
                        {
                            lane.feedback(received.peer(), frame.payload[40..72].try_into()?);
                        }
                    }
                    Err(_) => report.rejected += 1,
                }
            }
        }
        self.poll_feedback(now, quantum, &mut report)?;
        // Fair across retained jobs, with no per-object wait for an ACK.
        for _ in 0..quantum.min(self.order.len()) {
            let key = self.order.pop_front().expect("bounded job count");
            let sender = self.jobs.get_mut(&key).expect("retained send job");
            let cancelled = self
                .cancellations
                .get(&key)
                .is_some_and(NativeSendCancellation::is_cancelled);
            if cancelled {
                sender.cancel();
            }
            if sender.status() == ObjectSendState::Sending {
                match self
                    .endpoint
                    .poll_object(key.0, sender, now, quantum - report.data_sent)
                {
                    Ok(sent) => report.data_sent += sent,
                    Err(error) if error.downcast_ref::<std::io::Error>().is_some() => {
                        report.send_failures += 1
                    }
                    Err(error) => {
                        self.order.push_front(key);
                        return Err(error);
                    }
                }
            }
            if sender.status() == ObjectSendState::Sending {
                self.order.push_back(key);
                if report.data_sent == quantum
                    || (report.data_sent > 0 && self.endpoint.uses_deferred_writes())
                {
                    break;
                }
                continue;
            }
            // A complete ACK can arrive while a redundant repair frame is
            // partially written. Drain that frame under the original deadline
            // before releasing quota. Cancelling/expiring a partial stream is
            // still fatal; successful completion must not trigger cancellation.
            if self.endpoint.session_id(key.0) == Some(sender.session_id()) {
                if sender.status() == ObjectSendState::Delivered
                    && !cancelled
                    && now < sender.deadline()
                {
                    match self.endpoint.drain_object_send(key.0, key.1, key.2, now) {
                        Ok(true) => {}
                        Ok(false) => {
                            self.order.push_back(key);
                            continue;
                        }
                        Err(error) => {
                            self.order.push_front(key);
                            return Err(error);
                        }
                    }
                } else if let Err(error) = self.endpoint.cancel_object_send(key.0, key.1, key.2) {
                    self.order.push_front(key);
                    return Err(error);
                }
            }
            let sender = self.jobs.remove(&key).expect("finished send job");
            self.cancellations.remove(&key);
            let state = sender.status();
            let body = sender.into_body();
            let lane = self.lanes.get_mut(&key.1).expect("configured lane");
            lane.usage.remove(body.byte_len());
            lane.peers[key.0].remove(body.byte_len());
            report.completed.push(NativeSendCompletion {
                peer: key.0,
                stream: key.1,
                state,
                body,
            });
        }
        // ACK-first alone can starve data, and data-first alone can starve ACKs.
        // A deferred completion releases one slot: give feedback one bounded
        // opportunity after data completion instead of queuing another body.
        if self.endpoint.uses_deferred_writes()
            && report.data_sent > 0
            && report.feedback_sent < quantum
        {
            self.poll_feedback(now, quantum - report.feedback_sent, &mut report)?;
        }
        Ok(report)
    }
    fn poll_feedback(
        &mut self,
        now: Instant,
        quantum: usize,
        report: &mut NativeLinkPoll,
    ) -> Result<()> {
        // Control/body/ingress feedback have independent bounded hint queues.
        for _ in 0..quantum {
            let stream = self.streams[self.feedback_turn];
            self.feedback_turn = (self.feedback_turn + 1) % self.streams.len();
            let lane = self.lanes.get_mut(&stream).expect("configured lane");
            let Some((peer, id)) = lane.feedback.pop_front() else {
                continue;
            };
            lane.queued.remove(&(peer, id));
            let Some(session) = self.endpoint.session_id(peer) else {
                continue;
            };
            let key = (peer, id);
            let epoch = if let Some(pending) = lane.pending_feedback.get(&key) {
                pending.frame.ack_epoch
            } else {
                self.epoch = self
                    .epoch
                    .checked_add(1)
                    .context("native ACK epoch exhausted")?;
                self.epoch
            };
            // Also checks receiver TTL/history expiry. Never keep a blocked ACK
            // alive after the object it describes has left the receiver state.
            let Some(ack) = lane.receiver.transport_feedback(
                self.endpoint.peer_id(peer)?,
                id,
                session,
                epoch,
                now,
            )?
            else {
                if let Err(error) = self.endpoint.cancel_feedback_send(peer, stream, id) {
                    lane.requeue_feedback(peer, id);
                    return Err(error);
                }
                lane.pending_feedback.remove(&key);
                continue;
            };
            if let std::collections::btree_map::Entry::Vacant(entry) =
                lane.pending_feedback.entry(key)
            {
                entry.insert(PendingFeedback {
                    frame: encode_object_ack(id, stream, &ack)?,
                    dirty: false,
                });
            }
            let pending = lane
                .pending_feedback
                .get(&key)
                .expect("prepared native ACK");
            match self.endpoint.try_send(peer, &pending.frame) {
                Ok(true) => {
                    report.feedback_sent += 1;
                    let pending = lane
                        .pending_feedback
                        .remove(&key)
                        .expect("completed native ACK");
                    if pending.dirty {
                        lane.feedback(peer, id);
                    }
                    // Let data use the freed slot before queuing another ACK.
                    if self.endpoint.uses_deferred_writes() {
                        break;
                    }
                }
                Ok(false) => lane.requeue_feedback(peer, id),
                Err(error) if error.downcast_ref::<std::io::Error>().is_some() => {
                    report.send_failures += 1;
                    lane.requeue_feedback(peer, id);
                }
                Err(error) => {
                    lane.requeue_feedback(peer, id);
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    /// Separate assembly-owner call. A free caller output slot is REQUIRED;
    /// completion custody leaves the reassembly budget, not the process budget.
    pub fn poll_complete(
        &mut self,
        stream: u64,
        now: Instant,
        quantum: usize,
    ) -> Result<Option<CompletedMessage>> {
        let lane = self.lanes.get_mut(&stream).context("unknown native lane")?;
        let item = lane.receiver.poll_complete(now, quantum)?;
        if let Some(item) = &item {
            let peer = *self
                .peers
                .get(&item.peer)
                .context("completed unknown peer")?;
            lane.feedback(peer, item.id);
        }
        Ok(item)
    }
}

impl Drop for NativeObjectLink {
    fn drop(&mut self) {
        for (_, validity) in self.session_validity.values() {
            validity.invalidate();
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod receiver_tests;
