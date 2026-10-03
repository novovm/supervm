//! Single-owner forwarding: admission retains originals, I/O never waits for
//! one frame's outcome before allowing another independently correlated frame.
use super::*;
use crate::duplex::product_relay::RelayForwardOutcomeV1;
use crate::duplex::product_relay_client::pipeline::{PipelineProgress, ProductRelayPipelineV1};

enum Flight {
    Data {
        peer: String,
        entry: u64,
        reservation: u64,
        session: [u8; 16],
    },
    Handshake {
        peer: String,
        session: [u8; 16],
        response: bool,
    },
}

impl Flight {
    fn peer(&self) -> &str {
        match self {
            Self::Data { peer, .. } | Self::Handshake { peer, .. } => peer,
        }
    }
}

type Flights = BTreeMap<u64, Flight>;

#[cfg(test)]
mod tests;

pub(super) fn run_session(
    relay: &mut ProductRelayPipelineV1,
    config: &NetworkWorkerConfig,
    identity: &SigningKey,
    peers: &mut BTreeMap<String, Peer>,
    shared: &Mutex<Shared>,
    stop: &AtomicBool,
) -> Result<()> {
    let local = peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes());
    let mut preauth = Queues::new(&config.peers, config.limits.preauth.clone());
    let mut flights = Flights::new();
    let mut reservation = 0u64;
    let mut handshake_turn = 0;
    let mut heartbeat_at = Instant::now();
    let ttl = Duration::from_millis(config.queue_ttl_ms);
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        {
            let mut shared = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
            let expired = shared.outbound.expire(now, ttl);
            shared.status.outbound_expired = shared.status.outbound_expired.saturating_add(expired);
            let dropped = shared.inbound.expire(now, ttl)
                + preauth.expire(now, Duration::from_millis(config.handshake_timeout_ms));
            shared.status.inbound_dropped = shared.status.inbound_dropped.saturating_add(dropped);
        }
        for (id, peer) in peers.iter_mut() {
            let timed_out = matches!(&peer.phase,
                Phase::Handshaking { deadline, .. } | Phase::Responding { deadline, .. }
                    if *deadline <= now);
            let retired = peer.retiring && !flights.values().any(|flight| flight.peer() == id);
            if timed_out || retired {
                isolate(peer, config);
                let dropped = preauth.clear_peer(id);
                let mut shared = shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
                shared.status.inbound_dropped =
                    shared.status.inbound_dropped.saturating_add(dropped);
            }
        }

        // Each poll has bounded read AND write progress. No send loop may fill
        // the socket and then depend on an unbounded opposite-direction cache.
        let mut progressed = match relay.poll()? {
            PipelineProgress::Event(event) => {
                handle_event(
                    *event,
                    &local,
                    config,
                    identity,
                    peers,
                    shared,
                    &mut preauth,
                )?;
                true
            }
            PipelineProgress::Forward { ticket, outcome } => {
                settle_forward(ticket, outcome, &mut flights, config, peers, shared)?;
                true
            }
            PipelineProgress::Progress => true,
            PipelineProgress::Idle => false,
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        if let Some(id) = preauth.next_peer(|id| {
            peers
                .get(id)
                .is_some_and(|peer| matches!(peer.phase, Phase::Active(_)))
        }) {
            let delivery = preauth.pop(&id).expect("selected preauth queue");
            receive_delivery(delivery, config, peers, shared, &mut preauth)?;
            progressed = true;
        }
        if heartbeat_at.elapsed() >= Duration::from_millis(config.heartbeat_interval_ms)
            && relay.try_heartbeat()?
        {
            heartbeat_at = Instant::now();
            progressed = true;
        }
        progressed |= submit_handshake(
            relay,
            config,
            identity,
            peers,
            &mut flights,
            &mut handshake_turn,
        )?;
        progressed |= submit_payload(relay, config, peers, shared, &mut flights, &mut reservation)?;
        publish_active(shared, peers)?;
        {
            let mut shared = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
            let pending = flights
                .values()
                .filter(|flight| matches!(flight, Flight::Data { .. }))
                .count();
            shared.status.pending_forwards = pending;
            shared.status.peak_pending_forwards = shared.status.peak_pending_forwards.max(pending);
        }
        if !progressed {
            // Close admission-before-wait races without spinning when the
            // transport window is full or every retained original is in flight.
            let waker = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("network queue poisoned"))?
                .runnable_outbound_waker(peers, Instant::now(), ttl, relay.can_submit());
            if let Some(waker) = waker {
                waker.wake();
            }
            relay.wait()?;
        }
    }
    Ok(())
}

