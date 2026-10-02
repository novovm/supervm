// Migrated regression coverage from legacy/supervm-20261002/crates/novovm-node/src/product_relay_client_failure_tests.rs.
// Included in product_relay_client::tests to reuse its bounded real-TLS fixture.

fn failure_test_lower_pair_v1() -> (ProductRelayDeadlineTcpStreamV1, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let tcp = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (peer, _) = listener.accept().unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    peer.set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut lower =
        ProductRelayDeadlineTcpStreamV1::new(tcp, Instant::now() + Duration::from_secs(2)).unwrap();
    lower
        .finish_handshake_v1(Duration::from_millis(10), Duration::from_secs(2))
        .unwrap();
    (lower, peer)
}

#[test]
fn successful_short_socket_write_reports_progress_before_terminal_deadline() {
    let (mut lower, mut peer) = failure_test_lower_pair_v1();
    lower.test_write_fault = Some(ProductRelayClientWriteFaultV1::ExpireAfterProgress(3));
    assert_eq!(lower.write(b"abcdef").unwrap(), 3);
    let mut received = [0; 3];
    peer.read_exact(&mut received).unwrap();
    assert_eq!(&received, b"abc");
    assert!(lower.terminal_error.is_some());
    // Even cleaning up the old operation's deadline cannot permit another I/O.
    lower.write_deadline = None;
    let calls = lower
        .test_io_calls
        .load(std::sync::atomic::Ordering::Relaxed);
    assert!(lower.write(b"def").is_err());
    assert!(lower.read(&mut received).is_err());
    assert!(lower.flush().is_err());
    assert_eq!(
        lower
            .test_io_calls
            .load(std::sync::atomic::Ordering::Relaxed),
        calls
    );
}

#[test]
fn successful_socket_read_preserves_progress_on_deadline_or_restore_failure() {
    for restore_failure in [false, true] {
        let (mut lower, mut peer) = failure_test_lower_pair_v1();
        peer.write_all(b"abc").unwrap();
        let mut bytes = [0; 3];
        // Inject precisely between the successful syscall and the same
        // maintenance completion used by the production Read implementation.
        let result = lower.inner.read(&mut bytes);
        let progress = *result.as_ref().unwrap();
        assert!(progress > 0);
        let maintenance = if restore_failure {
            Err(io::Error::other("injected timeout restoration failure"))
        } else {
            lower.read_operation_deadline = Some(Instant::now() - Duration::from_millis(1));
            lower.check_io_deadlines_v1()
        };
        assert_eq!(
            lower.finish_io_progress_v1(result, maintenance).unwrap(),
            progress
        );
        assert_eq!(&bytes[..progress], &b"abc"[..progress]);
        lower.read_operation_deadline = None;
        assert!(lower.read(&mut bytes).is_err());
        assert!(lower.write(b"must not resume").is_err());
    }
}

#[test]
fn authenticated_write_failure_cannot_retry_or_write_during_close() {
    for fault in [
        ProductRelayClientWriteFaultV1::Timeout,
        ProductRelayClientWriteFaultV1::ExpireAfterProgress(7),
    ] {
        let fixture = delayed_test_relay_v1(|_| Ok(()));
        let mut client =
            ProductRelayClientV1::connect(&SigningKey::from_bytes(&[207; 32]), &fixture.config)
                .unwrap();
        let calls = Arc::clone(&client.stream.sock.test_io_calls);
        let before = calls.load(std::sync::atomic::Ordering::Relaxed);
        client.stream.sock.test_write_fault = Some(fault);
        let error = client.heartbeat().unwrap_err();
        assert!(!product_relay_client_read_is_idle_timeout_v1(&error));
        assert!(client.stream.sock.terminal_error.is_some());
        assert!(client.stream.sock.write_deadline.is_none());
        assert!(
            client.stream.conn.wants_write(),
            "TLS still owns unsent ciphertext"
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), before + 1);
        let retry: Result<()> = client
            .write_authenticated_v1(|_| panic!("terminal session invoked another write operation"));
        assert!(retry.is_err());
        assert!(client.recv_event().is_err());
        assert!(client.close().is_err());
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), before + 1);
        fixture.finish();
    }
}

