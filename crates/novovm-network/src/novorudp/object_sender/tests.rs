use super::*;
use crate::product_overlay::*;
use ed25519_dalek::SigningKey;

mod batch;
mod progress;
mod tail;

fn link() -> Result<(UdpSocket, UdpSocket, NativeUdpSession, NativeUdpSession)> {
    let left = UdpSocket::bind("127.0.0.1:0")?;
    let right = UdpSocket::bind("127.0.0.1:0")?;
    left.set_nonblocking(true)?;
    right.set_nonblocking(true)?;
    let a = SigningKey::from_bytes(&[31; 32]);
    let b = SigningKey::from_bytes(&[47; 32]);
    let offer = NodeHandshakeInitiatorV1::start(
        &a,
        peer_id_from_ed25519_public_key_v1(&b.verifying_key().to_bytes()),
        1000,
        5000,
    )?;
    let response = NodeHandshakeResponderV1::respond(
        offer.offer(),
        &b,
        1000,
        5000,
        &mut HandshakeReplayCacheV1::default(),
    )?;
    let a = offer.complete(
        response.response(),
        1000,
        &mut HandshakeReplayCacheV1::default(),
    )?;
    let a = NativeUdpSession::new(a, right.local_addr()?)?;
    let b = NativeUdpSession::new(response.into_channel(), left.local_addr()?)?;
    Ok((left, right, a, b))
}
fn receive(socket: &UdpSocket, session: &mut NativeUdpSession) -> Result<NovoRudpTransportFrameV0> {
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(frame) = session.try_receive(socket)? {
            return Ok(frame);
        }
        ensure!(Instant::now() < until, "sender test receive timeout");
        std::thread::yield_now();
    }
}
fn settings() -> (NovoRudpWindowConfig, NovoRudpPacingProfile) {
    (
        NovoRudpWindowConfig {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 8,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            max_window_retries: 2,
            no_progress_backoff: false,
            ..Default::default()
        },
        NovoRudpPacingProfile {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 8,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
        },
    )
}

#[test]
fn one_ack_drives_all_repair_polls_and_new_identical_snapshot_does_not_rewind() -> Result<()> {
    use super::super::fragments::*;
    let (left, right, mut a, mut b) = link()?;
    let now = Instant::now();
    let bytes = vec![5; 100 * UDP_CHUNK_BYTES];
    let body = Arc::new(OutgoingMessage::with_profile(
        [1; 32],
        bytes.clone(),
        100_000,
        FragmentProfile::NativeUdpV2,
    )?);
    let (config, pacing) = settings();
    let mut sender = NativeObjectSender::new(
        body.clone(),
        a.session_id(),
        11,
        config.clone(),
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    assert!(sender.diagnostic_progress_v1().is_none());
    sender.enable_diagnostics_v1();
    let peer = b.remote_peer_id().to_owned();
    let mut objects = Reassembler::with_profile(
        [1; 32],
        vec![peer.clone()],
        ReassemblyLimits {
            max_message_bytes: 100_000,
            messages: 1,
            bytes: 100_000,
            peer_messages: 1,
            peer_bytes: 100_000,
            ttl: Duration::from_secs(5),
        },
        FragmentProfile::NativeUdpV2,
    )?;
    while sender.initial < body.frame_count() {
        let count = sender.poll_send(&left, &mut a, now, 8)?;
        assert!(count > 0);
        for _ in 0..count {
            let frame = receive(&right, &mut b)?;
            if !(5..=12).contains(&frame.sequence) {
                objects.push(&peer, &frame.payload, now)?;
            }
        }
    }
    let (total, missing) = objects.missing_ranges(&peer, body.id()).unwrap();
    let observed = sender.diagnostic_progress_v1().unwrap();
    assert_eq!(observed.initial_packets, 100);
    assert_eq!(observed.repair_packets, 0);
    assert!(observed.initial_complete.is_some());
    let window = select_first_missing_window(&missing, total, &config).unwrap();
    assert_eq!(window.range.start, 5); // Preserve original non-grid window rule.
    let mut ack = NovoRudpAckFrame {
        header: NovoRudpFrameHeader {
            version: 1,
            kind: NovoRudpFrameKind::Ack,
            session_id: a.session_id(),
            epoch: 1,
            sequence: None,
            window_id: Some(window.window_id),
        },
        expected_total: total,
        receiver_done: false,
        missing_count: 8,
        current_window: Some(window.range),
        current_window_missing_ranges: window.missing_ranges,
    };
    assert!(b.try_send(&right, &encode_object_ack(body.id(), 11, &ack)?)?);
    let feedback = receive(&left, &mut a)?;
    sender.on_ack(&feedback, now)?;
    let mut received = Vec::new();
    for index in 0..8 {
        if index == 2 {
            // New secure packet AND newer ACK epoch, but unchanged missing set.
            ack.header.epoch = 2;
            assert!(b.try_send(&right, &encode_object_ack(body.id(), 11, &ack)?)?);
            sender.on_ack(&receive(&left, &mut a)?, now)?;
        }
        assert_eq!(sender.poll_send(&left, &mut a, now, 1)?, 1);
        let frame = receive(&right, &mut b)?;
        assert_eq!(frame.kind, NovoRudpTransportFrameKindV0::Repair);
        received.push(frame.sequence);
        objects.push(&peer, &frame.payload, now)?;
    }
    assert_eq!(received, (5..=12).collect::<Vec<_>>());
    let observed = sender.diagnostic_progress_v1().unwrap();
    assert_eq!(observed.initial_packets, 100);
    assert_eq!(observed.repair_packets, 8);
    assert_eq!(observed.probe_packets, 0);
    assert_eq!(observed.last_repair_index, Some(12));
    assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 0);
    let mut complete = None;
    for _ in 0..=body.frame_count().div_ceil(64) {
        if let Some(item) = objects.poll_complete(now, 64)? {
            complete = Some(item);
            break;
        }
    }
    assert_eq!(
        complete.context("missing completion")?.chunks.concat(),
        bytes
    );
    assert_eq!(sender.status(), ObjectSendState::Sending); // Completion on receiver is not a sender ACK.
    ack.receiver_done = true;
    ack.missing_count = 0;
    ack.current_window = None;
    ack.current_window_missing_ranges.clear();
    ack.header.window_id = None;
    ack.header.epoch = 3;
    assert!(b.try_send(&right, &encode_object_ack(body.id(), 11, &ack)?)?);
    sender.on_ack(&receive(&left, &mut a)?, now)?;
    assert_eq!(sender.status(), ObjectSendState::Delivered);
    assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 0);
    assert!(Arc::ptr_eq(&sender.into_body(), &body));
    Ok(())
}

