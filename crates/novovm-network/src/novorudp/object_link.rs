//! One native socket with separately bounded object lanes. Reuses the original
//! handshake, frame, window repair, pacing and reassembly implementations.
//! No body encoding/decoding, business execution, consensus rules or worker.
use super::{
    endpoint::{NativeEndpoint, NativePoll},
    fragments::{
        CompletedMessage, FragmentAdmission, FragmentProfile, OutgoingMessage, ReassemblyLimits,
    },
    object_ack::encode_object_ack,
    object_receiver::NativeObjectReceiver,
    object_sender::{NativeObjectSender, ObjectSendState},
    NovoRudpPacingProfile, NovoRudpTransportFrameKindV0, NovoRudpWindowConfig,
};
use anyhow::{ensure, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    net::SocketAddr,
    sync::Arc,
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
struct Lane {
    config: NativeLaneConfig,
    receiver: NativeObjectReceiver,
    usage: Usage,
    peers: Vec<Usage>,
    feedback: VecDeque<Feedback>,
    queued: BTreeSet<Feedback>,
}
impl Lane {
    fn feedback(&mut self, peer: usize, id: [u8; 32]) {
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
}

pub struct NativeObjectLink {
    endpoint: NativeEndpoint,
    lanes: BTreeMap<u64, Lane>,
    streams: Vec<u64>,
    peers: BTreeMap<String, usize>,
    jobs: BTreeMap<JobKey, NativeObjectSender>,
    order: VecDeque<JobKey>,
    feedback_turn: usize,
    epoch: u64,
    policy: NativeSendPolicy,
    // One application sequence owner per authenticated peer/stream. Bounded by
    // the existing peer and lane counts; not an admission/entitlement registry.
    application_owners: BTreeMap<(usize, u64, NativeApplicationRole), [u8; 16]>,
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
            feedback_turn: 0,
            epoch: 0,
            policy,
            application_owners: BTreeMap::new(),
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
        ensure!(peer < self.endpoint.peer_count(), "unknown native peer");
        let lane = self.lanes.get_mut(&stream).context("unknown native lane")?;
        ensure!(
            body.profile() == FragmentProfile::NativeUdpV2
                && body.domain() == lane.config.domain
                && body.byte_len() <= lane.config.receive.max_message_bytes,
            "native outgoing object scope/shape mismatch"
        );
        let key = (peer, stream, body.id());
        if self.jobs.contains_key(&key) {
            return Ok(true);
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
            self.policy.ttl,
        )?;
        lane.usage.add(bytes);
        lane.peers[peer].add(bytes);
        self.jobs.insert(key, sender);
        self.order.push_back(key);
        Ok(true)
    }
    pub fn poll(&mut self, wall_ms: u64, now: Instant, quantum: usize) -> Result<NativeLinkPoll> {
        let NativePoll {
            frames,
            rejected_datagrams,
            handshake_send_failures,
            receive_errors,
            ..
        } = self.endpoint.poll(wall_ms, now, quantum)?;
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
            ..Default::default()
        };
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
                        lane.feedback(received.peer(), frame.payload[40..72].try_into()?);
                    }
                    Err(_) => report.rejected += 1,
                }
            }
        }
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
            self.epoch = self
                .epoch
                .checked_add(1)
                .context("native ACK epoch exhausted")?;
            let Some(ack) = lane.receiver.transport_feedback(
                self.endpoint.peer_id(peer)?,
                id,
                session,
                self.epoch,
                now,
            )?
            else {
                continue;
            };
            let frame = encode_object_ack(id, stream, &ack)?;
            match self.endpoint.try_send(peer, &frame) {
                Ok(true) => report.feedback_sent += 1,
                Ok(false) => lane.feedback(peer, id),
                Err(error) if error.downcast_ref::<std::io::Error>().is_some() => {
                    report.send_failures += 1;
                    lane.feedback(peer, id);
                }
                Err(error) => return Err(error),
            }
        }
        // Fair across retained jobs, with no per-object wait for an ACK.
        for _ in 0..quantum.min(self.order.len()) {
            let key = self.order.pop_front().expect("bounded job count");
            let sender = self.jobs.get_mut(&key).expect("retained send job");
            if sender.status() == ObjectSendState::Sending {
                match self
                    .endpoint
                    .poll_object(key.0, sender, now, quantum - report.data_sent)
                {
                    Ok(sent) => report.data_sent += sent,
                    Err(error) if error.downcast_ref::<std::io::Error>().is_some() => {
                        report.send_failures += 1
                    }
                    Err(error) => return Err(error),
                }
            }
            if sender.status() == ObjectSendState::Sending {
                self.order.push_back(key);
                if report.data_sent == quantum {
                    break;
                }
                continue;
            }
            let sender = self.jobs.remove(&key).expect("finished send job");
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
        Ok(report)
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

#[cfg(test)]
mod tests;
