//! Exercise the actual original endpoint, not just the reassembly codec.
//! Full-size signed ML-DSA-65-shape test input proves validation is not bypassed.
//! The deterministic test provider is not production AOEM qualification.
use super::*;
use crate::{
    novorudp::handshake_continuation::{Sender, MAX_TRANSCRIPT_BYTES},
    product_overlay::{handshake_wire, NodeHandshakeInitiatorV1},
};

fn receive(socket: &UdpSocket) -> Result<Vec<u8>> {
    let mut wire = [0; MAX_DATAGRAM_BYTES + 1];
    let (length, _) = socket.recv_from(&mut wire)?;
    Ok(wire[..length].to_vec())
}

fn listener() -> Result<(
    NativeEndpoint,
    UdpSocket,
    TransportIdentity,
    TransportIdentity,
)> {
    let server = UdpSocket::bind("127.0.0.1:0")?;
    let client = UdpSocket::bind("127.0.0.1:0")?;
    client.connect(server.local_addr()?)?;
    client.set_read_timeout(Some(Duration::from_secs(2)))?;
    let local = crate::test_identity::key(14);
    let remote = crate::test_identity::key(19);
    let endpoint = NativeEndpoint::listen_clients(
        server,
        local.clone(),
        4,
        vec![1],
        NativeHandshakeBudget {
            lifetime: Duration::from_secs(5),
            retry: Duration::from_millis(10),
        },
    )?;
    Ok((endpoint, client, local, remote))
}

fn feed(endpoint: &mut NativeEndpoint, socket: &UdpSocket, wire: &[u8]) -> Result<NativePoll> {
    // A real datagram with its real source, the ordinary socket owner polls it.
    socket.send(wire)?;
    endpoint.poll(1000, Instant::now(), 8)
}

fn receive_response(endpoint: &mut NativeEndpoint, socket: &UdpSocket) -> Result<Vec<u8>> {
    let mut receiver = crate::novorudp::handshake_continuation::Receiver::new(Instant::now());
    for _ in 0..32 {
        let wire = receive(socket)?;
        match receiver.receive(endpoint.local_addr()?, &wire, Instant::now())? {
            crate::novorudp::handshake_continuation::Received::Reply(reply) => {
                feed(endpoint, socket, &reply)?;
            }
            crate::novorudp::handshake_continuation::Received::Pending => {
                endpoint.poll(1000, Instant::now(), 8)?;
            }
            crate::novorudp::handshake_continuation::Received::Complete(wire) => return Ok(wire),
            crate::novorudp::handshake_continuation::Received::Challenge => {
                anyhow::bail!("unexpected response continuation challenge")
            }
        }
    }
    anyhow::bail!("response continuation exceeded packet bound")
}

#[test]
fn continuation_cookie_never_admits_peer_but_complete_signed_input_uses_original_handshake(
) -> Result<()> {
    let (mut endpoint, socket, server_key, client_key) = listener()?;
    let offer = NodeHandshakeInitiatorV1::start(
        &client_key,
        peer_id_from_mldsa65_public_key_v1(server_key.public_key()),
        1000,
        5000,
    )?;
    let wire = handshake_wire::encode_offer(offer.offer())?;
    let mut sender = Sender::new(&wire)?;
    let init = sender.datagram(&wire)?;
    let result = feed(&mut endpoint, &socket, &init)?;
    sender.sent();
    assert_eq!(result.handshakes_sent, 1);
    assert!(endpoint.peers.is_empty() && endpoint.sources.is_empty());
    let reply = receive(&socket)?;
    assert_eq!(init.len(), reply.len());
    sender.challenge(&reply)?;
    loop {
        let part = sender.datagram(&wire)?;
        let last = sender.sent();
        let report = feed(&mut endpoint, &socket, &part)?;
        assert_eq!(report.rejected_datagrams, 0);
        assert_eq!(endpoint.peers.len(), usize::from(last));
        if last {
            break;
        }
    }
    let response = receive_response(&mut endpoint, &socket)?;
    let handshake_wire::HandshakeMessage::Response(response) = handshake_wire::decode(&response)?
    else {
        panic!("original endpoint did not sign response")
    };
    offer
        .complete_owned(&response, 1000, &mut HandshakeReplayCacheV1::default())
        .map_err(|_| anyhow::anyhow!("original signature verification failed"))?;
    Ok(())
}

