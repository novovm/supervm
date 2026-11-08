// Scheduling regression fixtures with real signed/Finished channel state and
// bounded in-memory carrier IO. Not a QUIC throughput or mobile acceptance test.
use super::{binding, finished_pair, NOW};
use crate::novorudp::{
    carrier::{NativeDatagramIo, NativeRoute, VerifiedCarrierRoute},
    datagram::NativeUdpSession,
    endpoint::NativeEndpoint,
    fragments::{FragmentProfile, OutgoingMessage, ReassemblyLimits, UDP_CHUNK_BYTES},
    object_ack::decode_object_ack,
    object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
    object_sender::ObjectSendState,
    NovoRudpTransportFrameKindV0 as Kind, NovoRudpTransportFrameV0 as Frame,
};
use anyhow::Result;
use std::{
    collections::VecDeque,
    io,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn udp_budget() -> crate::novorudp::endpoint::NativeHandshakeBudget {
    crate::novorudp::endpoint::NativeHandshakeBudget {
        lifetime: Duration::from_secs(2),
        retry: Duration::from_millis(10),
    }
}
fn udp_peer(seed: u8, address: SocketAddr) -> crate::novorudp::endpoint::NativePeer {
    crate::novorudp::endpoint::NativePeer {
        key: super::public(seed),
        address,
    }
}
fn configured_udp(
    socket: Arc<UdpSocket>,
    seed: u8,
    peers: Vec<crate::novorudp::endpoint::NativePeer>,
    now: Instant,
) -> Result<NativeEndpoint> {
    let mut endpoint =
        NativeEndpoint::bind_shared(socket, super::key(seed), peers, udp_budget(), NOW, now)?;
    endpoint.enable_configured_reconnect_v1()?;
    Ok(endpoint)
}
fn owned_poll(
    link: &mut NativeObjectLink,
    socket: &UdpSocket,
    now: Instant,
) -> Result<crate::novorudp::object_link::NativeLinkPoll> {
    link.poll_with_owned_receiver(NOW, now, 64, || {
        let mut bytes = vec![0; 65536];
        let (length, source) = socket.recv_from(&mut bytes)?;
        bytes.truncate(length);
        Ok(Some((bytes, source)))
    })
}

#[test]
fn udp_owner_initial_bound_fallback_keeps_other_udp_peer_clients_and_global_budget() -> Result<()> {
    let now = Instant::now();
    let a_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let old_b_socket = UdpSocket::bind("127.0.0.1:0")?;
    let c_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let mut endpoint = configured_udp(
        a_socket.clone(),
        1,
        vec![
            udp_peer(2, old_b_socket.local_addr()?),
            udp_peer(3, c_socket.local_addr()?),
        ],
        now,
    )?;
    endpoint.allow_clients(super::key(1), 1, vec![STREAM], udp_budget())?;
    endpoint.enable_client_reconnect_v1()?;
    endpoint.set_wire_budget_v1(16 * 1024 * 1024, now)?;
    let mut a = link(endpoint)?;
    assert!(a.session_id(0).is_none());
    let (fallback, b) = bound_pair_to(2)?;
    let report = a.replace_bound_carrier_v1(0, fallback, now)?;
    assert!(
        report.session_changes.is_empty(),
        "no invented previous session"
    );
    assert_eq!(a.local_addr()?, a_socket.local_addr()?);
    assert_eq!(a.peer_count(), 2);
    let mut b = link(b)?;
    let mut c = link(configured_udp(
        c_socket,
        3,
        vec![udp_peer(1, a.local_addr()?)],
        now,
    )?)?;
    let mut d = link(NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        super::key(4),
        udp_peer(1, a.local_addr()?),
        udp_budget(),
        NOW,
        now,
    )?)?;
    let setup = [body(1, 91)?, body(1, 92)?];
    let mut admitted = [false; 2];
    let mut received = [false; 2];
    let mut done = 0;
    for _ in 0..3000 {
        for (i, remote) in [&mut c, &mut d].into_iter().enumerate() {
            if !admitted[i] {
                admitted[i] = remote.try_send(0, STREAM, &setup[i], now)?;
            }
            done += remote.poll(NOW, now, 64)?.completed.len();
        }
        let report = owned_poll(&mut a, &a_socket, now)?;
        assert!(report.completed.is_empty(), "no outgoing setup work on A");
        while let Some(message) = a.poll_complete(STREAM, now, 64)? {
            let i = if message.id == setup[0].id() { 0 } else { 1 };
            assert_eq!(message.id, setup[i].id());
            assert!(!received[i]);
            received[i] = true;
        }
        if received == [true; 2] && done == 2 {
            break;
        }
    }
    assert_eq!(
        received, [true; 2],
        "original configured UDP and open client still deliver"
    );
    assert_eq!(done, 2);
    assert_eq!(a.peer_count(), 3, "client retains its own dynamic handle");
    let work = [body(65, 81)?, body(2, 82)?, body(1, 83)?];
    assert!(a.try_send(0, STREAM, &work[0], now)?);
    assert!(a.try_send(1, STREAM, &work[1], now)?);
    assert!(
        !a.try_send(0, STREAM, &work[2], now)?,
        "same global two-object budget"
    );
    assert!(b.try_send(0, STREAM, &work[2], now)?);
    let mut received = [false; 3];
    let mut completed = 0;
    for _ in 0..5000 {
        for (i, target) in [&mut a, &mut b, &mut c].into_iter().enumerate() {
            let report = if i == 0 {
                owned_poll(target, &a_socket, now)?
            } else {
                target.poll(NOW, now, 64)?
            };
            for item in report.completed {
                assert_eq!(item.state, ObjectSendState::Delivered);
                completed += 1;
            }
            if let Some(message) = target.poll_complete(STREAM, now, 64)? {
                assert!(!received[i]);
                received[i] = true;
                assert_eq!(message.id, work[[2, 0, 1][i]].id());
            }
        }
        if received == [true; 3] && completed == 3 {
            assert!(a.wire_usage_v1(now).unwrap().sent_bytes > 0);
            return Ok(());
        }
    }
    anyhow::bail!("mixed original owner did not complete: {received:?}/{completed}")
}