#[test]
fn bounded_no_feedback_retries_return_original_body_without_fake_delivery() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let now = Instant::now();
    let body = Arc::new(OutgoingMessage::with_profile(
        [1; 32],
        vec![7; 10],
        1024,
        FragmentProfile::NativeUdpV2,
    )?);
    let (mut config, pacing) = settings();
    config.no_progress_backoff = true;
    let mut sender = NativeObjectSender::new(
        body.clone(),
        a.session_id(),
        11,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    sender.enable_diagnostics_v1();
    assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 1);
    receive(&right, &mut b)?;
    for ms in [10, 20] {
        assert_eq!(
            sender.poll_send(&left, &mut a, now + Duration::from_millis(ms), 8)?,
            1
        );
        assert_eq!(
            receive(&right, &mut b)?.kind,
            NovoRudpTransportFrameKindV0::Repair
        );
    }
    let observed = sender.diagnostic_progress_v1().unwrap();
    assert_eq!(observed.initial_packets, 1);
    assert_eq!(observed.repair_packets, 0);
    assert_eq!(observed.probe_packets, 2);
    assert_eq!(
        sender.poll_send(&left, &mut a, now + Duration::from_millis(30), 8)?,
        0
    );
    assert_eq!(sender.status(), ObjectSendState::Sending);
    assert_eq!(
        sender.poll_send(&left, &mut a, now + Duration::from_secs(5), 8)?,
        0
    );
    assert_eq!(sender.status(), ObjectSendState::Expired);
    assert!(Arc::ptr_eq(&sender.into_body(), &body));
    Ok(())
}

