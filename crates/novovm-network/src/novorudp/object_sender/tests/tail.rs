use super::*;
use crate::novorudp::fragments::{Reassembler, ReassemblyLimits};

// Same object size, chunk size, default pacing/retry policy and ten-second
// operation deadline as the paired phone failure. Only loss/backpressure is
// controlled here; all frames/ACKs cross real UDP and the existing AEAD session.
struct TailFixture {
    left: UdpSocket,
    right: UdpSocket,
    a: NativeUdpSession,
    b: NativeUdpSession,
    sender: NativeObjectSender,
    objects: Reassembler,
    peer: String,
    bytes: Vec<u8>,
    epoch: u64,
}

impl TailFixture {
    fn new(extra_gap: Option<u64>) -> Result<Self> {
        let (left, right, a, b) = link()?;
        let now = Instant::now();
        let bytes = vec![9; 33_013];
        let body = Arc::new(OutgoingMessage::with_profile(
            [2; 32],
            bytes.clone(),
            100_000,
            FragmentProfile::NativeUdpV2,
        )?);
        assert_eq!(body.frame_count(), 43);
        let sender = NativeObjectSender::new(
            body,
            a.session_id(),
            11,
            NovoRudpWindowConfig::default(),
            NovoRudpPacingProfile::default(),
            now,
            Duration::from_secs(10),
        )?;
        let peer = b.remote_peer_id().to_owned();
        let objects = Reassembler::with_profile(
            [2; 32],
            vec![peer.clone()],
            ReassemblyLimits {
                max_message_bytes: 100_000,
                messages: 1,
                bytes: 100_000,
                peer_messages: 1,
                peer_bytes: 100_000,
                ttl: Duration::from_secs(30),
            },
            FragmentProfile::NativeUdpV2,
        )?;
        let mut fixture = Self {
            left,
            right,
            a,
            b,
            sender,
            objects,
            peer,
            bytes,
            epoch: 0,
        };
        while fixture.sender.initial < 43 {
            fixture.wait_pacing();
            let count =
                fixture
                    .sender
                    .poll_send(&fixture.left, &mut fixture.a, Instant::now(), 16)?;
            assert!(count > 0);
            for _ in 0..count {
                let frame = receive(&fixture.right, &mut fixture.b)?;
                if frame.sequence < 38 && Some(frame.sequence) != extra_gap {
                    fixture
                        .objects
                        .push(&fixture.peer, &frame.payload, Instant::now())?;
                }
            }
        }
        fixture.feedback(false)?;
        Ok(fixture)
    }

    fn wait_pacing(&self) {
        std::thread::sleep(
            self.sender
                .next_send
                .saturating_duration_since(Instant::now()),
        );
    }

    fn feedback(&mut self, done: bool) -> Result<()> {
        let (total, missing, window) = if done {
            (self.sender.body.frame_count() as u64, 0, None)
        } else {
            self.objects
                .window_snapshot(&self.peer, self.sender.body.id(), &self.sender.config)?
                .context("retained request")?
        };
        self.epoch += 1;
        let ack = NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: self.a.session_id(),
                epoch: self.epoch,
                sequence: None,
                window_id: window.as_ref().map(|w| w.window_id),
            },
            expected_total: total,
            receiver_done: done,
            missing_count: missing,
            current_window: window.as_ref().map(|w| w.range),
            current_window_missing_ranges: window.map(|w| w.missing_ranges).unwrap_or_default(),
        };
        assert!(self.b.try_send(
            &self.right,
            &encode_object_ack(self.sender.body.id(), 11, &ack)?
        )?);
        self.sender
            .on_ack(&receive(&self.left, &mut self.a)?, Instant::now())?;
        Ok(())
    }

    fn start_tail_repair(&mut self) -> Result<()> {
        self.wait_pacing();
        assert_eq!(
            self.sender
                .poll_send(&self.left, &mut self.a, Instant::now(), 1)?,
            0
        );
        // Wait the EXISTING one-second feedback interval, with over eight seconds
        // left. No simulated clock, deadline extension or late-loss expiry test.
        std::thread::sleep(
            self.sender
                .feedback_deadline
                .saturating_duration_since(Instant::now()),
        );
        Ok(())
    }

    fn accept_one(&mut self, expected: u64) -> Result<()> {
        let frame = receive(&self.right, &mut self.b)?;
        assert_eq!(frame.sequence, expected);
        self.objects
            .push(&self.peer, &frame.payload, Instant::now())?;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        let complete = self
            .objects
            .poll_complete(Instant::now(), 64)?
            .context("complete object")?;
        assert_eq!(complete.chunks.concat(), self.bytes);
        // Completion feedback follows the receiver's full integrity check.
        self.feedback(true)?;
        assert_eq!(self.sender.status(), ObjectSendState::Delivered);
        Ok(())
    }
}

