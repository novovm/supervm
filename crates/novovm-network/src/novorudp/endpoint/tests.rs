use super::*;
use crate::novorudp::NovoRudpTransportFrameKindV0;

fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_secs(1),
        retry: Duration::from_millis(10),
    }
}

fn drive_handshake(
    client: &mut NativeEndpoint,
    node: &mut NativeEndpoint,
    wall_ms: u64,
    now: Instant,
    stop_on_rejection: bool,
) -> Result<NativePoll> {
    for tick in 0..64 {
        let at = now + Duration::from_micros(tick * 100);
        client.poll(wall_ms, at, 64)?;
        let report = node.poll(wall_ms, at, 64)?;
        if (stop_on_rejection && report.rejected_datagrams != 0) || client.session_id(0).is_some() {
            return Ok(report);
        }
    }
    anyhow::bail!("handshake did not finish its bounded continuation flight")
}

#[test]
fn client_reconnect_requires_fresh_possession_and_keeps_old_session_until_confirmed() -> Result<()>
{
    for same_address in [false, true] {
        let start = Instant::now();
        let node_key = crate::test_identity::key(51);
        let client_key = crate::test_identity::key(52);
        let node_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
        let mut node = NativeEndpoint::listen_clients_shared(
            node_socket.clone(),
            node_key.clone(),
            1,
            vec![17],
            budget(),
        )?;
        let remote = NativePeer {
            key: node_key.public_key().to_vec(),
            address: node.local_addr()?,
        };
        let client_socket = UdpSocket::bind("127.0.0.1:0")?;
        let replacement_socket = if same_address {
            client_socket.try_clone()?
        } else {
            UdpSocket::bind("127.0.0.1:0")?
        };
        let mut old = NativeEndpoint::connect_client(
            client_socket,
            client_key.clone(),
            remote.clone(),
            budget(),
            1000,
            start,
        )?;
        let old_offer = old.peers[&0]
            .handshake
            .as_ref()
            .unwrap()
            .outgoing_wire_for_test()
            .unwrap()
            .to_vec();
        while old.session_id(0).is_none() {
            ensure!(
                start.elapsed() < Duration::from_secs(1),
                "initial handshake timeout"
            );
            old.poll(1000, Instant::now(), 64)?;
            node.poll(1000, Instant::now(), 64)?;
        }
        let original = node.session_id(0).unwrap();
        let start = Instant::now();
        let mut fresh = NativeEndpoint::connect_client(
            replacement_socket,
            client_key,
            remote,
            budget(),
            1000,
            Instant::now(),
        )?;
        assert_eq!(
            drive_handshake(&mut fresh, &mut node, 1000, start, true)?.rejected_datagrams,
            1,
            "reconnect default must remain closed"
        );
        node.enable_client_reconnect_v1()?;
        // The rejected default-policy attempt did not consume the transcript.
        drive_handshake(
            &mut fresh,
            &mut node,
            1000,
            start + Duration::from_millis(20),
            false,
        )?;
        assert!(fresh.session_id(0).is_some());
        assert_eq!(
            node.session_id(0),
            Some(original),
            "signed offer alone replaced a live session"
        );
        let frame = |session, stream| {
            NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                session,
                stream,
                3,
                4,
                0,
                vec![9; 10],
            )
        };
        // Old encrypted traffic cannot prove possession of the pending key.
        let old_wire = old
            .peers
            .get_mut(&0)
            .unwrap()
            .session
            .as_mut()
            .unwrap()
            .encode(&frame(original, 17))?;
        fresh
            .io
            .try_send(&old_wire, NativeRoute::Udp(node.local_addr()?))?;
        node.poll(1000, start + Duration::from_millis(40), 64)?;
        assert_eq!(node.session_id(0), Some(original));
        assert!(old.try_send(0, &frame(original, 17))?);
        assert_eq!(
            node.poll(1000, start + Duration::from_millis(41), 64)?
                .frames
                .len(),
            1,
            "old healthy connection stopped before confirmation"
        );
        let new_session = fresh.session_id(0).unwrap();
        assert_ne!(new_session, original);
        fresh.try_send(0, &frame(new_session, 99))?;
        assert_eq!(
            node.poll(1000, start + Duration::from_millis(42), 64)?
                .rejected_datagrams,
            1
        );
        assert_eq!(node.session_id(0), Some(original));
        // Old data in the SAME quantum must not escape after the transition.
        old.try_send(0, &frame(original, 17))?;
        fresh.try_send(0, &frame(new_session, 17))?;
        let report = node.poll(1000, start + Duration::from_millis(43), 64)?;
        assert_eq!(
            report.session_changes,
            vec![NativeSessionChange {
                peer: 0,
                previous: original,
                current: new_session
            }]
        );
        assert_eq!(report.frames.len(), 1);
        assert_eq!(report.frames[0].frame.session_id, new_session);
        assert_eq!(node.peer_count(), 1);
        // Reassembled old transcripts cannot reactivate the prior session;
        // fragmentation itself is exercised by the continuation tests.
        assert!(node
            .receive_handshake(
                NativeRoute::Udp(old.local_addr()?),
                &old_offer,
                1000,
                start + Duration::from_millis(44),
            )
            .is_err());
        old.try_send(0, &frame(original, 17))?;
        let report = node.poll(1000, start + Duration::from_millis(44), 64)?;
        assert_eq!(report.rejected_datagrams, 1);
        assert!(report.frames.is_empty());
        assert_eq!(node.session_id(0), Some(new_session));
    }
    Ok(())
}

