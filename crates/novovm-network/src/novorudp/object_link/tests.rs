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
fn client_retirement_protects_work_and_transport_then_invalidates_only_drained_identity(
) -> Result<()> {
    let identity = SigningKey::from_bytes(&[241; 32]);
    let budget = || NativeHandshakeBudget {
        lifetime: Duration::from_secs(10),
        retry: Duration::from_millis(5),
    };
    let endpoint = NativeEndpoint::listen_clients(
        UdpSocket::bind("127.0.0.1:0")?,
        identity.clone(),
        1,
        vec![CONTROL, BODY],
        budget(),
    )?;
    let remote = NativePeer {
        key: identity.verifying_key().to_bytes(),
        address: endpoint.local_addr()?,
    };
    let mut server = NativeObjectLink::new(endpoint, configs(), policy())?;
    let idle = Duration::from_secs(1);
    server.retire_idle_clients(Instant::now(), idle, &BTreeSet::new(), 1)?;
    for ordinal in 0..1usize {
        let start = Instant::now();
        let mut client = NativeObjectLink::new(
            NativeEndpoint::connect_client(
                UdpSocket::bind("127.0.0.1:0")?,
                SigningKey::from_bytes(&[242 + ordinal as u8; 32]),
                remote.clone(),
                budget(),
                1000,
                start,
            )?,
            configs(),
            policy(),
        )?;
        while client.session_id(0).is_none() {
            ensure!(start.elapsed() < Duration::from_secs(3), "client handshake");
            let _ = client.poll(1000, Instant::now(), 8)?;
            let _ = server.poll(1000, Instant::now(), 8)?;
        }
        let body = message(BODY, 7, 2048)?;
        assert!(client.try_send(0, BODY, &body, Instant::now())?);
        while server.lanes[&BODY].receiver.pending_messages() == 0 {
            ensure!(start.elapsed() < Duration::from_secs(3), "receive object");
            let _ = client.poll(1000, Instant::now(), 8)?;
            let _ = server.poll(1000, Instant::now(), 8)?;
        }
        let later = Instant::now() + Duration::from_secs(2);
        // Equality alone never evicts a healthy idle client. Only a verified
        // pending admission (or actual resource shrink) requests capacity.
        assert!(server
            .retire_idle_clients(later, idle, &BTreeSet::new(), 1)?
            .is_empty());
        let mut waiting = NativeObjectLink::new(
            NativeEndpoint::connect_client(
                UdpSocket::bind("127.0.0.1:0")?,
                SigningKey::from_bytes(&[246; 32]),
                remote.clone(),
                budget(),
                1000,
                Instant::now(),
            )?,
            configs(),
            policy(),
        )?;
        while !server.endpoint.needs_client_capacity(Instant::now()) {
            ensure!(
                start.elapsed() < Duration::from_secs(3),
                "pending admission"
            );
            let _ = waiting.poll(1000, Instant::now(), 8)?;
            let _ = server.poll(1000, Instant::now(), 8)?;
        }
        assert!(waiting.session_id(0).is_none());
        let admission = server.client_admission_snapshot_v1(Instant::now()).unwrap();
        assert_eq!(admission.peer_count, 1);
        assert_eq!(admission.capacity, 1);
        assert!(admission.authenticated_waiting);
        // Even a complete-but-unclaimed object protects its sender's identity.
        assert!(server
            .retire_idle_clients(later, idle, &BTreeSet::new(), 1)?
            .is_empty());
        let mut received = None;
        while received.is_none() || !client.jobs.is_empty() {
            ensure!(start.elapsed() < Duration::from_secs(3), "complete object");
            if received.is_none() {
                received = server.poll_complete(BODY, Instant::now(), 8)?;
            }
            let _ = client.poll(1000, Instant::now(), 8)?;
            let _ = server.poll(1000, Instant::now(), 8)?;
        }
        assert_eq!(received.unwrap().chunks.concat(), vec![7; 2048]);
        let validity = server.session_validity(ordinal)?;
        server.claim_application_session(
            ordinal,
            BODY,
            [BODY as u8; 32],
            NativeApplicationRole::Service,
        )?;
        assert!(server
            .retire_idle_clients(later, idle, &BTreeSet::from([ordinal]), 1)?
            .is_empty());
        assert!(validity.is_current()); // Represents an executing Watch/business operation.
        let reply = message(BODY, 9, 1024)?;
        assert!(server.try_send(ordinal, BODY, &reply, Instant::now())?);
        assert!(server
            .retire_idle_clients(later, idle, &BTreeSet::new(), 1)?
            .is_empty());
        assert!(server.cancel_send(ordinal, BODY, &reply));
        while !server.jobs.is_empty() {
            let _ = server.poll(1000, Instant::now(), 8)?;
        }
        let retired = server.retire_idle_clients(later, idle, &BTreeSet::new(), 1)?;
        assert_eq!(retired.len(), 1);
        assert_eq!(retired[0].peer, ordinal);
        assert!(!validity.is_current());
        assert!(server.application_owners.is_empty());
        assert!(server.peers.is_empty());
        assert!(server.lanes.values().all(|lane| lane.peers.is_empty()));
        assert!(server
            .try_send(ordinal, BODY, &reply, Instant::now())
            .is_err());
        // The verified waiting transcript survives capacity reclamation. It
        // need not restart authentication or bypass replay checks.
        while waiting.session_id(0).is_none() {
            ensure!(
                start.elapsed() < Duration::from_secs(3),
                "activate waiting admission"
            );
            let _ = server.poll(1000, Instant::now(), 8)?;
            let _ = waiting.poll(1000, Instant::now(), 8)?;
        }
        assert_eq!(server.peer_indices().collect::<Vec<_>>(), vec![ordinal + 1]);
        assert!(server.session_id(ordinal + 1).is_none());
        waiting.try_send(0, BODY, &body, Instant::now())?;
        loop {
            ensure!(
                start.elapsed() < Duration::from_secs(3),
                "fresh waiting client data"
            );
            let _ = waiting.poll(1000, Instant::now(), 8)?;
            let _ = server.poll(1000, Instant::now(), 8)?;
            if let Some(received) = server.poll_complete(BODY, Instant::now(), 8)? {
                assert_eq!(received.chunks.concat(), vec![7; 2048]);
                break;
            }
        }
        while !waiting.jobs.is_empty() {
            let _ = server.poll(1000, Instant::now(), 8)?;
            let _ = waiting.poll(1000, Instant::now(), 8)?;
        }
        assert!(server
            .retire_idle_clients(later, idle, &BTreeSet::new(), 1)?
            .is_empty());
    }
    Ok(())
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

#[test]
fn cancelled_partial_send_releases_quota_without_resetting_healthy_session() -> Result<()> {
    let (mut a, mut b, _c) = pair_and_third()?;
    let session = a.session_id(0);
    let now = Instant::now();
    let body = message(BODY, 81, 8192)?;
    let cancel = NativeSendCancellation::default();
    assert!(a.try_send_cancellable_until(
        0,
        BODY,
        &body,
        now,
        now + Duration::from_secs(1),
        &cancel
    )?);
    assert!(a
        .try_send_cancellable_until(
            0,
            BODY,
            &body,
            now,
            now + Duration::from_secs(2),
            &NativeSendCancellation::default()
        )
        .is_err());
    assert_eq!(a.poll(1000, now, 1)?.data_sent, 1);
    assert!(!a.cancel_send(0, BODY, &message(BODY, 81, 8192)?));
    cancel.cancel();
    cancel.cancel(); // idempotent, never resets a request
    assert!(a
        .try_send_cancellable_until(0, BODY, &body, now, now + Duration::from_secs(2), &cancel)
        .is_err());
    let report = a.poll(1000, now, 8)?;
    assert_eq!(report.data_sent, 0);
    assert_eq!(report.completed.len(), 1);
    assert_eq!(report.completed[0].state, ObjectSendState::Cancelled);
    assert!(Arc::ptr_eq(&report.completed[0].body, &body));
    assert_eq!(a.outbound_usage(BODY)?, (0, 0));
    assert!(a.cancellations.is_empty());
    assert_eq!(a.session_id(0), session);
    assert!(a.poll(1000, now, 8)?.completed.is_empty());

    // A different lane completes over the SAME connection. Remote partial
    // assembly is still bounded by its original TTL, not erased by local cancel.
    let next = message(CONTROL, 82, 600)?;
    assert!(a.try_send(0, CONTROL, &next, now)?);
    let mut received = false;
    let mut delivered = false;
    while !received || !delivered {
        ensure!(
            now.elapsed() < Duration::from_secs(2),
            "healthy send stalled after cancellation"
        );
        let t = Instant::now();
        for done in a.poll(1000, t, 8)?.completed {
            assert_eq!(done.state, ObjectSendState::Delivered);
            assert!(Arc::ptr_eq(&done.body, &next));
            delivered = true;
        }
        let _ = b.poll(1000, t, 8)?;
        if let Some(item) = b.poll_complete(CONTROL, t, 8)? {
            assert_eq!(item.id, next.id());
            received = true;
        }
        std::thread::yield_now();
    }
    assert_eq!(a.session_id(0), session);
    Ok(())
}

struct BlockedFeedbackIo {
    socket: UdpSocket,
    blocked: AtomicBool,
    cancellations: std::sync::atomic::AtomicUsize,
    stepped: AtomicBool,
    partial_writes: AtomicBool,
    failed: AtomicBool,
    write: std::sync::Mutex<Option<(Vec<u8>, crate::novorudp::carrier::NativeRoute, bool)>>,
}
impl crate::novorudp::carrier::NativeDatagramIo for BlockedFeedbackIo {
    fn deferred_writes(&self) -> bool {
        true
    }
    fn cancel_send(
        &self,
        _: &[u8],
        _: crate::novorudp::carrier::NativeRoute,
    ) -> std::io::Result<usize> {
        self.cancellations.fetch_add(1, Ordering::Relaxed);
        if self.partial_writes.load(Ordering::Relaxed)
            && self
                .write
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|(_, _, complete)| !complete)
        {
            // Match CarrierIo: cancelling an in-progress framed write would
            // leave an incomplete frame on a reliable stream.
            self.failed.store(true, Ordering::Relaxed);
            return Err(std::io::ErrorKind::ConnectionAborted.into());
        }
        Ok(0)
    }
    fn try_receive(
        &self,
        bytes: &mut [u8],
    ) -> std::io::Result<Option<(usize, crate::novorudp::carrier::NativeRoute)>> {
        if self.failed.load(Ordering::Relaxed) {
            return Err(std::io::ErrorKind::ConnectionAborted.into());
        }
        self.socket
            .recv_from(bytes)
            .map(|(n, address)| Some((n, address.into())))
    }
    fn try_send(
        &self,
        bytes: &[u8],
        route: crate::novorudp::carrier::NativeRoute,
    ) -> std::io::Result<usize> {
        if self.failed.load(Ordering::Relaxed) {
            return Err(std::io::ErrorKind::ConnectionAborted.into());
        }
        if self.blocked.load(Ordering::Relaxed) {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        // This fixture establishes a real UDP handshake before modelling the
        // bound carrier's data writer. UDP Finished retransmissions are not
        // writes on that reliable-data slot.
        if self.stepped.load(Ordering::Relaxed) && !bytes.starts_with(b"NVHSHK01") {
            let mut slot = self.write.lock().unwrap();
            if let Some((old, old_route, complete)) = slot.as_ref() {
                if old != bytes || *old_route != route {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "fixture deferred slot changed before completion",
                    ));
                }
                if !complete {
                    return Err(std::io::ErrorKind::WouldBlock.into());
                }
                *slot = None;
                return Ok(bytes.len());
            }
            *slot = Some((bytes.to_vec(), route, false));
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        self.socket.try_send(bytes, route)
    }
}
fn blocked_feedback_pair() -> Result<(NativeObjectLink, NativeObjectLink, Arc<BlockedFeedbackIo>)> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_nonblocking(true)?;
    let address = socket.local_addr()?;
    let io = Arc::new(BlockedFeedbackIo {
        socket,
        blocked: false.into(),
        cancellations: 0.into(),
        stepped: false.into(),
        partial_writes: false.into(),
        failed: false.into(),
        write: std::sync::Mutex::new(None),
    });
    let server_key = SigningKey::from_bytes(&[81; 32]);
    let budget = || NativeHandshakeBudget {
        lifetime: Duration::from_secs(2),
        retry: Duration::from_millis(10),
    };
    let mut server = NativeObjectLink::new(
        NativeEndpoint::listen_clients_with_io(
            io.clone(),
            server_key.clone(),
            1,
            vec![CONTROL, BODY],
            budget(),
        )?,
        configs(),
        policy(),
    )?;
    let mut client = NativeObjectLink::new(
        NativeEndpoint::connect_client(
            UdpSocket::bind("127.0.0.1:0")?,
            SigningKey::from_bytes(&[82; 32]),
            NativePeer {
                key: server_key.verifying_key().to_bytes(),
                address,
            },
            budget(),
            1000,
            Instant::now(),
        )?,
        configs(),
        policy(),
    )?;
    let start = Instant::now();
    while client.session_id(0).is_none() || server.session_id(0).is_none() {
        ensure!(
            start.elapsed() < Duration::from_secs(2),
            "feedback fixture handshake timeout"
        );
        let _ = client.poll(1000, Instant::now(), 8)?;
        let _ = server.poll(1000, Instant::now(), 8)?;
        std::thread::yield_now();
    }
    io.blocked.store(true, Ordering::Relaxed);
    Ok((client, server, io))
}
#[test]
fn blocked_feedback_preserves_frame_then_emits_newer_snapshot() -> Result<()> {
    let (mut client, mut server, io) = blocked_feedback_pair()?;
    let now = Instant::now();
    let body = message(CONTROL, 6, 1000)?;
    assert!(client.try_send(0, CONTROL, &body, now)?);
    assert_eq!(client.poll(1000, now, 1)?.data_sent, 1);
    let _ = server.poll(1000, now, 1)?;
    let key = (0, body.id());
    let original = server.lanes[&CONTROL].pending_feedback[&key].frame.clone();
    assert_eq!(client.poll(1000, now, 1)?.data_sent, 1);
    let _ = server.poll(1000, now, 1)?;
    assert_eq!(
        server.lanes[&CONTROL].pending_feedback[&key].frame,
        original
    );
    assert!(server.lanes[&CONTROL].pending_feedback[&key].dirty);
    io.blocked.store(false, Ordering::Relaxed);
    // CONTROL/BODY alternate; bounded polls eventually service CONTROL again.
    let epoch = server.epoch;
    let mut feedback = 0;
    for _ in 0..4 {
        feedback += server.poll(1000, now, 1)?.feedback_sent;
    }
    assert_eq!(feedback, 2);
    assert_eq!(server.epoch, epoch + 1);
    assert!(server.lanes[&CONTROL].pending_feedback.is_empty());
    assert!(server.lanes[&CONTROL].queued.is_empty());
    Ok(())
}
#[test]
fn blocked_feedback_expires_with_receiver_state() -> Result<()> {
    let (mut client, mut server, io) = blocked_feedback_pair()?;
    let now = Instant::now();
    let body = message(CONTROL, 7, 1000)?;
    assert!(client.try_send(0, CONTROL, &body, now)?);
    let _ = client.poll(1000, now, 1)?;
    let _ = server.poll(1000, now, 2)?;
    assert_eq!(server.lanes[&CONTROL].pending_feedback.len(), 1);
    let report = server.poll(1000, now + Duration::from_secs(6), 2)?;
    assert_eq!(report.feedback_sent, 0);
    assert_eq!(io.cancellations.load(Ordering::Relaxed), 1);
    assert!(server.lanes[&CONTROL].pending_feedback.is_empty());
    assert!(server.lanes[&CONTROL].queued.is_empty());
    Ok(())
}

