//! Real loopback UDP through the supplied IO seam, with explicit fault
//! injection for backpressure and adapter errors. Not Iroh/NAT acceptance.
use super::*;
use crate::novorudp::{
    fragments::{FragmentProfile, OutgoingMessage, ReassemblyLimits},
    object_link::{
        NativeLaneConfig, NativeObjectLink, NativeSendCancellation, NativeSendLimits,
        NativeSendPolicy,
    },
    object_sender::ObjectSendState,
    wire_budget::try_send_datagram,
    NovoRudpPacingProfile, NovoRudpTransportFrameKindV0, NovoRudpWindowConfig,
};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

struct SocketIo {
    socket: UdpSocket,
    blocked: AtomicBool,
    // 1 partial write, 2 impossible length, 3 fatal error (test faults only).
    send_fault: AtomicUsize,
    receive_fault: AtomicUsize,
    sent: AtomicU64,
    calls: AtomicUsize,
}
impl SocketIo {
    fn new() -> Result<Arc<Self>> {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.set_nonblocking(true)?;
        Ok(Arc::new(Self {
            socket,
            blocked: AtomicBool::new(false),
            send_fault: AtomicUsize::new(0),
            receive_fault: AtomicUsize::new(0),
            sent: AtomicU64::new(0),
            calls: AtomicUsize::new(0),
        }))
    }
}
impl NativeDatagramIo for SocketIo {
    fn try_receive(&self, buffer: &mut [u8]) -> std::io::Result<Option<(usize, NativeRoute)>> {
        match self.receive_fault.swap(0, Ordering::Relaxed) {
            1 => Err(ErrorKind::ConnectionRefused.into()),
            2 => Err(ErrorKind::BrokenPipe.into()),
            _ => self.socket.try_receive(buffer),
        }
    }
    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> std::io::Result<usize> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.blocked.load(Ordering::Relaxed) {
            return Err(ErrorKind::WouldBlock.into());
        }
        match self.send_fault.load(Ordering::Relaxed) {
            1 => return Ok(bytes.len().saturating_sub(1)),
            2 => return Ok(bytes.len() + 1),
            3 => return Err(ErrorKind::BrokenPipe.into()),
            _ => {}
        }
        let sent = self.socket.try_send(bytes, route)?;
        self.sent.fetch_add(sent as u64, Ordering::Relaxed);
        Ok(sent)
    }
    fn local_udp_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}
fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_secs(3),
        retry: Duration::from_millis(10),
    }
}
fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn endpoints() -> Result<(NativeEndpoint, NativeEndpoint, Arc<SocketIo>, Arc<SocketIo>)> {
    let a = SocketIo::new()?;
    let b = SocketIo::new()?;
    let mut server =
        NativeEndpoint::listen_clients_with_io(b.clone(), key(72), 1, vec![17], budget())?;
    let mut client = NativeEndpoint::connect_clients_with_io(
        a.clone(),
        key(73),
        vec![NativeRoutePeer {
            key: key(72).verifying_key().to_bytes(),
            route: NativeRoute::Udp(b.socket.local_addr()?),
        }],
        budget(),
        1000,
        Instant::now(),
    )?;
    for endpoint in [&mut client, &mut server] {
        endpoint.set_wire_budget_v1(1_000_000, Instant::now())?;
    }
    Ok((client, server, a, b))
}
fn authenticate(client: &mut NativeEndpoint, server: &mut NativeEndpoint) -> Result<()> {
    let start = Instant::now();
    while client.session_id(0).is_none() || server.session_id(0).is_none() {
        ensure!(
            start.elapsed() < Duration::from_secs(3),
            "IO seam handshake stalled"
        );
        client.poll(1000, Instant::now(), 8)?;
        server.poll(1000, Instant::now(), 8)?;
        std::thread::yield_now();
    }
    Ok(())
}
fn link(endpoint: NativeEndpoint) -> Result<NativeObjectLink> {
    NativeObjectLink::new(
        endpoint,
        vec![NativeLaneConfig {
            stream: 17,
            domain: [17; 32],
            receive: ReassemblyLimits {
                max_message_bytes: 8192,
                messages: 2,
                bytes: 16384,
                peer_messages: 2,
                peer_bytes: 16384,
                ttl: Duration::from_secs(5),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: 16384,
                peer_messages: 2,
                peer_bytes: 16384,
            },
        }],
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
            ttl: Duration::from_secs(3),
        },
    )
}
fn message(value: u8) -> Result<Arc<OutgoingMessage>> {
    Ok(Arc::new(OutgoingMessage::with_profile(
        [17; 32],
        vec![value; 4096],
        8192,
        FragmentProfile::NativeUdpV2,
    )?))
}

