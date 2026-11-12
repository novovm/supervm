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
pub const RELAY_COMMAND_BYTES: usize = COMMAND_BYTES + 32;
pub const DOUBLE_RELAY_COMMAND_BYTES: usize = RELAY_COMMAND_BYTES + 32;

#[derive(Clone, Debug)]
pub enum ProbeCommand {
    Start(NetworkProbe),
    Cancel(NetworkProbe),
    /// Local diagnostic reservation only; never installs a business peer route.
    RelayArm(NetworkProbe, Vec<Hash>),
    RelaySend(NetworkProbe, Vec<Hash>),
    RelayCancel(NetworkProbe, Vec<Hash>),
}
impl ProbeCommand {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let (mut tag, probe) = match self {
            Self::Start(p) => (1, p),
            Self::Cancel(p) => (2, p),
            Self::RelayArm(p, _) => (3, p),
            Self::RelaySend(p, _) => (4, p),
            Self::RelayCancel(p, _) => (5, p),
        };
        let codec = DecodeLimits {
            transactions: 1,
            transaction_bytes: 512,
            body_bytes: 512,
            message_bytes: 512,
        };
        let wire = super::super::transport::encode(&Message::NetworkProbe(probe.clone()), codec)?;
        if let Self::RelayArm(_, via) | Self::RelaySend(_, via) | Self::RelayCancel(_, via) = self {
            ensure!(
                (1..=2).contains(&via.len())
                    && via.iter().enumerate().all(|(i, peer)| *peer != [0; 32]
                        && *peer != probe.sender
                        && *peer != probe.recipient
                        && !via[..i].contains(peer)),
                "invalid diagnostic relay path"
            );
            // Tags3..5 retain the original single-hop bytes. Tags6..8 bind
            // exactly two ordered relays; old nodes reject rather than truncate.
            if via.len() == 2 {
                tag += 3;
            }
        }
        let mut out = COMMAND_MAGIC.to_vec();
        out.push(tag);
        out.extend_from_slice(&wire[11..]);
        if let Self::RelayArm(_, via) | Self::RelaySend(_, via) | Self::RelayCancel(_, via) = self {
            for peer in via {
                out.extend_from_slice(peer);
            }
        }
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            matches!(
                bytes.len(),
                COMMAND_BYTES | RELAY_COMMAND_BYTES | DOUBLE_RELAY_COMMAND_BYTES
            ) && &bytes[..8] == COMMAND_MAGIC,
            "invalid bounded probe command"
        );
        let mut wire = b"NVHOSTN1\0\x01\x15".to_vec();
        ensure!(
            (bytes[8] <= 2 && bytes.len() == COMMAND_BYTES)
                || ((3..=5).contains(&bytes[8]) && bytes.len() == RELAY_COMMAND_BYTES)
                || ((6..=8).contains(&bytes[8]) && bytes.len() == DOUBLE_RELAY_COMMAND_BYTES),
            "probe command kind/size mismatch"
        );
        wire.extend_from_slice(&bytes[9..COMMAND_BYTES]);
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
            3..=8 => {
                let via: Vec<Hash> = bytes[COMMAND_BYTES..]
                    .chunks_exact(32)
                    .map(|chunk| chunk.try_into().expect("exact hash chunk"))
                    .collect();
                ensure!(
                    via.iter().enumerate().all(|(i, peer)| *peer != [0; 32]
                        && *peer != probe.sender
                        && *peer != probe.recipient
                        && !via[..i].contains(peer)),
                    "invalid diagnostic relay"
                );
                Ok(match bytes[8] {
                    3 | 6 => Self::RelayArm(probe, via),
                    4 | 7 => Self::RelaySend(probe, via),
                    _ => Self::RelayCancel(probe, via),
                })
            }
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
struct PendingTimeReceipt {
    peer: usize,
    probe: NetworkProbe,
    object: Hash,
    ready: Instant,
    deadline: Instant,
    generation: u64,
    transport: ProbeTransport,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    peer_time_interval_ms: Option<(u64, u64)>,
}
pub(super) struct ProbeState {
    active: Option<Flight>,
    next_send: Instant,
    seen: VecDeque<(Hash, Instant)>,
    received: VecDeque<((Hash, Hash), Instant)>,
    pending_time: Vec<PendingTimeReceipt>,
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
            peer_time_interval_ms: None,
        });
    }
}