#[test]
fn cancelled_or_expired_object_retires_deferred_write_before_releasing_quota() -> Result<()> {
    for cancel in [true, false] {
        let (_client, mut server, io) = blocked_feedback_pair()?;
        let now = Instant::now();
        let body = message(CONTROL, 9, 1000)?;
        assert!(server.try_send(0, CONTROL, &body, now)?);
        assert_eq!(server.poll(1000, now, 2)?.data_sent, 0);
        assert_eq!(server.outbound_usage(CONTROL)?, (1, 1000));
        if cancel {
            assert!(server.cancel_send(0, CONTROL, &body));
        }
        let next = if cancel {
            now
        } else {
            now + Duration::from_secs(3)
        };
        let report = server.poll(1000, next, 2)?;
        assert_eq!(report.completed.len(), 1);
        assert_eq!(
            report.completed[0].state,
            if cancel {
                ObjectSendState::Cancelled
            } else {
                ObjectSendState::Expired
            }
        );
        assert!(Arc::ptr_eq(&report.completed[0].body, &body));
        assert_eq!(io.cancellations.load(Ordering::Relaxed), 1);
        assert_eq!(server.outbound_usage(CONTROL)?, (0, 0));
    }
    Ok(())
}

#[test]
fn deferred_slot_alternates_sustained_feedback_and_outbound_data() -> Result<()> {
    let (mut client, mut server, io) = blocked_feedback_pair()?;
    let now = Instant::now();
    let inbound = message(CONTROL, 11, 1000)?;
    assert!(client.try_send(0, CONTROL, &inbound, now)?);
    // Keep one actual incomplete object, generating valid repeated missing
    // snapshots as duplicate fragments would. Do not finish that input stream.
    assert_eq!(client.poll(1000, now, 1)?.data_sent, 1);
    io.stepped.store(true, Ordering::Relaxed);
    io.blocked.store(false, Ordering::Relaxed);
    let outbound = message(BODY, 12, 8000)?;
    assert!(server.try_send(0, BODY, &outbound, now)?);
    let mut feedback = 0;
    let mut data = 0;
    for _ in 0..16 {
        server
            .lanes
            .get_mut(&CONTROL)
            .unwrap()
            .feedback(0, inbound.id());
        let report = server.poll(1000, now, 2)?;
        feedback += report.feedback_sent;
        data += report.data_sent;
        let mut write = io.write.lock().unwrap();
        if let Some((bytes, route, complete)) = write.as_mut() {
            if !*complete {
                use crate::novorudp::carrier::NativeDatagramIo;
                assert_eq!(io.socket.try_send(bytes, *route)?, bytes.len());
                *complete = true;
            }
        }
    }
    // Neither a continuous stream of fresh ACK hints nor a large outgoing body
    // can reserve the sole deferred write slot forever ahead of the other.
    assert!(feedback >= 5, "feedback starved: {feedback}");
    assert!(data >= 5, "outbound data starved: {data}");
    assert!(data < outbound.frame_count());
    Ok(())
}