#[test]
fn valid_cookie_cannot_admit_invalid_signature_or_large_unrecognized_transcript() -> Result<()> {
    let (mut endpoint, socket, server_key, client_key) = listener()?;
    let offer = NodeHandshakeInitiatorV1::start(
        &client_key,
        peer_id_from_mldsa65_public_key_v1(server_key.public_key()),
        1000,
        5000,
    )?;
    let mut forged = handshake_wire::encode_offer(offer.offer())?;
    *forged.last_mut().unwrap() ^= 1;
    for wire in [forged, vec![7; MAX_TRANSCRIPT_BYTES]] {
        let mut sender = Sender::new(&wire)?;
        feed(&mut endpoint, &socket, &sender.datagram(&wire)?)?;
        sender.sent();
        sender.challenge(&receive(&socket)?)?;
        loop {
            let part = sender.datagram(&wire)?;
            let last = sender.sent();
            let report = feed(&mut endpoint, &socket, &part)?;
            assert!(endpoint.peers.is_empty() && endpoint.sources.is_empty());
            assert_eq!(report.rejected_datagrams, usize::from(last));
            if last {
                break;
            }
        }
    }
    Ok(())
}

#[test]
fn continuation_obeys_original_wire_budget_and_unsolicited_challenges_do_not_create_peers(
) -> Result<()> {
    let (mut endpoint, socket, _, _) = listener()?;
    let now = Instant::now();
    endpoint.wire_budget = Some(NativeWireBudget::new(2400, now)?);
    let bytes = vec![0; MAX_TRANSCRIPT_BYTES];
    let sender = Sender::new(&bytes)?;
    let init = sender.datagram(&bytes)?;
    feed(&mut endpoint, &socket, &init)?;
    let reply = receive(&socket)?;
    let report = feed(&mut endpoint, &socket, &reply)?;
    assert_eq!(report.rejected_datagrams, 1);
    assert!(endpoint.peers.is_empty());
    let usage = endpoint.wire_budget.as_mut().unwrap().usage(Instant::now());
    assert_eq!(usage.received_bytes, (init.len() + reply.len()) as u64);
    assert_eq!(usage.sent_bytes, reply.len() as u64);
    // Exhaust ingress budget without allocating any transcript buffer.
    for _ in 0..20 {
        feed(&mut endpoint, &socket, &init)?;
    }
    assert!(
        endpoint
            .wire_budget
            .as_mut()
            .unwrap()
            .usage(Instant::now())
            .receive_dropped
            > 0
    );
    assert!(endpoint.peers.is_empty());
    Ok(())
}