#[test]
fn configured_carrier_rejects_wrong_identity_streams_and_returns_to_original_udp() -> Result<()> {
    use crate::novorudp::diagnostics::NativeTransferDiagnosticsV1;
    let now = Instant::now();
    let a_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let b_socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let mut a = link(configured_udp(
        a_socket.clone(),
        1,
        vec![udp_peer(2, b_socket.local_addr()?)],
        now,
    )?)?;
    let mut b = link(configured_udp(
        b_socket.clone(),
        2,
        vec![udp_peer(1, a_socket.local_addr()?)],
        now,
    )?)?;
    let (ab, ba) = bound_pair_to(2)?;
    let a_observer = Arc::new(NativeTransferDiagnosticsV1::new());
    let b_observer = Arc::new(NativeTransferDiagnosticsV1::new());
    a.set_diagnostics_v1(a_observer.clone());
    b.set_diagnostics_v1(b_observer.clone());
    let report = a.replace_bound_carrier_v1(0, ab, now)?;
    assert!(report.session_changes.is_empty() && report.completed.is_empty());
    let report = b.replace_bound_carrier_v1(0, ba, now)?;
    assert!(report.session_changes.is_empty() && report.completed.is_empty());
    let previous = a.session_id(0).unwrap();
    let guard = a.session_validity(0)?;
    let (wrong, _) = bound_pair_to_generation(3, 34)?;
    assert!(a.replace_bound_carrier_v1(0, wrong, now).is_err());
    let (channel, _) = finished_pair();
    let (io, _) = io_pair();
    let wrong_stream =
        NativeEndpoint::from_bound_channel(io, channel, &binding(true, 7), vec![STREAM + 1])?;
    assert!(a.replace_bound_carrier_v1(0, wrong_stream, now).is_err());
    assert_eq!(a.session_id(0), Some(previous));
    assert!(guard.is_current());
    let old = body(65, 53)?;
    assert!(a.try_send(0, STREAM, &old, now)?);
    a.request_udp_recovery_v1(0, NOW, now)?;
    b.request_udp_recovery_v1(0, NOW, now)?;
    assert_eq!(
        a.session_id(0),
        Some(previous),
        "a request is not authentication"
    );
    assert_eq!(b.session_id(0), Some(previous));
    let proof = body(1, 54)?;
    let mut sent = [false; 2];
    let mut received = [false; 2];
    let mut completed = [false; 2];
    let mut retired = false;
    for _ in 0..3000 {
        for (i, target) in [&mut a, &mut b].into_iter().enumerate() {
            if matches!(target.peer_route(0)?, NativeRoute::Carrier(_)) {
                target.request_udp_recovery_v1(0, NOW, now)?;
            }
            let report = if i == 0 {
                owned_poll(target, &a_socket, now)?
            } else {
                owned_poll(target, &b_socket, now)?
            };
            retired |= report.completed.iter().any(|item| {
                item.body.id() == old.id() && item.state == ObjectSendState::SessionReplaced
            });
            for item in report
                .completed
                .iter()
                .filter(|item| item.body.id() == proof.id())
            {
                assert!(!completed[i], "recovered UDP send completed twice");
                assert_eq!(item.state, ObjectSendState::Delivered);
                completed[i] = true;
            }
            if target
                .session_id(0)
                .is_some_and(|session| session != previous)
            {
                if !sent[i] {
                    sent[i] = target.try_send(0, STREAM, &proof, now)?;
                }
                if let Some(message) = target.poll_complete(STREAM, now, 64)? {
                    assert!(!received[i], "recovered UDP object delivered twice");
                    assert_eq!(message.id, proof.id());
                    received[i] = true;
                }
            }
        }
        if a.session_id(0) == b.session_id(0)
            && a.session_id(0) != Some(previous)
            && received == [true; 2]
            && completed == [true; 2]
        {
            assert!(!guard.is_current());
            assert!(retired);
            assert_eq!(a.local_addr()?, a_socket.local_addr()?);
            assert_eq!(a.peer_id(0)?, super::key(2).peer_id());
            assert_eq!(a.peer_count(), 1);
            assert_eq!(b.peer_count(), 1);
            let mut started = 0;
            let mut passive = 0;
            for observer in [&a_observer, &b_observer] {
                let snapshot = observer.snapshot().unwrap();
                let row = &snapshot.udp_recovery[0];
                let count = |name| row.counts[snapshot.udp_recovery_event_names.iter().position(|n| *n == name).unwrap()];
                started += count("attempt_started");
                passive += count("passive_role");
                assert_eq!(count("udp_route_activated"), 1);
                assert!(count("datagram_sent") > 0);
                assert_eq!(row.active_session, a.session_id(0));
            }
            assert_eq!(started, 1, "one elected initiator, not two parallel handshakes");
            assert!(passive > 0);
            return Ok(());
        }
    }
    anyhow::bail!(
        "original UDP recovery did not authenticate and deliver: {received:?}/{completed:?}"
    )
}

