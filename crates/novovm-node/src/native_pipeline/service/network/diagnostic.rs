//! Opt-in route constraint on the original relay owner. This session never
//! enters `Connected` and cannot replace the business NativeChannel endpoint.
use super::*;
use crate::native_pipeline::consensus::{
    native_lifecycle::probe::ProbeCommand,
    transport::{self, DecodeLimits, Message, NetworkProbe},
};
use novovm_network::{
    network_time::{
        coordination::{self, PeerTimeReferenceV1, MEASUREMENT_LIFETIME, SAMPLE_INTERVAL},
        ClockAnchorV1,
    },
    novorudp::{
        fragments::{FragmentProfile, OutgoingMessage, ReassemblyLimits},
        object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
        object_sender::ObjectSendState,
    },
    relay::hop::{HopBudget, HopRoute, HopStream},
};
use std::collections::VecDeque;
#[cfg(test)]
mod tests;

type Hash = [u8; 32];
pub(super) type Shared = Arc<Mutex<State>>;
const LEASE: Duration = Duration::from_secs(60);
const STREAM: u64 = u64::from_be_bytes(*b"NVBFT001");
const HISTORY: usize = 64;
const LIMITS: DecodeLimits = DecodeLimits {
    transactions: 1,
    transaction_bytes: 512,
    body_bytes: 512,
    message_bytes: 512,
};