#[test]
fn delayed_challenge_from_sent_init_does_not_reject_or_replace_authenticated_session() -> Result<()>
{
    use crate::novorudp::NovoRudpTransportFrameKindV0;

    let (mut node, socket, node_key, client_key) = listener()?;
    let start = Instant::now();
    let mut client = NativeEndpoint::connect_client(
        socket.try_clone()?,
        client_key,
        NativePeer {
            key: node_key.public_key().to_vec(),
            address: node.local_addr()?,
        },
        NativeHandshakeBudget {
            lifetime: Duration::from_secs(5),
            retry: Duration::from_millis(10),
        },
        1000,
        start,
    )?;

    // Withhold both genuine replies while the original sender's retry timer
    // produces a second Init for its unchanged signed transcript.
    assert_eq!(client.poll(1000, start, 64)?.handshakes_sent, 1);
    assert_eq!(node.poll(1000, start, 64)?.handshakes_sent, 1);
    let first_challenge = receive(&socket)?;
    let retry_at = start + Duration::from_millis(10);
    assert_eq!(client.poll(1010, retry_at, 64)?.handshakes_sent, 1);
    assert_eq!(node.poll(1010, retry_at, 64)?.handshakes_sent, 1);
    let delayed_challenge = receive(&socket)?;
    assert!(node.peers.is_empty());
    assert!(client.session_id(0).is_none());

    node.io
        .try_send(&first_challenge, NativeRoute::Udp(client.local_addr()?))?;
    for tick in 0..64 {
        let at = retry_at + Duration::from_micros(tick * 100);
        let left = client.poll(1010, at, 64)?;
        let right = node.poll(1010, at, 64)?;
        assert_eq!(left.rejected_datagrams, 0);
        assert_eq!(right.rejected_datagrams, 0);
        if client.session_id(0).is_some() {
            break;
        }
    }
    let session = client
        .session_id(0)
        .context("continued handshake did not finish")?;
    assert_eq!(node.session_id(0), Some(session));
    assert_eq!(client.peer_count(), 1);
    assert_eq!(node.peer_count(), 1);

    let proof = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        session,
        1,
        1,
        0,
        0,
        vec![7; 4],
    );
    assert!(client.try_send(0, &proof)?);
    let before = node.poll(1010, retry_at + Duration::from_millis(7), 64)?;
    assert_eq!(before.rejected_datagrams, 0);
    assert_eq!(before.frames.len(), 1);

    let mut unsolicited = delayed_challenge.clone();
    unsolicited[9] ^= 1;
    node.io
        .try_send(&unsolicited, NativeRoute::Udp(client.local_addr()?))?;
    let wrong_flight = client.poll(1010, retry_at + Duration::from_micros(7500), 64)?;
    assert_eq!(wrong_flight.rejected_datagrams, 1);
    assert_eq!(client.session_id(0), Some(session));

    node.io
        .try_send(&delayed_challenge, NativeRoute::Udp(client.local_addr()?))?;
    let delayed = client.poll(1010, retry_at + Duration::from_millis(8), 64)?;
    assert_eq!(client.session_id(0), Some(session));
    assert_eq!(node.session_id(0), Some(session));
    assert_eq!(client.peer_count(), 1);
    assert_eq!(node.peer_count(), 1);
    assert!(delayed.frames.is_empty() && delayed.session_changes.is_empty());
    assert_eq!(delayed.handshakes_sent, 0);

    // Verify live progress before testing the rejection-counter expectation:
    // a counted stale challenge alone does not demonstrate a stalled channel.
    assert!(client.try_send(0, &proof)?);
    let after = node.poll(1010, retry_at + Duration::from_millis(9), 64)?;
    assert_eq!(after.rejected_datagrams, 0);
    assert_eq!(after.frames.len(), 1);
    assert_eq!(after.frames[0].frame(), &proof);
    assert_eq!(
        delayed.rejected_datagrams, 0,
        "delayed reply to our sent Init was counted as rejection: {:?}; session and subsequent AEAD delivery remained valid",
        delayed.rejection_reasons
    );
    node.io
        .try_send(&delayed_challenge, NativeRoute::Udp(client.local_addr()?))?;
    let expired = client.poll(6000, start + Duration::from_secs(5), 64)?;
    assert_eq!(expired.rejected_datagrams, 1);
    assert_eq!(client.session_id(0), Some(session));
    Ok(())
}

