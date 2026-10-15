use super::*;
use crate::novorudp::NovoRudpTransportFrameKindV0;

fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_secs(1),
        retry: Duration::from_millis(10),
    }
}

#[test]
fn open_client_possession_is_bounded_and_cannot_enter_validator_streams() -> Result<()> {
    use crate::product_overlay::{handshake_wire, NodeHandshakeInitiatorV1};
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let address = socket.local_addr()?;
    let key = SigningKey::from_bytes(&[91; 32]);
    let start = Instant::now();
    let absent = UdpSocket::bind("127.0.0.1:0")?;
    let mut node = NativeEndpoint::bind(
        socket,
        key.clone(),
        vec![NativePeer {
            key: SigningKey::from_bytes(&[92; 32]).verifying_key().to_bytes(),
            address: absent.local_addr()?,
        }],
        budget(),
        1000,
        start,
    )?;
    assert!(node
        .allow_clients(SigningKey::from_bytes(&[93; 32]), 1, vec![17], budget())
        .is_err());
    node.allow_clients(key.clone(), 1, vec![17], budget())?;
    let client_key = SigningKey::from_bytes(&[94; 32]);
    let invalid = UdpSocket::bind("127.0.0.1:0")?;
    let offer = NodeHandshakeInitiatorV1::start(
        &client_key,
        peer_id_from_ed25519_public_key_v1(&key.verifying_key().to_bytes()),
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
            key: key.verifying_key().to_bytes(),
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
        &SigningKey::from_bytes(&[95; 32]),
        peer_id_from_ed25519_public_key_v1(&key.verifying_key().to_bytes()),
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
    let a_key = SigningKey::from_bytes(&[23; 32]);
    let b_key = SigningKey::from_bytes(&[47; 32]);
    let now = Instant::now();
    let mut a = NativeEndpoint::bind(
        a_socket,
        a_key.clone(),
        vec![
            NativePeer {
                key: b_key.verifying_key().to_bytes(),
                address: b_address,
            },
            NativePeer {
                key: SigningKey::from_bytes(&[71; 32]).verifying_key().to_bytes(),
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
            key: a_key.verifying_key().to_bytes(),
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
    let key = SigningKey::from_bytes(&[23; 32]);
    let remote = SigningKey::from_bytes(&[47; 32]);
    let peer = NativePeer {
        key: remote.verifying_key().to_bytes(),
        address: "127.0.0.1:12345".parse()?,
    };
    let now = Instant::now();
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
