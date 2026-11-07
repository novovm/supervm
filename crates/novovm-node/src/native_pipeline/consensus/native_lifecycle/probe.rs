//! A single opt-in diagnostic obligation, charged to the original delivery
//! queue/lane. No separate sender, retries, socket, business receipt or ledger.
use super::super::native::ProbeTransport;
use super::*;
use crate::native_pipeline::consensus::transport::NetworkProbe;
use crate::native_pipeline::profile;
use novovm_network::novorudp::object_link::NativeSendCompletion;
use serde::Serialize;

const HISTORY: usize = 64;
const COMMAND_MAGIC: &[u8; 8] = b"NVPROBC1";
fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}
pub const COMMAND_BYTES: usize = 9 + super::super::transport::probe::WIRE_BYTES;

#[derive(Clone, Debug)]
pub enum ProbeCommand {
    Start(NetworkProbe),
    Cancel(NetworkProbe),
}
impl ProbeCommand {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let (tag, probe) = match self {
            Self::Start(p) => (1, p),
            Self::Cancel(p) => (2, p),
        };
        let codec = DecodeLimits {
            transactions: 1,
            transaction_bytes: 512,
            body_bytes: 512,
            message_bytes: 512,
        };
        let wire = super::super::transport::encode(&Message::NetworkProbe(probe.clone()), codec)?;
        let mut out = COMMAND_MAGIC.to_vec();
        out.push(tag);
        out.extend_from_slice(&wire[11..]);
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() == COMMAND_BYTES && &bytes[..8] == COMMAND_MAGIC,
            "invalid bounded probe command"
        );
        let mut wire = b"NVHOSTN1\0\x01\x15".to_vec();
        wire.extend_from_slice(&bytes[9..]);
        let codec = DecodeLimits {
            transactions: 1,
            transaction_bytes: 512,
            body_bytes: 512,
            message_bytes: 512,
        };
        let Message::NetworkProbe(probe) = super::super::transport::decode(&wire, codec)? else {
            unreachable!()
        };
        match bytes[8] {
            1 => Ok(Self::Start(probe)),
            2 => Ok(Self::Cancel(probe)),
            _ => anyhow::bail!("unknown probe command"),
        }
    }
}

struct Flight {
    peer: usize,
    probe: NetworkProbe,
    object: Hash,
    deadline: Instant,
    transport: ProbeTransport,
    cancel_requested: bool,
}
#[derive(Clone, Serialize)]
struct Observation {
    event: &'static str,
    object_id: Hash,
    nonce: Hash,
    sender: Hash,
    recipient: Hash,
    issued_at_ms: u64,
    expires_at_ms: u64,
    peer: Hash,
    #[serde(flatten)]
    transport: ProbeTransport,
    wall_ms: u64,
}
pub(super) struct ProbeState {
    active: Option<Flight>,
    next_send: Instant,
    seen: VecDeque<(Hash, Instant)>,
    received: VecDeque<((Hash, Hash), Instant)>,
    receive_window: Instant,
    receive_count: usize,
    observations: VecDeque<Observation>,
    evicted: u64,
}
impl ProbeState {
    pub(super) fn object_for_peer(&self, peer: usize) -> Option<Hash> {
        self.active
            .as_ref()
            .filter(|flight| flight.peer == peer)
            .map(|flight| flight.object)
    }
    pub(super) fn deadline(&self, object: Hash) -> Option<Instant> {
        self.active
            .as_ref()
            .filter(|f| f.object == object)
            .map(|f| f.deadline)
    }
    pub(super) fn admitted(&mut self, object: Hash, transport: ProbeTransport, peer: Hash) {
        let Some(flight) = self
            .active
            .as_mut()
            .filter(|flight| flight.object == object)
        else {
            return;
        };
        flight.transport = transport;
        let probe = flight.probe.clone();
        self.note("probe_admitted", object, &probe, peer, transport);
    }
    fn note(
        &mut self,
        event: &'static str,
        object: Hash,
        probe: &NetworkProbe,
        peer: Hash,
        transport: ProbeTransport,
    ) {
        // The original bounded delivery_event remains the timeline owner.
        profile::delivery_event(event, 0, peer, object, 0);
        if self.observations.len() == HISTORY {
            self.observations.pop_front();
            self.evicted += 1;
        }
        self.observations.push_back(Observation {
            event,
            object_id: object,
            nonce: probe.nonce,
            sender: probe.sender,
            recipient: probe.recipient,
            issued_at_ms: probe.issued_at_ms,
            expires_at_ms: probe.expires_at_ms,
            peer,
            transport,
            wall_ms: wall_ms(),
        });
    }
}

