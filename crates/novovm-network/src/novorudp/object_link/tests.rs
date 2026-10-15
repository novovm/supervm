use super::*;
use crate::novorudp::endpoint::{NativeHandshakeBudget, NativePeer};
use ed25519_dalek::SigningKey;
use std::net::UdpSocket;

const CONTROL: u64 = 1;
const BODY: u64 = 2;
fn configs() -> Vec<NativeLaneConfig> {
    [CONTROL, BODY]
        .into_iter()
        .map(|stream| NativeLaneConfig {
            stream,
            domain: [stream as u8; 32],
            receive: ReassemblyLimits {
                max_message_bytes: 8192,
                messages: 2,
                bytes: 16_384,
                peer_messages: 1,
                peer_bytes: 8192,
                ttl: Duration::from_secs(5),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: 16_384,
                peer_messages: 1,
                peer_bytes: 8192,
            },
        })
        .collect()
}
fn policy() -> NativeSendPolicy {
    NativeSendPolicy {
        window: NovoRudpWindowConfig {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 16,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
            ..Default::default()
        },
        pacing: NovoRudpPacingProfile {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 16,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
        },
        ttl: Duration::from_secs(2),
    }
}
fn message(stream: u64, value: u8, size: usize) -> Result<Arc<OutgoingMessage>> {
    Ok(Arc::new(OutgoingMessage::with_profile(
        [stream as u8; 32],
        vec![value; size],
        8192,
        FragmentProfile::NativeUdpV2,
    )?))
}
fn pair_and_third() -> Result<(NativeObjectLink, NativeObjectLink, NativeObjectLink)> {
    let sockets = (0..3)
        .map(|_| UdpSocket::bind("127.0.0.1:0"))
        .collect::<std::io::Result<Vec<_>>>()?;
    let addresses = sockets
        .iter()
        .map(UdpSocket::local_addr)
        .collect::<std::io::Result<Vec<_>>>()?;
    let keys = [23, 47, 71].map(|seed| SigningKey::from_bytes(&[seed; 32]));
    let now = Instant::now();
    let mut links = Vec::new();
    for (i, socket) in sockets.into_iter().enumerate() {
        let peers = if i == 0 { vec![1, 2] } else { vec![0] };
        let peers = peers
            .into_iter()
            .map(|n| NativePeer {
                key: keys[n].verifying_key().to_bytes(),
                address: addresses[n],
            })
            .collect();
        let endpoint = NativeEndpoint::bind(
            socket,
            keys[i].clone(),
            peers,
            NativeHandshakeBudget {
                lifetime: Duration::from_secs(2),
                retry: Duration::from_millis(10),
            },
            1000,
            now,
        )?;
        links.push(NativeObjectLink::new(endpoint, configs(), policy())?);
    }
    while links
        .iter()
        .any(|l| (0..l.peer_count()).any(|p| l.session_id(p).is_none()))
    {
        ensure!(
            now.elapsed() < Duration::from_secs(2),
            "native link handshake timeout"
        );
        for link in &mut links {
            let result = link.poll(1000 + now.elapsed().as_millis() as u64, Instant::now(), 8)?;
            assert!(result.completed.is_empty());
        }
        std::thread::yield_now();
    }
    let c = links.pop().unwrap();
    let b = links.pop().unwrap();
    let a = links.pop().unwrap();
    Ok((a, b, c))
}