#[test]
fn configured_election_retains_own_delayed_challenge_after_responder_confirmation() -> Result<()> {
    use crate::novorudp::NovoRudpTransportFrameKindV0;

    let first = crate::test_identity::key(14);
    let second = crate::test_identity::key(19);
    let (low_key, high_key) = if first.peer_id() < second.peer_id() {
        (first, second)
    } else {
        (second, first)
    };
    let low_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let high_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let start = Instant::now();
    let budget = NativeHandshakeBudget {
        lifetime: Duration::from_secs(5),
        retry: Duration::from_millis(10),
    };
    let mut low = NativeEndpoint::bind_shared(
        low_socket.clone(),
        low_key.clone(),
        vec![NativePeer {
            key: high_key.public_key().to_vec(),
            address: high_socket.local_addr()?,
        }],
        budget.clone(),
        1000,
        start,
    )?;
    let mut high = NativeEndpoint::bind_shared(
        high_socket.clone(),
        high_key,
        vec![NativePeer {
            key: low_key.public_key().to_vec(),
            address: low_socket.local_addr()?,
        }],
        budget,
        1000,
        start,
    )?;
    low.enable_configured_reconnect_v1()?;
    high.enable_configured_reconnect_v1()?;

    // Both original owners initiate. Low's immediate cookie reply precedes its
    // own Init; retain the former while delivering the elected lower offer.
    assert_eq!(high.poll(1000, start, 64)?.handshakes_sent, 1);
    let low_start = low.poll(1000, start, 64)?;
    assert_eq!(low_start.rejected_datagrams, 0);
    assert_eq!(low_start.handshakes_sent, 2);
    let delayed_challenge = receive(&high_socket)?;
    let low_init = receive(&high_socket)?;
    assert!(delayed_challenge.starts_with(crate::novorudp::handshake_continuation::MAGIC));
    assert_eq!(delayed_challenge[8], 2);
    assert!(low_init.starts_with(crate::novorudp::handshake_continuation::MAGIC));
    assert_eq!(low_init[8], 1);
    low.io
        .try_send(&low_init, NativeRoute::Udp(high.local_addr()?))?;

    for tick in 1..64 {
        let at = start + Duration::from_micros(tick * 100);
        let right = high.poll(1000, at, 64)?;
        let left = low.poll(1000, at, 64)?;
        assert_eq!(right.rejected_datagrams, 0);
        assert_eq!(left.rejected_datagrams, 0);
        if low.session_id(0).is_some() {
            break;
        }
    }
    let session = low
        .session_id(0)
        .context("elected initiator did not authenticate")?;
    assert!(high.session_id(0).is_none());
    assert!(high.peers[&0].reconnect.is_some());
    let proof = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        session,
        1,
        1,
        0,
        0,
        vec![7; 4],
    );
    assert!(low.try_send(0, &proof)?);
    let confirmed = high.poll(1000, start + Duration::from_millis(7), 64)?;
    assert_eq!(confirmed.rejected_datagrams, 0);
    assert_eq!(confirmed.frames.len(), 1);
    assert_eq!(high.session_id(0), Some(session));
    assert!(high.peers[&0].reconnect.is_none());
    assert!(high.peers[&0]
        .handshake
        .as_ref()
        .unwrap()
        .outgoing_wire_for_test()
        .is_some_and(|wire| matches!(
            handshake_wire::decode(wire),
            Ok(handshake_wire::HandshakeMessage::Response(_))
        )));

    let mut wrong_flight = delayed_challenge.clone();
    wrong_flight[9] ^= 1;
    low.io
        .try_send(&wrong_flight, NativeRoute::Udp(high.local_addr()?))?;
    assert_eq!(
        high.poll(1000, start + Duration::from_micros(7100), 64)?
            .rejected_datagrams,
        1
    );
    let stranger = UdpSocket::bind("127.0.0.1:0")?;
    stranger.send_to(&delayed_challenge, high.local_addr()?)?;
    assert_eq!(
        high.poll(1000, start + Duration::from_micros(7200), 64)?
            .rejected_datagrams,
        1
    );

    low.io
        .try_send(&delayed_challenge, NativeRoute::Udp(high.local_addr()?))?;
    let delayed = high.poll(1000, start + Duration::from_micros(7300), 64)?;
    assert_eq!(high.session_id(0), Some(session));
    assert_eq!(low.session_id(0), Some(session));
    assert_eq!(high.peer_count(), 1);
    assert_eq!(low.peer_count(), 1);
    assert!(delayed.frames.is_empty() && delayed.session_changes.is_empty());
    assert_eq!(delayed.handshakes_sent, 0);

    assert!(low.try_send(0, &proof)?);
    let forward = high.poll(1000, start + Duration::from_micros(7400), 64)?;
    assert_eq!(forward.rejected_datagrams, 0);
    assert_eq!(forward.frames.len(), 1);
    assert!(high.try_send(0, &proof)?);
    let reverse = low.poll(1000, start + Duration::from_micros(7500), 64)?;
    assert_eq!(reverse.rejected_datagrams, 0);
    assert_eq!(reverse.frames.len(), 1);
    assert_eq!(
        delayed.rejected_datagrams, 0,
        "election discarded our sent Init's delayed challenge: {:?}; elected session and bidirectional AEAD delivery remained valid",
        delayed.rejection_reasons
    );
    // The newer responder's lifetime must not extend the retired initiator's
    // original deadline, even though the elected session remains healthy.
    low.io
        .try_send(&delayed_challenge, NativeRoute::Udp(high.local_addr()?))?;
    let expired = high.poll(6000, start + Duration::from_secs(5), 64)?;
    assert_eq!(expired.rejected_datagrams, 1);
    assert_eq!(high.session_id(0), Some(session));
    Ok(())
}