impl NativeLifecycle {
    #[cfg(feature = "iroh-transport")]
    pub(crate) fn network_probe_diagnostics(
        &self,
    ) -> Option<std::sync::Arc<novovm_network::novorudp::diagnostics::NativeTransferDiagnosticsV1>>
    {
        self.probe.as_ref()?;
        self.channel.probe_diagnostics()
    }

    pub fn enable_network_probe(&mut self, now: Instant) {
        if self.probe.is_none() {
            self.channel.enable_probe_observation();
            profile::enable_native_delivery_observation();
            self.probe = Some(ProbeState {
                active: None,
                next_send: now,
                seen: VecDeque::with_capacity(HISTORY),
                received: VecDeque::with_capacity(HISTORY),
                pending_time: Vec::new(),
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
            _ => anyhow::bail!("relay diagnostic command requires original NOVONET owner"),
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
        let validity = self.peer_validity_window(peer, probe.issued_at_ms, probe.expires_at_ms);
        let Some(state) = &mut self.probe else {
            self.rejected_messages += 1;
            return;
        };
        state.received.retain(|(_, expiry)| now < *expiry);
        if now.saturating_duration_since(state.receive_window) >= Duration::from_secs(1) {
            state.receive_window = now;
            state.receive_count = 0;
        }
        let event = if probe.sender != self.peers[peer]
            || probe.recipient != self.progress.local_validator()
        {
            "probe_rejected_receiver_or_sender"
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
            // Remember rejected as well as accepted nonces through the full
            // original lifetime. Re-measuring must not revive this object.
            state.received.push_back((
                (probe.sender, probe.nonce),
                now + Duration::from_millis(super::super::transport::probe::MAX_LIFETIME_MS),
            ));
            match &validity {
                Ok((ready, deadline, generation, _)) if *ready > now => {
                    if state.pending_time.len() >= 4 {
                        "probe_rejected_rate_or_quota"
                    } else {
                        state.pending_time.push(PendingTimeReceipt {
                            peer,
                            probe: probe.clone(),
                            object,
                            ready: *ready,
                            deadline: *deadline,
                            generation: *generation,
                            transport: self.channel.probe_transport(peer),
                        });
                        "probe_pending_time_boundary"
                    }
                }
                Ok(_) => "probe_received_integrity_verified",
                Err(_) => "probe_rejected_time_untrusted_or_outside_validity",
            }
        };
        state.note(
            event,
            object,
            probe,
            self.peers[peer],
            self.channel.probe_transport(peer),
        );
        if let Some(observation) = state.observations.back_mut() {
            observation.peer_time_interval_ms = validity.ok().map(|(_, _, _, interval)| interval);
        }
        if event != "probe_received_integrity_verified" && event != "probe_pending_time_boundary" {
            self.rejected_messages += 1;
        }
    }
    pub(super) fn poll_network_probe_time(&mut self, now: Instant) {
        let Some(state) = &mut self.probe else { return };
        let mut index = 0;
        while index < state.pending_time.len() {
            let pending = &state.pending_time[index];
            let reference = &self.time_peers[pending.peer].reference;
            let invalid = reference.generation() != pending.generation
                || reference.interval(now).is_err()
                || self.channel.probe_transport(pending.peer).session_id
                    != pending.transport.session_id;
            let event = if invalid || now >= pending.deadline {
                "probe_rejected_time_untrusted_or_outside_validity"
            } else if now < pending.ready {
                index += 1;
                continue;
            } else {
                "probe_received_integrity_verified"
            };
            let pending = state.pending_time.remove(index);
            state.note(
                event,
                pending.object,
                &pending.probe,
                self.peers[pending.peer],
                pending.transport,
            );
            if event != "probe_received_integrity_verified" {
                self.rejected_messages += 1;
            }
        }
    }
    pub fn network_probe_observation(&self) -> Option<serde_json::Value> {
        let state = self.probe.as_ref()?;
        Some(
            serde_json::json!({"enabled":true,"scope":"diagnostic only; original Object DONE is bounded custody, never business acceptance or finality",
            "max_active":1,"max_events":HISTORY,"evicted_events":state.evicted,
            "events":state.observations,"transport":self.channel.probe_observation(),
            "protocol_time":self.network_time_observation()}),
        )
    }
}

#[cfg(test)]
mod tests;
