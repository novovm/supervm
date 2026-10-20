//! Actual loopback UDP, unchanged signed handshake/AEAD frames. The shortened
//! attempt budget is advanced monotonically; no production timers are relaxed.
use super::*;
use crate::novorudp::NovoRudpTransportFrameKindV0;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_secs(1),
        retry: Duration::from_millis(10),
    }
}
fn peer(identity: &SigningKey, address: SocketAddr) -> NativePeer {
    NativePeer {
        key: identity.verifying_key().to_bytes(),
        address,
    }
}
fn configured(
    socket: Arc<UdpSocket>,
    identity: SigningKey,
    peers: Vec<NativePeer>,
    budget: NativeHandshakeBudget,
    wall: u64,
    now: Instant,
) -> Result<NativeEndpoint> {
    let mut endpoint = NativeEndpoint::bind_shared(socket, identity, peers, budget, wall, now)?;
    endpoint.enable_configured_reconnect_v1()?;
    Ok(endpoint)
}
fn proof(session: [u8; 16]) -> NovoRudpTransportFrameV0 {
    // Ordinary existing authenticated application traffic, not a new Finished
    // message or a transport ACK. The node uses its existing RequestDecision.
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
fn exchange(
    a: &mut NativeEndpoint,
    b: &mut NativeEndpoint,
    wall: u64,
    now: Instant,
) -> Result<Vec<NativeSessionChange>> {
    let mut changes = Vec::new();
    for tick in 0..100 {
        let time = now + Duration::from_millis(tick * 10);
        changes.extend(a.poll(wall + tick * 10, time, 64)?.session_changes);
        changes.extend(b.poll(wall + tick * 10, time, 64)?.session_changes);
        if a.session_id(0).is_some() && a.session_id(0) == b.session_id(0) {
            return Ok(changes);
        }
        if let Some(session) = a.session_id(0) {
            a.try_send(0, &proof(session))?;
        }
        if let Some(session) = b.session_id(0) {
            b.try_send(0, &proof(session))?;
        }
    }
    anyhow::bail!("configured peers did not authenticate the same generation")
}

#[test]
fn configured_recovery_requires_explicit_pre_traffic_policy() -> Result<()> {
    let start = Instant::now();
    let absent = UdpSocket::bind("127.0.0.1:0")?;
    let mut endpoint = NativeEndpoint::bind(
        UdpSocket::bind("127.0.0.1:0")?,
        key(23),
        vec![peer(&key(47), absent.local_addr()?)],
        budget(),
        1000,
        start,
    )?;
    endpoint.poll(1000, start, 64)?;
    let later = start + Duration::from_secs(3);
    endpoint.poll(4000, later, 64)?;
    assert_eq!(
        endpoint.expired_handshakes(later),
        1,
        "default fixed attempt contract changed"
    );
    assert!(
        endpoint.enable_configured_reconnect_v1().is_err(),
        "live owner changed policy"
    );
    let mut clients = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        key(23),
        1,
        vec![17],
        budget(),
    )?;
    assert!(clients.enable_configured_reconnect_v1().is_err());
    let mut configured = configured(
        Arc::new(UdpSocket::bind("127.0.0.1:0")?),
        key(23),
        vec![peer(&key(47), absent.local_addr()?)],
        budget(),
        1000,
        start,
    )?;
    assert!(configured.enable_configured_reconnect_v1().is_err());
    Ok(())
}