#[test]
fn reconnect_retires_only_old_generation_and_preserves_unknown_outgoing_custody() -> Result<()> {
    let node_key = SigningKey::from_bytes(&[51; 32]);
    let client_key = SigningKey::from_bytes(&[52; 32]);
    let budget = || NativeHandshakeBudget {
        lifetime: Duration::from_secs(2),
        retry: Duration::from_millis(5),
    };
    let mut endpoint = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        node_key.clone(),
        1,
        vec![CONTROL, BODY],
        budget(),
    )?;
    endpoint.enable_client_reconnect_v1()?;
    let remote = NativePeer {
        key: node_key.verifying_key().to_bytes(),
        address: endpoint.local_addr()?,
    };
    let mut server = NativeObjectLink::new(endpoint, configs(), policy())?;
    let start = Instant::now();
    let connect = || -> Result<NativeObjectLink> {
        NativeObjectLink::new(
            NativeEndpoint::connect_client(
                UdpSocket::bind("127.0.0.1:0")?,
                client_key.clone(),
                remote.clone(),
                budget(),
                1000,
                Instant::now(),
            )?,
            configs(),
            policy(),
        )
    };
    let mut old = connect()?;
    while old.session_id(0).is_none() {
        ensure!(
            start.elapsed() < Duration::from_secs(2),
            "initial handshake timeout"
        );
        let _ = old.poll(1000, Instant::now(), 64)?;
        let _ = server.poll(1000, Instant::now(), 64)?;
    }
    let original = server.session_id(0).unwrap();
    let validity = server.session_validity(0)?;
    server.claim_application_session(0, BODY, [2; 32], NativeApplicationRole::Service)?;
    // A completed hash exists in the old generation. The identical object must
    // be delivered again, NOT mistaken for already-delivered, in the new one.
    let body = message(BODY, 7, 600)?;
    old.try_send(0, BODY, &body, Instant::now())?;
    loop {
        ensure!(start.elapsed() < Duration::from_secs(2), "old body timeout");
        let _ = old.poll(1000, Instant::now(), 64)?;
        let _ = server.poll(1000, Instant::now(), 64)?;
        if server.poll_complete(BODY, Instant::now(), 64)?.is_some() {
            break;
        }
    }
    let partial = message(BODY, 8, 2000)?;
    let frame = super::super::NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        original,
        BODY,
        u64::from_be_bytes(partial.id()[..8].try_into()?),
        0,
        0,
        partial.frame(0)?,
    );
    old.endpoint.try_send(0, &frame)?;
    let _ = server.poll(1000, Instant::now(), 64)?;
    assert_eq!(server.reserved_bytes(BODY)?, 2000);
    let reply = message(CONTROL, 9, 600)?;
    server.try_send(0, CONTROL, &reply, Instant::now())?;
    let mut fresh = connect()?;
    while fresh.session_id(0).is_none() {
        ensure!(
            start.elapsed() < Duration::from_secs(2),
            "fresh handshake timeout"
        );
        let _ = fresh.poll(1000, Instant::now(), 64)?;
        let _ = server.poll(1000, Instant::now(), 64)?;
    }
    assert!(validity.is_current());
    fresh.try_send(0, BODY, &body, Instant::now())?;
    let mut retired = false;
    let mut received = false;
    while !retired || !received {
        ensure!(
            start.elapsed() < Duration::from_secs(2),
            "replacement object timeout"
        );
        let _ = fresh.poll(1000, Instant::now(), 64)?;
        let result = server.poll(1000, Instant::now(), 64)?;
        if !result.session_changes.is_empty() {
            assert_eq!(result.session_changes.len(), 1);
            assert_eq!(result.retired_inbound_messages, 1);
            assert!(!validity.is_current());
            assert_ne!(server.session_id(0), Some(original));
        }
        for event in result.completed {
            assert_eq!(event.state, ObjectSendState::SessionReplaced);
            assert!(Arc::ptr_eq(&event.body, &reply));
            retired = true;
        }
        if let Some(receipt) = server.poll_complete(BODY, Instant::now(), 64)? {
            assert_eq!(receipt.id, body.id());
            assert_eq!(receipt.chunks.concat(), vec![7; 600]);
            received = true;
        }
    }
    assert_eq!(server.outbound_usage(CONTROL)?, (0, 0));
    assert_eq!(server.reserved_bytes(BODY)?, 0);
    assert!(server.session_validity(0)?.is_current());
    server.claim_application_session(0, BODY, [2; 32], NativeApplicationRole::Service)?;
    assert!(server
        .claim_application_session(0, BODY, [2; 32], NativeApplicationRole::Service)
        .is_err());
    let current = server.session_validity(0)?;
    drop(server);
    assert!(!current.is_current());
    Ok(())
}

#[test]
fn application_sequence_owner_is_bound_to_peer_lane_and_authenticated_session() -> Result<()> {
    let (mut a, _, _) = pair_and_third()?;
    let role = NativeApplicationRole::Client;
    assert!(a.claim_application_session(0, BODY, [9; 32], role).is_err());
    assert!(a
        .claim_application_session(99, BODY, [2; 32], role)
        .is_err());
    assert!(a.application_owners.is_empty());
    assert_eq!(
        Some(a.claim_application_session(0, BODY, [2; 32], role)?),
        a.session_id(0)
    );
    assert!(a.claim_application_session(0, BODY, [2; 32], role).is_err());
    assert_eq!(
        Some(a.claim_application_session(1, BODY, [2; 32], role)?),
        a.session_id(1)
    );
    assert_eq!(
        Some(a.claim_application_session(0, CONTROL, [1; 32], role)?),
        a.session_id(0)
    );
    assert_eq!(a.application_owners.len(), 3);
    assert_eq!(
        Some(a.claim_application_session(0, BODY, [2; 32], NativeApplicationRole::Service)?),
        a.session_id(0)
    );
    assert!(a
        .claim_application_session(0, BODY, [2; 32], NativeApplicationRole::Service)
        .is_err());
    assert_eq!(a.application_owners.len(), 4);
    assert_eq!(
        a.local_identity(),
        SigningKey::from_bytes(&[23; 32]).verifying_key().to_bytes()
    );
    Ok(())
}

