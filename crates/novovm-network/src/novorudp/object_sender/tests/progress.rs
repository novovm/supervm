use super::*;
use crate::novorudp::fragments::UDP_CHUNK_BYTES;

fn initial_sent() -> Result<(
    UdpSocket,
    UdpSocket,
    NativeUdpSession,
    NativeUdpSession,
    NativeObjectSender,
    Instant,
)> {
    initial_prefix(128)
}

fn initial_prefix(
    count: usize,
) -> Result<(
    UdpSocket,
    UdpSocket,
    NativeUdpSession,
    NativeUdpSession,
    NativeObjectSender,
    Instant,
)> {
    let (left, right, mut a, mut b) = link()?;
    let now = Instant::now();
    let body = Arc::new(OutgoingMessage::with_profile(
        [8; 32],
        vec![5; 128 * UDP_CHUNK_BYTES],
        200_000,
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
    while sender.initial < count {
        for _ in 0..sender.poll_send(&left, &mut a, now, 8)? {
            receive(&right, &mut b)?;
        }
    }
    Ok((left, right, a, b, sender, now))
}

fn early_ack(
    sender: &NativeObjectSender,
    epoch: u64,
    tail: u64,
) -> Result<NovoRudpTransportFrameV0> {
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
                window_id: Some(0),
            },
            expected_total: 128,
            receiver_done: false,
            missing_count: 129 - tail,
            current_window: Some(NovoRudpRange::new(1, 64)),
            current_window_missing_ranges: vec![
                NovoRudpRange::new(1, 1),
                NovoRudpRange::new(tail, 64),
            ],
        },
    )
}

#[test]
fn lost_early_repair_retries_while_unrelated_initial_progress_continues() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_prefix(8)?;
    sender.on_ack(&early_ack(&sender, 1, 8)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 1); // This repair is lost.
    let middle = now + Duration::from_millis(5);
    assert_eq!(sender.poll_send(&left, &mut a, middle, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 8);
    sender.on_ack(&early_ack(&sender, 2, 9)?, middle)?;
    assert_eq!(
        sender.poll_send(&left, &mut a, now + Duration::from_millis(11), 2)?,
        2
    );
    let frames = [receive(&right, &mut b)?, receive(&right, &mut b)?];
    assert!(
        frames
            .iter()
            .any(|f| f.kind == NovoRudpTransportFrameKindV0::Repair && f.sequence == 1),
        "unrelated initial/ACK progress must not postpone the original gap retry deadline"
    );
    assert!(frames
        .iter()
        .any(|f| f.kind == NovoRudpTransportFrameKindV0::Data && f.sequence == 9));
    assert!(sender.initial < sender.body.frame_count());
    Ok(())
}

#[test]
fn repair_pass_waits_for_its_last_actual_write_before_reopening() -> Result<()> {
    for start in [0, 120] {
        let (left, right, mut a, mut b, mut sender, now) = initial_sent()?;
        sender.on_ack(&window_ack(&sender, 1, start, start + 7, 8)?, now)?;
        let first = now + Duration::from_millis(if start == 120 { 10 } else { 0 });
        assert_eq!(sender.poll_send(&left, &mut a, first, 1)?, 1);
        assert_eq!(receive(&right, &mut b)?.sequence, start);
        assert_eq!(
            sender.poll_send(&left, &mut a, first + Duration::from_millis(16), 8)?,
            7
        );
        for index in start + 1..start + 8 {
            assert_eq!(receive(&right, &mut b)?.sequence, index);
        }
        assert_eq!(
            sender.poll_send(&left, &mut a, first + Duration::from_millis(20), 1)?,
            0
        );
        assert_eq!(
            sender.poll_send(&left, &mut a, first + Duration::from_millis(26), 1)?,
            1
        );
        assert_eq!(receive(&right, &mut b)?.sequence, start);
    }
    Ok(())
}