enum PartialRepairEnd {
    Drain,
    Cancel,
    Deadline,
}

fn verified_partial_repair_completion(end: PartialRepairEnd) -> Result<()> {
    let (mut receiver, mut sender, io) =
        blocked_feedback_pair().context("partial repair fixture handshake")?;
    let now = Instant::now();
    let deadline = now + Duration::from_secs(1);
    let cancellation = NativeSendCancellation::default();
    let session = sender.session_id(0);
    let body = message(BODY, 21, 1000)?;
    io.blocked.store(false, Ordering::Relaxed);
    assert!(sender
        .try_send_cancellable_until(0, BODY, &body, now, deadline, &cancellation)
        .context("queue initial object")?);
    // Send all initial fragments, retaining the real receiver's ACK until a
    // repair is in progress. The fixture uses authenticated native datagrams;
    // no fabricated completion is injected into the sender.
    let mut sent = 0;
    for _ in 0..body.frame_count() + 1 {
        sent += sender
            .poll(1000, now, 8)
            .context("transmit initial object before any receiver ACK")?
            .data_sent;
    }
    assert_eq!(sent, body.frame_count());
    io.stepped.store(true, Ordering::Relaxed);
    io.partial_writes.store(true, Ordering::Relaxed);
    let later = now + Duration::from_millis(11);
    assert_eq!(
        sender
            .poll(1011, later, 8)
            .context("queue repair in Writing state")?
            .data_sent,
        0
    );
    assert!(io
        .write
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|(_, _, complete)| !complete));

    let _ = receiver
        .poll(1011, later, 8)
        .context("receive initial fragments before complete ACK")?;
    let assembled = receiver
        .poll_complete(BODY, later, 8)
        .context("verify receiver object")?
        .context("receiver did not verify the initial object")?;
    assert_eq!(assembled.id, body.id());
    // poll_complete performs full-object verification before this real ACK.
    assert!(
        receiver
            .poll(1011, later, 8)
            .context("send verified receiver completion ACK")?
            .feedback_sent
            > 0
    );
    let report = sender
        .poll(1011, later, 8)
        .context("verified completion aborted a partial repair frame")?;
    assert!(
        report.completed.is_empty(),
        "release quota only after drain"
    );
    assert!(!io.failed.load(Ordering::Relaxed));
    assert_eq!(io.cancellations.load(Ordering::Relaxed), 0);
    assert_eq!(sender.outbound_usage(BODY)?, (1, 1000));

    if !matches!(end, PartialRepairEnd::Drain) {
        let boundary = match end {
            PartialRepairEnd::Cancel => {
                cancellation.cancel();
                later
            }
            // Exact caller deadline, shorter than the two-second send policy.
            PartialRepairEnd::Deadline => deadline,
            PartialRepairEnd::Drain => unreachable!(),
        };
        let error = sender
            .poll(2000, boundary, 8)
            .err()
            .context("revoked/expired partial frame continued after completion ACK")?;
        assert!(error.is::<crate::novorudp::carrier::NativeCarrierIoFailure>());
        assert!(io.failed.load(Ordering::Relaxed));
        assert_eq!(io.cancellations.load(Ordering::Relaxed), 1);
        assert_eq!(sender.outbound_usage(BODY)?, (1, 1000));
        assert!(io
            .write
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|(_, _, complete)| !complete));
        return Ok(());
    }

    // Existing carrier progress completes the same frame, without replacing
    // the owner, session, object or its original deadline.
    {
        use crate::novorudp::carrier::NativeDatagramIo;
        let mut write = io.write.lock().unwrap();
        let (bytes, route, complete) = write.as_mut().context("repair lost before drain")?;
        assert_eq!(
            io.socket
                .try_send(bytes, *route)
                .context("finish original partial repair frame")?,
            bytes.len()
        );
        *complete = true;
    }
    let report = sender
        .poll(1011, later, 8)
        .context("observe drained repair and release Delivered object")?;
    assert_eq!(report.completed.len(), 1);
    assert_eq!(report.completed[0].state, ObjectSendState::Delivered);
    assert!(Arc::ptr_eq(&report.completed[0].body, &body));
    assert_eq!(sender.outbound_usage(BODY)?, (0, 0));
    assert_eq!(sender.session_id(0), session);
    assert!(!io.failed.load(Ordering::Relaxed));
    Ok(())
}

#[test]
fn verified_completion_drains_partial_repair_without_aborting_carrier() -> Result<()> {
    verified_partial_repair_completion(PartialRepairEnd::Drain)
}

#[test]
fn verified_completion_partial_repair_stops_at_original_deadline() -> Result<()> {
    verified_partial_repair_completion(PartialRepairEnd::Deadline)
}

#[test]
fn verified_completion_partial_repair_stops_after_owner_cancellation() -> Result<()> {
    verified_partial_repair_completion(PartialRepairEnd::Cancel)
}