#[test]
fn udp_recovery_observer_tracks_timed_retry_without_replacing_healthy_carrier() -> Result<()> {
    use crate::novorudp::diagnostics::NativeTransferDiagnosticsV1;
    let now = Instant::now();
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let remote_socket = UdpSocket::bind("127.0.0.1:0")?;
    let local = if super::key(1).peer_id() < super::key(2).peer_id() { 1 } else { 2 };
    let remote = if local == 1 { 2 } else { 1 };
    let mut target = link(configured_udp(socket, local, vec![udp_peer(remote, remote_socket.local_addr()?)], now)?)?;
    let (ab, ba) = bound_pair_to(2)?;
    let no_handover = |report: crate::novorudp::object_link::NativeLinkPoll| {
        assert!(report.session_changes.is_empty() && report.completed.is_empty());
    };
    no_handover(target.replace_bound_carrier_v1(0, if local == 1 { ab } else { ba }, now)?);
    let previous = target.session_id(0);
    let observer = Arc::new(NativeTransferDiagnosticsV1::new());
    target.set_diagnostics_v1(observer.clone());
    target.request_udp_recovery_v1(0, NOW, now)?;
    no_handover(target.poll(NOW, now, 64)?);
    target.request_udp_recovery_v1(0, NOW + 1, now + Duration::from_millis(1))?;
    no_handover(target.poll(NOW + 1, now + Duration::from_millis(1), 64)?);
    // No remote UDP poll: the first original attempt really reaches its deadline.
    let later = now + Duration::from_secs(3);
    no_handover(target.poll(NOW + 3000, later, 64)?);
    target.request_udp_recovery_v1(0, NOW + 3000, later)?;
    no_handover(target.poll(NOW + 3000, later, 64)?);
    assert_eq!(target.session_id(0), previous, "no authentication, no invented handover");
    assert!(matches!(target.peer_route(0)?, NativeRoute::Carrier(_)));
    let snapshot = observer.snapshot().unwrap();
    let count = |name| snapshot.udp_recovery[0].counts[snapshot.udp_recovery_event_names.iter().position(|n| *n == name).unwrap()];
    assert_eq!(count("attempt_started"), 2);
    assert_eq!(count("active_attempt"), 1);
    assert_eq!(count("datagram_sent"), 2);
    assert_eq!(count("attempt_deadline"), 1);
    assert_eq!(count("udp_route_activated"), 0);
    Ok(())
}

struct HeldCarrier {
    route: NativeRoute,
    uncertain: bool,
}

struct FailedCarrier;
impl NativeDatagramIo for FailedCarrier {
    fn deferred_writes(&self) -> bool {
        true
    }
    fn try_receive(&self, _: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        Err(io::ErrorKind::WouldBlock.into())
    }
    fn try_send(&self, _: &[u8], _: NativeRoute) -> io::Result<usize> {
        Err(io::ErrorKind::ConnectionAborted.into())
    }
    fn cancel_send(&self, _: &[u8], _: NativeRoute) -> io::Result<usize> {
        Err(io::ErrorKind::ConnectionAborted.into())
    }
}