#[test]
fn native_read_request_recovers_after_initial_loss_with_both_backoff_flags_disabled() -> Result<()>
{
    use super::super::fragments::{Reassembler, ReassemblyLimits};
    use novovm_protocol::native_query::{self as query, Frame, Query, Request};

    // Reproduce the old process fixture's window default / pacing override,
    // then the service/operator's explicit two-flag policy. No changed retry
    // count, ACK interval, operation deadline, object TTL or sender algorithm.
    for window_backoff in [true, false] {
        let (left, right, mut a, mut b) = link()?;
        let now = Instant::now();
        let request = Frame::Request(Request {
            session: [9; 32],
            sequence: 40,
            query: Query::Receipt { hash: [4; 32] },
        });
        let bytes = query::encode(request.clone()).map_err(anyhow::Error::msg)?;
        let body = Arc::new(OutgoingMessage::with_profile(
            [1; 32],
            bytes.clone(),
            query::MAX_BYTES,
            FragmentProfile::NativeUdpV2,
        )?);
        assert_eq!(body.frame_count(), 1);
        let config = NovoRudpWindowConfig {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 32,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: window_backoff,
            ..Default::default()
        };
        assert_eq!(config.max_window_retries, 8);
        let pacing = NovoRudpPacingProfile {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 32,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
        };
        let mut sender = NativeObjectSender::new(
            body.clone(),
            a.session_id(),
            query::STREAM,
            config,
            pacing,
            now,
            Duration::from_secs(30),
        )?;
        // Drop actual authenticated packets before they reach reassembly.
        // Supplying monotonic timestamps makes the loss window deterministic,
        // without sleeping or fabricating transport success/ACK progress.
        for attempt in 0..=8 {
            let at = now + Duration::from_millis(attempt * 10);
            assert_eq!(sender.poll_send(&left, &mut a, at, 64)?, 1);
            let dropped = receive(&right, &mut b)?;
            assert_eq!(dropped.stream_id, query::STREAM);
            assert_eq!(sender.status(), ObjectSendState::Sending);
        }
        let recovered_at = now + Duration::from_millis(90);
        assert!(recovered_at < now + Duration::from_secs(5));
        if window_backoff {
            // A false pacing flag does not undo a true window flag: no more
            // request packets arrive even after the network recovers.
            for at in [recovered_at, now + Duration::from_secs(5)] {
                assert_eq!(sender.poll_send(&left, &mut a, at, 64)?, 0);
                assert_eq!(sender.status(), ObjectSendState::Sending);
            }
            assert_eq!(
                sender.poll_send(&left, &mut a, now + Duration::from_secs(30), 64)?,
                0
            );
            assert_eq!(sender.status(), ObjectSendState::Expired);
            assert!(Arc::ptr_eq(&sender.into_body(), &body));
            continue;
        }
        assert_eq!(sender.poll_send(&left, &mut a, recovered_at, 64)?, 1);
        let received = receive(&right, &mut b)?;
        let peer = b.remote_peer_id().to_owned();
        let mut objects = Reassembler::with_profile(
            [1; 32],
            vec![peer.clone()],
            ReassemblyLimits {
                max_message_bytes: query::MAX_BYTES,
                messages: 1,
                bytes: query::MAX_BYTES,
                peer_messages: 1,
                peer_bytes: query::MAX_BYTES,
                ttl: Duration::from_secs(5),
            },
            FragmentProfile::NativeUdpV2,
        )?;
        objects.push(&peer, &received.payload, recovered_at)?;
        let complete = objects
            .poll_complete(recovered_at, 64)?
            .context("recovered original query missing")?;
        let decoded = complete.chunks.concat();
        assert_eq!(decoded, bytes);
        assert_eq!(
            query::decode(&decoded).map_err(anyhow::Error::msg)?,
            request
        );
        assert_eq!(sender.status(), ObjectSendState::Sending);
        let ack = NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: a.session_id(),
                epoch: 1,
                sequence: None,
                window_id: None,
            },
            expected_total: 1,
            receiver_done: true,
            missing_count: 0,
            current_window: None,
            current_window_missing_ranges: vec![],
        };
        assert!(b.try_send(&right, &encode_object_ack(body.id(), query::STREAM, &ack)?)?);
        sender.on_ack(&receive(&left, &mut a)?, recovered_at)?;
        assert_eq!(sender.status(), ObjectSendState::Delivered);
        assert!(Arc::ptr_eq(&sender.into_body(), &body));
    }
    Ok(())
}

