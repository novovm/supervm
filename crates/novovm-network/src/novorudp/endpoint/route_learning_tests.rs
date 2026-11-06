//! Actual original socket/handshake/AEAD owner. No probe protocol or new peer
//! registry; a guessed route is deliberately different from the sender socket.
use super::*;
use crate::novorudp::NovoRudpTransportFrameKindV0;

fn key(seed: u8) -> TransportIdentity {
    crate::test_identity::key(seed)
}
fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_secs(1),
        retry: Duration::from_millis(10),
    }
}
fn peer(identity: &TransportIdentity, address: SocketAddr) -> NativePeer {
    NativePeer {
        key: identity.public_key().to_vec(),
        address,
    }
}
fn configured(
    socket: Arc<UdpSocket>,
    identity: TransportIdentity,
    peers: Vec<NativePeer>,
    learn: bool,
    now: Instant,
) -> Result<NativeEndpoint> {
    let mut endpoint = NativeEndpoint::bind_shared(socket, identity, peers, budget(), 1000, now)?;
    endpoint.enable_configured_reconnect_v1()?;
    if learn {
        endpoint.enable_configured_route_learning_v1()?;
    }
    Ok(endpoint)
}
fn proof(session: [u8; 16]) -> NovoRudpTransportFrameV0 {
    NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        session,
        2,
        1,
        0,
        0,
        vec![7; 4],
    )
}
fn finish_offer(
    client: &mut NativeEndpoint,
    node: &mut NativeEndpoint,
    wall: u64,
    now: Instant,
) -> Result<[u8; 16]> {
    for tick in 0..100 {
        let at = now + Duration::from_micros(tick * 100);
        client.poll(wall, at, 64)?;
        node.poll(wall, at, 64)?;
        if let Some(session) = client.session_id(0) {
            return Ok(session);
        }
    }
    anyhow::bail!("continued original handshake did not reach initiating socket")
}
fn exchange(a: &mut NativeEndpoint, b: &mut NativeEndpoint, now: Instant) -> Result<()> {
    for tick in 0..100 {
        let at = now + Duration::from_millis(tick * 5);
        a.poll(1000 + tick * 5, at, 64)?;
        b.poll(1000 + tick * 5, at, 64)?;
        if a.session_id(0).is_some() && a.session_id(0) == b.session_id(0) {
            return Ok(());
        }
        if let Some(session) = a.session_id(0) {
            a.try_send(0, &proof(session))?;
        }
        if let Some(session) = b.session_id(0) {
            b.try_send(0, &proof(session))?;
        }
    }
    anyhow::bail!("configured peers did not confirm their shared generation")
}
fn offer(endpoint: &NativeEndpoint) -> Vec<u8> {
    endpoint.peers[&0]
        .handshake
        .as_ref()
        .unwrap()
        .outgoing_wire_for_test()
        .unwrap()
        .to_vec()
}

#[test]
fn route_learning_requires_explicit_configured_recovery_before_traffic() -> Result<()> {
    let now = Instant::now();
    let absent = UdpSocket::bind("127.0.0.1:0")?;
    let mut node = NativeEndpoint::bind(
        UdpSocket::bind("127.0.0.1:0")?,
        key(61),
        vec![peer(&key(62), absent.local_addr()?)],
        budget(),
        1000,
        now,
    )?;
    assert!(node.enable_configured_route_learning_v1().is_err());
    node.enable_configured_reconnect_v1()?;
    node.enable_configured_route_learning_v1()?;
    assert!(node.enable_configured_route_learning_v1().is_err());
    let mut late = configured(
        Arc::new(UdpSocket::bind("127.0.0.1:0")?),
        key(61),
        vec![peer(&key(62), absent.local_addr()?)],
        false,
        now,
    )?;
    late.poll(1000, now, 1)?;
    assert!(late.enable_configured_route_learning_v1().is_err());
    let mut open = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        key(61),
        1,
        vec![2],
        budget(),
    )?;
    open.enable_client_reconnect_v1()?;
    assert!(open.enable_configured_route_learning_v1().is_err());
    Ok(())
}

