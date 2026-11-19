//! Shared NOVONET time messages on the original bounded object queues. No
//! socket, clock service, ledger rules, detached worker or reliability owner.
use super::*;
use novovm_network::network_time::{
    coordination::{
        self, InsufficientTimeSamplesV1, PeerTimeReferenceV1, TimeExchangeV1, MEASUREMENT_LIFETIME,
        SAMPLE_INTERVAL,
    },
    ClockAnchorV1,
};
use novovm_network::novorudp::object_link::NativeSendCompletion;

struct Flight {
    id: Hash,
    not_before: Instant,
    deadline: Instant,
    session: Option<[u8; 16]>,
    response: bool,
}

#[derive(Clone, Copy, Default, serde::Serialize)]
struct TimeFlow {
    queue_attempted: u64,
    queued: u64,
    admitted: u64,
    expired_before_admission: u64,
    transport_done: u64,
    transport_expired: u64,
    transport_cancelled: u64,
    transport_session_replaced: u64,
    received: u64,
    accepted: u64,
    rejected: u64,
    reply_deferred: u64,
    reply_rate_limited: u64,
}

#[derive(Clone, Copy, Default, serde::Serialize)]
struct TimeSessionFlow {
    requests: TimeFlow,
    responses: TimeFlow,
}

impl TimeSessionFlow {
    fn direction(&mut self, response: bool) -> &mut TimeFlow {
        if response {
            &mut self.responses
        } else {
            &mut self.requests
        }
    }
}

#[derive(Default)]
pub(super) struct TimePeer {
    pub reference: PeerTimeReferenceV1,
    accepted_samples: u64,
    rejected_samples: u64,
    session: Option<[u8; 16]>,
    established_on_session: bool,
    flights: Vec<Flight>,
    next_reply: Option<Instant>,
    flow: TimeSessionFlow,
}

pub(super) enum ProbeTimeCheck {
    WarmingUp {
        generation: u64,
    },
    Valid {
        ready: Instant,
        deadline: Instant,
        generation: u64,
        interval: (u64, u64),
    },
    Reject {
        reason: &'static str,
        interval: Option<(u64, u64)>,
    },
}
impl TimePeer {
    fn observe_session(&mut self, session: Option<[u8; 16]>) -> bool {
        if self.session == session {
            return false;
        }
        self.reference.invalidate();
        self.session = session;
        self.established_on_session = false;
        self.flow = TimeSessionFlow::default();
        true
    }

    fn flight_flow(&mut self, id: Hash) -> Option<&mut TimeFlow> {
        let flight = self.flights.iter().find(|f| f.id == id)?;
        // The original transport can return stale handles after rebind.
        (self.session.is_some() && flight.session == self.session)
            .then(|| self.flow.direction(flight.response))
    }

    pub(super) fn note_admission(&mut self, id: Hash, now: Instant) {
        let response = self.flights.iter().any(|f| f.id == id && f.response);
        if let Some(flow) = self.flight_flow(id) {
            flow.admitted = flow.admitted.saturating_add(1);
            if response {
                // Keep the per-peer pacing guard across session changes.
                self.next_reply = Some(now + SAMPLE_INTERVAL);
            }
        }
    }