#[test]
fn failed_configured_carrier_retains_work_until_authenticated_replacement_or_original_expiry(
) -> Result<()> {
    for expire in [false, true] {
        let now = Instant::now();
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
        let remote = UdpSocket::bind("127.0.0.1:0")?;
        let mut endpoint = configured_udp(socket, 1, vec![udp_peer(2, remote.local_addr()?)], now)?;
        endpoint.set_wire_budget_v1(1024 * 1024, now)?;
        let mut original = link(endpoint)?;
        let (channel, _) = finished_pair();
        let failed = NativeEndpoint::from_bound_channel(
            Arc::new(FailedCarrier),
            channel,
            &binding(true, 7),
            vec![STREAM],
        )?;
        let adoption = original.replace_bound_carrier_v1(0, failed, now)?;
        assert!(adoption.completed.is_empty());
        let session = original.session_id(0);
        let validity = original.session_validity(0)?;
        let work = body(2, 98)?;
        assert!(original.try_send(0, STREAM, &work, now)?);
        let report = original.poll(NOW, now, 64)?;
        assert!(report.send_failures > 0);
        assert!(
            report.completed.is_empty(),
            "failed IO is neither delivery nor a replacement session"
        );
        assert_eq!(original.session_id(0), session);
        assert!(validity.is_current());
        assert_eq!(original.wire_usage_v1(now).unwrap().sent_bytes, 0);
        if expire {
            let report = original.poll(NOW, now + Duration::from_secs(6), 64)?;
            assert_eq!(report.completed.len(), 1);
            assert_eq!(report.completed[0].state, ObjectSendState::Expired);
        } else {
            let (fresh, _) = bound_pair_to_generation(2, 43)?;
            let report = original.replace_bound_carrier_v1(0, fresh, now)?;
            assert_eq!(report.completed.len(), 1);
            assert_eq!(report.completed[0].state, ObjectSendState::SessionReplaced);
            assert!(!validity.is_current());
        }
    }
    // A stand-alone reliable stream has no configured reconnect owner. Its
    // terminal failure must remain an error, never generic UDP noise.
    let (channel, _) = finished_pair();
    let mut standalone = link(NativeEndpoint::from_bound_channel(
        Arc::new(FailedCarrier),
        channel,
        &binding(true, 7),
        vec![STREAM],
    )?)?;
    let now = Instant::now();
    assert!(standalone.try_send(0, STREAM, &body(1, 99)?, now)?);
    assert!(standalone.poll(NOW, now, 64).is_err());
    Ok(())
}
impl NativeDatagramIo for HeldCarrier {
    fn deferred_writes(&self) -> bool {
        true
    }
    fn try_receive(&self, _: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        Err(io::ErrorKind::WouldBlock.into())
    }
    fn try_send(&self, _: &[u8], route: NativeRoute) -> io::Result<usize> {
        assert_eq!(route, self.route);
        Err(io::ErrorKind::WouldBlock.into())
    }
    fn cancel_send(&self, _: &[u8], route: NativeRoute) -> io::Result<usize> {
        assert_eq!(route, self.route);
        if self.uncertain {
            Err(io::ErrorKind::ConnectionAborted.into())
        } else {
            Ok(0)
        }
    }
}

#[test]
fn bound_handover_resolves_original_wire_reservation_without_false_delivery() -> Result<()> {
    for uncertain in [false, true] {
        let now = Instant::now();
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
        let remote = UdpSocket::bind("127.0.0.1:0")?;
        let mut endpoint = configured_udp(socket, 1, vec![udp_peer(2, remote.local_addr()?)], now)?;
        endpoint.set_wire_budget_v1(1024 * 1024, now)?;
        let mut a = link(endpoint)?;
        let (channel, _) = finished_pair();
        let binding = binding(true, 7);
        let io = Arc::new(HeldCarrier {
            route: NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(&binding)),
            uncertain,
        });
        let bound = NativeEndpoint::from_bound_channel(io, channel, &binding, vec![STREAM])?;
        let report = a.replace_bound_carrier_v1(0, bound, now)?;
        assert!(report.session_changes.is_empty() && report.completed.is_empty());
        let object = body(2, 45)?;
        assert!(a.try_send(0, STREAM, &object, now)?);
        assert!(a.poll(NOW, now, 64)?.completed.is_empty());
        let before = a.wire_usage_v1(now).unwrap();
        assert!(before.reserved_send_bytes > 0);
        let (fresh, _) = bound_pair_to_generation(2, 41)?;
        let report = a.replace_bound_carrier_v1(0, fresh, now)?;
        assert_eq!(report.completed.len(), 1);
        assert_eq!(report.completed[0].state, ObjectSendState::SessionReplaced);
        let after = a.wire_usage_v1(now).unwrap();
        assert_eq!(
            after.reserved_send_bytes, 0,
            "retired reservations cannot leak"
        );
        assert_eq!(
            after.sent_bytes, before.sent_bytes,
            "no invented actual write"
        );
        assert_eq!(
            after.uncertain_send_bytes - before.uncertain_send_bytes,
            if uncertain {
                before.reserved_send_bytes
            } else {
                0
            }
        );
    }
    Ok(())
}

