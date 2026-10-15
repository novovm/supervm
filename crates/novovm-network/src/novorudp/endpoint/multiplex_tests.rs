use super::*;
use crate::novorudp::NovoRudpTransportFrameKindV0;

fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_secs(3),
        retry: Duration::from_millis(10),
    }
}

#[test]
fn multiplex_other_protocol_consumes_quantum_without_native_admission() -> Result<()> {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let now = Instant::now();
    let mut endpoint = NativeEndpoint::listen_clients_shared(
        Arc::clone(&socket),
        SigningKey::from_bytes(&[81; 32]),
        1,
        vec![17],
        budget(),
    )?;
    let shared_io: Arc<dyn NativeDatagramIo> = socket.clone();
    assert!(Arc::ptr_eq(&endpoint.io, &shared_io));
    endpoint.set_wire_budget_v1(2400, now)?;
    let mut calls = 0;
    let report = endpoint.poll_with_receiver(1000, now, 3, |_| {
        calls += 1;
        Ok(None)
    })?;
    assert_eq!(calls, 3);
    assert_eq!(report.rejected_datagrams, 0);
    assert_eq!(report.receive_errors, 0);
    assert!(report.frames.is_empty());
    assert_eq!(endpoint.peer_count(), 0);
    assert_eq!(endpoint.wire_usage_v1(now).unwrap().received_bytes, 0);
    assert!(endpoint.set_wire_budget_v1(2400, now).is_err());
    let mut calls = 0;
    endpoint.poll_with_receiver(1000, now, 64, |_| {
        calls += 1;
        Err(ErrorKind::WouldBlock.into())
    })?;
    assert_eq!(calls, 1);
    // Oversize frames are accounted and rejected before any header/slicing.
    let report = endpoint.poll_with_receiver(1000, now, 1, |wire| {
        Ok(Some((wire.len() + 1, socket.local_addr()?)))
    })?;
    assert_eq!(report.rejected_datagrams, 1);
    assert_eq!(endpoint.wire_usage_v1(now).unwrap().received_bytes, 65537);
    assert_eq!(endpoint.peer_count(), 0);
    Ok(())
}

#[test]
fn multiplex_shared_socket_keeps_handshake_source_replay_and_byte_checks() -> Result<()> {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let server_key = SigningKey::from_bytes(&[82; 32]);
    let now = Instant::now();
    let mut server = NativeEndpoint::listen_clients_shared(
        Arc::clone(&socket),
        server_key.clone(),
        1,
        vec![17],
        budget(),
    )?;
    server.set_wire_budget_v1(1_000_000, now)?;
    let mut client = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        SigningKey::from_bytes(&[83; 32]),
        NativePeer {
            key: server_key.verifying_key().to_bytes(),
            address: socket.local_addr()?,
        },
        budget(),
        1000,
        now,
    )?;
    let mut received_bytes = 0;
    while client.session_id(0).is_none() || server.session_id(0).is_none() {
        ensure!(now.elapsed() < Duration::from_secs(3), "handshake timeout");
        client.poll(1000, Instant::now(), 64)?;
        let mut other_protocol = true;
        let report = server.poll_with_receiver(1000, Instant::now(), 64, |wire| {
            if other_protocol {
                other_protocol = false;
                return Ok(None);
            }
            socket.recv_from(wire).map(|(length, source)| {
                received_bytes += length as u64;
                Some((length, source))
            })
        })?;
        assert_eq!(report.rejected_datagrams, 0);
    }
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        client.session_id(0).unwrap(),
        17,
        3,
        4,
        0,
        vec![7; 20],
    );
    let encrypted = client.peers[0].session.as_mut().unwrap().encode(&frame)?;
    // A third socket cannot claim the authenticated peer's source.
    let stranger = UdpSocket::bind("127.0.0.1:0")?;
    stranger.send_to(&encrypted, socket.local_addr()?)?;
    client
        .io
        .try_send(&encrypted, NativeRoute::Udp(socket.local_addr()?))?;
    client
        .io
        .try_send(&encrypted, NativeRoute::Udp(socket.local_addr()?))?;
    let start = Instant::now();
    let mut frames = 0;
    let mut rejected = 0;
    while frames + rejected < 3 {
        ensure!(start.elapsed() < Duration::from_secs(3), "receive timeout");
        let report = server.poll_with_receiver(1000, Instant::now(), 64, |wire| {
            socket.recv_from(wire).map(|(length, source)| {
                received_bytes += length as u64;
                Some((length, source))
            })
        })?;
        for received in report.frames {
            assert_eq!(received.peer(), 0);
            assert_eq!(received.frame().payload, frame.payload);
            frames += 1;
        }
        rejected += report.rejected_datagrams;
    }
    assert_eq!(frames, 1);
    assert_eq!(rejected, 2);
    assert_eq!(server.peer_count(), 1);
    let usage = server.wire_usage_v1(Instant::now()).unwrap();
    assert_eq!(usage.received_bytes, received_bytes);
    assert_eq!(usage.admitted_receive_bytes, received_bytes);
    assert!(
        usage.sent_bytes > 0,
        "handshake sends must retain accounting"
    );
    assert_eq!(usage.used, usage.admitted_receive_bytes + usage.sent_bytes);
    Ok(())
}
