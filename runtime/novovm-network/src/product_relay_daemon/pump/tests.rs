use super::super::tests::daemon_tls_io_fixture_v1;
use super::*;

fn fixture() -> (RelayServerPumpV1, rustls::ClientConnection, TcpStream) {
    let (mut stream, client, peer) = daemon_tls_io_fixture_v1();
    stream
        .sock
        .inner
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    stream
        .sock
        .inner
        .set_write_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    (RelayServerPumpV1::new(stream).unwrap(), client, peer)
}

fn masked(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x80 | opcode];
    match payload.len() {
        0..=125 => bytes.push(0x80 | payload.len() as u8),
        126..=65_535 => {
            bytes.push(0x80 | 126);
            bytes.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        _ => {
            bytes.push(0x80 | 127);
            bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
    }
    let key = [3, 7, 11, 17];
    bytes.extend_from_slice(&key);
    bytes.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ key[index % 4]),
    );
    bytes
}

fn send_plaintext(
    client: &mut rustls::ClientConnection,
    peer: &mut TcpStream,
    bytes: &[u8],
    close: bool,
) {
    client.writer().write_all(bytes).unwrap();
    if close {
        client.send_close_notify();
    }
    while client.wants_write() {
        client.write_tls(peer).unwrap();
    }
}

fn next_frame(pump: &mut RelayServerPumpV1) -> (WebSocketFrameV1, Instant) {
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        let progress = pump.poll().unwrap();
        if let Some(frame) = progress.frame {
            return frame;
        }
        assert!(Instant::now() < end, "server pump frame did not arrive");
        if !progress.progressed {
            pump.wait(Some(end)).unwrap();
        }
    }
}

fn assert_terminal(pump: &mut RelayServerPumpV1) {
    let calls = pump.stream.sock.test_writes.calls;
    assert!(pump.poll().is_err());
    assert!(pump.wait(None).is_err());
    assert!(pump
        .start_control(0xA, Vec::new(), Instant::now() + Duration::from_secs(1))
        .is_err());
    assert_eq!(pump.stream.sock.test_writes.calls, calls);
    assert!(!pump.can_write());
    assert!(pump.writing.is_none());
}

#[test]
fn masked_frame_preflight_shares_strict_parser_and_bounds() {
    for size in [0, 3, 126, 65_536] {
        let bytes = masked(0x2, &vec![71; size]);
        for truncated in [0, 1, bytes.len() - 1] {
            assert_eq!(
                complete_masked_frame_len(&bytes[..truncated]).unwrap(),
                None
            );
        }
        assert_eq!(
            complete_masked_frame_len(&bytes).unwrap(),
            Some(bytes.len())
        );
        assert!(
            matches!(read_websocket_frame_with_guard_v1(&mut bytes.as_slice(), true,
            PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1, None).unwrap(), WebSocketFrameV1::Binary(value) if value == vec![71;size])
        );
    }
    for invalid in [
        vec![0x82, 0],
        vec![0x02, 0x80],
        vec![0xC2, 0x80],
        vec![0x89, 0xFE, 0, 126],
    ] {
        assert!(complete_masked_frame_len(&invalid).is_err());
    }
    let mut oversized = vec![0x82, 0xFF];
    oversized
        .extend_from_slice(&((PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 + 1) as u64).to_be_bytes());
    assert!(complete_masked_frame_len(&oversized).is_err());
}