const STREAM: u64 = 11;
const DOMAIN: [u8; 32] = [41; 32];
const MAX: usize = 2 * 1024 * 1024;
type Queue = Arc<Mutex<VecDeque<Vec<u8>>>>;
struct MemoryCarrier {
    incoming: Queue,
    outgoing: Queue,
    route: NativeRoute,
}
impl NativeDatagramIo for MemoryCarrier {
    fn try_receive(&self, dst: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        let Some(bytes) = self.incoming.lock().unwrap().pop_front() else {
            return Err(io::ErrorKind::WouldBlock.into());
        };
        assert!(bytes.len() <= dst.len());
        dst[..bytes.len()].copy_from_slice(&bytes);
        Ok(Some((bytes.len(), self.route)))
    }
    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        assert_eq!(route, self.route);
        let mut queue = self.outgoing.lock().unwrap();
        if queue.len() >= 128 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        queue.push_back(bytes.to_vec());
        Ok(bytes.len())
    }
}
fn io_pair() -> (Arc<MemoryCarrier>, Arc<MemoryCarrier>) {
    io_pair_for(&binding(true, 7), &binding(false, 7))
}
fn io_pair_for(
    left: &super::VerifiedTransportBinding,
    right: &super::VerifiedTransportBinding,
) -> (Arc<MemoryCarrier>, Arc<MemoryCarrier>) {
    let a = Queue::default();
    let b = Queue::default();
    (
        Arc::new(MemoryCarrier {
            incoming: a.clone(),
            outgoing: b.clone(),
            route: NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(left)),
        }),
        Arc::new(MemoryCarrier {
            incoming: b,
            outgoing: a,
            route: NativeRoute::Carrier(VerifiedCarrierRoute::from_binding(right)),
        }),
    )
}

fn bound_pair_to(remote: u8) -> Result<(NativeEndpoint, NativeEndpoint)> {
    bound_pair_to_generation(remote, remote)
}
fn bound_pair_to_generation(
    remote: u8,
    generation: u8,
) -> Result<(NativeEndpoint, NativeEndpoint)> {
    use super::{
        BoundHandshakeInitiatorV1, BoundHandshakeResponderV1, HandshakeReplayCacheV1,
        VerifiedTransportBinding, BOUND_TRANSPORT_ALPN_V1, TTL,
    };
    let binding = |initiator| {
        VerifiedTransportBinding::from_authenticated_transport(
            super::carrier_public(if initiator { 11 } else { 20 + remote }),
            super::carrier_public(if initiator { 20 + remote } else { 11 }),
            [generation; 32],
            BOUND_TRANSPORT_ALPN_V1,
            initiator,
        )
    };
    let left = binding(true)?;
    let right = binding(false)?;
    let initiator = BoundHandshakeInitiatorV1::start(
        &super::key(1),
        super::public(remote),
        binding(true)?,
        NOW,
        TTL,
    )?;
    let responder = BoundHandshakeResponderV1::respond(
        initiator.offer(),
        &super::key(remote),
        super::public(1),
        binding(false)?,
        NOW,
        TTL,
        &mut HandshakeReplayCacheV1::new(8),
    )?;
    let mut a = initiator.complete(
        responder.response(),
        NOW,
        &mut HandshakeReplayCacheV1::new(8),
    )?;
    let mut b = responder.into_pending();
    let a_finished = a.make_finished(NOW)?;
    let b_finished = b.make_finished(NOW)?;
    a.verify_finished(&b_finished, NOW)?;
    b.verify_finished(&a_finished, NOW)?;
    let (a_io, b_io) = io_pair_for(&left, &right);
    Ok((
        NativeEndpoint::from_bound_channel(a_io, a.into_channel(NOW)?, &left, vec![STREAM])?,
        NativeEndpoint::from_bound_channel(b_io, b.into_channel(NOW)?, &right, vec![STREAM])?,
    ))
}