#[test]
fn open_client_possession_is_bounded_and_cannot_enter_validator_streams() -> Result<()> {
    use crate::product_overlay::{handshake_wire, NodeHandshakeInitiatorV1};
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let address = socket.local_addr()?;
    let key = crate::test_identity::key(91);
    let start = Instant::now();
    let absent = UdpSocket::bind("127.0.0.1:0")?;
    let mut node = NativeEndpoint::bind(
        socket,
        key.clone(),
        vec![NativePeer {
            key: crate::test_identity::key(92).public_key().to_vec(),
            address: absent.local_addr()?,
        }],
        budget(),
        1000,
        start,
    )?;
    assert!(node
        .allow_clients(crate::test_identity::key(93), 1, vec![17], budget())
        .is_err());
    node.allow_clients(key.clone(), 1, vec![17], budget())?;
    let client_key = crate::test_identity::key(94);
    let invalid = UdpSocket::bind("127.0.0.1:0")?;
    let offer = NodeHandshakeInitiatorV1::start(
        &client_key,
        peer_id_from_mldsa65_public_key_v1(key.public_key()),
        1000,
        1000,
    )?;
    let mut wire = handshake_wire::encode_offer(offer.offer())?;
    *wire.last_mut().unwrap() ^= 1;
    invalid.send_to(&wire, address)?;
    assert_eq!(node.poll(1000, start, 64)?.rejected_datagrams, 1);
    assert_eq!(node.peer_count(), 1, "invalid offer consumed a slot");
    let mut client = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        client_key,
        NativePeer {
            key: key.public_key().to_vec(),
            address,
        },
        budget(),
        1000,
        start,
    )?;
    while client.session_id(0).is_none() {
        ensure!(
            start.elapsed() < Duration::from_secs(1),
            "client handshake timeout"
        );
        let now = Instant::now();
        let wall = 1000 + start.elapsed().as_millis() as u64;
        client.poll(wall, now, 64)?;
        node.poll(wall, now, 64)?;
    }
    assert_eq!(node.peer_count(), 2);
    // A second correctly signed identity is refused at the resource bound.
    let other = NodeHandshakeInitiatorV1::start(
        &crate::test_identity::key(95),
        peer_id_from_mldsa65_public_key_v1(key.public_key()),
        1000,
        1000,
    )?;
    invalid.send_to(&handshake_wire::encode_offer(other.offer())?, address)?;
    node.poll(1000, start, 64)?;
    assert_eq!(node.peer_count(), 2);
    // Session stays alive after the discovery/handshake lifetime. Forbidden
    // streams never reach reassembly or the consensus consumer.
    let later = start + Duration::from_secs(2);
    for (stream, accepted) in [(99, false), (17, true)] {
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            client.session_id(0).unwrap(),
            stream,
            3,
            4,
            0,
            vec![9; 100],
        );
        assert!(client.try_send(0, &frame)?);
        let report = node.poll(3000, later, 64)?;
        assert_eq!(report.frames.len(), usize::from(accepted));
        if accepted {
            assert_eq!(report.frames[0].frame(), &frame);
        } else {
            assert_eq!(report.rejected_datagrams, 1);
        }
    }
    Ok(())
}