#[test]
fn body_backpressure_and_offline_peer_do_not_consume_control_lane_or_other_peer_quota() -> Result<()>
{
    let (mut a, mut b, c) = pair_and_third()?;
    drop(c); // Peer went offline AFTER the original authenticated handshake.
    let body = message(BODY, 11, 4096)?;
    let other = message(BODY, 12, 4096)?;
    let control = message(CONTROL, 21, 1800)?; // Multi-fragment QC-sized object.
    let reply = message(CONTROL, 22, 200)?;
    let start = Instant::now();
    assert!(a.try_send(1, BODY, &body, start)?);
    assert!(!a.try_send(1, BODY, &other, start)?);
    assert!(a.try_send(0, BODY, &body, start)?);
    assert!(!a.try_send(0, BODY, &other, start)?);
    assert_eq!(a.outbound_usage(BODY)?, (2, 8192));
    assert!(a.try_send(0, CONTROL, &control, start)?);
    // Duplicate enqueue retains the exact existing reservation/cursor.
    assert!(a.try_send(0, CONTROL, &control, start)?);
    assert_eq!(a.outbound_usage(CONTROL)?, (1, 1800));
    let mut got_control = false;
    let mut got_reply = false;
    let mut control_done = false;
    let mut reply_done = false;
    while !(got_control && got_reply && control_done && reply_done) {
        ensure!(
            start.elapsed() < Duration::from_secs(1),
            "healthy control starved by body/offline peer"
        );
        let now = Instant::now();
        let wall = 1100 + start.elapsed().as_millis() as u64;
        for done in a.poll(wall, now, 8)?.completed {
            assert_eq!(done.stream, CONTROL);
            assert_eq!(done.state, ObjectSendState::Delivered);
            assert!(Arc::ptr_eq(&done.body, &control));
            control_done = true;
        }
        for done in b.poll(wall, now, 8)?.completed {
            assert_eq!(done.stream, CONTROL);
            assert_eq!(done.state, ObjectSendState::Delivered);
            assert!(Arc::ptr_eq(&done.body, &reply));
            reply_done = true;
        }
        if let Some(item) = b.poll_complete(CONTROL, now, 1)? {
            assert_eq!(item.id, control.id());
            assert_eq!(item.chunks.concat(), vec![21; 1800]);
            assert!(!got_control);
            got_control = true;
            assert!(b.try_send(0, CONTROL, &reply, now)?);
        }
        if let Some(item) = a.poll_complete(CONTROL, now, 1)? {
            assert_eq!(item.id, reply.id());
            assert_eq!(item.chunks.concat(), vec![22; 200]);
            assert!(!got_reply);
            got_reply = true;
        }
        // Deliberately never take the healthy receiver's body assembly slot.
        assert_eq!(a.outbound_usage(BODY)?.0, 2);
        std::thread::yield_now();
    }
    let later = start + Duration::from_secs(3);
    let expired = a.poll(4100, later, 64)?.completed;
    assert_eq!(expired.len(), 2);
    for item in expired {
        assert_eq!(item.state, ObjectSendState::Expired);
        assert_eq!(item.stream, BODY);
        assert!(Arc::ptr_eq(&item.body, &body));
    }
    assert_eq!(a.outbound_usage(BODY)?, (0, 0));
    assert_eq!(a.outbound_usage(CONTROL)?, (0, 0));
    Ok(())
}

#[test]
fn scope_profile_and_quantum_reject_without_reserving_or_relabeling_bytes() -> Result<()> {
    let (mut a, _b, _c) = pair_and_third()?;
    let now = Instant::now();
    let body = message(BODY, 11, 100)?;
    assert!(a.try_send(0, CONTROL, &body, now).is_err());
    assert!(a.try_send(8, BODY, &body, now).is_err());
    assert!(a.try_send(0, 99, &body, now).is_err());
    let legacy = Arc::new(OutgoingMessage::new([BODY as u8; 32], vec![1; 100], 8192)?);
    assert!(a.try_send(0, BODY, &legacy, now).is_err());
    assert!(a.poll(1000, now, 0).is_err());
    assert!(a.poll(1000, now, 65).is_err());
    assert_eq!(a.outbound_usage(BODY)?, (0, 0));
    Ok(())
}

#[test]
fn operation_deadline_caps_repairs_without_resetting_a_live_send() -> Result<()> {
    let (mut a, _b, _c) = pair_and_third()?;
    let now = Instant::now();
    let deadline = now + Duration::from_millis(20);
    let body = message(BODY, 11, 100)?;
    assert!(a.try_send_until(0, BODY, &body, now, now).is_err());
    assert_eq!(a.outbound_usage(BODY)?, (0, 0));
    assert!(a.try_send_until(0, BODY, &body, now, deadline)?);
    // Neither re-enqueue nor the normal two-second transport TTL extends 20ms.
    assert!(a.try_send(0, BODY, &body, now + Duration::from_millis(10))?);
    let report = a.poll(1030, now + Duration::from_millis(30), 64)?;
    assert_eq!(report.completed.len(), 1);
    assert_eq!(report.completed[0].state, ObjectSendState::Expired);
    assert!(Arc::ptr_eq(&report.completed[0].body, &body));
    assert_eq!(report.data_sent, 0);
    assert_eq!(a.outbound_usage(BODY)?, (0, 0));
    Ok(())
}