#[test]
fn early_repair_backoff_keeps_initial_data_moving() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_prefix(8)?;
    sender.config.max_window_retries = 1;
    sender.config.no_progress_backoff = true;
    sender.on_ack(&early_ack(&sender, 1, 8)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 1);
    // Initial gets the next turn even though a repair retry is now due.
    let later = now + Duration::from_millis(11);
    assert_eq!(sender.poll_send(&left, &mut a, later, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 8);
    assert_eq!(sender.attempts, 0, "initial is not a repair attempt");
    assert!(sender.retry_pending);
    assert_eq!(sender.poll_send(&left, &mut a, later, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 1);
    assert_eq!(sender.attempts, 1);
    assert_eq!(
        sender.poll_send(&left, &mut a, now + Duration::from_millis(22), 1)?,
        1
    );
    let next = receive(&right, &mut b)?;
    assert_eq!(next.kind, NovoRudpTransportFrameKindV0::Data);
    assert_eq!(next.sequence, 9);
    assert_eq!(sender.attempts, 1);
    Ok(())
}

#[test]
fn early_blocked_repair_keeps_pending_ownership_across_ack_and_deadline() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_prefix(8)?;
    let io = BlockedWriter {
        socket: left,
        blocked: true.into(),
    };
    sender.on_ack(&early_ack(&sender, 1, 8)?, now)?;
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 1, None)?, 0);
    let held = sender.pending.as_ref().unwrap().frame.clone();
    let generation = sender.repair_generation;
    sender.on_ack(&early_ack(&sender, 2, 9)?, now + Duration::from_millis(5))?;
    let later = now + Duration::from_millis(11);
    assert_eq!(sender.poll_send_limited(&io, &mut a, later, 1, None)?, 0);
    assert_eq!(sender.repair_generation, generation);
    assert!(sender.repair_feedback_deadline.is_none());
    io.blocked
        .store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(sender.poll_send_limited(&io, &mut a, later, 1, None)?, 1);
    assert_eq!(receive(&right, &mut b)?, held);
    assert_eq!(
        sender.repair_feedback_deadline,
        Some(later + Duration::from_millis(10))
    );
    assert_eq!(sender.poll_send_limited(&io, &mut a, later, 1, None)?, 1);
    let next = receive(&right, &mut b)?;
    assert_eq!(next.kind, NovoRudpTransportFrameKindV0::Data);
    assert_eq!(next.sequence, 8);
    Ok(())
}

#[test]
fn ongoing_initial_never_arms_an_unconfirmed_tail_as_early_repair() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_prefix(8)?;
    sender.on_ack(&window_ack(&sender, 1, 8, 71, 120)?, now)?;
    assert_eq!(
        sender.poll_send(&left, &mut a, now + Duration::from_millis(100), 8)?,
        8
    );
    for index in 8..16 {
        let frame = receive(&right, &mut b)?;
        assert_eq!(frame.kind, NovoRudpTransportFrameKindV0::Data);
        assert_eq!(frame.sequence, index);
    }
    assert!(sender.retry_tail_end.is_none());
    assert!(sender.repair_feedback_deadline.is_none());
    Ok(())
}

fn window_ack(
    sender: &NativeObjectSender,
    epoch: u64,
    start: u64,
    end: u64,
    missing: u64,
) -> Result<NovoRudpTransportFrameV0> {
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
            expected_total: 128,
            receiver_done: false,
            missing_count: missing,
            current_window: Some(NovoRudpRange::new(
                start,
                (start + OBJECT_WINDOW - 1).min(127),
            )),
            current_window_missing_ranges: vec![NovoRudpRange::new(start, end)],
        },
    )
}

#[test]
fn progressing_ack_does_not_resend_repairs_still_in_flight() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_sent()?;
    sender.on_ack(&window_ack(&sender, 1, 0, 63, 64)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 8);
    for index in 0..8 {
        assert_eq!(receive(&right, &mut b)?.sequence, index);
    }
    // Only repair 0 is reflected in this newer authenticated snapshot. Repairs
    // 1..7 have been written already, but remain in flight at the snapshot time.
    sender.on_ack(&window_ack(&sender, 2, 1, 63, 63)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 8);
    Ok(())
}

#[test]
fn nonfinal_window_unconfirmed_tail_waits_without_delaying_proven_gap() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_sent()?;
    // Snapshot taken when only 0..31 had arrived. 32..127 may all still be in
    // flight; the 64-fragment feedback window alone is not evidence of loss.
    sender.on_ack(&window_ack(&sender, 1, 32, 95, 96)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 0);
    // Now fragment 127 has arrived, proving the gap lies behind received data.
    sender.on_ack(&window_ack(&sender, 2, 32, 95, 95)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 32);
    Ok(())
}

#[test]
fn lost_repair_reopens_only_at_original_feedback_deadline() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_sent()?;
    sender.on_ack(&window_ack(&sender, 1, 5, 12, 8)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 8);
    for index in 5..13 {
        assert_eq!(receive(&right, &mut b)?.sequence, index);
    }
    sender.on_ack(&window_ack(&sender, 2, 6, 12, 7)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 0);
    let later = now + Duration::from_millis(10);
    assert_eq!(sender.poll_send(&left, &mut a, later, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 6);
    Ok(())
}

#[test]
fn nonfinal_retry_pass_survives_progress_without_reopening_sent_prefix() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_sent()?;
    sender.on_ack(&window_ack(&sender, 1, 32, 95, 96)?, now)?;
    let later = now + Duration::from_millis(10);
    assert_eq!(sender.poll_send(&left, &mut a, later, 8)?, 8);
    for index in 32..40 {
        assert_eq!(receive(&right, &mut b)?.sequence, index);
    }
    sender.on_ack(&window_ack(&sender, 2, 40, 103, 88)?, later)?;
    assert_eq!(sender.poll_send(&left, &mut a, later, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 40);
    assert_eq!(
        sender
            .repair
            .as_ref()
            .unwrap()
            .plan
            .window
            .missing_ranges
            .last()
            .unwrap()
            .end_inclusive,
        95
    );
    Ok(())
}

