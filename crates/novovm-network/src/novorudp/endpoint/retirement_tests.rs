//! Real UDP lifecycle tests: one listener, bounded live peers, no process reset.
use super::*;
use crate::novorudp::NovoRudpTransportFrameKindV0;

#[test]
fn one_listener_serves_128_identities_without_reusing_handles_or_accepting_old_frames() -> Result<()>
{
    let key = SigningKey::from_bytes(&[250; 32]);
    let budget = || NativeHandshakeBudget {
        lifetime: Duration::from_secs(2),
        retry: Duration::from_millis(5),
    };
    let mut server = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        key.clone(),
        1,
        vec![17],
        budget(),
    )?;
    server.update_client_capacity(1)?;
    let remote = NativePeer {
        key: key.verifying_key().to_bytes(),
        address: server.local_addr()?,
    };
    let mut old_frames = Vec::<Vec<u8>>::new();
    for ordinal in 0..128usize {
        let start = Instant::now();
        let mut client = NativeEndpoint::connect_client(
            UdpSocket::bind("127.0.0.1:0")?,
            SigningKey::from_bytes(&[(ordinal + 1) as u8; 32]),
            remote.clone(),
            budget(),
            1000,
            start,
        )?;
        while client.session_id(0).is_none() {
            ensure!(
                start.elapsed() < Duration::from_secs(3),
                "client handshake timeout"
            );
            client.poll(1000, Instant::now(), 8)?;
            server.poll(1000, Instant::now(), 8)?;
            std::thread::yield_now();
        }
        assert_eq!(server.peer_count(), 1);
        assert_eq!(server.peer_indices().collect::<Vec<_>>(), vec![ordinal]);
        // A signed offer alone cannot activate a retired/new identity.
        assert!(server.session_id(ordinal).is_none());
        for wire in &old_frames {
            client.io.try_send(wire, NativeRoute::Udp(remote.address))?;
        }
        let mut rejected = 0;
        while rejected < old_frames.len() {
            ensure!(
                start.elapsed() < Duration::from_secs(3),
                "old frame drain timeout"
            );
            let report = server.poll(1000, Instant::now(), 8)?;
            assert!(report.frames.is_empty());
            rejected += report.rejected_datagrams;
        }
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            client.session_id(0).unwrap(),
            17,
            3,
            4,
            0,
            vec![ordinal as u8; 20],
        );
        let wire = client
            .peers
            .get_mut(&0)
            .unwrap()
            .session
            .as_mut()
            .unwrap()
            .encode(&frame)?;
        client
            .io
            .try_send(&wire, NativeRoute::Udp(remote.address))?;
        loop {
            ensure!(
                start.elapsed() < Duration::from_secs(3),
                "fresh channel confirmation timeout"
            );
            let report = server.poll(1000, Instant::now(), 8)?;
            if let Some(received) = report.frames.first() {
                assert_eq!(received.peer(), ordinal);
                assert_eq!(received.frame().payload, frame.payload);
                break;
            }
        }
        assert!(server.session_id(ordinal).is_some());
        // Also reject an old ACK after the new channel has activated.
        for old in &old_frames {
            client.io.try_send(old, NativeRoute::Udp(remote.address))?;
        }
        let mut rejected = 0;
        while rejected < old_frames.len() {
            let report = server.poll(1000, Instant::now(), 8)?;
            assert!(report.frames.is_empty());
            rejected += report.rejected_datagrams;
            ensure!(
                start.elapsed() < Duration::from_secs(3),
                "old ACK drain timeout"
            );
        }
        let ack = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Ack,
            client.session_id(0).unwrap(),
            17,
            3,
            5,
            0,
            vec![9; 20],
        );
        old_frames = vec![
            wire,
            client
                .peers
                .get_mut(&0)
                .unwrap()
                .session
                .as_mut()
                .unwrap()
                .encode(&ack)?,
        ];
        assert!(server
            .idle_client_candidates(Instant::now(), Duration::from_secs(1))
            .is_empty());
        let later = Instant::now() + Duration::from_secs(3);
        assert_eq!(
            server.idle_client_candidates(later, Duration::from_secs(1)),
            vec![ordinal]
        );
        let retired = server.retire_client(ordinal)?;
        assert_eq!(retired.peer, ordinal);
        assert!(retired.previous_session.is_some());
        assert_eq!(server.peer_count(), 0);
        assert!(server.sources.is_empty());
        assert!(server.session_id(ordinal).is_none());
    }
    assert_eq!(server.next_peer, 128);
    assert_eq!(server.peer_count(), 0);
    Ok(())
}