#[test]
fn configured_losing_offer_retry_is_idempotent_before_and_after_elected_session() -> Result<()> {
    use crate::product_overlay::NodeHandshakeResponderV1;

    let first = crate::test_identity::key(14);
    let second = crate::test_identity::key(19);
    let (low_key, high_key) = if first.peer_id() < second.peer_id() {
        (first, second)
    } else {
        (second, first)
    };
    let high_socket = UdpSocket::bind("127.0.0.1:0")?;
    let source = NativeRoute::Udp(high_socket.local_addr()?);
    let start = Instant::now();
    let mut low = NativeEndpoint::bind(
        UdpSocket::bind("127.0.0.1:0")?,
        low_key.clone(),
        vec![NativePeer {
            key: high_key.public_key().to_vec(),
            address: high_socket.local_addr()?,
        }],
        NativeHandshakeBudget {
            lifetime: Duration::from_secs(5),
            retry: Duration::from_millis(10),
        },
        1000,
        start,
    )?;
    low.enable_configured_reconnect_v1()?;
    low.poll(1000, start, 64)?;
    let elected = low.peers[&0]
        .handshake
        .as_ref()
        .unwrap()
        .outgoing_wire_for_test()
        .context("configured initiator did not prepare offer")?
        .to_vec();
    let loser = NodeHandshakeInitiatorV1::start(&high_key, low_key.peer_id(), 1000, 5000)?;
    let loser_wire = handshake_wire::encode_offer(loser.offer())?;
    low.receive_handshake(source, &loser_wire, 1000, start)?;
    let before = low.receive_handshake(source, &loser_wire, 1000, start);
    assert!(low.session_id(0).is_none() && low.peers[&0].reconnect.is_none());
    assert_eq!(
        low.peers[&0]
            .handshake
            .as_ref()
            .unwrap()
            .outgoing_wire_for_test(),
        Some(elected.as_slice())
    );

    // Complete the elected transcript through the original signed responder
    // and endpoint owner; no session or replay cache is injected into Low.
    let handshake_wire::HandshakeMessage::Offer(offer) = handshake_wire::decode(&elected)? else {
        anyhow::bail!("elected transcript was not an offer")
    };
    let responder = NodeHandshakeResponderV1::respond(
        &offer,
        &high_key,
        1000,
        5000,
        &mut HandshakeReplayCacheV1::default(),
    )?;
    let response = handshake_wire::encode_response(responder.response())?;
    low.receive_handshake(source, &response, 1000, start)?;
    let session = low
        .session_id(0)
        .context("elected session was not installed")?;
    let after = low.receive_handshake(source, &loser_wire, 1000, start);
    assert_eq!(low.session_id(0), Some(session));
    assert_eq!(low.peer_count(), 1);
    assert!(low.peers[&0].reconnect.is_none());

    let mut forged = loser_wire.clone();
    *forged.last_mut().unwrap() ^= 1;
    assert!(low.receive_handshake(source, &forged, 1000, start).is_err());
    assert!(low
        .receive_handshake(
            source,
            &loser_wire,
            6001,
            start + Duration::from_millis(5001),
        )
        .is_err());
    assert_eq!(low.session_id(0), Some(session));
    assert!(low.peers[&0].reconnect.is_none());
    assert!(
        before.is_ok() && after.is_ok(),
        "same validated losing offer was not idempotent: before={before:?}, after={after:?}"
    );
    Ok(())
}