#[test]
fn supplied_udp_io_transfers_objects_acks_and_preserves_backpressure_and_cancellation() -> Result<()>
{
    let (mut client, mut server, a, b) = endpoints()?;
    let now = Instant::now();
    a.blocked.store(true, Ordering::Relaxed);
    assert_eq!(client.poll(1000, now, 8)?.handshakes_sent, 0);
    assert_eq!(client.wire_usage_v1(now).unwrap().sent_bytes, 0);
    assert_eq!(a.sent.load(Ordering::Relaxed), 0);
    a.blocked.store(false, Ordering::Relaxed);
    // Same timestamp: WouldBlock did not advance the retry deadline.
    assert_eq!(client.poll(1000, now, 8)?.handshakes_sent, 1);
    authenticate(&mut client, &mut server)?;
    assert_eq!(
        client.wire_usage_v1(now).unwrap().sent_bytes,
        a.sent.load(Ordering::Relaxed)
    );
    assert_eq!(
        server.wire_usage_v1(now).unwrap().sent_bytes,
        b.sent.load(Ordering::Relaxed)
    );
    let mut client = link(client)?;
    let mut server = link(server)?;
    let body = message(11)?;
    assert!(client.try_send(0, 17, &body, Instant::now())?);
    a.blocked.store(true, Ordering::Relaxed);
    let report = client.poll(1000, Instant::now(), 8)?;
    assert_eq!(report.data_sent, 0);
    assert!(report.completed.is_empty());
    assert_eq!(client.outbound_usage(17)?, (1, 4096));
    a.blocked.store(false, Ordering::Relaxed);
    let (mut received, mut delivered, mut acknowledgements) = (false, false, 0);
    let start = Instant::now();
    while !(received && delivered) {
        ensure!(
            start.elapsed() < Duration::from_secs(3),
            "IO seam object stalled"
        );
        let now = Instant::now();
        for done in client.poll(1000, now, 8)?.completed {
            assert_eq!(done.state, ObjectSendState::Delivered);
            assert!(Arc::ptr_eq(&done.body, &body));
            delivered = true;
        }
        acknowledgements += server.poll(1000, now, 8)?.feedback_sent;
        if let Some(item) = server.poll_complete(17, now, 1)? {
            assert!(!received);
            assert_eq!(item.id, body.id());
            assert_eq!(item.chunks.concat(), vec![11; 4096]);
            received = true;
        }
        std::thread::yield_now();
    }
    assert!(acknowledgements > 0);
    assert_eq!(client.outbound_usage(17)?, (0, 0));
    let cancel = NativeSendCancellation::default();
    let cancelled = message(12)?;
    let now = Instant::now();
    assert!(client.try_send_cancellable_until(
        0,
        17,
        &cancelled,
        now,
        now + Duration::from_secs(2),
        &cancel
    )?);
    let before = a.sent.load(Ordering::Relaxed);
    cancel.cancel();
    let report = client.poll(1000, now, 8)?;
    assert_eq!(report.data_sent, 0);
    assert_eq!(report.completed.len(), 1);
    assert_eq!(report.completed[0].state, ObjectSendState::Cancelled);
    assert_eq!(a.sent.load(Ordering::Relaxed), before);
    Ok(())
}

#[test]
fn supplied_udp_io_keeps_exact_source_replay_and_receive_error_semantics() -> Result<()> {
    let (mut client, mut server, a, b) = endpoints()?;
    authenticate(&mut client, &mut server)?;
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        client.session_id(0).unwrap(),
        17,
        1,
        1,
        0,
        vec![9; 24],
    );
    let wire = client.peers[0].session.as_mut().unwrap().encode(&frame)?;
    let remote = NativeRoute::Udp(server.local_addr()?);
    let stranger = UdpSocket::bind("127.0.0.1:0")?;
    stranger.try_send(&wire, remote)?;
    a.try_send(&wire, remote)?;
    a.try_send(&wire, remote)?;
    let start = Instant::now();
    let (mut frames, mut rejected) = (0, 0);
    while frames + rejected < 3 {
        ensure!(
            start.elapsed() < Duration::from_secs(3),
            "IO seam source test stalled"
        );
        let report = server.poll(1000, Instant::now(), 8)?;
        for received in report.frames {
            assert_eq!(received.frame().payload, frame.payload);
            frames += 1;
        }
        rejected += report.rejected_datagrams;
    }
    assert_eq!((frames, rejected), (1, 2));
    b.receive_fault.store(1, Ordering::Relaxed);
    assert_eq!(server.poll(1000, Instant::now(), 1)?.receive_errors, 1);
    b.receive_fault.store(2, Ordering::Relaxed);
    let error = match server.poll(1000, Instant::now(), 1) {
        Err(error) => error,
        Ok(_) => anyhow::bail!("fatal IO error lost"),
    };
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        ErrorKind::BrokenPipe
    );
    Ok(())
}

#[test]
fn supplied_send_sink_partial_and_error_never_claim_success_or_advance_handshake() -> Result<()> {
    let (mut client, _server, a, _b) = endpoints()?;
    let now = Instant::now();
    a.send_fault.store(1, Ordering::Relaxed);
    let report = client.poll(1000, now, 8)?;
    assert_eq!(
        (report.handshakes_sent, report.handshake_send_failures),
        (0, 1)
    );
    assert!(client.wire_usage_v1(now).unwrap().sent_bytes > 0); // Conservatively charge reported partial bytes.
    a.send_fault.store(0, Ordering::Relaxed);
    assert_eq!(client.poll(1000, now, 8)?.handshakes_sent, 1);
    let mut budget = NativeWireBudget::new(2400, now)?;
    let route = NativeRoute::Udp(a.socket.local_addr()?);
    a.send_fault.store(2, Ordering::Relaxed);
    assert!(try_send_datagram(a.as_ref(), &[1; 24], route, now, Some(&mut budget)).is_err());
    assert_eq!(budget.usage(now).used, 0);
    a.send_fault.store(3, Ordering::Relaxed);
    let error = try_send_datagram(a.as_ref(), &[1; 24], route, now, Some(&mut budget)).unwrap_err();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        ErrorKind::BrokenPipe
    );
    assert_eq!(budget.usage(now).used, 0);
    a.blocked.store(true, Ordering::Relaxed);
    assert!(!try_send_datagram(
        a.as_ref(),
        &[1; 24],
        route,
        now,
        Some(&mut budget)
    )?);
    assert_eq!(budget.usage(now).used, 0);
    Ok(())
}