#[test]
fn configured_peer_late_start_renews_attempt_in_both_identity_orders() -> Result<()> {
    for reverse in [false, true] {
        let start = Instant::now();
        let (a_key, b_key) = if reverse {
            (key(47), key(23))
        } else {
            (key(23), key(47))
        };
        let a_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
        let b_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
        let mut a = configured(
            a_socket.clone(),
            a_key.clone(),
            vec![peer(&b_key, b_socket.local_addr()?)],
            budget(),
            1000,
            start,
        )?;
        a.poll(1000, start, 64)?;
        let later = start + Duration::from_secs(3);
        assert_eq!(a.expired_handshakes(later), 1);
        a.poll(4000, later, 64)?;
        assert_eq!(
            a.expired_handshakes(later),
            0,
            "renewed, not extended an old transcript"
        );
        let mut b = configured(
            b_socket,
            b_key,
            vec![peer(&a_key, a_socket.local_addr()?)],
            budget(),
            4000,
            later,
        )?;
        assert!(exchange(&mut a, &mut b, 4000, later)?.is_empty());
        let original = a.session_id(0).unwrap();
        // Healthy sessions are not rotated when their handshake attempt ages.
        a.poll(8000, start + Duration::from_secs(7), 64)?;
        b.poll(8000, start + Duration::from_secs(7), 64)?;
        assert_eq!(a.session_id(0), Some(original));
        assert_eq!(b.session_id(0), Some(original));
        assert_eq!(a.peer_count(), 1);
        assert!(!a.accepts_clients());
    }
    Ok(())
}

#[test]
fn configured_peer_restart_requires_fresh_aead_and_rejects_old_generation() -> Result<()> {
    for reverse in [false, true] {
        let start = Instant::now();
        let (a_key, b_key) = if reverse {
            (key(47), key(23))
        } else {
            (key(23), key(47))
        };
        let a_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
        let b_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
        let mut a = configured(
            a_socket.clone(),
            a_key.clone(),
            vec![peer(&b_key, b_socket.local_addr()?)],
            budget(),
            1000,
            start,
        )?;
        let mut b = configured(
            b_socket.clone(),
            b_key,
            vec![peer(&a_key, a_socket.local_addr()?)],
            budget(),
            1000,
            start,
        )?;
        a.poll(1000, start, 64)?;
        let mut captured = [0; MAX_DATAGRAM_BYTES];
        let (length, _) = b_socket.peek_from(&mut captured)?;
        let old_offer = captured[..length].to_vec();
        exchange(&mut a, &mut b, 1000, start)?;
        let original = a.session_id(0).unwrap();
        let old_wire = a.peers[0]
            .session
            .as_mut()
            .unwrap()
            .encode(&proof(original))?;
        // Keep the exact route, configured identity and surviving endpoint.
        // Only the remote transport owner is independently restarted.
        let later = start + Duration::from_millis(300);
        let mut fresh = configured(
            a_socket.clone(),
            a_key,
            vec![peer(
                &key(if reverse { 23 } else { 47 }),
                b_socket.local_addr()?,
            )],
            budget(),
            1300,
            later,
        )?;
        fresh.poll(1300, later, 64)?;
        b.poll(1300, later, 64)?;
        fresh.poll(1301, later + Duration::from_millis(1), 64)?;
        let current = fresh
            .session_id(0)
            .context("restart initiator did not authenticate")?;
        assert_ne!(current, original);
        assert_eq!(
            b.session_id(0),
            Some(original),
            "signed offer replaced a live channel"
        );
        a_socket.send_to(&old_wire, b_socket.local_addr()?)?;
        let before = b.poll(1302, later + Duration::from_millis(2), 64)?;
        assert!(before.session_changes.is_empty());
        assert_eq!(b.session_id(0), Some(original));
        fresh.try_send(0, &proof(current))?;
        let after = b.poll(1303, later + Duration::from_millis(3), 64)?;
        assert_eq!(
            after.session_changes,
            vec![NativeSessionChange {
                peer: 0,
                previous: original,
                current
            }]
        );
        assert_eq!(b.session_id(0), Some(current));
        // Replay of an old signed offer and old encrypted traffic cannot
        // restore the old session, including when they share the same source.
        a_socket.send_to(&old_offer, b_socket.local_addr()?)?;
        a_socket.send_to(&old_wire, b_socket.local_addr()?)?;
        let replay = b.poll(1304, later + Duration::from_millis(4), 64)?;
        assert!(replay.session_changes.is_empty());
        assert!(replay.frames.is_empty());
        assert_eq!(b.session_id(0), Some(current));
        assert_eq!(b.peer_count(), 1);
        assert!(!b.accepts_clients());
    }
    Ok(())
}