#[test]
fn original_socket_owner_keeps_healthy_peer_live_with_absent_peer_and_invalid_datagrams(
) -> Result<()> {
    let a_socket = UdpSocket::bind("127.0.0.1:0")?;
    let b_socket = UdpSocket::bind("127.0.0.1:0")?;
    let absent = UdpSocket::bind("127.0.0.1:0")?;
    let unknown = UdpSocket::bind("127.0.0.1:0")?;
    let a_address = a_socket.local_addr()?;
    let b_address = b_socket.local_addr()?;
    let a_key = crate::test_identity::key(23);
    let b_key = crate::test_identity::key(47);
    let now = Instant::now();
    let mut a = NativeEndpoint::bind(
        a_socket,
        a_key.clone(),
        vec![
            NativePeer {
                key: b_key.public_key().to_vec(),
                address: b_address,
            },
            NativePeer {
                key: crate::test_identity::key(71).public_key().to_vec(),
                address: absent.local_addr()?,
            },
        ],
        budget(),
        1000,
        now,
    )?;
    let mut b = NativeEndpoint::bind(
        b_socket,
        b_key,
        vec![NativePeer {
            key: a_key.public_key().to_vec(),
            address: a_address,
        }],
        budget(),
        1000,
        now,
    )?;
    while a.session_id(0).is_none() || b.session_id(0).is_none() {
        ensure!(
            now.elapsed() < Duration::from_secs(1),
            "endpoint handshake timed out"
        );
        let wall = 1000 + now.elapsed().as_millis() as u64;
        a.poll(wall, Instant::now(), 1)?;
        b.poll(wall, Instant::now(), 1)?;
        std::thread::yield_now();
    }
    assert!(a.session_id(1).is_none());
    let later = now + Duration::from_secs(2);
    a.poll(3000, later, 1)?;
    b.poll(3000, later, 1)?;
    assert_eq!(a.expired_handshakes(later), 1);
    unknown.send_to(&[7; 1201], b_address)?;
    unknown.send_to(&[7; 4096], b_address)?;
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        a.session_id(0).unwrap(),
        2,
        3,
        4,
        0,
        vec![9; 100],
    );
    assert!(a.try_send(0, &frame)?);
    let mut rejected = 0;
    let mut received = None;
    let deadline = Instant::now() + Duration::from_secs(1);
    while received.is_none() {
        ensure!(Instant::now() < deadline, "healthy data starved");
        let poll = b.poll(3000, later, 1)?;
        assert!(poll.frames.len() <= 1);
        rejected += poll.rejected_datagrams;
        received = poll.frames.into_iter().next();
    }
    assert!(rejected >= 2);
    let received = received.unwrap();
    assert_eq!(received.peer(), 0);
    assert_eq!(received.peer_id(), b.peer_id(0)?);
    assert_eq!(received.frame(), &frame);
    assert!(a.try_send(1, &frame).is_err());
    Ok(())
}

#[test]
fn duplicate_identity_or_source_and_invalid_quantum_are_rejected() -> Result<()> {
    let key = crate::test_identity::key(23);
    let remote = crate::test_identity::key(47);
    let peer = NativePeer {
        key: remote.public_key().to_vec(),
        address: "127.0.0.1:12345".parse()?,
    };
    let now = Instant::now();
    assert!(
        NativeEndpoint::connect_client(
            UdpSocket::bind("127.0.0.1:0")?,
            key.clone(),
            NativePeer {
                key: vec![47; 32],
                address: peer.address,
            },
            budget(),
            1000,
            now,
        )
        .is_err(),
        "legacy 32-byte identity must not enter the ML-DSA endpoint"
    );
    assert!(NativeEndpoint::bind(
        UdpSocket::bind("127.0.0.1:0")?,
        key.clone(),
        vec![peer.clone(), peer.clone()],
        budget(),
        1000,
        now
    )
    .is_err());
    let mut endpoint = NativeEndpoint::bind(
        UdpSocket::bind("127.0.0.1:0")?,
        key,
        vec![peer],
        budget(),
        1000,
        now,
    )?;
    assert!(endpoint.poll(1000, now, 0).is_err());
    assert!(endpoint.poll(1000, now, 65).is_err());
    Ok(())
}

