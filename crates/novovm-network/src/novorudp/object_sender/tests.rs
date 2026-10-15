use super::*;
use crate::product_overlay::*;
use ed25519_dalek::SigningKey;

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
fn invalid_and_stale_feedback_cannot_deliver_or_rewrite_repair_state() -> Result<()> {
    let (_, _, a, _) = link()?;
    let now = Instant::now();
    let body = Arc::new(OutgoingMessage::with_profile(
        [1; 32],
        vec![1; 1000],
        2000,
        FragmentProfile::NativeUdpV2,
    )?);
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
        expected_total: 2,
        receiver_done: false,
        missing_count: 1,
        current_window: Some(NovoRudpRange::new(1, 1)),
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