#[test]
fn ping_reply_write_timeout_is_not_read_idle() {
    let fixture = delayed_test_relay_v1(|_| Ok(()));
    let mut client =
        ProductRelayClientV1::connect(&SigningKey::from_bytes(&[208; 32]), &fixture.config)
            .unwrap();
    // A complete buffered Ping reaches the real recv_event/control-reply path
    // without depending on network scheduling to trigger the write failure.
    client.read_buffer.extend_from_slice(&[0x89, 0]);
    client.stream.sock.test_write_fault = Some(ProductRelayClientWriteFaultV1::Timeout);
    let calls = Arc::clone(&client.stream.sock.test_io_calls);
    let before = calls.load(std::sync::atomic::Ordering::Relaxed);
    let error = client.recv_event().unwrap_err();
    assert!(!product_relay_client_read_is_idle_timeout_v1(&error));
    assert!(format!("{error:#}").contains("injected authenticated socket write timeout"));
    assert!(client.close().is_err());
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), before + 1);
    fixture.finish();
}

#[test]
fn authenticated_write_error_keeps_timeout_source_but_marks_it_terminal() {
    let fixture = delayed_test_relay_v1(|_| Ok(()));
    let mut client =
        ProductRelayClientV1::connect(&SigningKey::from_bytes(&[209; 32]), &fixture.config)
            .unwrap();
    let error = client
        .write_authenticated_v1::<()>(|_| {
            Err(io::Error::new(io::ErrorKind::TimedOut, "injected application write").into())
        })
        .unwrap_err();
    assert!(error.chain().any(|cause| cause
        .downcast_ref::<io::Error>()
        .is_some_and(|cause| cause.kind() == io::ErrorKind::TimedOut)));
    assert!(!product_relay_client_read_is_idle_timeout_v1(&error));
    assert!(client.close().is_err());
    fixture.finish();
}

#[test]
fn implicit_tls_write_during_read_timeout_is_terminal_on_first_error() {
    let fixture = delayed_test_relay_v1(|_| Ok(()));
    let mut client =
        ProductRelayClientV1::connect(&SigningKey::from_bytes(&[211; 32]), &fixture.config)
            .unwrap();
    client
        .stream
        .conn
        .writer()
        .write_all(b"pending TLS plaintext")
        .unwrap();
    assert!(client.stream.conn.wants_write());
    client.stream.sock.test_write_fault = Some(ProductRelayClientWriteFaultV1::Timeout);
    let calls = Arc::clone(&client.stream.sock.test_io_calls);
    let before = calls.load(std::sync::atomic::Ordering::Relaxed);
    let error = client.recv_event().unwrap_err();
    assert!(!product_relay_client_read_is_idle_timeout_v1(&error));
    assert!(format!("{error:#}").contains("injected authenticated socket write timeout"));
    assert!(client.close().is_err());
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), before + 1);
    fixture.finish();
}

#[test]
fn ordinary_read_idle_still_allows_heartbeat_and_ack() {
    let fixture = delayed_test_relay_v1(|stream| {
        let (message, _) = read_test_client_wire_v1(stream)?;
        assert!(matches!(message, ProductRelayWireMessageV1::Heartbeat));
        write_fragmented_test_server_wire_v1(stream, &ProductRelayWireMessageV1::HeartbeatAck)
    });
    let mut client =
        ProductRelayClientV1::connect(&SigningKey::from_bytes(&[210; 32]), &fixture.config)
            .unwrap();
    let idle = client.recv_event().unwrap_err();
    assert!(product_relay_client_read_is_idle_timeout_v1(&idle));
    assert!(client.stream.sock.terminal_error.is_none());
    client.heartbeat().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match client.recv_event() {
            Ok(ProductRelayClientEventV1::HeartbeatAck) => break,
            Err(error) if product_relay_client_read_is_idle_timeout_v1(&error) => {
                assert!(
                    Instant::now() < deadline,
                    "heartbeat acknowledgement deadline"
                );
            }
            result => panic!("unexpected heartbeat response: {result:?}"),
        }
    }
    assert!(client.stream.sock.terminal_error.is_none());
    drop(client);
    fixture.finish();
}