    pub(super) fn send_window(&self, object: Hash) -> Option<(Instant, Instant)> {
        self.flights
            .iter()
            .find(|f| f.id == object)
            .map(|f| (f.not_before, f.deadline))
    }
}
impl NativeLifecycle {
    pub(super) fn time_clock(&self) -> Result<ClockAnchorV1> {
        let clock = ClockAnchorV1::capture()?;
        #[cfg(test)]
        let clock = ClockAnchorV1::from_parts(
            clock
                .wall_ms()
                .checked_add_signed(self.time_test_offset_ms)
                .context("test clock overflow")?,
            clock.instant(),
        );
        Ok(clock)
    }
    fn time_context(&self, peer: usize, reverse: bool) -> Option<[u8; 32]> {
        let session = self.channel.probe_transport(peer).session_id?;
        let context = self.progress.context();
        let domain = super::super::transport::fragment_domain(
            context.chain_id,
            context.genesis_config_commitment,
            context.protocol_commitment,
        );
        let local = self.progress.local_validator();
        let remote = self.peers[peer];
        Some(coordination::context(
            domain,
            if reverse { remote } else { local },
            if reverse { local } else { remote },
            session,
        ))
    }
    fn queue_network_time(
        &mut self,
        peer: usize,
        message: TimeExchangeV1,
        now: Instant,
    ) -> Result<bool> {
        let response = message.response;
        let state = &mut self.time_peers[peer];
        let flow = state.flow.direction(response);
        flow.queue_attempted = flow.queue_attempted.saturating_add(1);
        // One request plus one response at most, charged to existing auxiliary
        // capacity. Never consume the reserved consensus-control allocation.
        if state.flights.len() >= 2 || state.flights.iter().any(|f| f.response == response) {
            return Ok(false);
        }
        let not_before = if response {
            state.next_reply.unwrap_or(now).max(now)
        } else {
            now
        };
        let deadline = now + MEASUREMENT_LIFETIME;
        if not_before >= deadline {
            return Ok(false);
        }
        let prepared = self.channel.prepare(&Message::NetworkTime(message))?;
        let id = prepared.fragment_id();
        if !self.enqueue(peer, prepared) {
            return Ok(false);
        }
        let state = &mut self.time_peers[peer];
        let flow = state.flow.direction(response);
        flow.queued = flow.queued.saturating_add(1);
        state.flights.push(Flight {
            id,
            not_before,
            deadline,
            session: state.session,
            response,
        });
        if not_before > now {
            state.flow.requests.reply_deferred =
                state.flow.requests.reply_deferred.saturating_add(1);
        }
        Ok(true)
    }
    // I/O may run while assembly/execution is waiting. Retire stale auxiliary
    // work at its own send boundary, independently of wall-clock availability.
    // Admitted objects remain charged until the original link returns custody.
    pub(super) fn retire_network_time(&mut self, now: Instant) {
        for peer in 0..self.peers.len() {
            let session = self.channel.probe_transport(peer).session_id;
            let changed = self.time_peers[peer].observe_session(session);
            let expired: Vec<_> = self.time_peers[peer]
                .flights
                .iter()
                .filter(|f| changed || now >= f.deadline)
                .map(|f| f.id)
                .collect();
            for id in expired {
                if let Some(index) = self.deliveries[peer]
                    .iter()
                    .position(|d| d.prepared.fragment_id() == id)
                {
                    if self.deliveries[peer][index].admitted {
                        self.deliveries[peer][index].retiring = true;
                    } else {
                        let delivery = self.deliveries[peer]
                            .remove(index)
                            .expect("located time delivery");
                        self.bytes[peer] -= delivery.prepared.byte_len();
                        let state = &mut self.time_peers[peer];
                        if let Some(flow) = state.flight_flow(id) {
                            flow.expired_before_admission =
                                flow.expired_before_admission.saturating_add(1);
                        }
                        state.flights.retain(|f| f.id != id);
                    }
                }
            }
        }
    }
    pub(super) fn poll_network_time(&mut self) -> Result<()> {
        let Ok(clock) = self.time_clock() else {
            for peer in &mut self.time_peers {
                peer.reference.invalidate();
            }
            return Ok(());
        };
        let now = clock.instant();
        self.retire_network_time(now);
        for peer in 0..self.peers.len() {
            let Some(context) = self.time_context(peer, false) else {
                continue;
            };
            self.time_peers[peer].reference.observe(context, clock);
            // Wait for the existing transport to return its handle. No new
            // measurements can accumulate behind a broken path.
            if self.time_peers[peer].flights.is_empty() && self.time_peers[peer].reference.due(now)
            {
                let message = self.time_peers[peer]
                    .reference
                    .begin(clock, rand::random())?;
                self.queue_network_time(peer, message, now)?;
            }
        }
        Ok(())
    }
    pub(super) fn receive_network_time(
        &mut self,
        peer: usize,
        message: &TimeExchangeV1,
    ) -> Result<()> {
        self.receive_network_time_at(peer, message, self.time_clock()?)
    }