#[test]
fn invalid_and_stale_feedback_cannot_deliver_or_rewrite_repair_state() -> Result<()> {
    let (_, _, a, _) = link()?;
    let now = Instant::now();
    // Three fragments so the ACK reports a gap behind the newest received one.
    let body = Arc::new(OutgoingMessage::with_profile(
        [1; 32],
        vec![1; 2 * super::super::fragments::UDP_CHUNK_BYTES + 1],
        100_000,
        FragmentProfile::NativeUdpV2,
    )?);
    assert_eq!(body.frame_count(), 3);
    let (config, pacing) = settings();
    let mut sender = NativeObjectSender::new(
        body.clone(),
        a.session_id(),
        11,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    let mut ack = NovoRudpAckFrame {
        header: NovoRudpFrameHeader {
            version: 1,
            kind: NovoRudpFrameKind::Ack,
            session_id: a.session_id(),
            epoch: 2,
            sequence: None,
            window_id: Some(0),
        },
        expected_total: 3,
        receiver_done: false,
        missing_count: 1,
        current_window: Some(NovoRudpRange::new(1, 2)),
        current_window_missing_ranges: vec![NovoRudpRange::new(1, 1)],
    };
    let frame = encode_object_ack(body.id(), 11, &ack)?;
    for field in 0..4 {
        let mut wrong = frame.clone();
        match field {
            0 => wrong.stream_id ^= 1,
            1 => wrong.session_id[0] ^= 1,
            2 => wrong.payload[39] ^= 1,
            _ => wrong.payload[43] ^= 1,
        }
        assert!(sender.on_ack(&wrong, now).is_err());
        assert_eq!(sender.state.latest_ack_epoch, 0);
        assert_eq!(sender.status(), ObjectSendState::Sending);
    }
    assert!(matches!(
        sender.on_ack(&frame, now)?,
        NovoRudpSenderRepairDecision::Repair(_)
    ));
    ack.header.epoch = 1;
    ack.header.window_id = None;
    ack.current_window = None;
    ack.current_window_missing_ranges.clear();
    ack.missing_count = 0;
    ack.receiver_done = true;
    assert!(matches!(
        sender.on_ack(&encode_object_ack(body.id(), 11, &ack)?, now)?,
        NovoRudpSenderRepairDecision::StaleAck { .. }
    ));
    assert_eq!(sender.status(), ObjectSendState::Sending);
    assert_eq!(sender.repair.as_ref().unwrap().index, 1);
    Ok(())
}

struct BlockedWriter {
    socket: UdpSocket,
    blocked: std::sync::atomic::AtomicBool,
}
impl crate::novorudp::carrier::NativeDatagramIo for BlockedWriter {
    fn deferred_writes(&self) -> bool {
        true
    }
    fn try_receive(
        &self,
        _: &mut [u8],
    ) -> std::io::Result<Option<(usize, crate::novorudp::carrier::NativeRoute)>> {
        Err(std::io::ErrorKind::WouldBlock.into())
    }
    fn try_send(
        &self,
        bytes: &[u8],
        route: crate::novorudp::carrier::NativeRoute,
    ) -> std::io::Result<usize> {
        if self.blocked.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        self.socket.try_send(bytes, route)
    }
}
fn missing_ack(
    sender: &NativeObjectSender,
    epoch: u64,
    start: u64,
) -> Result<NovoRudpTransportFrameV0> {
    let total = sender.body.frame_count() as u64;
    encode_object_ack(
        sender.body.id(),
        sender.stream,
        &NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: sender.session,
                epoch,
                sequence: None,
                window_id: Some(start / OBJECT_WINDOW),
            },
            expected_total: total,
            receiver_done: false,
            missing_count: total - start,
            current_window: Some(NovoRudpRange::new(start, total - 1)),
            current_window_missing_ranges: vec![NovoRudpRange::new(start, total - 1)],
        },
    )
}
/// Missing [start, end] behind the receiver's newest fragment (the last one),
/// so the sender treats it as a real gap rather than an unconfirmed tail.
fn gap_ack(
    sender: &NativeObjectSender,
    epoch: u64,
    start: u64,
    end: u64,
) -> Result<NovoRudpTransportFrameV0> {
    let total = sender.body.frame_count() as u64;
    encode_object_ack(
        sender.body.id(),
        sender.stream,
        &NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: sender.session,
                epoch,
                sequence: None,
                window_id: Some(start / OBJECT_WINDOW),
            },
            expected_total: total,
            receiver_done: false,
            missing_count: end - start + 1,
            current_window: Some(NovoRudpRange::new(start, total - 1)),
            current_window_missing_ranges: vec![NovoRudpRange::new(start, end)],
        },
    )
}
#[test]
fn blocked_initial_write_keeps_exact_frame_across_new_ack_epoch() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let io = BlockedWriter {
        socket: left,
        blocked: true.into(),
    };
    let now = Instant::now();
    let body = Arc::new(OutgoingMessage::with_profile(
        [1; 32],
        vec![5; 1000],
        2000,
        FragmentProfile::NativeUdpV2,
    )?);
    let (config, pacing) = settings();
    let mut sender = NativeObjectSender::new(
        body,
        a.session_id(),
        11,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 1, None)?, 0);
    let original = sender.pending.as_ref().unwrap().frame.clone();
    sender.on_ack(&missing_ack(&sender, 4, 1)?, now)?;
    assert_eq!(sender.state.latest_ack_epoch, 4);
    io.blocked
        .store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 1, None)?, 1);
    assert_eq!(receive(&right, &mut b)?, original);
    assert_eq!(sender.initial, 1);
    assert!(sender.pending.is_none());
    Ok(())
}
#[test]
fn blocked_repair_completion_does_not_skip_replacement_cursor() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let io = BlockedWriter {
        socket: left,
        blocked: false.into(),
    };
    let now = Instant::now();
    // Three fragments: both ACKs report gaps behind the received last fragment.
    let body = Arc::new(OutgoingMessage::with_profile(
        [1; 32],
        vec![5; 2 * super::super::fragments::UDP_CHUNK_BYTES + 1],
        100_000,
        FragmentProfile::NativeUdpV2,
    )?);
    assert_eq!(body.frame_count(), 3);
    let (config, pacing) = settings();
    let mut sender = NativeObjectSender::new(
        body,
        a.session_id(),
        11,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    for _ in 0..3 {
        assert_eq!(sender.poll_send_limited(&io, &mut a, now, 2, None)?, 1);
        receive(&right, &mut b)?;
    }
    sender.on_ack(&gap_ack(&sender, 1, 0, 1)?, now)?;
    io.blocked.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 1, None)?, 0);
    let original = sender.pending.as_ref().unwrap().frame.clone();
    sender.on_ack(&gap_ack(&sender, 2, 1, 1)?, now)?;
    io.blocked
        .store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 1, None)?, 1);
    assert_eq!(receive(&right, &mut b)?, original);
    assert_eq!(sender.repair.as_ref().unwrap().index, 1);
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 1, None)?, 1);
    let repaired = receive(&right, &mut b)?;
    assert_eq!(repaired.sequence, 1);
    assert_eq!(repaired.ack_epoch, 2);
    assert!(sender.repair.is_none());
    Ok(())
}