impl NativeLifecycle {
    pub fn enable_network_probe(&mut self, now: Instant) {
        if self.probe.is_none() {
            self.channel.enable_probe_observation();
            profile::enable_native_delivery_observation();
            self.probe = Some(ProbeState {
                active: None,
                next_send: now,
                seen: VecDeque::with_capacity(HISTORY),
                received: VecDeque::with_capacity(HISTORY),
                receive_window: now,
                receive_count: 0,
                observations: VecDeque::with_capacity(HISTORY),
                evicted: 0,
            });
        }
    }
    pub fn network_probe_command(
        &mut self,
        command: ProbeCommand,
        wall_ms: u64,
        now: Instant,
    ) -> Result<Hash> {
        ensure!(!self.is_frozen(), "native lifecycle frozen");
        let probe = match &command {
            ProbeCommand::Start(p) | ProbeCommand::Cancel(p) => p,
        };
        probe.validate()?;
        ensure!(
            probe.sender == self.progress.local_validator(),
            "probe sender differs from original owner"
        );
        let peer = self
            .peers
            .iter()
            .position(|key| *key == probe.recipient)
            .context("unknown probe recipient")?;
        let state = self.probe.as_mut().context("network probe disabled")?;
        if matches!(command, ProbeCommand::Cancel(_)) {
            let flight = state.active.as_ref().context("no active probe")?;
            ensure!(flight.probe == *probe, "cancel does not match active probe");
            let object = flight.object;
            let delivery = self.deliveries[peer]
                .iter()
                .find(|d| d.prepared.fragment_id() == object)
                .context("probe queue owner missing")?;
            if delivery.admitted {
                // The shared carrier may have a partially written frame. Keep
                // its original bounded drain, just like retiring native work;
                // cancellation cannot promise the remote never took custody.
                state
                    .active
                    .as_mut()
                    .expect("active probe")
                    .cancel_requested = true;
                state.note(
                    "probe_cancel_requested_drain",
                    object,
                    probe,
                    self.peers[peer],
                    self.channel.probe_transport(peer),
                );
            } else {
                self.finish_network_probe("probe_cancelled", now);
            }
            return Ok(object);
        }
        probe.validate_time(wall_ms)?;
        state.seen.retain(|(_, expiry)| now < *expiry);
        ensure!(
            state.active.is_none() && now >= state.next_send,
            "probe busy or rate limited"
        );
        ensure!(
            !state.seen.iter().any(|(nonce, _)| *nonce == probe.nonce)
                && state.seen.len() < HISTORY,
            "duplicate probe nonce or retained quota"
        );
        let prepared = self
            .channel
            .prepare(&Message::NetworkProbe(probe.clone()))?;
        let object = prepared.fragment_id();
        ensure!(
            self.enqueue(peer, prepared),
            "original delivery queue backpressure"
        );
        let deadline = now + Duration::from_millis(probe.expires_at_ms - wall_ms);
        let state = self.probe.as_mut().expect("enabled probe");
        state.seen.push_back((probe.nonce, deadline));
        state.next_send = now + Duration::from_secs(1);
        state.note(
            "probe_owned",
            object,
            probe,
            self.peers[peer],
            self.channel.probe_transport(peer),
        );
        state.active = Some(Flight {
            peer,
            probe: probe.clone(),
            object,
            deadline,
            transport: ProbeTransport {
                session_id: None,
                path: "not_admitted",
                route: None,
            },
            cancel_requested: false,
        });
        Ok(object)
    }
    fn finish_network_probe(&mut self, event: &'static str, _now: Instant) {
        let Some(state) = &mut self.probe else { return };
        let Some(flight) = state.active.take() else {
            return;
        };
        if let Some(index) = self.deliveries[flight.peer]
            .iter()
            .position(|d| d.prepared.fragment_id() == flight.object)
        {
            let delivery = self.deliveries[flight.peer]
                .remove(index)
                .expect("probe delivery");
            self.bytes[flight.peer] -= delivery.prepared.byte_len();
        }
        state.note(
            event,
            flight.object,
            &flight.probe,
            self.peers[flight.peer],
            flight.transport,
        );
    }
    pub(super) fn expire_network_probe(&mut self, now: Instant) {
        let Some(flight) = self.probe.as_ref().and_then(|s| s.active.as_ref()) else {
            return;
        };
        if now < flight.deadline {
            return;
        }
        if self.deliveries[flight.peer]
            .iter()
            .any(|d| d.prepared.fragment_id() == flight.object && !d.admitted)
        {
            self.finish_network_probe("probe_expired_before_admission", now);
        }
        // Admitted bytes stay owned until the SAME link returns real expiry.
    }
    pub(super) fn complete_network_probe(
        &mut self,
        done: &NativeSendCompletion,
        now: Instant,
    ) -> bool {
        let Some(flight) = self.probe.as_ref().and_then(|s| s.active.as_ref()) else {
            return false;
        };
        if flight.object != done.body.id() || flight.peer != done.peer {
            return false;
        }
        if done.state == ObjectSendState::SessionReplaced
            && now < flight.deadline
            && !flight.cancel_requested
        {
            let probe = flight.probe.clone();
            let old_transport = flight.transport;
            if let Some(delivery) = self.deliveries[done.peer]
                .iter_mut()
                .find(|d| d.prepared.fragment_id() == done.body.id())
            {
                delivery.admitted = false;
            }
            self.probe.as_mut().expect("active probe").note(
                "probe_session_replaced_retry_original",
                done.body.id(),
                &probe,
                self.peers[done.peer],
                old_transport,
            );
            return true;
        }
        let event = match (done.state, flight.cancel_requested) {
            (ObjectSendState::Delivered, true) => "probe_cancelled_drained_object_done",
            (ObjectSendState::Expired, true) => "probe_cancelled_drained_expired",
            (ObjectSendState::SessionReplaced, true) => "probe_cancelled_session_replaced",
            (ObjectSendState::Delivered, false) => "probe_object_done",
            (ObjectSendState::Expired, false) => "probe_expired",
            (ObjectSendState::SessionReplaced, false) => "probe_session_replaced_after_deadline",
            (ObjectSendState::Cancelled, _) => "probe_cancelled",
            (ObjectSendState::Sending, _) => return false,
        };
        self.finish_network_probe(event, now);
        true
    }
    pub(super) fn receive_network_probe(
        &mut self,
        peer: usize,
        probe: &NetworkProbe,
        object: Hash,
        now: Instant,
    ) {
        let Some(state) = &mut self.probe else {
            self.rejected_messages += 1;
            return;
        };
        let wall = wall_ms();
        state.received.retain(|(_, expiry)| now < *expiry);
        if now.saturating_duration_since(state.receive_window) >= Duration::from_secs(1) {
            state.receive_window = now;
            state.receive_count = 0;
        }
        let event = if probe.sender != self.peers[peer]
            || probe.recipient != self.progress.local_validator()
        {
            "probe_rejected_receiver_or_sender"
        } else if probe.validate_time(wall).is_err() {
            "probe_rejected_outside_validity"
        } else if state
            .received
            .iter()
            .any(|(key, _)| *key == (probe.sender, probe.nonce))
        {
            "probe_rejected_duplicate"
        } else if state.receive_count >= 4 || state.received.len() >= HISTORY {
            "probe_rejected_rate_or_quota"
        } else {
            state.receive_count += 1;
            state.received.push_back((
                (probe.sender, probe.nonce),
                now + Duration::from_millis(probe.expires_at_ms - wall),
            ));
            "probe_received_integrity_verified"
        };
        state.note(
            event,
            object,
            probe,
            self.peers[peer],
            self.channel.probe_transport(peer),
        );
        if event != "probe_received_integrity_verified" {
            self.rejected_messages += 1;
        }
    }
    pub fn network_probe_observation(&self) -> Option<serde_json::Value> {
        let state = self.probe.as_ref()?;
        Some(
            serde_json::json!({"enabled":true,"scope":"diagnostic only; original Object DONE is bounded custody, never business acceptance or finality",
            "max_active":1,"max_events":HISTORY,"evicted_events":state.evicted,
            "events":state.observations,"transport":self.channel.probe_observation()}),
        )
    }
}

#[cfg(test)]
mod tests;