#[test]
fn changed_ack_keeps_partial_copy_progress() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_sent()?;
    sender.config.packet_copies = 3;
    sender.on_ack(&window_ack(&sender, 1, 0, 63, 64)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 2)?, 2);
    for _ in 0..2 {
        assert_eq!(receive(&right, &mut b)?.sequence, 0);
    }
    sender.on_ack(&window_ack(&sender, 2, 0, 62, 63)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 2)?, 2);
    assert_eq!(receive(&right, &mut b)?.sequence, 0);
    assert_eq!(receive(&right, &mut b)?.sequence, 1);
    Ok(())
}

#[test]
fn changed_ack_keeps_completion_of_same_blocked_cursor() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_sent()?;
    let io = BlockedWriter {
        socket: left,
        blocked: true.into(),
    };
    sender.on_ack(&window_ack(&sender, 1, 0, 63, 64)?, now)?;
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 1, None)?, 0);
    let held = sender.pending.as_ref().unwrap().frame.clone();
    sender.on_ack(&window_ack(&sender, 2, 0, 62, 63)?, now)?;
    io.blocked
        .store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 1, None)?, 1);
    assert_eq!(receive(&right, &mut b)?, held);
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 1, None)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 1);
    Ok(())
}

#[test]
fn reduced_copy_target_cannot_stall_existing_partial_cursor() -> Result<()> {
    let (left, right, mut a, mut b, mut sender, now) = initial_sent()?;
    sender.config.packet_copies = 3;
    sender.on_ack(&window_ack(&sender, 1, 0, 63, 64)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 2)?, 2);
    for _ in 0..2 {
        assert_eq!(receive(&right, &mut b)?.sequence, 0);
    }
    sender.config.packet_copies = 1;
    sender.on_ack(&window_ack(&sender, 2, 0, 62, 63)?, now)?;
    assert_eq!(sender.poll_send(&left, &mut a, now, 1)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 1);
    Ok(())
}

#[test]
fn proven_internal_gaps_progress_before_whole_initial_body_finishes() -> Result<()> {
    use crate::novorudp::fragments::{Reassembler, ReassemblyLimits};
    for quantum in [1, 8] {
        let (left, right, mut a, mut b) = link()?;
        let now = Instant::now();
        let body = Arc::new(OutgoingMessage::with_profile(
            [8; 32],
            vec![5; 128 * UDP_CHUNK_BYTES],
            200_000,
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
        let mut receiver = Reassembler::with_profile(
            [8; 32],
            vec![peer.clone()],
            ReassemblyLimits {
                max_message_bytes: 200_000,
                messages: 1,
                bytes: 200_000,
                peer_messages: 1,
                peer_bytes: 200_000,
                ttl: Duration::from_secs(5),
            },
            FragmentProfile::NativeUdpV2,
        )?;
        assert_eq!(sender.poll_send(&left, &mut a, now, 8)?, 8);
        for _ in 0..8 {
            let frame = receive(&right, &mut b)?;
            if ![1, 3, 5].contains(&frame.sequence) {
                receiver.push(&peer, &frame.payload, now)?;
            }
        }
        let (total, missing, window) = receiver
            .window_snapshot(&peer, body.id(), &config)?
            .unwrap();
        let window = window.unwrap();
        let ack = NovoRudpAckFrame {
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
            missing_count: missing,
            current_window: Some(window.range),
            current_window_missing_ranges: window.missing_ranges,
        };
        assert!(b.try_send(&right, &encode_object_ack(body.id(), 11, &ack)?)?);
        sender.on_ack(&receive(&left, &mut a)?, now)?;
        let mut repaired = Vec::new();
        let mut initial = Vec::new();
        for _ in 0..8 / quantum {
            let count = sender.poll_send(&left, &mut a, now, quantum)?;
            assert_eq!(count, quantum);
            for _ in 0..count {
                let frame = receive(&right, &mut b)?;
                if frame.kind == NovoRudpTransportFrameKindV0::Repair {
                    repaired.push(frame.sequence);
                } else {
                    initial.push(frame.sequence);
                }
                receiver.push(&peer, &frame.payload, now)?;
            }
        }
        assert_eq!(
            repaired,
            vec![1, 3, 5],
            "confirmed gaps cannot wait for the other 120 initial frames"
        );
        assert_eq!(
            initial,
            vec![8, 9, 10, 11, 12],
            "repair cannot starve initial data"
        );
    }
    Ok(())
}