#[test]
fn deferred_writer_paces_a_batch_not_each_completed_frame() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let io = BlockedWriter {
        socket: left,
        blocked: false.into(),
    };
    let now = Instant::now();
    let body = Arc::new(OutgoingMessage::with_profile(
        [1; 32],
        vec![5; 20_000],
        30_000,
        FragmentProfile::NativeUdpV2,
    )?);
    let (mut config, mut pacing) = settings();
    config.batch_pause_ms = 10;
    pacing.batch_pause_ms = 10;
    let mut sender = NativeObjectSender::new(
        body,
        a.session_id(),
        11,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    // A reliable carrier returns its sole write slot after EVERY completion.
    // Fair handoff is not the end of the configured eight-frame batch.
    for index in 0..8 {
        assert_eq!(
            sender.poll_send_limited(&io, &mut a, now, 8, None)?,
            1,
            "deferred frame {index} must not wait for a batch pause"
        );
        receive(&right, &mut b)?;
    }
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 8, None)?, 0);
    let resumed = now + Duration::from_millis(10);
    io.blocked.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(sender.poll_send_limited(&io, &mut a, resumed, 8, None)?, 0);
    io.blocked
        .store(false, std::sync::atomic::Ordering::Relaxed);
    for _ in 0..8 {
        assert_eq!(sender.poll_send_limited(&io, &mut a, resumed, 8, None)?, 1);
        receive(&right, &mut b)?;
    }
    assert_eq!(sender.poll_send_limited(&io, &mut a, resumed, 8, None)?, 0);
    Ok(())
}

#[test]
fn udp_small_poll_keeps_existing_pause_policy() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let now = Instant::now();
    let body = Arc::new(OutgoingMessage::with_profile(
        [1; 32],
        vec![5; 2000],
        3000,
        FragmentProfile::NativeUdpV2,
    )?);
    let (mut config, mut pacing) = settings();
    config.batch_pause_ms = 10;
    pacing.batch_pause_ms = 10;
    let mut sender = NativeObjectSender::new(
        body,
        a.session_id(),
        11,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 1)?, 1);
    receive(&right, &mut b)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 1)?, 0);
    assert_eq!(
        sender.poll_send(&left, &mut a, now + Duration::from_millis(10), 1)?,
        1
    );
    receive(&right, &mut b)?;
    Ok(())
}

