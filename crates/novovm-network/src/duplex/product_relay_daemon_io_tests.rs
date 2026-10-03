// Migrated regression coverage from legacy/supervm-20261002/crates/novovm-node/src/product_relay_daemon_io_tests.rs.
// Included inside product_relay_daemon::tests. Faults use real TLS records and
// physical loopback writes; they do not change the non-test socket adapter.
impl ProductRelayDaemonDeadlineTcpStreamV1 {
    pub(super) fn test_read_v1(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let fault = self.test_writes.read_fault.take();
        if matches!(
            fault,
            Some(ProductRelayDaemonTestReadFaultV1::ConnectionReset)
        ) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "injected reset",
            ));
        }
        let read = self.inner.read(output)?;
        if read > 0
            && matches!(
                fault,
                Some(ProductRelayDaemonTestReadFaultV1::ExpireAfterProgress)
            )
        {
            self.deadline.state.lock().unwrap().deadline = Some(Instant::now());
        }
        Ok(read)
    }

    pub(super) fn test_write_v1(&mut self, input: &[u8]) -> io::Result<usize> {
        self.test_writes.calls += 1;
        if self.test_writes.capture {
            self.test_writes.attempted.push(input.to_vec());
        }
        let fault = self.test_writes.fault.take();
        if matches!(
            fault,
            Some(ProductRelayDaemonTestWriteFaultV1::ZeroProgressTimeout)
        ) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "injected zero progress",
            ));
        }
        let limit = match fault {
            Some(ProductRelayDaemonTestWriteFaultV1::TimeoutAfterProgress(limit))
            | Some(ProductRelayDaemonTestWriteFaultV1::ExpireAfterProgress(limit)) => limit,
            _ => input.len(),
        };
        let written = self.inner.write(&input[..input.len().min(limit)])?;
        if self.test_writes.capture {
            self.test_writes.sent.push(input[..written].to_vec());
        }
        match fault {
            Some(ProductRelayDaemonTestWriteFaultV1::TimeoutAfterProgress(_)) => {
                // Model an indeterminate send: bytes reached the peer, but the
                // socket API reports only an error, not their count.
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "injected indeterminate send",
                ))
            }
            Some(ProductRelayDaemonTestWriteFaultV1::ExpireAfterProgress(_)) => {
                self.deadline.state.lock().unwrap().deadline = Some(Instant::now());
                Ok(written)
            }
            _ => Ok(written),
        }
    }
}

type TestDaemonTlsStreamV1 =
    rustls::StreamOwned<rustls::ServerConnection, ProductRelayDaemonDeadlineTcpStreamV1>;

pub(super) fn daemon_tls_io_fixture_v1(
) -> (TestDaemonTlsStreamV1, rustls::ClientConnection, TcpStream) {
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let certificate_der = certificate.serialize_der().unwrap();
    let server_config = rustls::ServerConfig::builder_with_provider(tls_crypto_provider_v1())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(certificate_der.clone())],
            load_private_key_v1(certificate.serialize_private_key_pem().as_bytes()).unwrap(),
        )
        .unwrap();
    let mut server = rustls::ServerConnection::new(Arc::new(server_config)).unwrap();
    let mut client = rustls::ClientConnection::new(
        test_client_tls_config_v1(certificate_der),
        ServerName::try_from("localhost").unwrap(),
    )
    .unwrap();
    // Complete real TLS, including queued tickets, without timing-sensitive
    // worker threads. Only the authenticated write under test uses the socket.
    for _ in 0..16 {
        let mut to_server = Vec::new();
        client.write_tls(&mut to_server).unwrap();
        if !to_server.is_empty() {
            server.read_tls(&mut Cursor::new(to_server)).unwrap();
            server.process_new_packets().unwrap();
        }
        let mut to_client = Vec::new();
        server.write_tls(&mut to_client).unwrap();
        if !to_client.is_empty() {
            client.read_tls(&mut Cursor::new(to_client)).unwrap();
            client.process_new_packets().unwrap();
        }
        if !server.is_handshaking()
            && !client.is_handshaking()
            && !server.wants_write()
            && !client.wants_write()
        {
            break;
        }
    }
    assert!(!server.is_handshaking() && !client.is_handshaking());
    assert!(!server.wants_write() && !client.wants_write());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(2))
        .unwrap();
    let (socket, _) = listener.accept().unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let deadline = ProductRelayDaemonIoDeadlineV1::new(Arc::new(AtomicBool::new(false)));
    deadline
        .begin_v1(Instant::now() + Duration::from_secs(5))
        .unwrap();
    let socket = ProductRelayDaemonDeadlineTcpStreamV1 {
        inner: crate::duplex::product_relay_io::ProductRelaySocketV1::new(socket).unwrap(),
        deadline,
        inbound_tls_records: super::pump::InboundTlsRecords::default(),
        test_writes: ProductRelayDaemonTestWritesV1 {
            capture: true,
            ..Default::default()
        },
    };
    (rustls::StreamOwned::new(server, socket), client, peer)
}