fn submit_payload(
    relay: &mut ProductRelayPipelineV1,
    config: &NetworkWorkerConfig,
    peers: &mut BTreeMap<String, Peer>,
    shared: &Mutex<Shared>,
    flights: &mut Flights,
    reservation: &mut u64,
) -> Result<bool> {
    if !relay.can_submit() {
        return Ok(false);
    }
    let tag = reservation
        .checked_add(1)
        .context("network reservation exhausted")?;
    let selected = {
        let mut shared = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
        let expired = shared
            .outbound
            .expire(Instant::now(), Duration::from_millis(config.queue_ttl_ms));
        shared.status.outbound_expired = shared.status.outbound_expired.saturating_add(expired);
        shared
            .outbound
            .reserve_next(|id| peers.get(id).is_some_and(Peer::can_send), tag)
    };
    let Some((id, entry, bytes)) = selected else {
        return Ok(false);
    };
    *reservation = tag;
    let peer = peers.get_mut(&id).expect("configured outgoing peer");
    let Phase::Active(channel) = &mut peer.phase else {
        unreachable!("reserved active peer")
    };
    let session = channel.session_id();
    // Both inner frame sequence and the channel's AEAD nonce are consumed at
    // sealing, never at ACK. Rejection or a partial write cannot roll them back.
    let sequence = peer.frame_sequence;
    let envelope = (|| {
        peer.frame_sequence = sequence
            .checked_add(1)
            .context("network frame sequence exhausted")?;
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            session,
            config.chain_id,
            FRAME_DOMAIN,
            sequence,
            0,
            bytes,
        );
        Ok::<_, anyhow::Error>(channel.seal_novorudp_frame(&frame)?)
    })();
    let envelope = match envelope {
        Ok(envelope) => envelope,
        Err(error) => {
            shared
                .lock()
                .map_err(|_| anyhow::anyhow!("network queue poisoned"))?
                .outbound
                .settle_exact(&id, entry, tag, false)?;
            record_error(shared, &error);
            peer.retiring = true;
            return Ok(true);
        }
    };
    let Some(ticket) = relay.try_submit_envelope(envelope)? else {
        shared
            .lock()
            .map_err(|_| anyhow::anyhow!("network queue poisoned"))?
            .outbound
            .settle_exact(&id, entry, tag, false)?;
        return Ok(false);
    };
    if flights
        .insert(
            ticket,
            Flight::Data {
                peer: id,
                entry,
                reservation: tag,
                session,
            },
        )
        .is_some()
    {
        bail!("network transport reused a pending ticket");
    }
    Ok(true)
}

fn submit_handshake(
    relay: &mut ProductRelayPipelineV1,
    config: &NetworkWorkerConfig,
    identity: &SigningKey,
    peers: &mut BTreeMap<String, Peer>,
    flights: &mut Flights,
    turn: &mut usize,
) -> Result<bool> {
    if !relay.can_submit_handshake() {
        return Ok(false);
    }
    for offset in 0..config.peers.len() {
        let index = (*turn + offset) % config.peers.len();
        let id = &config.peers[index];
        // Old-generation data still has independently meaningful outcomes.
        // Settle them before sending this peer's replacement handshake.
        if flights.values().any(|flight| flight.peer() == id) {
            continue;
        }
        let peer = peers.get_mut(id).expect("configured handshake peer");
        let due = matches!(peer.phase, Phase::Idle | Phase::Responding { .. })
            || matches!(peer.phase, Phase::Cooldown(at) if at <= Instant::now());
        if !due {
            continue;
        }
        let (message, next, response) = match &peer.phase {
            Phase::Responding { response, .. } => (
                RelayPeerHandshakeV1::Response((**response).clone()),
                None,
                true,
            ),
            _ => {
                let initiator = NodeHandshakeInitiatorV1::start(
                    identity,
                    id,
                    now_ms(),
                    config.handshake_timeout_ms,
                )?;
                let message = RelayPeerHandshakeV1::Offer(initiator.offer().clone());
                (
                    message,
                    Some(Phase::Handshaking {
                        initiator,
                        deadline: Instant::now()
                            + Duration::from_millis(config.handshake_timeout_ms),
                    }),
                    false,
                )
            }
        };
        let session = match &message {
            RelayPeerHandshakeV1::Offer(offer) => offer.session_id,
            RelayPeerHandshakeV1::Response(response) => response.session_id,
        };
        let Some(ticket) = relay.try_submit_handshake(id.clone(), message)? else {
            return Ok(false);
        };
        if let Some(next) = next {
            peer.phase = next;
            peer.frame_sequence = 0;
            peer.retiring = false;
        }
        if flights
            .insert(
                ticket,
                Flight::Handshake {
                    peer: id.clone(),
                    session,
                    response,
                },
            )
            .is_some()
        {
            bail!("network transport reused a pending handshake ticket");
        }
        *turn = (index + 1) % config.peers.len();
        return Ok(true);
    }
    Ok(false)
}