#[derive(Clone)]
pub(super) struct Request {
    probe: NetworkProbe,
    via: Hash,
    deadline: Instant,
    started: bool,
    ready: bool,
    send: Option<NetworkProbe>,
    sent: bool,
    cancelled: bool,
}
#[derive(Default)]
pub(super) struct State {
    pub identities: Option<(Hash, Vec<Hash>)>,
    pub verified_locators: usize,
    request: Option<Request>,
    busy: bool,
    seen: VecDeque<((Hash, Hash), Instant)>,
    events: VecDeque<serde_json::Value>,
    evicted: u64,
}
impl State {
    pub fn enabled(&self) -> bool {
        self.identities.is_some()
    }
    pub fn observation(&self) -> serde_json::Value {
        serde_json::json!({"enabled":self.enabled(), "active":self.busy,"verified_locator_count":self.verified_locators,
            "reservation":self.request.as_ref().map(|r| serde_json::json!({
                "nonce":r.probe.nonce,"via":r.via,"recipient":r.probe.recipient,
                "started":r.started,"ready":r.ready,"send_requested":r.sent,
                "cancelled":r.cancelled})),"events":self.events,"evicted":self.evicted})
    }
    pub fn command(&mut self, command: ProbeCommand, wall: u64, now: Instant) -> Result<Hash> {
        let (probe, via) = match &command {
            ProbeCommand::RelayArm(p, v)
            | ProbeCommand::RelaySend(p, v)
            | ProbeCommand::RelayCancel(p, v) => (p, *v),
            _ => anyhow::bail!("not a constrained relay diagnostic"),
        };
        probe.validate()?;
        let (local, peers) = self
            .identities
            .as_ref()
            .context("relay diagnostic disabled")?;
        ensure!(
            probe.sender == *local
                && peers.contains(&probe.recipient)
                && peers.contains(&via)
                && via != probe.recipient,
            "diagnostic identity or route differs"
        );
        if matches!(command, ProbeCommand::RelayArm(..)) {
            probe.validate_time(wall)?;
            ensure!(
                !self.busy && self.request.as_ref().is_none_or(|r| now >= r.deadline),
                "diagnostic already reserved"
            );
            self.remember(probe, now)?;
            self.request = Some(Request {
                probe: probe.clone(),
                via,
                deadline: now + LEASE,
                started: false,
                ready: false,
                send: None,
                sent: false,
                cancelled: false,
            });
        } else {
            let r = self.request.as_mut().context("no relay reservation")?;
            ensure!(
                r.probe.nonce == probe.nonce
                    && r.probe.sender == probe.sender
                    && r.probe.recipient == probe.recipient
                    && r.via == via,
                "relay reservation differs"
            );
            if matches!(command, ProbeCommand::RelayCancel(..)) {
                r.cancelled = true;
            } else {
                probe.validate_time(wall)?;
                ensure!(
                    r.ready && !r.sent && !r.cancelled && now < r.deadline,
                    "relay is not ready or already sent"
                );
                r.send = Some(probe.clone());
                r.sent = true;
            }
        }
        Ok(probe.nonce)
    }
    fn remember(&mut self, probe: &NetworkProbe, now: Instant) -> Result<()> {
        self.seen.retain(|(_, expiry)| now < *expiry);
        ensure!(
            self.seen.len() < HISTORY
                && !self
                    .seen
                    .iter()
                    .any(|(k, _)| *k == (probe.sender, probe.nonce)),
            "duplicate diagnostic nonce or retained quota"
        );
        self.seen
            .push_back(((probe.sender, probe.nonce), now + LEASE));
        Ok(())
    }
    fn receive(&mut self, probe: &NetworkProbe, remote: Hash, now: Instant) -> Result<()> {
        let (local, peers) = self.identities.as_ref().context("diagnostic disabled")?;
        ensure!(
            probe.sender == remote && probe.recipient == *local && peers.contains(&remote),
            "diagnostic wrong sender or receiver"
        );
        self.remember(probe, now)
    }
}
pub(super) fn context(pins: (u64, Hash, Hash)) -> Hash {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(b"NOVONET-BOUNDED-RELAY-DIAGNOSTIC-V1");
    digest.update(super::relay::context(pins));
    digest.finalize().into()
}
fn note(
    state: &Shared,
    event: &'static str,
    session: Option<[u8; 16]>,
    object: Option<Hash>,
    probe: Option<&NetworkProbe>,
    detail: serde_json::Value,
) {
    if let Ok(mut s) = state.try_lock() {
        if let (Some(object), Some(probe), Some((local, _))) =
            (object, probe, s.identities.as_ref())
        {
            let remote = if *local == probe.sender {
                probe.recipient
            } else {
                probe.sender
            };
            crate::native_pipeline::profile::delivery_event(event, 0, remote, object, 0);
        }
        if s.events.len() == HISTORY {
            s.events.pop_front();
            s.evicted += 1;
        }
        s.events
            .push_back(serde_json::json!({"event":event,"wall_ms":wall_ms().ok(),
            "session":session,"object":object,"probe":probe.map(|p|serde_json::json!({
                "nonce":p.nonce,"sender":p.sender,"recipient":p.recipient,
                "issued_at_ms":p.issued_at_ms,"expires_at_ms":p.expires_at_ms})),"detail":detail}));
    }
}
pub(super) fn take_request(state: &Shared) -> Option<Request> {
    let mut s = state.lock().ok()?;
    if s.busy {
        return None;
    }
    let r = s.request.as_mut()?;
    if r.started || r.cancelled || Instant::now() >= r.deadline {
        return None;
    }
    r.started = true;
    Some(r.clone())
}
pub(super) fn stop_requested(state: &Shared) -> bool {
    state.lock().map_or(true, |s| {
        s.request
            .as_ref()
            .is_none_or(|r| r.cancelled || Instant::now() >= r.deadline)
    })
}
pub(super) fn stopped(state: &Shared) {
    note(
        state,
        "relay_diagnostic_cancelled_or_lease_expired",
        None,
        None,
        None,
        serde_json::json!({}),
    );
}
pub(super) struct Occupancy(Shared);
impl Occupancy {
    pub(super) fn acquire(state: &Shared) -> Result<Self> {
        let mut s = state
            .lock()
            .map_err(|_| anyhow::anyhow!("diagnostic state poisoned"))?;
        ensure!(s.enabled() && !s.busy, "diagnostic disabled or occupied");
        s.busy = true;
        Ok(Self(state.clone()))
    }
}
impl Drop for Occupancy {
    fn drop(&mut self) {
        if let Ok(mut s) = self.0.lock() {
            s.busy = false;
            if let Some(r) = &mut s.request {
                r.ready = false;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn outgoing(
    scope: &IrohScopeV1,
    identity: TransportIdentity,
    keys: Vec<Vec<u8>>,
    pins: (u64, Hash, Hash),
    candidates: Vec<Option<Candidate>>,
    budget: HopBudget,
    observer: Option<Arc<NativeTransferDiagnosticsV1>>,
    state: Shared,
    request: Request,
) -> Result<()> {
    let _occupancy = Occupancy::acquire(&state)?;
    let work = async {
        let ids = state
            .lock()
            .map_err(|_| anyhow::anyhow!("diagnostic state poisoned"))?
            .identities
            .clone()
            .context("diagnostic disabled")?
            .1;
        let mut path = Vec::new();
        for id in [request.via, request.probe.recipient] {
            let peer = ids
                .iter()
                .position(|v| *v == id)
                .context("unknown diagnostic peer")?;
            let candidate = candidates
                .get(peer)
                .and_then(Clone::clone)
                .context("no verified diagnostic locator")?;
            ensure!(
                candidate.key == keys[peer] && wall_ms()? < candidate.expires,
                "diagnostic locator stale or mismatched"
            );
            path.push(candidate);
        }
        let route = HopRoute::new(
            identity.public_key().to_vec(),
            path.iter().map(|c| c.key.clone()).collect(),
            context(pins),
        )?;
        note(
            &state,
            "relay_diagnostic_dial",
            None,
            None,
            Some(&request.probe),
            serde_json::json!({"via":request.via,"first_address":path[0].route}),
        );
        let mut hop = tokio::time::timeout(Duration::from_secs(24), async {
            HopStream::connect(
                scope.connect(&path[0].route).await?.into(),
                &identity,
                route,
                Some(budget),
            )
            .await
        })
        .await??;
        if let Some(ref observer) = observer {
            hop.observe(observer.clone(), &identity);
        }
        let proof = hop
            .authenticate_native(&identity, &path[1].key, true)
            .await?;
        let (endpoint, pump) = NativeCarrierPump::attach_relayed_v1(
            &mut hop,
            proof,
            NativeChannel::carrier_streams(),
        )?;
        session(
            endpoint,
            pump,
            pins,
            state.clone(),
            request.probe.recipient,
            Some(request.clone()),
            observer,
        )
        .await
    };
    let result = tokio::time::timeout_at(request.deadline.into(), work)
        .await
        .context("diagnostic setup lease expired")
        .and_then(|r| r);
    if let Err(ref e) = result {
        note(
            &state,
            "relay_diagnostic_failed",
            None,
            None,
            Some(&request.probe),
            serde_json::json!({"error":format!("{e:#}").chars().take(512).collect::<String>()}),
        );
    }
    result
}
pub(super) async fn incoming(
    mut hop: HopStream<'_>,
    identity: &TransportIdentity,
    remote: Hash,
    pins: (u64, Hash, Hash),
    state: Shared,
    observer: Option<Arc<NativeTransferDiagnosticsV1>>,
) -> Result<()> {
    tokio::time::timeout(LEASE, async {
        let origin = hop.route().origin().to_vec();
        hop.confirm().await?;
        let proof = hop.authenticate_native(identity, &origin, false).await?;
        let (endpoint, pump) = NativeCarrierPump::attach_relayed_v1(
            &mut hop,
            proof,
            NativeChannel::carrier_streams(),
        )?;
        session(endpoint, pump, pins, state, remote, None, observer).await
    })
    .await
    .context("diagnostic receive lease expired")?
}
fn send(
    link: &mut NativeObjectLink,
    domain: Hash,
    message: Message,
    now: Instant,
    deadline: Instant,
) -> Result<Hash> {
    let body = Arc::new(OutgoingMessage::with_profile(
        domain,
        transport::encode(&message, LIMITS)?,
        512,
        FragmentProfile::NativeUdpV2,
    )?);
    ensure!(
        link.try_send_until(0, STREAM, &body, now, deadline)?,
        "diagnostic original object queue backpressure"
    );
    Ok(body.id())
}
fn decode_message(bytes: &[u8]) -> Result<Message> {
    let message = transport::decode(bytes, LIMITS)?;
    ensure!(
        matches!(message, Message::NetworkTime(_) | Message::NetworkProbe(_)),
        "business message forbidden on diagnostic relay session"
    );
    Ok(message)
}

#[allow(clippy::too_many_arguments)]
async fn session(
    endpoint: NativeEndpoint,
    mut pump: NativeCarrierPump<'_>,
    pins: (u64, Hash, Hash),
    state: Shared,
    remote: Hash,
    request: Option<Request>,
    observer: Option<Arc<NativeTransferDiagnosticsV1>>,
) -> Result<()> {
    let session_id = endpoint
        .session_id(0)
        .context("diagnostic authenticated session missing")?;
    let local = state
        .lock()
        .map_err(|_| anyhow::anyhow!("diagnostic state poisoned"))?
        .identities
        .as_ref()
        .context("diagnostic disabled")?
        .0;
    let domain = context(pins);
    let mut link = NativeObjectLink::new(
        endpoint,
        vec![NativeLaneConfig {
            stream: STREAM,
            domain,
            receive: ReassemblyLimits {
                max_message_bytes: 512,
                messages: 4,
                bytes: 2048,
                peer_messages: 4,
                peer_bytes: 2048,
                ttl: Duration::from_secs(10),
            },
            send: NativeSendLimits {
                messages: 4,
                bytes: 2048,
                peer_messages: 4,
                peer_bytes: 2048,
            },
        }],
        NativeSendPolicy::reliable_carrier_v1(Duration::from_secs(10)),
    )?;
    if let Some(observer) = observer {
        link.set_diagnostics_v1(observer);
    }
    let mut reference = PeerTimeReferenceV1::default();
    let time_context = coordination::context(domain, local, remote, session_id);
    let reverse = coordination::context(domain, remote, local, session_id);
    let mut flights: Vec<(Hash, bool)> = Vec::new();
    let mut replies = BTreeSet::new();
    let mut completed_replies = 0;
    let session_deadline = Instant::now() + Duration::from_secs(20);
    let mut next_reply = Instant::now();
    let mut sent: Option<(Hash, NetworkProbe)> = None;
    let mut done_at: Option<Instant> = None;
    let mut received = false;
    let mut pending: Option<(Hash, NetworkProbe, Instant, Instant, u64)> = None;
    note(
        &state,
        "relay_diagnostic_authenticated",
        Some(session_id),
        None,
        None,
        serde_json::json!({"remote":remote}),
    );
    loop {
        let clock = ClockAnchorV1::capture()?;
        let now = clock.instant();
        ensure!(
            now < session_deadline,
            "diagnostic ready/send window expired"
        );
        // Keep the SAME pump alive for bounded receiver time-boundary handling.
        // DONE is custody, not a new business acknowledgement protocol.
        if done_at.is_some_and(|at| now >= at + MEASUREMENT_LIFETIME) {
            return Ok(());
        }
        reference.observe(time_context, clock);
        if let Some(ref request) = request {
            let mut s = state
                .lock()
                .map_err(|_| anyhow::anyhow!("diagnostic state poisoned"))?;
            let active = s.request.as_mut().context("diagnostic reservation lost")?;
            ensure!(
                active.probe.nonce == request.probe.nonce && !active.cancelled,
                "diagnostic cancelled"
            );
            // Only the recipient needs the sender's relative time. Sender
            // readiness means the three original replies reached custody;
            // recipient validity is still checked independently, never inferred.
            active.ready = completed_replies == coordination::REQUIRED_SAMPLES;
            if let Some(probe) = active.send.take() {
                probe.validate_time(clock.wall_ms())?;
                let id = send(
                    &mut link,
                    domain,
                    Message::NetworkProbe(probe.clone()),
                    now,
                    now + Duration::from_millis(probe.expires_at_ms - clock.wall_ms()),
                )?;
                drop(s);
                note(
                    &state,
                    "relay_probe_admitted",
                    Some(session_id),
                    Some(id),
                    Some(&probe),
                    serde_json::json!({}),
                );
                sent = Some((id, probe));
            }
        }
        if request.is_none()
            && !received
            && reference.interval(now).is_err()
            && flights.is_empty()
            && reference.due(now)
        {
            let exchange = reference.begin(clock, rand::random())?;
            flights.push((
                send(
                    &mut link,
                    domain,
                    Message::NetworkTime(exchange),
                    now,
                    now + MEASUREMENT_LIFETIME,
                )?,
                false,
            ));
        }
        let polled = link.poll(clock.wall_ms(), now, 8)?;
        for completed in polled.completed {
            if let Some(index) = flights
                .iter()
                .position(|(id, _)| *id == completed.body.id())
            {
                let (_, reply) = flights.remove(index);
                ensure!(
                    completed.state == ObjectSendState::Delivered,
                    "diagnostic time exchange did not reach custody"
                );
                if reply {
                    completed_replies += 1;
                }
            }
            if let Some((id, probe)) = &sent {
                if *id == completed.body.id() {
                    let success = completed.state == ObjectSendState::Delivered;
                    note(
                        &state,
                        if success {
                            "relay_probe_object_done"
                        } else {
                            "relay_probe_terminal_failure"
                        },
                        Some(session_id),
                        Some(*id),
                        Some(probe),
                        serde_json::json!({"state":format!("{:?}",completed.state)}),
                    );
                    ensure!(success, "diagnostic object did not reach DONE");
                    done_at = Some(now);
                }
            }
        }
        if let Some(item) = link.poll_complete(STREAM, now, 8)? {
            let bytes = item.chunks.concat();
            match decode_message(&bytes)? {
                Message::NetworkTime(message) => {
                    if message.response {
                        ensure!(request.is_none(), "unexpected reverse time response");
                        reference.accept(&message, clock)?;
                        if reference.interval(now).is_ok() {
                            note(
                                &state,
                                "relay_time_reference_ready",
                                Some(session_id),
                                None,
                                None,
                                serde_json::json!({"remote":remote,"interval":reference.interval(now)?}),
                            );
                        }
                    } else {
                        ensure!(
                            request.is_some()
                                && now >= next_reply
                                && flights.len() < 2
                                && replies.len() < coordination::REQUIRED_SAMPLES
                                && replies.insert(message.challenge),
                            "diagnostic time reply budget"
                        );
                        let reply = message.reply(reverse, clock)?;
                        flights.push((
                            send(
                                &mut link,
                                domain,
                                Message::NetworkTime(reply),
                                now,
                                now + MEASUREMENT_LIFETIME,
                            )?,
                            true,
                        ));
                        next_reply = now + SAMPLE_INTERVAL;
                    }
                }
                Message::NetworkProbe(probe) => {
                    ensure!(
                        request.is_none() && !received,
                        "diagnostic wrong receiver or duplicate"
                    );
                    received = true;
                    state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("diagnostic state poisoned"))?
                        .receive(&probe, remote, now)?;
                    let validity =
                        reference.validity_window(probe.issued_at_ms, probe.expires_at_ms, now);
                    match validity {
                        Ok((ready, deadline)) => {
                            pending =
                                Some((item.id, probe, ready, deadline, reference.generation()));
                        }
                        Err(error) => {
                            note(
                                &state,
                                "relay_probe_rejected_time",
                                Some(session_id),
                                Some(item.id),
                                Some(&probe),
                                serde_json::json!({"error":error.to_string()}),
                            );
                        }
                    }
                }
                _ => anyhow::bail!("business message forbidden on diagnostic relay session"),
            }
        }
        if pending
            .as_ref()
            .is_some_and(|(_, _, ready, deadline, generation)| {
                now >= *ready || now >= *deadline || *generation != reference.generation()
            })
        {
            let (id, probe, _, deadline, generation) = pending.take().expect("pending diagnostic");
            let valid = now < deadline
                && generation == reference.generation()
                && reference.interval(now).is_ok();
            note(
                &state,
                if valid {
                    "relay_probe_received_integrity_verified"
                } else {
                    "relay_probe_rejected_time"
                },
                Some(session_id),
                Some(id),
                Some(&probe),
                serde_json::json!({"peer_time_interval":reference.interval(now).ok()}),
            );
        }
        tokio::select! {result=pump.progress()=>result?, _=tokio::time::sleep(Duration::from_millis(1))=>{}}
    }
}