    fn receive_network_time_at(
        &mut self,
        peer: usize,
        message: &TimeExchangeV1,
        clock: ClockAnchorV1,
    ) -> Result<()> {
        let now = clock.instant();
        let context = self
            .time_context(peer, false)
            .context("time session unavailable")?;
        let flow = self.time_peers[peer].flow.direction(message.response);
        flow.received = flow.received.saturating_add(1);
        self.time_peers[peer].reference.observe(context, clock);
        if message.response {
            let state = &mut self.time_peers[peer];
            let result = state.reference.accept(message, clock);
            if result.is_ok() {
                state.accepted_samples = state.accepted_samples.saturating_add(1);
                state.flow.responses.accepted = state.flow.responses.accepted.saturating_add(1);
                state.established_on_session |= state.reference.interval(now).is_ok();
            } else {
                state.rejected_samples = state.rejected_samples.saturating_add(1);
                state.flow.responses.rejected = state.flow.responses.rejected.saturating_add(1);
            }
            result
        } else {
            let expected = self
                .time_context(peer, true)
                .context("time session unavailable")?;
            let response = message.reply(expected, clock);
            let flow = &mut self.time_peers[peer].flow.requests;
            if response.is_ok() {
                flow.accepted = flow.accepted.saturating_add(1);
            } else {
                flow.rejected = flow.rejected.saturating_add(1);
            }
            let response = response?;
            // Arrival spacing may be shorter than the sender's issue spacing.
            // Keep one response in the existing bounded queue instead of
            // dropping it. The original timestamp keeps the queueing delay in
            // measured uncertainty; pacing does not extend its expiry.
            if self.time_peers[peer].flights.iter().any(|f| f.response) {
                let flow = &mut self.time_peers[peer].flow.requests;
                flow.reply_rate_limited = flow.reply_rate_limited.saturating_add(1);
                anyhow::bail!("time reply slot occupied");
            }
            ensure!(
                self.queue_network_time(peer, response, now)?,
                "time reply backpressure"
            );
            Ok(())
        }
    }
    pub(super) fn complete_network_time(&mut self, completed: &NativeSendCompletion) -> bool {
        let Some(state) = self.time_peers.get_mut(completed.peer) else {
            return false;
        };
        let id = completed.body.id();
        if !state.flights.iter().any(|f| f.id == id) {
            return false;
        }
        if completed.state == ObjectSendState::Sending {
            return false;
        }
        if let Some(flow) = state.flight_flow(id) {
            let counter = match completed.state {
                ObjectSendState::Delivered => &mut flow.transport_done,
                ObjectSendState::Expired => &mut flow.transport_expired,
                ObjectSendState::SessionReplaced => &mut flow.transport_session_replaced,
                ObjectSendState::Cancelled => &mut flow.transport_cancelled,
                ObjectSendState::Sending => unreachable!("nonterminal time completion"),
            };
            *counter = counter.saturating_add(1);
        }
        state.flights.retain(|f| f.id != id);
        if let Some(index) = self.deliveries[completed.peer]
            .iter()
            .position(|d| d.prepared.fragment_id() == id)
        {
            let delivery = self.deliveries[completed.peer]
                .remove(index)
                .expect("located time completion");
            self.bytes[completed.peer] -= delivery.prepared.byte_len();
        }
        true
    }
    pub(super) fn probe_time_check(
        &mut self,
        peer: usize,
        issued: u64,
        expires: u64,
    ) -> ProbeTimeCheck {
        let unavailable = ProbeTimeCheck::Reject {
            reason: "reference_unavailable",
            interval: None,
        };
        let (Ok(clock), Some(context)) = (self.time_clock(), self.time_context(peer, false)) else {
            return unavailable;
        };
        let state = &mut self.time_peers[peer];
        let reference = &mut state.reference;
        reference.observe(context, clock);
        let now = clock.instant();
        let generation = reference.generation();
        let interval = match reference.interval(now) {
            Ok(interval) => interval,
            Err(err) if err.is::<InsufficientTimeSamplesV1>() && !state.established_on_session => {
                return ProbeTimeCheck::WarmingUp { generation };
            }
            Err(_) => return unavailable,
        };
        match reference.validity_window(issued, expires, now) {
            Ok((ready, deadline)) => ProbeTimeCheck::Valid {
                ready,
                deadline,
                generation,
                interval,
            },
            Err(_) => ProbeTimeCheck::Reject {
                reason: "validity_window_rejected",
                interval: Some(interval),
            },
        }
    }
    pub(super) fn network_time_snapshot(&self) -> impl serde::Serialize + Send + use<> {
        use crate::native_pipeline::report::record;
        let now = Instant::now();
        record! {
            scope: "session-relative peer-clock consistency, not trusted UTC or chain authorization",
            peers: self.time_peers.iter().enumerate().map(|(peer, state)| {
                record! {peer: self.peers[peer], session: state.session,
                    relative_ready: state.reference.interval(now).is_ok(),
                    accepted_samples: state.accepted_samples,rejected_samples: state.rejected_samples,
                    inflight: state.flights.len(),
                    session_inflight: state.flights.iter().filter(|flight|
                        state.session.is_some() && flight.session == state.session).count(),
                    session_flow: state.flow}
            }).collect::<Vec<_>>()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::consensus::tests::progress_integration::with_delivery_nodes_for_test;

    #[test]
    #[ignore = "real AOEM and original local UDP lifecycle; not physical Iroh acceptance"]
    fn early_time_reply_waits_in_bounded_original_sender() -> Result<()> {
        with_delivery_nodes_for_test(|nodes, leader, _, _| {
            let node = &mut nodes[leader];
            let peer = 0;
            let now = Instant::now();
            let clock = ClockAnchorV1::from_parts(10_000, now);
            node.retire_network_time(now);
            let expected = node.time_context(peer, true).context("test session")?;
            let ready = now + Duration::from_millis(5);
            node.time_peers[peer].next_reply = Some(ready);
            let request = TimeExchangeV1 {
                response: false,
                context: expected,
                challenge: [8; 32],
                receive_ms: 0,
                send_ms: 0,
                residence_ns: 0,
            };
            let mut invalid = request.clone();
            invalid.context[0] ^= 1;
            assert!(node.receive_network_time_at(peer, &invalid, clock).is_err());
            assert!(node.time_peers[peer].flights.is_empty());

            node.receive_network_time_at(peer, &request, clock)?;
            let flight = &node.time_peers[peer].flights[0];
            let id = flight.id;
            assert_eq!(flight.not_before, ready);
            assert_eq!(flight.deadline, now + MEASUREMENT_LIFETIME);
            let second = TimeExchangeV1 {
                challenge: [9; 32],
                ..request
            };
            assert!(node.receive_network_time_at(peer, &second, clock).is_err());
            assert_eq!(node.time_peers[peer].flights.len(), 1);
            assert_eq!(node.time_peers[peer].flow.requests.reply_deferred, 1);
            assert_eq!(node.time_peers[peer].flow.requests.reply_rate_limited, 1);

            node.poll_io(clock.wall_ms(), now)?;
            assert!(node.deliveries[peer]
                .iter()
                .any(|d| d.prepared.fragment_id() == id && !d.admitted));
            node.poll_io(clock.at(ready)?, ready)?;
            assert!(node.deliveries[peer]
                .iter()
                .any(|d| d.prepared.fragment_id() == id && d.admitted));
            assert_eq!(
                node.time_peers[peer].next_reply,
                Some(ready + SAMPLE_INTERVAL)
            );
            node.poll_io(12_000, now + MEASUREMENT_LIFETIME)?;
            assert!(node.time_peers[peer].flights.is_empty());
            Ok(())
        })
    }

    #[test]
    fn time_flow_is_partitioned_and_discards_stale_flight_updates() {
        let now = Instant::now();
        let mut state = TimePeer::default();
        let old = Some([1; 16]);
        let current = Some([2; 16]);
        assert!(state.observe_session(old));
        let request = Hash::from([3; 32]);
        let response = Hash::from([4; 32]);
        for (id, reply) in [(request, false), (response, true)] {
            state.flights.push(Flight {
                id,
                not_before: now,
                deadline: now,
                session: old,
                response: reply,
            });
        }
        state.accepted_samples = 5;
        state.note_admission(request, now);
        assert!(!state.observe_session(old));
        assert_eq!(state.flow.requests.admitted, 1);
        assert_eq!(state.flow.responses.admitted, 0);
        assert!(state.observe_session(current));
        state.note_admission(response, now);
        assert!(state.flight_flow(request).is_none());
        assert!(state.flight_flow(response).is_none());
        let snapshot = serde_json::to_value(state.flow).unwrap();
        assert_eq!(snapshot["requests"]["admitted"], 0);
        assert_eq!(snapshot["responses"]["admitted"], 0);
        assert_eq!(state.accepted_samples, 5);
    }

    #[test]
    #[ignore = "real AOEM and original UDP lifecycle; not physical four-host acceptance"]
    fn expired_time_queue_is_retired_before_io_without_assembly_poll() -> Result<()> {
        with_delivery_nodes_for_test(|nodes, leader, message, _| {
            let node = &mut nodes[leader];
            let peer = 0;
            let now = Instant::now();
            node.time_peers[peer].session = node.channel.probe_transport(peer).session_id;
            let prepared = node.channel.prepare(&message)?;
            let consensus_id = prepared.fragment_id();
            let consensus_bytes = prepared.byte_len();
            let request = TimeExchangeV1 {
                response: false,
                context: [7; 32],
                challenge: [8; 32],
                receive_ms: 0,
                send_ms: 0,
                residence_ns: 0,
            };
            // Deterministic queue age; no sleep, enlarged TTL or mocked send.
            assert!(node.queue_network_time(peer, request.clone(), now - MEASUREMENT_LIFETIME)?);
            assert_eq!(node.time_peers[peer].flow.requests.queued, 1);
            let expired_id = node.time_peers[peer].flights[0].id;
            assert!(node.enqueue(peer, prepared));
            assert!(node.bytes[peer] > consensus_bytes);
            // Recovery/execution can keep I/O progressing without assembly.
            // The expired auxiliary object must not freeze that same owner.
            node.poll_io(1000, now)?;
            assert!(!node.is_frozen());
            assert!(node.time_peers[peer].flights.is_empty());
            assert!(!node.deliveries[peer]
                .iter()
                .any(|d| d.prepared.fragment_id() == expired_id));
            assert!(node.deliveries[peer]
                .iter()
                .any(|d| d.prepared.fragment_id() == consensus_id));
            assert_eq!(node.bytes[peer], consensus_bytes);
            // Fresh work is still admitted. Once admitted, expiry is returned
            // by the original object owner before bytes/capacity are released.
            let fresh = TimeExchangeV1 {
                challenge: [9; 32],
                ..request
            };
            assert!(node.queue_network_time(peer, fresh, now)?);
            let fresh_id = node.time_peers[peer].flights[0].id;
            node.poll_io(1000, now)?;
            assert!(node.deliveries[peer]
                .iter()
                .any(|d| d.prepared.fragment_id() == fresh_id && d.admitted));
            assert!(node.bytes[peer] > consensus_bytes);
            node.poll_io(3000, now + MEASUREMENT_LIFETIME)?;
            assert!(!node.is_frozen());
            assert!(node.time_peers[peer].flights.is_empty());
            assert_eq!(node.bytes[peer], consensus_bytes);
            Ok(())
        })
    }
}