fn daemon_actual_tls_wire_v1(stream: &TestDaemonTlsStreamV1, peer: &mut TcpStream) -> Vec<u8> {
    let expected: Vec<u8> = stream
        .sock
        .test_writes
        .sent
        .iter()
        .flatten()
        .copied()
        .collect();
    let mut actual = vec![0; expected.len()];
    peer.read_exact(&mut actual).unwrap();
    assert_eq!(
        actual, expected,
        "fault trace must match bytes actually received by peer"
    );
    actual
}

fn assert_daemon_tls_terminal_v1(stream: &mut TestDaemonTlsStreamV1) {
    let before = (
        stream.sock.test_writes.calls,
        stream.sock.test_writes.reads,
        stream.sock.test_writes.flushes,
    );
    assert!(stream.sock.deadline.clear_v1().is_err());
    assert!(stream
        .sock
        .deadline
        .begin_if_idle_v1(Instant::now() + Duration::from_secs(5))
        .is_err());
    assert!(stream
        .sock
        .deadline
        .begin_v1(Instant::now() + Duration::from_secs(5))
        .is_err());
    assert!(stream.sock.write_all(b"must not be sent").is_err());
    // Stream may accept plaintext before observing its lower error. Regardless,
    // the mandatory application flush fails and no ciphertext reaches the peer.
    let _ = stream.write_all(b"must not be sent");
    assert!(stream.flush().is_err());
    assert!(stream.sock.read(&mut [0]).is_err());
    assert!(stream.sock.flush().is_err());
    stream.conn.send_close_notify();
    assert!(stream.flush().is_err());
    assert_eq!(
        before,
        (
            stream.sock.test_writes.calls,
            stream.sock.test_writes.reads,
            stream.sock.test_writes.flushes
        )
    );
}

#[test]
fn daemon_tls_zero_progress_write_timeout_is_terminal_without_retry() {
    let (mut stream, _client, _peer) = daemon_tls_io_fixture_v1();
    stream.sock.test_writes.fault = Some(ProductRelayDaemonTestWriteFaultV1::ZeroProgressTimeout);
    let payload = b"a real TLS record, not a mocked cipher";
    // Rustls still accepts plaintext despite the immediate lower write error,
    // but its next flush must observe our permanent lower-stream failure.
    stream.write_all(payload).unwrap();
    assert_eq!(stream.sock.test_writes.calls, 1);
    assert!(stream.sock.test_writes.sent.is_empty());
    assert!(stream.conn.wants_write());
    assert_daemon_tls_terminal_v1(&mut stream);
    assert!(stream.sock.test_writes.sent.is_empty());
}