#[test]
fn route_learning_confirms_initial_actual_socket_in_both_identity_orders() -> Result<()> {
    for reverse in [false, true] {
        let now = Instant::now();
        let (client_key, node_key) = if reverse {
            (key(62), key(61))
        } else {
            (key(61), key(62))
        };
        let guessed = UdpSocket::bind("127.0.0.1:0")?;
        let node_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
        let mut node = configured(
            node_socket.clone(),
            node_key.clone(),
            vec![peer(&client_key, guessed.local_addr()?)],
            true,
            now,
        )?;
        let mut client = NativeEndpoint::connect_client(
            UdpSocket::bind("127.0.0.1:0")?,
            client_key.clone(),
            peer(&node_key, node_socket.local_addr()?),
            budget(),
            1000,
            now,
        )?;
        let actual = NativeRoute::Udp(client.local_addr()?);
        let session = finish_offer(&mut client, &mut node, 1000, now)?;
        assert!(node.session_id(0).is_none());
        assert_eq!(node.sources[&NativeRoute::Udp(guessed.local_addr()?)], 0);
        assert!(!node.sources.contains_key(&actual));
        assert!(node.peers[&0].reconnect.is_some());
        client.try_send(0, &proof(session))?;
        let confirmed = node.poll(1011, now + Duration::from_millis(11), 64)?;
        assert_eq!(confirmed.frames.len(), 1);
        assert!(confirmed.session_changes.is_empty());
        assert_eq!(node.session_id(0), Some(session));
        assert_eq!(node.peer_route(0)?, actual);
        assert_eq!(node.peer_count(), 1);
        assert!(!node.accepts_clients());
        assert_eq!(
            node.peers[&0].configured_key.as_deref(),
            Some(client_key.public_key())
        );
        node.try_send(0, &proof(session))?;
        let reply = client.poll(1012, now + Duration::from_millis(12), 64)?;
        assert_eq!(reply.frames.len(), 1);
    }
    Ok(())
}

#[test]
fn route_learning_preserves_live_session_until_new_aead_and_rejects_old_ciphertext() -> Result<()> {
    let now = Instant::now();
    let old_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let node_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let mut old = configured(
        old_socket.clone(),
        key(61),
        vec![peer(&key(62), node_socket.local_addr()?)],
        false,
        now,
    )?;
    let mut node = configured(
        node_socket.clone(),
        key(62),
        vec![peer(&key(61), old_socket.local_addr()?)],
        true,
        now,
    )?;
    exchange(&mut old, &mut node, now)?;
    let previous = node.session_id(0).unwrap();
    let old_wire = old
        .peers
        .get_mut(&0)
        .unwrap()
        .session
        .as_mut()
        .unwrap()
        .encode(&proof(previous))?;
    let later = now + Duration::from_millis(600);
    let mut fresh = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        key(61),
        peer(&key(62), node_socket.local_addr()?),
        budget(),
        1600,
        later,
    )?;
    let current = finish_offer(&mut fresh, &mut node, 1600, later)?;
    assert_ne!(current, previous);
    assert_eq!(node.session_id(0), Some(previous));
    assert_eq!(
        node.peer_route(0)?,
        NativeRoute::Udp(old_socket.local_addr()?)
    );
    old_socket.send_to(&old_wire, node_socket.local_addr()?)?;
    let old_delivery = node.poll(1611, later + Duration::from_millis(11), 64)?;
    assert_eq!(old_delivery.frames.len(), 1);
    assert!(old_delivery.session_changes.is_empty());
    fresh.try_send(0, &proof(current))?;
    let confirmed = node.poll(1612, later + Duration::from_millis(12), 64)?;
    assert_eq!(confirmed.frames.len(), 1);
    assert_eq!(
        confirmed.session_changes,
        vec![NativeSessionChange {
            peer: 0,
            previous,
            current
        }]
    );
    assert_eq!(node.peer_route(0)?, NativeRoute::Udp(fresh.local_addr()?));
    old_socket.send_to(&old_wire, node_socket.local_addr()?)?;
    let rejected = node.poll(1613, later + Duration::from_millis(13), 64)?;
    assert!(rejected.rejected_datagrams > 0);
    assert!(rejected.frames.is_empty());
    fresh.try_send(0, &proof(current))?;
    assert_eq!(
        node.poll(1614, later + Duration::from_millis(14), 64)?
            .frames
            .len(),
        1
    );
    assert_eq!(node.peer_count(), 1);
    Ok(())
}