#[test]
fn joined_carriers_deliver_both_peers_under_one_global_object_budget() -> Result<()> {
    let (ab, b) = bound_pair_to(2)?;
    let (ac, c) = bound_pair_to(3)?;
    let endpoint = NativeEndpoint::join_bound_carriers_v1(vec![ab, ac])?;
    endpoint.validate_bound_carriers_v1(&[STREAM])?;
    assert_eq!(endpoint.peer_count(), 2);
    assert_eq!(endpoint.peer_id(0)?, super::key(2).peer_id());
    assert_eq!(endpoint.peer_id(1)?, super::key(3).peer_id());
    assert!(endpoint.validate_bound_carriers_v1(&[STREAM + 1]).is_err());
    let mut a = link(endpoint)?;
    let mut b = link(b)?;
    let mut c = link(c)?;
    let now = Instant::now();
    let bodies = [body(65, 10)?, body(3, 11)?];
    assert!(a.try_send(0, STREAM, &bodies[0], now)?);
    assert!(a.try_send(1, STREAM, &bodies[1], now)?);
    // Per-peer capacity is two, but the SAME global capacity is also two.
    assert!(!a.try_send(1, STREAM, &body(1, 12)?, now)?);
    assert!(c.try_send(0, STREAM, &body(1, 13)?, now)?);
    let mut delivered = [false; 3];
    let mut completed = 0;
    for _ in 0..5000 {
        for (index, target) in [&mut a, &mut b, &mut c].into_iter().enumerate() {
            let report = target.poll(NOW, now, 1)?;
            assert_eq!(
                report.rejected + report.receive_errors + report.send_failures,
                0
            );
            completed += report.completed.len();
            if let Some(message) = target.poll_complete(STREAM, now, 64)? {
                assert!(!delivered[index], "object delivered twice");
                let expected = [13, 10, 11][index];
                assert!(message.chunks.iter().flatten().all(|v| *v == expected));
                delivered[index] = true;
            }
        }
        if delivered.iter().all(|done| *done) && completed == 3 {
            return Ok(());
        }
    }
    anyhow::bail!("joined carrier traffic did not complete: {delivered:?}, {completed}")
}

#[test]
fn joined_carriers_reject_duplicate_identity_wrong_local_and_started_sessions() -> Result<()> {
    assert!(NativeEndpoint::join_bound_carriers_v1(Vec::new()).is_err());
    let (ab, _) = bound_pair_to(2)?;
    let (duplicate, _) = bound_pair_to(2)?;
    assert!(NativeEndpoint::join_bound_carriers_v1(vec![ab, duplicate]).is_err());
    let (ab, _) = bound_pair_to(2)?;
    let (_, wrong_local) = bound_pair_to(3)?;
    assert!(NativeEndpoint::join_bound_carriers_v1(vec![ab, wrong_local]).is_err());
    let (mut ab, _) = bound_pair_to(2)?;
    ab.poll(NOW, Instant::now(), 1)?;
    assert!(NativeEndpoint::join_bound_carriers_v1(vec![ab]).is_err());
    Ok(())
}

