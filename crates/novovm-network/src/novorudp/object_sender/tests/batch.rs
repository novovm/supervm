use super::*;
use crate::novorudp::{
    carrier::{NativeDatagramIo, NativeRoute},
    fragments::UDP_CHUNK_BYTES,
    wire_budget::NativeWireBudget,
};
use std::{
    collections::VecDeque,
    io,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

enum BatchOutcome {
    Prefix(usize),
    WouldBlock,
    Error,
}

// Only OS admission is controlled. Every accepted wire crosses the real socket
// and is authenticated by the original receiving session.
struct BatchWriter {
    socket: UdpSocket,
    outcomes: Mutex<VecDeque<BatchOutcome>>,
    batches: Mutex<Vec<Vec<usize>>>,
    singles: AtomicUsize,
    blocked_single: AtomicBool,
    deferred: bool,
}

impl BatchWriter {
    fn new(socket: UdpSocket, outcomes: impl IntoIterator<Item = BatchOutcome>) -> Self {
        Self {
            socket,
            outcomes: Mutex::new(outcomes.into_iter().collect()),
            batches: Mutex::new(Vec::new()),
            singles: AtomicUsize::new(0),
            blocked_single: AtomicBool::new(false),
            deferred: false,
        }
    }
}

impl NativeDatagramIo for BatchWriter {
    fn try_receive(&self, buffer: &mut [u8]) -> io::Result<Option<(usize, NativeRoute)>> {
        self.socket.try_receive(buffer)
    }

    fn try_send(&self, bytes: &[u8], route: NativeRoute) -> io::Result<usize> {
        self.singles.fetch_add(1, Ordering::Relaxed);
        if self.blocked_single.load(Ordering::Relaxed) {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.socket.try_send(bytes, route)
    }

    fn send_batch_limit(&self) -> usize {
        32
    }

    fn try_send_batch(&self, datagrams: &[&[u8]], route: NativeRoute) -> io::Result<usize> {
        self.batches
            .lock()
            .unwrap()
            .push(datagrams.iter().map(|wire| wire.len()).collect());
        let count = match self.outcomes.lock().unwrap().pop_front() {
            Some(BatchOutcome::Prefix(count)) => count.min(datagrams.len()),
            Some(BatchOutcome::WouldBlock) => return Err(io::ErrorKind::WouldBlock.into()),
            Some(BatchOutcome::Error) => return Err(io::Error::other("controlled batch error")),
            None => datagrams.len(),
        };
        let mut sent = 0;
        for wire in datagrams.iter().take(count) {
            match self.socket.try_send(wire, route) {
                Ok(size) if size == wire.len() => sent += 1,
                Ok(_) => return Err(io::Error::other("unexpected partial UDP datagram")),
                Err(_) if sent != 0 => return Ok(sent),
                Err(error) => return Err(error),
            }
        }
        Ok(sent)
    }

    fn deferred_writes(&self) -> bool {
        self.deferred
    }
}

fn initial_sender(
    session: &NativeUdpSession,
    bytes: Vec<u8>,
    now: Instant,
) -> Result<NativeObjectSender> {
    let body = Arc::new(OutgoingMessage::with_profile(
        [7; 32],
        bytes,
        100_000,
        FragmentProfile::NativeUdpV2,
    )?);
    let (config, pacing) = settings();
    let mut sender = NativeObjectSender::new(
        body,
        session.session_id(),
        11,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    sender.enable_diagnostics_v1();
    sender.traffic = Some(Default::default());
    Ok(sender)
}

#[test]
fn initial_batch_commits_only_actual_prefix_and_retains_cursor_after_block_or_error() -> Result<()>
{
    let (left, right, mut a, mut b) = link()?;
    let io = BatchWriter::new(
        left,
        [
            BatchOutcome::Prefix(2),
            BatchOutcome::WouldBlock,
            BatchOutcome::Error,
        ],
    );
    let now = Instant::now();
    let mut sender = initial_sender(&a, vec![3; 4 * UDP_CHUNK_BYTES], now)?;
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 4, None)?, 2);
    let mut received_bytes = 0;
    for sequence in 0..2 {
        let frame = receive(&right, &mut b)?;
        assert_eq!(frame.sequence, sequence);
        assert_eq!(frame.kind, NovoRudpTransportFrameKindV0::Data);
        received_bytes += frame.payload.len();
    }
    assert_eq!((sender.initial, sender.copy), (2, 0));
    let sent_bytes = sender.traffic().unwrap().initial.bytes;
    assert!(sent_bytes > received_bytes as u64);
    let feedback_deadline = sender.feedback_deadline;
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 4, None)?, 0);
    assert!(sender.poll_send_limited(&io, &mut a, now, 4, None).is_err());
    assert_eq!((sender.initial, sender.copy), (2, 0));
    assert_eq!(sender.feedback_deadline, feedback_deadline);
    assert!(sender.pending.is_none());
    assert_eq!(sender.traffic().unwrap().initial.bytes, sent_bytes);
    assert_eq!(sender.diagnostic_progress_v1().unwrap().initial_packets, 2);
    assert!(b.try_receive(&right)?.is_none());
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 4, None)?, 2);
    for sequence in 2..4 {
        assert_eq!(receive(&right, &mut b)?.sequence, sequence);
    }
    assert_eq!((sender.initial, sender.copy), (4, 0));
    assert_eq!(sender.traffic().unwrap().initial.packets, 4);
    assert_eq!(io.singles.load(Ordering::Relaxed), 0);
    assert_eq!(io.batches.lock().unwrap().len(), 4);
    Ok(())
}