#[test]
fn completed_prefix_advances_timestamp_but_append_never_refreshes_unread_prefix() {
    let (mut pump, _client, _peer) = fixture();
    let first = masked(0x2, b"first");
    let second = masked(0x2, b"second");
    let third = masked(0x2, b"third");
    let old = Instant::now() + Duration::from_secs(2);
    let new = old + Duration::from_secs(2);
    let mut initial = first.clone();
    initial.extend_from_slice(&second[..3]);
    pump.append_plaintext(&initial, old).unwrap();
    pump.read_deadline = Some(old);
    assert_eq!(pump.take_frame().unwrap().unwrap().1, old);
    pump.append_plaintext(&second[3..], new).unwrap();
    pump.append_plaintext(&third, new).unwrap();
    assert_eq!(pump.read_deadline, Some(old));
    assert_eq!(pump.take_frame().unwrap().unwrap().1, old);
    assert_eq!(pump.read_deadline, Some(new));
    assert_eq!(pump.take_frame().unwrap().unwrap().1, new);
    assert!(pump.read_buffer.is_empty() && pump.read_stamps.is_empty());
    assert_eq!(pump.read_deadline, None);
}

#[test]
fn plaintext_storage_and_timestamp_metadata_are_bounded() {
    let (mut pump, _client, _peer) = fixture();
    let end = Instant::now() + Duration::from_secs(2);
    for _ in 0..MAX_READ_BUFFER / QUANTUM {
        pump.append_plaintext(&[0; QUANTUM], end).unwrap();
    }
    pump.append_plaintext(&[0; MAX_READ_BUFFER % QUANTUM], end)
        .unwrap();
    assert_eq!(pump.read_buffer.len(), MAX_READ_BUFFER);
    assert!(pump.read_buffer.capacity() <= MAX_READ_BUFFER);
    assert!(pump.read_stamps.len() <= MAX_READ_STAMPS);
    assert!(pump.read_stamps.capacity() <= MAX_READ_STAMPS);
    assert!(pump.append_plaintext(&[0], end).is_err());
}

