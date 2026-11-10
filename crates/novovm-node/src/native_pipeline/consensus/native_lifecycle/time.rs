//! Shared NOVONET time messages on the original bounded object queues. No
//! socket, clock service, ledger rules, detached worker or reliability owner.
use super::*;
use novovm_network::network_time::{
    coordination::{
        self, PeerTimeReferenceV1, TimeExchangeV1, MEASUREMENT_LIFETIME, SAMPLE_INTERVAL,
    },
    ClockAnchorV1,
};
use novovm_network::novorudp::object_link::NativeSendCompletion;

struct Flight {
    id: Hash,
    deadline: Instant,
}
#[derive(Default)]
pub(super) struct TimePeer {
    pub reference: PeerTimeReferenceV1,
    accepted_samples: u64,
    rejected_samples: u64,
    session: Option<[u8; 16]>,
    flights: Vec<Flight>,
    next_reply: Option<Instant>,
}
impl TimePeer {
    pub(super) fn deadline(&self, object: Hash) -> Option<Instant> {
        self.flights
            .iter()
            .find(|f| f.id == object)
            .map(|f| f.deadline)
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
        // One request plus one response at most, charged to existing auxiliary
        // capacity. Never consume the reserved consensus-control allocation.
        if self.time_peers[peer].flights.len() >= 2 {
            return Ok(false);
        }
        let prepared = self.channel.prepare(&Message::NetworkTime(message))?;
        let id = prepared.fragment_id();
        if !self.enqueue(peer, prepared) {
            return Ok(false);
        }
        self.time_peers[peer].flights.push(Flight {
            id,
            deadline: now + MEASUREMENT_LIFETIME,
        });
        Ok(true)
    }
    pub(super) fn poll_network_time(&mut self) -> Result<()> {
        let Ok(clock) = self.time_clock() else {
            for peer in &mut self.time_peers {
                peer.reference.invalidate();
            }
            return Ok(());
        };
        let now = clock.instant();
        for peer in 0..self.peers.len() {
            let session = self.channel.probe_transport(peer).session_id;
            let changed = self.time_peers[peer].session != session;
            if changed {
                self.time_peers[peer].reference.invalidate();
                self.time_peers[peer].session = session;
            }
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
                        self.time_peers[peer].flights.retain(|f| f.id != id);
                    }
                }
            }
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
        let clock = self.time_clock()?;
        let now = clock.instant();
        let context = self
            .time_context(peer, false)
            .context("time session unavailable")?;
        self.time_peers[peer].reference.observe(context, clock);
        if message.response {
            let state = &mut self.time_peers[peer];
            let result = state.reference.accept(message, clock);
            if result.is_ok() {
                state.accepted_samples = state.accepted_samples.saturating_add(1);
            } else {
                state.rejected_samples = state.rejected_samples.saturating_add(1);
            }
            result
        } else {
            ensure!(
                self.time_peers[peer]
                    .next_reply
                    .is_none_or(|next| now >= next),
                "time reply rate limited"
            );
            let expected = self
                .time_context(peer, true)
                .context("time session unavailable")?;
            let response = message.reply(expected, clock)?;
            self.time_peers[peer].next_reply = Some(now + SAMPLE_INTERVAL);
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
    pub(super) fn peer_validity_window(
        &mut self,
        peer: usize,
        issued: u64,
        expires: u64,
    ) -> Result<(Instant, Instant, u64, (u64, u64))> {
        let clock = self.time_clock()?;
        let context = self
            .time_context(peer, false)
            .context("time session unavailable")?;
        let reference = &mut self.time_peers[peer].reference;
        reference.observe(context, clock);
        let (ready, deadline) = reference.validity_window(issued, expires, clock.instant())?;
        Ok((
            ready,
            deadline,
            reference.generation(),
            reference.interval(clock.instant())?,
        ))
    }
    pub(super) fn network_time_observation(&self) -> serde_json::Value {
        let now = Instant::now();
        serde_json::json!({
            "scope":"session-relative peer-clock consistency, not trusted UTC or chain authorization",
            "peers":self.time_peers.iter().enumerate().map(|(peer, state)| {
                serde_json::json!({"peer":self.peers[peer], "session":state.session,
                    "relative_ready":state.reference.interval(now).is_ok(),
                    "accepted_samples":state.accepted_samples,"rejected_samples":state.rejected_samples,
                    "inflight":state.flights.len()})
            }).collect::<Vec<_>>()
        })
    }
}