#[test]
fn lost_five_tail_fragments_repair_after_original_feedback_wait() -> Result<()> {
    let mut f = TailFixture::new(None)?;
    f.start_tail_repair()?;
    let mut seen = Vec::new();
    loop {
        f.wait_pacing();
        let count = f.sender.poll_send(&f.left, &mut f.a, Instant::now(), 16)?;
        assert!(count > 0);
        for _ in 0..count {
            let frame = receive(&f.right, &mut f.b)?;
            seen.push(frame.sequence);
            f.objects.push(&f.peer, &frame.payload, Instant::now())?;
        }
        if f.sender.repair.is_none() {
            break;
        }
    }
    assert!(seen.iter().all(|index| (38..43).contains(index)));
    assert!((38..43).all(|index| seen.contains(&index)));
    f.finish()
}

#[test]
fn progress_ack_must_not_cancel_an_active_five_fragment_tail_repair() -> Result<()> {
    let mut f = TailFixture::new(None)?;
    let original_deadline = f.sender.deadline;
    f.start_tail_repair()?;
    for index in 38..43 {
        f.wait_pacing();
        let now = Instant::now();
        assert_eq!(
            f.sender.poll_send(&f.left, &mut f.a, now, 1)?,
            1,
            "tail {index} must continue after new progress ACK, not wait another feedback interval"
        );
        f.accept_one(index)?;
        f.feedback(false)?;
        if index < 42 {
            assert!(f.sender.next_send < f.sender.feedback_deadline);
            assert_eq!(f.sender.deadline, original_deadline);
        }
    }
    f.finish()
}

#[test]
fn progress_ack_during_blocked_tail_copy_preserves_new_repair_cursor() -> Result<()> {
    let mut f = TailFixture::new(None)?;
    let original_deadline = f.sender.deadline;
    f.start_tail_repair()?;
    assert_eq!(f.sender.poll_send(&f.left, &mut f.a, Instant::now(), 1)?, 1);
    f.accept_one(38)?;
    f.wait_pacing();
    let io = BlockedWriter {
        socket: f.left.try_clone()?,
        blocked: true.into(),
    };
    assert_eq!(
        f.sender
            .poll_send_limited(&io, &mut f.a, Instant::now(), 1, None)?,
        0
    );
    let original = f
        .sender
        .pending
        .as_ref()
        .context("blocked duplicate")?
        .frame
        .clone();
    assert_eq!(original.sequence, 38);
    f.feedback(false)?; // 38 arrived; a redundant copy is still owned by the writer.
    assert_eq!(
        f.sender.repair.as_ref().context("new tail repair")?.index,
        39
    );
    io.blocked
        .store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        f.sender
            .poll_send_limited(&io, &mut f.a, Instant::now(), 1, None)?,
        1
    );
    assert_eq!(receive(&f.right, &mut f.b)?, original);
    let cursor = f.sender.repair.as_ref().context("new cursor retained")?;
    assert_eq!((cursor.index, cursor.copy), (39, 0));
    assert!(f.sender.pending.is_none());
    assert_eq!(
        f.sender
            .poll_send_limited(&io, &mut f.a, Instant::now(), 1, None)?,
        1
    );
    f.accept_one(39)?;
    let cursor = f
        .sender
        .repair
        .as_ref()
        .context("new cursor advanced once")?;
    assert_eq!((cursor.index, cursor.copy), (39, 1));
    assert_eq!(f.sender.deadline, original_deadline);
    Ok(())
}