#[test]
fn independent_expired_read_write_stall_and_queued_response_remain_terminal() {
    for direction in 0..4 {
        let (mut pump, mut client, mut peer) = fixture();
        pump.start_control(
            0x2,
            b"outbound".to_vec(),
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        send_plaintext(
            &mut client,
            &mut peer,
            &masked(0x2, b"inbound cannot extend write"),
            false,
        );
        let expired = Instant::now() - Duration::from_millis(1);
        match direction {
            0 => pump.read_deadline = Some(expired),
            1 => pump.write_deadline = Some(expired),
            2 => pump.write_stall_deadline = Some(expired),
            _ => {
                assert!(pump.wait(Some(expired)).is_err());
            }
        }
        assert_terminal(&mut pump);
    }
}

#[test]
fn initial_tls_plaintext_inherits_handshake_deadline_without_refresh() {
    let (mut stream, mut client, _peer) = daemon_tls_io_fixture_v1();
    let deadline = Instant::now() + Duration::from_secs(2);
    stream.sock.deadline.begin_v1(deadline).unwrap();
    client
        .writer()
        .write_all(&masked(0x2, b"already buffered"))
        .unwrap();
    let mut wire = Vec::new();
    client.write_tls(&mut wire).unwrap();
    stream.conn.read_tls(&mut wire.as_slice()).unwrap();
    stream.conn.process_new_packets().unwrap();
    let mut pump = RelayServerPumpV1::new(stream).unwrap();
    assert_eq!(pump.read_deadline, Some(deadline));
    assert_eq!(
        pump.stream.sock.deadline.state.lock().unwrap().deadline,
        None
    );
    let (frame, original) = next_frame(&mut pump);
    assert_eq!(original, deadline);
    assert!(matches!(frame, WebSocketFrameV1::Binary(bytes) if bytes == b"already buffered"));
}

#[test]
fn real_tls_close_notify_delivers_all_complete_frames_then_terminates() {
    let (mut pump, mut client, mut peer) = fixture();
    let mut bytes = masked(0x2, b"last one");
    bytes.extend_from_slice(&masked(0x2, b"last two"));
    send_plaintext(&mut client, &mut peer, &bytes, true);
    for expected in [b"last one", b"last two"] {
        assert!(
            matches!(next_frame(&mut pump).0, WebSocketFrameV1::Binary(bytes) if bytes == expected)
        );
    }
    let end = Instant::now() + Duration::from_secs(2);
    while pump.poll().is_ok() {
        assert!(Instant::now() < end, "close_notify became permanent idle");
    }
    assert_terminal(&mut pump);
}

#[test]
fn real_tls_close_notify_rejects_partial_websocket_frame() {
    let (mut pump, mut client, mut peer) = fixture();
    let bytes = masked(0x2, b"partial");
    send_plaintext(&mut client, &mut peer, &bytes[..bytes.len() - 1], true);
    let end = Instant::now() + Duration::from_secs(2);
    while let Ok(progress) = pump.poll() {
        assert!(progress.frame.is_none());
        assert!(Instant::now() < end);
    }
    assert_terminal(&mut pump);
}

#[test]
fn consumed_frame_keeps_its_deadline_after_read_timer_advances() {
    for has_successor in [false, true] {
        let (mut pump, _client, _peer) = fixture();
        let original = Instant::now() + Duration::from_secs(60);
        let successor = original + Duration::from_secs(60);
        let mut prefix = masked(0x2, b"already consumed prefix");
        prefix.extend_from_slice(&masked(0x2, b"deadline-bearing frame"));
        pump.append_plaintext(&prefix, original).unwrap();
        pump.read_deadline = Some(original);
        // Consuming the prefix seals its stamp, so a later frame owns its
        // later timestamp rather than extending this old unread prefix.
        pump.take_frame().unwrap().unwrap();
        if has_successor {
            pump.append_plaintext(&masked(0x2, b"later frame"), successor)
                .unwrap();
        }
        let frame = pump.take_frame().unwrap().unwrap();
        assert_eq!(frame.1, original);
        assert_eq!(pump.read_deadline, has_successor.then_some(successor));
        pump.check_deadlines().unwrap();

        // Model expiry during the remaining write quantum without sleeping
        // or depending on parser speed. This is the actual final poll gate.
        let progress = ServerPumpProgress {
            frame: Some(frame),
            delivery_finished: false,
            progressed: true,
        }
        .checked_frame_deadline(original - Duration::from_nanos(1))
        .unwrap();
        assert!(progress.checked_frame_deadline(original).is_err());
    }
}

#[test]
fn real_tls_incremental_control_exact_wire_and_quantum_no_implicit_complete_io() {
    let (mut pump, mut client, mut peer) = fixture();
    let payload = vec![37; QUANTUM * 3];
    let mut expected = Vec::new();
    write_websocket_frame_v1(&mut expected, 0x2, &payload).unwrap();
    pump.start_control(0x2, payload, Instant::now() + Duration::from_secs(2))
        .unwrap();
    let mut polls = 0;
    while !pump.can_write() {
        let progress = pump.poll().unwrap();
        assert!(!progress.delivery_finished);
        polls += 1;
        assert!(polls < 128);
        if !progress.progressed {
            pump.wait(None).unwrap();
        }
    }
    assert!(polls >= 4);
    assert!(pump
        .stream
        .sock
        .test_writes
        .attempted
        .iter()
        .all(|bytes| bytes.len() <= QUANTUM));
    let mut plaintext = Vec::new();
    while plaintext.len() < expected.len() {
        assert!(client.read_tls(&mut peer).unwrap() != 0);
        client.process_new_packets().unwrap();
        let mut bytes = [0; QUANTUM];
        loop {
            match client.reader().read(&mut bytes) {
                Ok(0) => panic!("unexpected TLS close"),
                Ok(count) => plaintext.extend_from_slice(&bytes[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("TLS read: {error}"),
            }
        }
    }
    assert_eq!(plaintext, expected);
}

#[test]
fn real_tls_partial_or_indeterminate_write_is_terminal_without_retransmit() {
    for fault in [
        ProductRelayDaemonTestWriteFaultV1::ZeroProgressTimeout,
        ProductRelayDaemonTestWriteFaultV1::TimeoutAfterProgress(7),
        ProductRelayDaemonTestWriteFaultV1::ExpireAfterProgress(7),
    ] {
        let (mut pump, _client, mut peer) = fixture();
        pump.stream.sock.test_writes.fault = Some(fault);
        pump.start_control(
            0x2,
            b"cannot retry".to_vec(),
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        assert!(pump.poll().is_err());
        let sent: Vec<u8> = pump
            .stream
            .sock
            .test_writes
            .sent
            .iter()
            .flatten()
            .copied()
            .collect();
        let mut actual = vec![0; sent.len()];
        peer.read_exact(&mut actual).unwrap();
        assert_eq!(actual, sent);
        if matches!(
            fault,
            ProductRelayDaemonTestWriteFaultV1::ExpireAfterProgress(_)
        ) {
            // Rustls consumed the actual successful prefix before poison was
            // returned; only its unsent suffix remains, never a second prefix.
            let mut pending = Vec::new();
            pump.stream.conn.write_tls(&mut pending).unwrap();
            assert!(pending.starts_with(&pump.stream.sock.test_writes.attempted[0][7..]));
        }
        assert_terminal(&mut pump);
    }
}

#[test]
fn tls_record_tracker_retains_split_header_body_and_advances_only_on_completion() {
    let mut records = InboundTlsRecords::default();
    let old = Instant::now();
    let new = old + Duration::from_secs(1);
    records.observe(&[23, 3], old).unwrap();
    let original = frame_deadline(old).unwrap();
    assert_eq!(records.pending_deadline(), Some(original));
    records.observe(&[3, 0, 2, 91], new).unwrap();
    assert_eq!(records.pending_deadline(), Some(original));
    records.observe(&[92, 23, 3, 3, 0, 1], new).unwrap();
    assert_eq!(records.observed, Some(original));
    assert_eq!(
        records.pending_deadline(),
        Some(frame_deadline(new).unwrap())
    );
    records.observe(&[93], new).unwrap();
    assert_eq!(records.pending_deadline(), None);
}

#[test]
fn real_tls_completed_frame_does_not_clear_next_partial_record_deadline() {
    let (mut pump, mut client, mut peer) = fixture();
    let mut first_record = Vec::new();
    client
        .writer()
        .write_all(&masked(0x2, b"complete prefix"))
        .unwrap();
    client.write_tls(&mut first_record).unwrap();
    let mut second_record = Vec::new();
    client
        .writer()
        .write_all(&masked(0x2, b"unfinished suffix"))
        .unwrap();
    client.write_tls(&mut second_record).unwrap();
    let mut first_read = first_record;
    first_read.extend_from_slice(&second_record[..7]);
    peer.write_all(&first_read).unwrap();
    assert!(
        matches!(next_frame(&mut pump).0, WebSocketFrameV1::Binary(bytes) if bytes == b"complete prefix")
    );
    let original = pump
        .stream
        .sock
        .inbound_tls_records
        .pending_deadline()
        .expect("partial encrypted record retained");
    assert_eq!(pump.read_deadline, Some(original));
    assert!(pump.read_buffer.is_empty());
    assert_eq!(pump.tls_plaintext_bytes, 0);
    peer.write_all(&second_record[7..]).unwrap();
    let (frame, deadline) = next_frame(&mut pump);
    assert_eq!(deadline, original);
    assert!(matches!(frame, WebSocketFrameV1::Binary(bytes) if bytes == b"unfinished suffix"));
    assert_eq!(pump.read_deadline, None);
}

#[test]
fn shutdown_poison_is_shared_and_does_not_write_a_new_frame() {
    let (mut pump, _client, _peer) = fixture();
    pump.stream
        .sock
        .deadline
        .stopping
        .store(true, Ordering::Release);
    assert_terminal(&mut pump);
}

#[test]
fn completed_frame_flush_watermark_excludes_later_tls_key_update() {
    let (mut pump, mut client, mut peer) = fixture();
    pump.start_control(
        0x2,
        b"frame before key update".to_vec(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    let first = pump.poll().unwrap();
    assert!(first.progressed);
    assert!(pump
        .writing
        .as_ref()
        .is_some_and(|writing| writing.flush_remaining.is_some()));
    pump.stream.conn.refresh_traffic_keys().unwrap();
    assert!(pump.poll().unwrap().progressed);
    assert!(
        pump.writing.is_none(),
        "completed frame must not wait for later TLS protocol output"
    );
    assert!(pump.stream.conn.wants_write());
    assert!(
        !pump.can_write(),
        "next frame still waits for pending protocol output"
    );

    let mut plaintext = Vec::new();
    let mut expected = Vec::new();
    write_websocket_frame_v1(&mut expected, 0x2, b"frame before key update").unwrap();
    while plaintext.len() < expected.len() {
        assert!(client.read_tls(&mut peer).unwrap() != 0);
        client.process_new_packets().unwrap();
        let mut bytes = [0; 128];
        loop {
            match client.reader().read(&mut bytes) {
                Ok(0) => panic!("unexpected TLS close"),
                Ok(count) => plaintext.extend_from_slice(&bytes[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("TLS read: {error}"),
            }
        }
    }
    assert_eq!(plaintext, expected);
    assert!(pump.poll().unwrap().progressed);
    assert!(pump.can_write());
}

#[test]
fn real_tls_only_key_update_does_not_start_a_phantom_websocket_deadline() {
    let (mut pump, mut client, mut peer) = fixture();
    client.refresh_traffic_keys().unwrap();
    while client.wants_write() {
        client.write_tls(&mut peer).unwrap();
    }
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        let progress = pump.poll().unwrap();
        assert!(progress.frame.is_none());
        if progress.progressed {
            break;
        }
        assert!(Instant::now() < end);
        pump.wait(Some(end)).unwrap();
    }
    assert!(pump.read_buffer.is_empty());
    assert_eq!(pump.tls_plaintext_bytes, 0);
    assert_eq!(
        pump.stream.sock.inbound_tls_records.pending_deadline(),
        None
    );
    assert_eq!(
        pump.read_deadline, None,
        "complete key update must not make idle into a partial WS frame"
    );
    send_plaintext(
        &mut client,
        &mut peer,
        &masked(0x2, b"after key update"),
        false,
    );
    assert!(
        matches!(next_frame(&mut pump).0, WebSocketFrameV1::Binary(bytes) if bytes == b"after key update")
    );
}

#[test]
fn real_tls_key_update_during_partial_websocket_keeps_original_deadline() {
    let (mut pump, mut client, mut peer) = fixture();
    let frame = masked(0x2, b"same frame across key update");
    send_plaintext(&mut client, &mut peer, &frame[..3], false);
    let end = Instant::now() + Duration::from_secs(2);
    while pump.read_buffer.len() != 3 {
        let progress = pump.poll().unwrap();
        assert!(progress.frame.is_none());
        assert!(Instant::now() < end);
        if !progress.progressed {
            pump.wait(Some(end)).unwrap();
        }
    }
    let original = pump.read_deadline.unwrap();
    client.refresh_traffic_keys().unwrap();
    while client.wants_write() {
        client.write_tls(&mut peer).unwrap();
    }
    loop {
        let progress = pump.poll().unwrap();
        assert!(progress.frame.is_none());
        assert_eq!(pump.read_deadline, Some(original));
        if progress.progressed {
            break;
        }
        assert!(Instant::now() < end);
        pump.wait(Some(end)).unwrap();
    }
    send_plaintext(&mut client, &mut peer, &frame[3..], false);
    let (received, deadline) = next_frame(&mut pump);
    assert_eq!(deadline, original);
    assert!(
        matches!(received, WebSocketFrameV1::Binary(bytes) if bytes == b"same frame across key update")
    );
}