#[test]
fn joined_carriers_replacement_retires_old_jobs_and_preserves_other_peer() -> Result<()> {
    let (ab, old_b) = bound_pair_to(2)?;
    let (ac, c) = bound_pair_to(3)?;
    let mut a = link(NativeEndpoint::join_bound_carriers_v1(vec![ab, ac])?)?;
    let mut old_b = link(old_b)?;
    let mut c = link(c)?;
    let now = Instant::now();
    let old = body(65, 21)?;
    let other = body(3, 22)?;
    assert!(a.try_send(0, STREAM, &old, now)?);
    assert!(a.try_send(1, STREAM, &other, now)?);
    let report = a.poll(NOW, now, 1)?;
    assert!(report.completed.is_empty());
    let report = old_b.poll(NOW, now, 1)?;
    assert_eq!(report.rejected, 0);
    let previous = a.session_id(0).unwrap();
    let (wrong_peer, _) = bound_pair_to_generation(4, 30)?;
    assert!(a.replace_bound_carrier_v1(0, wrong_peer, now).is_err());
    assert_eq!(a.session_id(0), Some(previous));
    let (fresh, fresh_b) = bound_pair_to_generation(2, 31)?;
    let report = a.replace_bound_carrier_v1(0, fresh, now)?;
    assert_eq!(report.session_changes.len(), 1);
    assert_eq!(report.session_changes[0].previous, previous);
    assert_ne!(report.session_changes[0].current, previous);
    assert_eq!(report.completed.len(), 1);
    assert_eq!(report.completed[0].state, ObjectSendState::SessionReplaced);
    assert_eq!(report.completed[0].body.id(), old.id());
    // Old IO is detached, not reinterpreted as the new generation. Fresh
    // messages use the released global quota while C's existing job survives.
    let fresh_body = body(2, 23)?;
    assert!(a.try_send(0, STREAM, &fresh_body, now)?);
    let mut b = link(fresh_b)?;
    let mut delivered = [false; 2];
    let mut completed = 0;
    for _ in 0..5000 {
        for (index, target) in [&mut a, &mut b, &mut c].into_iter().enumerate() {
            let report = target.poll(NOW, now, 1)?;
            assert_eq!(
                report.rejected + report.receive_errors + report.send_failures,
                0
            );
            for done in report.completed {
                assert_eq!(done.state, ObjectSendState::Delivered);
                completed += 1;
            }
            if let Some(message) = target.poll_complete(STREAM, now, 64)? {
                assert_ne!(index, 0);
                assert!(!delivered[index - 1]);
                assert!(message
                    .chunks
                    .iter()
                    .flatten()
                    .all(|v| *v == [23, 22][index - 1]));
                delivered[index - 1] = true;
            }
        }
        if delivered.iter().all(|done| *done) && completed == 2 {
            return Ok(());
        }
    }
    anyhow::bail!("replacement traffic did not complete: {delivered:?}, {completed}")
}
fn link(endpoint: NativeEndpoint) -> Result<NativeObjectLink> {
    NativeObjectLink::new(
        endpoint,
        vec![NativeLaneConfig {
            stream: STREAM,
            domain: DOMAIN,
            receive: ReassemblyLimits {
                max_message_bytes: MAX,
                messages: 2,
                bytes: MAX * 2,
                peer_messages: 2,
                peer_bytes: MAX * 2,
                ttl: Duration::from_secs(5),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: MAX * 2,
                peer_messages: 2,
                peer_bytes: MAX * 2,
            },
        }],
        NativeSendPolicy::reliable_carrier_v1(Duration::from_secs(5)),
    )
}
fn body(frames: usize, value: u8) -> Result<Arc<OutgoingMessage>> {
    Ok(Arc::new(OutgoingMessage::with_profile(
        DOMAIN,
        vec![value; frames * UDP_CHUNK_BYTES - 7],
        MAX,
        FragmentProfile::NativeUdpV2,
    )?))
}

#[test]
fn reliable_carrier_feedback_window_boundaries_and_bidirectional_completion() -> Result<()> {
    for frames in [1, 63, 64, 65, 2084] {
        let (a_channel, b_channel) = finished_pair();
        let (a_io, b_io) = io_pair();
        let mut a = link(NativeEndpoint::from_bound_channel(
            a_io,
            a_channel,
            &binding(true, 7),
            vec![STREAM],
        )?)?;
        let mut b = link(NativeEndpoint::from_bound_channel(
            b_io,
            b_channel,
            &binding(false, 7),
            vec![STREAM],
        )?)?;
        let now = Instant::now();
        let large = body(frames, 3)?;
        let small = body(1, 9)?;
        assert!(a.try_send(0, STREAM, &large, now)?);
        assert!(b.try_send(0, STREAM, &small, now)?);
        let mut done = [false; 2];
        let mut received = [false; 2];
        let mut feedback = [0; 2];
        let mut duplicates = [0; 2];
        for _ in 0..5000 {
            for (side, target) in [&mut a, &mut b].into_iter().enumerate() {
                // One-frame polls expose accidental per-frame feedback even
                // when a larger poll quantum would incidentally coalesce it.
                let report = target.poll(NOW, now, 1)?;
                assert_eq!(
                    report.rejected + report.receive_errors + report.send_failures,
                    0
                );
                feedback[side] += report.feedback_sent;
                duplicates[side] += report.duplicate_fragments;
                for completed in report.completed {
                    assert_eq!(completed.state, ObjectSendState::Delivered);
                    assert_eq!(
                        completed.body.id(),
                        if side == 0 { large.id() } else { small.id() }
                    );
                    done[side] = true;
                }
                if let Some(item) = target.poll_complete(STREAM, now, 64)? {
                    let expected = if side == 0 { &small } else { &large };
                    assert_eq!(item.id, expected.id());
                    assert_eq!(
                        item.chunks.iter().map(Vec::len).sum::<usize>(),
                        expected.byte_len()
                    );
                    assert!(item
                        .chunks
                        .iter()
                        .flatten()
                        .all(|b| *b == if side == 0 { 9 } else { 3 }));
                    assert!(!received[side], "duplicate completion");
                    received[side] = true;
                }
            }
            if done.iter().all(|v| *v) && received.iter().all(|v| *v) {
                break;
            }
        }
        assert_eq!(done, [true; 2], "sender completion for {frames} fragments");
        assert_eq!(received, [true; 2]);
        assert_eq!(
            feedback[0], 1,
            "small reverse object needs only integrity DONE"
        );
        // on_ack may retain a missing-window Repair cursor while initial Data
        // is still advancing. Its first post-initial frame can already be at
        // the receiver: that duplicate intentionally receives immediate ACK.
        // Count those observed frames instead of attributing their ACK to Data
        // scheduling. Keep a tight lossless-fixture guard against repair storms.
        assert!(
            duplicates.iter().all(|count| *count <= 2),
            "unexpected lossless repair traffic: {duplicates:?}"
        );
        assert!(
            feedback[1] <= frames / 64 + 1 + duplicates[1],
            "per-fragment ACK regression: feedback={feedback:?} duplicates={duplicates:?}"
        );
    }
    Ok(())
}