#[test]
fn blocked_tail_write_retains_exact_frame_and_resumes_original_cursor() -> Result<()> {
    let mut f = TailFixture::new(None)?;
    f.start_tail_repair()?;
    let io = BlockedWriter {
        socket: f.left.try_clone()?,
        blocked: true.into(),
    };
    assert_eq!(
        f.sender
            .poll_send_limited(&io, &mut f.a, Instant::now(), 1, None)?,
        0
    );
    let original = f
        .sender
        .pending
        .as_ref()
        .context("blocked tail retained")?
        .frame
        .clone();
    assert_eq!(original.sequence, 38);
    f.feedback(false)?; // New authenticated epoch, unchanged missing snapshot.
    io.blocked
        .store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        f.sender
            .poll_send_limited(&io, &mut f.a, Instant::now(), 1, None)?,
        1
    );
    let resumed = receive(&f.right, &mut f.b)?;
    assert_eq!(resumed, original);
    assert_eq!(f.sender.repair.as_ref().context("repair cursor")?.copy, 1);
    assert!(f.sender.pending.is_none());
    assert_eq!(f.sender.status(), ObjectSendState::Sending);
    Ok(())
}

#[test]
fn budget_wait_retains_exact_tail_frame_without_spending_retry_or_cursor() -> Result<()> {
    let mut f = TailFixture::new(None)?;
    f.start_tail_repair()?;
    let now = Instant::now();
    let mut budget = wire_budget::NativeWireBudget::new(wire_budget::MIN_BYTES_PER_MINUTE, now)?;
    let held = budget
        .reserve_send(wire_budget::MIN_BYTES_PER_MINUTE as usize, now)?
        .context("other write")?;
    assert_eq!(
        f.sender
            .poll_send_limited(&f.left, &mut f.a, now, 1, Some(&mut budget))?,
        0
    );
    let original = f
        .sender
        .pending
        .as_ref()
        .context("budget-blocked tail retained")?
        .frame
        .clone();
    assert_eq!(original.sequence, 38);
    assert_eq!(f.sender.attempts, 0);
    assert_eq!(f.sender.repair.as_ref().context("repair cursor")?.copy, 0);
    budget.finish_send(held, 0, false, Instant::now())?;
    assert_eq!(
        f.sender
            .poll_send_limited(&f.left, &mut f.a, Instant::now(), 1, Some(&mut budget))?,
        1
    );
    assert_eq!(receive(&f.right, &mut f.b)?, original);
    assert_eq!(f.sender.attempts, 1);
    assert_eq!(f.sender.repair.as_ref().context("repair cursor")?.copy, 1);
    assert!(f.sender.pending.is_none());
    Ok(())
}

#[test]
fn progress_during_internal_gap_repair_does_not_promote_unconfirmed_tail() -> Result<()> {
    let mut f = TailFixture::new(Some(10))?;
    f.wait_pacing();
    assert_eq!(f.sender.poll_send(&f.left, &mut f.a, Instant::now(), 1)?, 1);
    f.accept_one(10)?;
    f.feedback(false)?;
    assert!(
        f.sender.repair.is_none(),
        "internal-gap repair is not an armed tail retry"
    );
    f.wait_pacing();
    assert_eq!(f.sender.poll_send(&f.left, &mut f.a, Instant::now(), 1)?, 0);
    assert_eq!(f.sender.status(), ObjectSendState::Sending);
    Ok(())
}