fn settle_forward(
    ticket: u64,
    outcome: RelayForwardOutcomeV1,
    flights: &mut Flights,
    config: &NetworkWorkerConfig,
    peers: &mut BTreeMap<String, Peer>,
    shared: &Mutex<Shared>,
) -> Result<()> {
    // The transport checked the full wire correlation and flags before handing
    // this result over. The owner still requires its exact live reservation.
    let flight = flights
        .remove(&ticket)
        .context("network outcome has no live reservation")?;
    let accepted = admitted(outcome.disposition);
    match flight {
        Flight::Data {
            peer: id,
            entry,
            reservation,
            session,
        } => {
            {
                let mut shared = shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
                shared
                    .outbound
                    .settle_exact(&id, entry, reservation, accepted)?;
                if accepted {
                    shared.status.relay_admissions =
                        shared.status.relay_admissions.saturating_add(1);
                }
            }
            let peer = peers.get_mut(&id).context("outcome peer disappeared")?;
            if !accepted && peer.expected_session(session) {
                // Other frames from this generation may already be accepted.
                // Keep their exact reservations until their own results arrive.
                peer.retiring = true;
            }
        }
        Flight::Handshake {
            peer: id,
            session,
            response,
        } => {
            let peer = peers
                .get_mut(&id)
                .context("handshake outcome peer disappeared")?;
            if peer.expected_session(session) {
                if !accepted {
                    if matches!(peer.phase, Phase::Active(_)) {
                        peer.retiring = true;
                    } else {
                        isolate(peer, config);
                    }
                } else if response && matches!(peer.phase, Phase::Responding { .. }) {
                    let Phase::Responding { channel, .. } =
                        std::mem::replace(&mut peer.phase, Phase::Idle)
                    else {
                        unreachable!()
                    };
                    peer.phase = Phase::Active(channel);
                }
            }
        }
    }
    if !accepted {
        record_error(
            shared,
            &anyhow::anyhow!("relay admission rejected: {:?}", outcome.disposition),
        );
    }
    Ok(())
}

fn handle_event(
    event: ProductRelayClientEventV1,
    local: &str,
    config: &NetworkWorkerConfig,
    identity: &SigningKey,
    peers: &mut BTreeMap<String, Peer>,
    shared: &Mutex<Shared>,
    preauth: &mut Queues<OpaqueRelayDeliveryV1>,
) -> Result<()> {
    match event {
        ProductRelayClientEventV1::Delivery(delivery) => {
            receive_delivery(delivery, config, peers, shared, preauth)?
        }
        ProductRelayClientEventV1::PeerHandshake(delivery) => {
            if delivery.target_peer_id != local {
                return Ok(());
            }
            let Some(peer) = peers.get_mut(&delivery.source_peer_id) else {
                return Ok(());
            };
            match delivery.handshake {
                RelayPeerHandshakeV1::Offer(offer) => {
                    if offer.initiator_peer_id != delivery.source_peer_id
                        || offer.responder_peer_id != local
                    {
                        return Ok(());
                    }
                    if matches!(peer.phase, Phase::Handshaking { .. })
                        && local < delivery.source_peer_id.as_str()
                    {
                        return Ok(());
                    }
                    match NodeHandshakeResponderV1::respond(
                        &offer,
                        identity,
                        now_ms(),
                        config.handshake_timeout_ms,
                        &mut peer.replay,
                    ) {
                        Ok(responder) => {
                            peer.phase = Phase::Responding {
                                response: Box::new(responder.response().clone()),
                                channel: responder.into_channel(),
                                deadline: Instant::now()
                                    + Duration::from_millis(config.handshake_timeout_ms),
                            };
                            peer.frame_sequence = 0;
                            peer.retiring = false;
                            let dropped = preauth.clear_peer(&delivery.source_peer_id);
                            let mut shared = shared
                                .lock()
                                .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
                            shared.status.inbound_dropped =
                                shared.status.inbound_dropped.saturating_add(dropped);
                        }
                        Err(error) => record_error(shared, &error.into()),
                    }
                }
                RelayPeerHandshakeV1::Response(response) => {
                    if !peer.expected_session(response.session_id)
                        || response.responder_peer_id != delivery.source_peer_id
                        || !matches!(peer.phase, Phase::Handshaking { .. })
                    {
                        return Ok(());
                    }
                    let Phase::Handshaking { initiator, .. } =
                        std::mem::replace(&mut peer.phase, Phase::Idle)
                    else {
                        unreachable!()
                    };
                    match initiator.complete(&response, now_ms(), &mut peer.replay) {
                        Ok(channel) => {
                            peer.phase = Phase::Active(channel);
                            peer.frame_sequence = 0;
                            peer.retiring = false;
                        }
                        Err(error) => {
                            record_error(shared, &error.into());
                            isolate(peer, config);
                        }
                    }
                }
            }
        }
        ProductRelayClientEventV1::HeartbeatAck => {}
        ProductRelayClientEventV1::Closed => bail!("relay closed network session"),
    }
    Ok(())
}