#[test]
fn daemon_tls_indeterminate_write_is_terminal_without_duplicate_ciphertext() {
    let (mut stream, mut client, mut peer) = daemon_tls_io_fixture_v1();
    stream.sock.test_writes.fault =
        Some(ProductRelayDaemonTestWriteFaultV1::TimeoutAfterProgress(7));
    stream.write_all(&[0x42; 96]).unwrap();
    assert_eq!(stream.sock.test_writes.sent[0].len(), 7);
    assert!(stream.conn.wants_write());
    assert_daemon_tls_terminal_v1(&mut stream);
    assert_eq!(stream.sock.test_writes.sent.len(), 1);
    let wire = daemon_actual_tls_wire_v1(&stream, &mut peer);
    assert_eq!(wire.len(), 7);
    client.read_tls(&mut Cursor::new(wire)).unwrap();
    client.process_new_packets().unwrap();
    assert_eq!(
        client.reader().read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn daemon_tls_post_write_deadline_preserves_progress_and_cannot_revive() {
    let (mut stream, _client, mut peer) = daemon_tls_io_fixture_v1();
    stream.sock.test_writes.fault =
        Some(ProductRelayDaemonTestWriteFaultV1::ExpireAfterProgress(7));
    stream.conn.writer().write_all(&[0x43; 96]).unwrap();
    assert_eq!(stream.conn.write_tls(&mut stream.sock).unwrap(), 7);
    assert_eq!(stream.sock.test_writes.sent[0].len(), 7);
    // Inspect rustls' remaining buffer using a Vec, not the failed transport:
    // exactly the successful prefix was consumed, so it cannot be repeated.
    let mut remaining = Vec::new();
    stream.conn.write_tls(&mut remaining).unwrap();
    assert_eq!(remaining, stream.sock.test_writes.attempted[0][7..]);
    assert_daemon_tls_terminal_v1(&mut stream);
    let wire = daemon_actual_tls_wire_v1(&stream, &mut peer);
    assert_eq!(wire.len(), 7);
}

#[test]
fn daemon_tls_read_idle_remains_usable_for_real_authenticated_traffic() {
    let (mut stream, mut client, mut peer) = daemon_tls_io_fixture_v1();
    stream
        .sock
        .inner
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let error = stream.read(&mut [0]).unwrap_err();
    assert!(matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ));
    stream.sock.deadline.clear_v1().unwrap();
    stream
        .sock
        .deadline
        .begin_if_idle_v1(Instant::now() + Duration::from_secs(5))
        .unwrap();
    let request = b"after idle";
    client.writer().write_all(request).unwrap();
    client.write_tls(&mut peer).unwrap();
    let mut decoded = vec![0; request.len()];
    stream.read_exact(&mut decoded).unwrap();
    assert_eq!(decoded, request);
    stream.write_all(b"response").unwrap();
    stream.flush().unwrap();
    let wire = daemon_actual_tls_wire_v1(&stream, &mut peer);
    client.read_tls(&mut Cursor::new(wire)).unwrap();
    client.process_new_packets().unwrap();
    let mut response = [0; 8];
    client.reader().read_exact(&mut response).unwrap();
    assert_eq!(&response, b"response");
    assert!(stream
        .sock
        .deadline
        .state
        .lock()
        .unwrap()
        .terminal_error
        .is_none());
}

#[test]
fn daemon_tls_post_read_deadline_preserves_received_bytes_and_cannot_revive() {
    let (mut stream, mut client, mut peer) = daemon_tls_io_fixture_v1();
    client.writer().write_all(b"received progress").unwrap();
    let mut wire = Vec::new();
    client.write_tls(&mut wire).unwrap();
    peer.write_all(&wire).unwrap();
    stream.sock.test_writes.read_fault =
        Some(ProductRelayDaemonTestReadFaultV1::ExpireAfterProgress);
    let mut received = [0; 7];
    let count = stream.sock.read(&mut received).unwrap();
    assert!(count > 0);
    assert_eq!(received[..count], wire[..count]);
    assert!(
        stream
            .sock
            .deadline
            .state
            .lock()
            .unwrap()
            .lower_read_progressed
    );
    assert_daemon_tls_terminal_v1(&mut stream);
}

#[test]
fn daemon_tls_non_idle_read_error_is_permanently_terminal() {
    let (mut stream, _client, _peer) = daemon_tls_io_fixture_v1();
    stream.sock.test_writes.read_fault = Some(ProductRelayDaemonTestReadFaultV1::ConnectionReset);
    let error = stream.read(&mut [0]).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    assert!(error.to_string().contains("injected reset"));
    assert_daemon_tls_terminal_v1(&mut stream);
}