#[test]
fn full_listener_retains_idle_peers_without_verified_admission_demand() -> Result<()> {
    use crate::product_overlay::{handshake_wire, NodeHandshakeInitiatorV1};
    let key = SigningKey::from_bytes(&[231; 32]);
    let budget = NativeHandshakeBudget {
        lifetime: Duration::from_secs(5),
        retry: Duration::from_millis(5),
    };
    let mut server = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        key.clone(),
        1,
        vec![17],
        budget,
    )?;
    server.update_client_capacity(1)?;
    let start = Instant::now();
    let target = peer_id_from_ed25519_public_key_v1(&key.verifying_key().to_bytes());
    let source = UdpSocket::bind("127.0.0.1:0")?;
    let offer = NodeHandshakeInitiatorV1::start(
        &SigningKey::from_bytes(&[232; 32]),
        target.clone(),
        1000,
        1000,
    )?;
    server.accept_client(
        NativeRoute::Udp(source.local_addr()?),
        &handshake_wire::encode_offer(offer.offer())?,
        1000,
        start,
    )?;
    assert_eq!(server.peer_count(), 1);
    let later = start + Duration::from_secs(6);
    assert!(!server.needs_client_capacity(later));
    let other = UdpSocket::bind("127.0.0.1:0")?;
    let offer =
        NodeHandshakeInitiatorV1::start(&SigningKey::from_bytes(&[233; 32]), target, 1000, 1000)?;
    let mut wire = handshake_wire::encode_offer(offer.offer())?;
    *wire.last_mut().unwrap() ^= 1;
    assert!(server
        .accept_client(NativeRoute::Udp(other.local_addr()?), &wire, 1000, start)
        .is_err());
    assert!(server.waiting_client.is_none());
    assert!(!server.needs_client_capacity(later));
    assert_eq!(
        server.client_admission_snapshot_v1(start),
        Some(NativeClientAdmissionSnapshotV1 {
            peer_count: 1,
            capacity: 1,
            authenticated_waiting: false,
        }),
        "an invalid signed offer must not produce adaptive demand"
    );
    *wire.last_mut().unwrap() ^= 1;
    server.accept_client(NativeRoute::Udp(other.local_addr()?), &wire, 1000, start)?;
    assert!(server.needs_client_capacity(start));
    assert_eq!(server.peer_count(), 1);
    assert!(
        server
            .client_admission_snapshot_v1(start)
            .unwrap()
            .authenticated_waiting
    );
    assert!(
        !server
            .client_admission_snapshot_v1(start + Duration::from_secs(5))
            .unwrap()
            .authenticated_waiting,
        "exact original deadline ends demand even before the next poll"
    );
    assert!(
        !server.needs_client_capacity(later),
        "expired offer cannot demand eviction"
    );
    server.admit_waiting_client(later)?;
    assert!(server.waiting_client.is_none());
    assert_eq!(server.peer_count(), 1);
    Ok(())
}

#[test]
fn admission_snapshot_tracks_verified_waiter_without_evicting_existing_peer() -> Result<()> {
    use crate::product_overlay::{handshake_wire, NodeHandshakeInitiatorV1};
    let key = SigningKey::from_bytes(&[234; 32]);
    let start = Instant::now();
    let mut server = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        key.clone(),
        1,
        vec![17],
        NativeHandshakeBudget {
            lifetime: Duration::from_secs(5),
            retry: Duration::from_millis(5),
        },
    )?;
    server.update_client_capacity(1)?;
    let target = peer_id_from_ed25519_public_key_v1(&key.verifying_key().to_bytes());
    let sources = [
        UdpSocket::bind("127.0.0.1:0")?,
        UdpSocket::bind("127.0.0.1:0")?,
    ];
    for (index, source) in sources.iter().enumerate() {
        let offer = NodeHandshakeInitiatorV1::start(
            &SigningKey::from_bytes(&[235 + index as u8; 32]),
            target.clone(),
            1000,
            1000,
        )?;
        server.accept_client(
            NativeRoute::Udp(source.local_addr()?),
            &handshake_wire::encode_offer(offer.offer())?,
            1000,
            start,
        )?;
    }
    let snapshot = server.client_admission_snapshot_v1(start).unwrap();
    assert_eq!(snapshot.peer_count, 1);
    assert_eq!(snapshot.capacity, 1);
    assert!(snapshot.authenticated_waiting);
    // Only the existing owner changes capacity. Observing demand never does.
    assert_eq!(server.peer_count(), 1);
    server.update_client_capacity(2)?;
    server.admit_waiting_client(start)?;
    let snapshot = server.client_admission_snapshot_v1(start).unwrap();
    assert_eq!(snapshot.peer_count, 2);
    assert!(!snapshot.authenticated_waiting);
    assert_eq!(server.peer_indices().collect::<Vec<_>>(), vec![0, 1]);
    server.update_client_capacity(1)?;
    let snapshot = server.client_admission_snapshot_v1(start).unwrap();
    assert_eq!(snapshot.peer_count, 2);
    assert_eq!(snapshot.capacity, 1);
    assert!(!snapshot.authenticated_waiting, "shrink is not new demand");
    Ok(())
}