#[test]
fn initial_batch_preserves_copy_cursor_short_tail_poll_quantum_and_pause() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let io = BatchWriter::new(left, []);
    let now = Instant::now();
    let mut sender = initial_sender(&a, vec![8; 2 * UDP_CHUNK_BYTES + 7], now)?;
    sender.config.packet_copies = 2;
    sender.pacing.packet_copies = 3;
    sender.config.batch_size = 4;
    sender.pacing.batch_size = 4;
    sender.config.batch_pause_ms = 5;
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 7, None)?, 4);
    for sequence in [0, 0, 0, 1] {
        assert_eq!(receive(&right, &mut b)?.sequence, sequence);
    }
    assert_eq!((sender.initial, sender.copy), (1, 1));
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 7, None)?, 0);
    assert_eq!(io.batches.lock().unwrap().len(), 1);
    let later = now + Duration::from_millis(5);
    // The full-size prefix ends before the shorter final fragment.
    assert_eq!(sender.poll_send_limited(&io, &mut a, later, 4, None)?, 2);
    for _ in 0..2 {
        assert_eq!(receive(&right, &mut b)?.sequence, 1);
    }
    assert_eq!((sender.initial, sender.copy), (2, 0));
    assert_eq!(
        sender.poll_send_limited(&io, &mut a, later + Duration::from_millis(5), 4, None)?,
        3
    );
    for _ in 0..3 {
        let frame = receive(&right, &mut b)?;
        assert_eq!(frame.sequence, 2);
        assert!(frame.payload.len() < UDP_CHUNK_BYTES);
    }
    assert_eq!((sender.initial, sender.copy), (3, 0));
    assert_eq!(sender.diagnostic_progress_v1().unwrap().initial_packets, 9);
    let batches = io.batches.lock().unwrap();
    assert_eq!(batches.iter().map(Vec::len).collect::<Vec<_>>(), [4, 2]);
    assert!(batches
        .iter()
        .all(|batch| batch.iter().all(|len| *len == batch[0])));
    assert_eq!(io.singles.load(Ordering::Relaxed), 3);
    Ok(())
}

#[test]
fn single_fragment_reply_and_single_write_quantum_skip_bulk_preparation() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let io = BatchWriter::new(left, []);
    let now = Instant::now();
    let mut reply = initial_sender(&a, vec![9; 80], now)?;
    assert_eq!(reply.poll_send_limited(&io, &mut a, now, 8, None)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 0);
    let mut body = initial_sender(&a, vec![5; 3 * UDP_CHUNK_BYTES], now)?;
    assert_eq!(body.poll_send_limited(&io, &mut a, now, 1, None)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 0);
    assert!(io.batches.lock().unwrap().is_empty());
    assert_eq!(io.singles.load(Ordering::Relaxed), 2);
    Ok(())
}

#[test]
fn metered_initial_and_authenticated_gap_repair_keep_original_single_writes() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let io = BatchWriter::new(left, []);
    let now = Instant::now();
    let mut sender = initial_sender(&a, vec![2; 3 * UDP_CHUNK_BYTES], now)?;
    let mut budget = NativeWireBudget::new(1024 * 1024, now)?;
    assert_eq!(
        sender.poll_send_limited(&io, &mut a, now, 3, Some(&mut budget))?,
        3
    );
    for sequence in 0..3 {
        assert_eq!(receive(&right, &mut b)?.sequence, sequence);
    }
    assert!(io.batches.lock().unwrap().is_empty());
    assert_eq!(io.singles.load(Ordering::Relaxed), 3);
    assert_eq!(
        budget.usage(now).sent_bytes,
        sender.traffic().unwrap().initial.bytes
    );
    assert_eq!(budget.usage(now).reserved_send_bytes, 0);
    assert!(b.try_send(&right, &gap_ack(&sender, 1, 0, 0)?)?);
    sender.on_ack(&receive(&io.socket, &mut a)?, now)?;
    assert!(sender.repair.is_some());
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 4, None)?, 1);
    let repaired = receive(&right, &mut b)?;
    assert_eq!(
        (repaired.kind, repaired.sequence),
        (NovoRudpTransportFrameKindV0::Repair, 0)
    );
    assert!(io.batches.lock().unwrap().is_empty());
    assert_eq!(sender.diagnostic_progress_v1().unwrap().repair_packets, 1);
    Ok(())
}