#[test]
fn configured_reconnect_cannot_migrate_source_or_identity_via_open_admission() -> Result<()> {
    let start = Instant::now();
    let a_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let b_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let mut a = configured(
        a_socket.clone(),
        key(23),
        vec![peer(&key(47), b_socket.local_addr()?)],
        budget(),
        1000,
        start,
    )?;
    let mut b = configured(
        b_socket,
        key(47),
        vec![peer(&key(23), a_socket.local_addr()?)],
        budget(),
        1000,
        start,
    )?;
    b.allow_clients(key(47), 1, vec![17], budget())?;
    b.enable_client_reconnect_v1()?;
    exchange(&mut a, &mut b, 1000, start)?;
    let original = b.session_id(0);
    let mut migrated = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        key(23),
        peer(&key(47), b.local_addr()?),
        budget(),
        1100,
        start + Duration::from_millis(100),
    )?;
    migrated.poll(1100, start + Duration::from_millis(100), 64)?;
    assert_eq!(
        b.poll(1100, start + Duration::from_millis(100), 64)?
            .rejected_datagrams,
        1
    );
    assert_eq!(b.session_id(0), original);
    assert_eq!(b.peer_count(), 1);
    let mut impostor = NativeEndpoint::connect_client(
        a_socket.try_clone()?,
        key(99),
        peer(&key(47), b.local_addr()?),
        budget(),
        1200,
        start + Duration::from_millis(200),
    )?;
    impostor.poll(1200, start + Duration::from_millis(200), 64)?;
    assert_eq!(
        b.poll(1200, start + Duration::from_millis(200), 64)?
            .rejected_datagrams,
        1
    );
    assert_eq!(b.session_id(0), original);
    assert_eq!(b.peer_count(), 1);
    Ok(())
}

#[test]
fn replayed_initial_offer_cannot_pin_restarted_responder_to_orphan_session() -> Result<()> {
    let start = Instant::now();
    let mut keys = [key(23), key(47)];
    keys.sort_by_key(|identity| {
        peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes())
    });
    let [a_key, b_key] = keys;
    let a_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let b_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let mut a = configured(
        a_socket.clone(),
        a_key.clone(),
        vec![peer(&b_key, b_socket.local_addr()?)],
        budget(),
        1000,
        start,
    )?;
    let mut b = configured(
        b_socket.clone(),
        b_key.clone(),
        vec![peer(&a_key, a_socket.local_addr()?)],
        budget(),
        1000,
        start,
    )?;
    a.poll(1000, start, 64)?;
    let mut captured = [0; MAX_DATAGRAM_BYTES];
    let (length, _) = b_socket.peek_from(&mut captured)?;
    let old_offer = captured[..length].to_vec();
    exchange(&mut a, &mut b, 1000, start)?;
    let original = a.session_id(0).unwrap();
    // The old Offer is still signed/in-window, but the higher peer has lost
    // its former DH secret after an independent process restart.
    drop(b);
    let mut fresh = configured(
        b_socket.clone(),
        b_key,
        vec![peer(&a_key, a_socket.local_addr()?)],
        budget(),
        1200,
        start + Duration::from_millis(200),
    )?;
    a_socket.send_to(&old_offer, b_socket.local_addr()?)?;
    fresh.poll(1200, start + Duration::from_millis(200), 64)?;
    assert!(
        fresh.session_id(0).is_none(),
        "Offer alone installed an orphan key"
    );
    assert_eq!(a.session_id(0), Some(original));
    // The unconfirmed responder attempt expires, retaining the replay cache.
    // Fresh attempts use new signed material, not an extended old deadline.
    let changes = exchange(
        &mut a,
        &mut fresh,
        2400,
        start + Duration::from_millis(1400),
    )?;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].previous, original);
    assert_ne!(a.session_id(0), Some(original));
    Ok(())
}