#[test]
fn route_learning_rejects_unknown_identity_bad_signature_and_occupied_source() -> Result<()> {
    let now = Instant::now();
    let reserved_a = UdpSocket::bind("127.0.0.1:0")?;
    let reserved_c = UdpSocket::bind("127.0.0.1:0")?;
    let node_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let mut node = configured(
        node_socket.clone(),
        key(62),
        vec![
            peer(&key(61), reserved_a.local_addr()?),
            peer(&key(63), reserved_c.local_addr()?),
        ],
        true,
        now,
    )?;
    let make_client = |identity| {
        NativeEndpoint::connect_client(
            UdpSocket::bind("127.0.0.1:0")?,
            identity,
            peer(&key(62), node_socket.local_addr()?),
            budget(),
            1000,
            now,
        )
    };
    let impostor = make_client(key(64))?;
    assert!(node
        .receive_handshake(
            NativeRoute::Udp(impostor.local_addr()?),
            &offer(&impostor),
            1000,
            now
        )
        .is_err());
    let client = make_client(key(61))?;
    let wire = offer(&client);
    let mut bad_signature = wire.clone();
    *bad_signature.last_mut().unwrap() ^= 1;
    let source = NativeRoute::Udp(client.local_addr()?);
    assert!(node
        .receive_handshake(source, &bad_signature, 1000, now)
        .is_err());
    assert!(node.peers[&0].reconnect.is_none());
    assert!(node
        .receive_handshake(NativeRoute::Udp(reserved_c.local_addr()?), &wire, 1000, now)
        .is_err());
    assert!(node.peers[&0].reconnect.is_none());
    assert_eq!(node.sources.len(), 2);
    assert_eq!(node.peer_count(), 2);
    assert!(node
        .peers
        .values()
        .all(|p| p.session.is_none() && p.reconnect.is_none()));
    assert!(!node.accepts_clients());
    Ok(())
}

#[test]
fn route_learning_pending_is_bounded_and_expired_confirmation_cannot_install_route() -> Result<()> {
    let now = Instant::now();
    let reserved = UdpSocket::bind("127.0.0.1:0")?;
    let reserved_c = UdpSocket::bind("127.0.0.1:0")?;
    let node_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let mut node = configured(
        node_socket.clone(),
        key(62),
        vec![
            peer(&key(61), reserved.local_addr()?),
            peer(&key(63), reserved_c.local_addr()?),
        ],
        true,
        now,
    )?;
    let mut first = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        key(61),
        peer(&key(62), node_socket.local_addr()?),
        budget(),
        1000,
        now,
    )?;
    let first_wire = offer(&first);
    let session = finish_offer(&mut first, &mut node, 1000, now)?;
    let source = NativeRoute::Udp(first.local_addr()?);
    let deadline = node.peers[&0].reconnect.as_ref().unwrap().deadline;
    node.receive_handshake(source, &first_wire, 1011, now + Duration::from_millis(11))?;
    assert_eq!(
        node.peers[&0].reconnect.as_ref().unwrap().deadline,
        deadline
    );
    let other = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        key(61),
        peer(&key(62), node_socket.local_addr()?),
        budget(),
        1012,
        now + Duration::from_millis(12),
    )?;
    assert!(node
        .receive_handshake(
            NativeRoute::Udp(other.local_addr()?),
            &offer(&other),
            1012,
            now + Duration::from_millis(12)
        )
        .is_err());
    let other_identity = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        key(63),
        peer(&key(62), node_socket.local_addr()?),
        budget(),
        1012,
        now + Duration::from_millis(12),
    )?;
    assert!(node
        .receive_handshake(
            source,
            &offer(&other_identity),
            1012,
            now + Duration::from_millis(12)
        )
        .is_err());
    assert!(node.peers[&1].reconnect.is_none());
    assert_eq!(node.peers[&0].reconnect.as_ref().unwrap().source, source);
    let wire = first
        .peers
        .get_mut(&0)
        .unwrap()
        .session
        .as_mut()
        .unwrap()
        .encode(&proof(session))?;
    assert!(node.confirm_reconnect(source, &wire, deadline).is_none());
    assert!(node.session_id(0).is_none());
    node.expire_reconnects(deadline);
    assert!(node.peers[&0].reconnect.is_none());
    assert_eq!(node.sources.len(), 2);
    assert_eq!(node.sources[&NativeRoute::Udp(reserved.local_addr()?)], 0);
    let later = deadline + Duration::from_millis(1);
    let mut replacement = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        key(61),
        peer(&key(62), node_socket.local_addr()?),
        budget(),
        2100,
        later,
    )?;
    let replacement_session = finish_offer(&mut replacement, &mut node, 2100, later)?;
    replacement.try_send(0, &proof(replacement_session))?;
    let confirmed = node.poll(2111, later + Duration::from_millis(11), 64)?;
    assert_eq!(confirmed.frames.len(), 1);
    assert_eq!(node.session_id(0), Some(replacement_session));
    assert_eq!(
        node.peer_route(0)?,
        NativeRoute::Udp(replacement.local_addr()?)
    );
    Ok(())
}