#[test]
fn final_window_repairs_gaps_now_and_unconfirmed_tail_only_after_ack_wait() -> Result<()> {
    use super::super::fragments::*;
    let (left, right, mut a, mut b) = link()?;
    let now = Instant::now();
    let body = Arc::new(OutgoingMessage::with_profile(
        [2; 32],
        vec![9; 20 * UDP_CHUNK_BYTES],
        100_000,
        FragmentProfile::NativeUdpV2,
    )?);
    let total = body.frame_count() as u64;
    let (config, pacing) = settings();
    let mut sender = NativeObjectSender::new(
        body.clone(),
        a.session_id(),
        11,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    while sender.initial < body.frame_count() {
        for _ in 0..sender.poll_send(&left, &mut a, now, 8)? {
            receive(&right, &mut b)?;
        }
    }
    // An ACK taken mid-flight: one real gap, then a trailing unconfirmed run.
    let ack = NovoRudpAckFrame {
        header: NovoRudpFrameHeader {
            version: 1,
            kind: NovoRudpFrameKind::Ack,
            session_id: a.session_id(),
            epoch: 1,
            sequence: None,
            window_id: Some(0),
        },
        expected_total: total,
        receiver_done: false,
        missing_count: 6,
        current_window: Some(NovoRudpRange::new(3, total - 1)),
        current_window_missing_ranges: vec![
            NovoRudpRange::new(3, 3),
            NovoRudpRange::new(total - 5, total - 1),
        ],
    };
    assert!(b.try_send(&right, &encode_object_ack(body.id(), 11, &ack)?)?);
    sender.on_ack(&receive(&left, &mut a)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 1);
    let gap = receive(&right, &mut b)?;
    assert_eq!(
        (gap.kind, gap.sequence),
        (NovoRudpTransportFrameKindV0::Repair, 3)
    );
    assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 0);
    // No newer feedback within ack_wait: the existing retry repairs the tail.
    let later = now + Duration::from_millis(20);
    let mut repaired = Vec::new();
    for _ in 0..sender.poll_send(&left, &mut a, later, 8)? {
        repaired.push(receive(&right, &mut b)?.sequence);
    }
    assert!((total - 5..total).all(|index| repaired.contains(&index)));
    Ok(())
}

#[test]
fn diagnostic_ack_high_water_requires_new_verified_information() -> Result<()> {
    let (_, _, channel, _) = link()?;
    let now = Instant::now();
    let body = Arc::new(OutgoingMessage::with_profile(
        [1; 32],
        vec![5; 3 * 768],
        4096,
        FragmentProfile::NativeUdpV2,
    )?);
    let (config, pacing) = settings();
    let mut sender = NativeObjectSender::new(
        body.clone(),
        channel.session_id(),
        11,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    for (epoch, start, best) in [(1, 0, 0), (2, 1, 1), (3, 0, 1), (4, 1, 1), (5, 2, 2)] {
        sender.on_ack(&missing_ack(&sender, epoch, start)?, now)?;
        assert_eq!(sender.acknowledged_best_v1(), best);
    }
    assert!(matches!(
        sender.on_ack(&missing_ack(&sender, 4, 2)?, now)?,
        NovoRudpSenderRepairDecision::StaleAck { .. }
    ));
    assert_eq!(sender.acknowledged_best_v1(), 2);
    let mut bad = missing_ack(&sender, 6, 2)?;
    bad.session_id[0] ^= 1;
    assert!(sender.on_ack(&bad, now).is_err());
    assert_eq!(sender.acknowledged_best_v1(), 2);
    let done = encode_object_ack(
        body.id(),
        11,
        &NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: channel.session_id(),
                epoch: 6,
                sequence: None,
                window_id: None,
            },
            expected_total: 3,
            receiver_done: true,
            missing_count: 0,
            current_window: None,
            current_window_missing_ranges: vec![],
        },
    )?;
    assert!(matches!(
        sender.on_ack(&done, now)?,
        NovoRudpSenderRepairDecision::ReceiverDone
    ));
    assert_eq!(sender.acknowledged_best_v1(), 3);
    Ok(())
}