struct ManualPeer {
    sender: NativeUdpSession,
    receiver: NativeObjectLink,
    tx: Arc<MemoryCarrier>,
    now: Instant,
}
impl ManualPeer {
    fn new() -> Result<Self> {
        let (a, b) = finished_pair();
        let (tx, rx) = io_pair();
        Ok(Self {
            sender: NativeUdpSession::from_bound_channel(a, &binding(true, 7))?,
            receiver: link(NativeEndpoint::from_bound_channel(
                rx,
                b,
                &binding(false, 7),
                vec![STREAM],
            )?)?,
            tx,
            now: Instant::now(),
        })
    }
    fn send(
        &mut self,
        body: &OutgoingMessage,
        index: usize,
        kind: Kind,
        corrupt: bool,
    ) -> Result<usize> {
        let mut payload = body.frame(index)?;
        if corrupt {
            payload[80] ^= 1;
        }
        let frame = Frame::new(
            kind,
            self.sender.session_id(),
            STREAM,
            u64::from_be_bytes(body.id()[..8].try_into()?),
            index as u64,
            0,
            payload,
        );
        let bytes = self.sender.encode(&frame)?;
        self.tx.try_send(&bytes, self.tx.route)?;
        let report = self.receiver.poll(NOW, self.now, 1)?;
        assert_eq!(report.rejected, 0);
        Ok(report.feedback_sent)
    }
    fn ack(&mut self, body: &OutgoingMessage) -> Result<crate::novorudp::NovoRudpAckFrame> {
        let bytes = self
            .tx
            .incoming
            .lock()
            .unwrap()
            .pop_front()
            .expect("ACK expected");
        let frame = self.sender.open_route(self.tx.route, &bytes)?;
        decode_object_ack(
            &frame,
            self.sender.session_id(),
            STREAM,
            body.id(),
            body.frame_count() as u64,
        )
    }
}

#[test]
fn reliable_carrier_duplicate_and_repair_feedback_do_not_wait_for_window() -> Result<()> {
    let mut pair = ManualPeer::new()?;
    let body = body(3, 5)?;
    assert_eq!(pair.send(&body, 0, Kind::Data, false)?, 0);
    assert_eq!(pair.send(&body, 0, Kind::Data, false)?, 1);
    let duplicate = pair.ack(&body)?;
    assert!(!duplicate.receiver_done);
    assert_eq!(duplicate.missing_count, 2);
    // A missing earlier index can be repaired after a later index arrived.
    assert_eq!(pair.send(&body, 2, Kind::Data, false)?, 0);
    assert_eq!(pair.send(&body, 1, Kind::Repair, false)?, 1);
    let received_only = pair.ack(&body)?;
    assert_eq!(received_only.missing_count, 0);
    assert!(
        !received_only.receiver_done,
        "receipt before integrity verification"
    );
    assert!(pair.receiver.poll_complete(STREAM, pair.now, 64)?.is_some());
    assert_eq!(pair.receiver.poll(NOW, pair.now, 1)?.feedback_sent, 1);
    assert!(pair.ack(&body)?.receiver_done);
    // A fresh encrypted repair of an already-completed object re-emits DONE,
    // without handing the application the object a second time.
    assert_eq!(pair.send(&body, 2, Kind::Repair, false)?, 1);
    assert!(pair.ack(&body)?.receiver_done);
    assert!(pair.receiver.poll_complete(STREAM, pair.now, 64)?.is_none());
    Ok(())
}

#[test]
fn reliable_carrier_corrupt_complete_object_never_emits_done() -> Result<()> {
    let mut pair = ManualPeer::new()?;
    let body = body(1, 7)?;
    assert_eq!(pair.send(&body, 0, Kind::Data, true)?, 0);
    let error = pair
        .receiver
        .poll_complete(STREAM, pair.now, 64)
        .err()
        .expect("object hash must reject");
    assert!(error.to_string().contains("hash mismatch"));
    assert_eq!(pair.receiver.poll(NOW, pair.now, 1)?.feedback_sent, 0);
    assert!(pair.tx.incoming.lock().unwrap().is_empty());
    Ok(())
}