#[test]
fn node_wire_budget_counts_handshake_and_rejects_ingress_before_identity_admission() -> Result<()> {
    let now = Instant::now();
    let key = crate::test_identity::key(81);
    let mut node = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        key.clone(),
        2,
        vec![17],
        budget(),
    )?;
    node.set_wire_budget_v1(32 * 1024, now)?;
    assert!(node.set_wire_budget_v1(32 * 1024, now).is_err());
    let mut client = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        crate::test_identity::key(82),
        NativePeer {
            key: key.public_key().to_vec(),
            address: node.local_addr()?,
        },
        budget(),
        1000,
        now,
    )?;
    client.set_wire_budget_v1(32 * 1024, now)?;
    client.poll(1000, now, 64)?;
    let sent = client.wire_usage_v1(now).unwrap().sent_bytes;
    assert!(sent > 0);
    node.poll(1000, now, 64)?;
    let usage = node.wire_usage_v1(now).unwrap();
    assert_eq!(usage.received_bytes, sent);
    assert_eq!(usage.admitted_receive_bytes, sent);
    assert!(usage.sent_bytes > 0);
    drive_handshake(&mut client, &mut node, 1000, now, false)?;
    assert!(client.session_id(0).is_some());
    let sent = client.wire_usage_v1(now).unwrap().sent_bytes;
    assert_eq!(node.wire_usage_v1(now).unwrap().received_bytes, sent);
    let session = node.session_id(0);
    let remaining = node.wire_usage_v1(now).unwrap().remaining as usize;
    let attacker = UdpSocket::bind("127.0.0.1:0")?;
    // Invalid bytes still consume admission work; excess is never parsed into
    // a new peer. Full datagram length (not a truncated 1201-byte prefix) counts.
    attacker.send_to(&vec![7; remaining], node.local_addr()?)?;
    node.poll(1000, now, 64)?;
    attacker.send_to(&[9; 4096], node.local_addr()?)?;
    assert_eq!(node.poll(1000, now, 64)?.rejected_datagrams, 1);
    let usage = node.wire_usage_v1(now).unwrap();
    assert_eq!(usage.remaining, 0);
    assert_eq!(usage.receive_dropped, 1);
    assert_eq!(usage.received_bytes, sent + remaining as u64 + 4096);
    assert_eq!(node.peer_count(), 1);
    assert_eq!(node.session_id(0), session);
    Ok(())
}

#[test]
fn node_wire_budget_blocks_data_and_object_cursor_but_keeps_authenticated_session() -> Result<()> {
    use crate::novorudp::{
        fragments::{FragmentProfile, OutgoingMessage},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    };
    let now = Instant::now();
    let key = crate::test_identity::key(83);
    let mut node = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        key.clone(),
        1,
        vec![17],
        budget(),
    )?;
    node.set_wire_budget_v1(32 * 1024, now)?;
    let mut client = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        crate::test_identity::key(84),
        NativePeer {
            key: key.public_key().to_vec(),
            address: node.local_addr()?,
        },
        budget(),
        1000,
        now,
    )?;
    drive_handshake(&mut client, &mut node, 1000, now, false)?;
    let session = node.session_id(0).unwrap();
    let remaining = node.wire_usage_v1(now).unwrap().remaining as usize;
    assert!(node
        .wire_budget
        .as_mut()
        .unwrap()
        .admit_receive(remaining, now));
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Ack,
        session,
        17,
        1,
        0,
        0,
        vec![],
    );
    assert!(!node.try_send(0, &frame)?);
    let body = Arc::new(OutgoingMessage::with_profile(
        [5; 32],
        vec![7; 10],
        1024,
        FragmentProfile::NativeUdpV2,
    )?);
    let mut sender = NativeObjectSender::new(
        body,
        session,
        17,
        NovoRudpWindowConfig::default(),
        NovoRudpPacingProfile::default(),
        now,
        Duration::from_secs(120),
    )?;
    assert_eq!(node.poll_object(0, &mut sender, now, 1)?, 0);
    assert_eq!(node.session_id(0), Some(session));
    assert_eq!(node.wire_usage_v1(now).unwrap().send_deferred, 2);
    // No sleeping: advance the monotonic accounting window. The original first
    // object fragment is still pending, rather than a fabricated transport ACK.
    let later = now + Duration::from_secs(61);
    assert_eq!(node.poll_object(0, &mut sender, later, 1)?, 1);
    let poll = client.poll(1000, later, 64)?;
    assert_eq!(poll.frames.len(), 1);
    assert_eq!(
        poll.frames[0].frame().kind,
        NovoRudpTransportFrameKindV0::Data
    );
    assert_eq!(poll.frames[0].frame().sequence, 0);
    Ok(())
}
