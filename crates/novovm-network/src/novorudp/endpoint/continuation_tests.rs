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
