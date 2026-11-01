//! Exercise the actual original endpoint, not just the reassembly codec.
//! Existing signed V1 input proves validation is not bypassed. Large opaque
//! input proves bounds, NOT production ML-DSA identity migration.
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

fn listener() -> Result<(NativeEndpoint, UdpSocket, SigningKey, SigningKey)> {
    let server = UdpSocket::bind("127.0.0.1:0")?;
    let client = UdpSocket::bind("127.0.0.1:0")?;
    client.connect(server.local_addr()?)?;
    client.set_read_timeout(Some(Duration::from_secs(2)))?;
    let local = SigningKey::from_bytes(&[14; 32]);
    let remote = SigningKey::from_bytes(&[19; 32]);
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

#[test]
fn continuation_cookie_never_admits_peer_but_complete_signed_input_uses_original_handshake(
) -> Result<()> {
    let (mut endpoint, socket, server_key, client_key) = listener()?;
    let offer = NodeHandshakeInitiatorV1::start(
        &client_key,
        peer_id_from_ed25519_public_key_v1(&server_key.verifying_key().to_bytes()),
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
    let report = feed(&mut endpoint, &socket, &sender.datagram(&wire)?)?;
    assert_eq!(report.rejected_datagrams, 0);
    assert_eq!(endpoint.peers.len(), 1);
    let response = receive(&socket)?;
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
        peer_id_from_ed25519_public_key_v1(&server_key.verifying_key().to_bytes()),
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