#[test]
fn deferred_pending_write_never_enters_batch_or_advances_before_real_completion() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let mut io = BatchWriter::new(left, []);
    io.deferred = true;
    io.blocked_single.store(true, Ordering::Relaxed);
    let now = Instant::now();
    let mut sender = initial_sender(&a, vec![4; 3 * UDP_CHUNK_BYTES], now)?;
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 8, None)?, 0);
    assert!(sender.pending.is_some());
    assert_eq!((sender.initial, sender.copy), (0, 0));
    assert!(b.try_receive(&right)?.is_none());
    io.blocked_single.store(false, Ordering::Relaxed);
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 8, None)?, 1);
    assert_eq!(receive(&right, &mut b)?.sequence, 0);
    assert_eq!((sender.initial, sender.copy), (1, 0));
    assert!(sender.pending.is_none());
    assert!(io.batches.lock().unwrap().is_empty());
    assert_eq!(sender.traffic().unwrap().initial.packets, 1);
    Ok(())
}

#[test]
fn each_batched_datagram_retains_original_authentication_and_replay_contract() -> Result<()> {
    let (left, right, mut a, mut b) = link()?;
    let io = BatchWriter::new(left, []);
    let now = Instant::now();
    let mut sender = initial_sender(&a, vec![6; 2 * UDP_CHUNK_BYTES], now)?;
    assert_eq!(sender.poll_send_limited(&io, &mut a, now, 8, None)?, 2);
    let mut wire = [0; 1201];
    right.set_nonblocking(false)?;
    right.set_read_timeout(Some(Duration::from_secs(2)))?;
    for sequence in 0..2 {
        let (count, source) = right.recv_from(&mut wire)?;
        assert!(count <= 1200);
        let mut tampered = wire[..count].to_vec();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(b.open(source, &tampered).is_err());
        let frame = b.open(source, &wire[..count])?;
        assert_eq!(frame.sequence, sequence);
        assert_eq!(frame.kind, NovoRudpTransportFrameKindV0::Data);
        assert!(b.open(source, &wire[..count]).is_err());
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_owned_socket_segmented_batch_reassembles_and_acknowledges_original_object() -> Result<()>
{
    use crate::novorudp::{
        carrier::owned_udp_io,
        fragments::{Reassembler, ReassemblyLimits},
    };
    let (left, right, mut a, mut b) = link()?;
    let left = Arc::new(left);
    let io = owned_udp_io(left.clone(), true);
    if io.send_batch_limit() == 1 {
        eprintln!("Windows UDP segmentation unavailable; owned-socket batch case not exercised");
        return Ok(());
    }
    let now = Instant::now();
    let bytes = vec![1; 3 * UDP_CHUNK_BYTES + 7];
    let mut sender = initial_sender(&a, bytes.clone(), now)?;
    let peer = b.remote_peer_id().to_owned();
    let mut receiver = Reassembler::with_profile(
        [7; 32],
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
    for expected in [3, 1] {
        assert_eq!(
            sender.poll_send_limited(io.as_ref(), &mut a, now, 8, None)?,
            expected
        );
        for _ in 0..expected {
            let frame = receive(&right, &mut b)?;
            assert_eq!(frame.kind, NovoRudpTransportFrameKindV0::Data);
            receiver.push(&peer, &frame.payload, now)?;
        }
    }
    let complete = receiver
        .poll_complete(now, 64)?
        .context("complete segmented object")?;
    assert_eq!(complete.chunks.concat(), bytes);
    // A normal single control frame must reset the socket-wide segment size.
    let done = encode_object_ack(
        sender.body.id(),
        11,
        &NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: a.session_id(),
                epoch: 1,
                sequence: None,
                window_id: None,
            },
            expected_total: 4,
            receiver_done: true,
            missing_count: 0,
            current_window: None,
            current_window_missing_ranges: vec![],
        },
    )?;
    assert!(a.try_send_limited(io.as_ref(), &done, now, None)?.is_some());
    assert_eq!(receive(&right, &mut b)?, done);
    assert!(b.try_send(&right, &done)?);
    sender.on_ack(&receive(&left, &mut a)?, now)?;
    assert_eq!(sender.status(), ObjectSendState::Delivered);
    assert_eq!(sender.traffic().unwrap().initial.packets, 4);
    Ok(())
}
