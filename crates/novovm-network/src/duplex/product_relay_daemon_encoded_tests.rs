// Included inside product_relay_daemon::tests. Authentication and envelopes
// are real; quota observations occur within actual daemon write/flush calls.

struct EncodedDeliveryFixtureV1 {
    runtime: Runtime,
    manager: ProductRelaySessionManagerV1,
    registration: crate::duplex::RelaySessionRegistrationV1,
    inbox: Option<crate::duplex::RelaySessionInboxV1>,
    _source_inbox: crate::duplex::RelaySessionInboxV1,
    relay: SigningKey,
    target: SigningKey,
    expected_wire: Vec<u8>,
    control: bool,
}

impl EncodedDeliveryFixtureV1 {
    fn new(control: bool) -> Self {
        let runtime = TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let manager =
            ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default()).unwrap();
        let relay = SigningKey::from_bytes(&[181; 32]);
        let source = SigningKey::from_bytes(&[182; 32]);
        let target = SigningKey::from_bytes(&[183; 32]);
        let now = now_ms_v1();
        let (source_registration, source_inbox) = runtime
            .block_on(manager.register_authenticated_session(
                authenticate_test_peer_v1(&source, &relay, now),
                now,
            ))
            .unwrap();
        let (registration, inbox) = runtime
            .block_on(manager.register_authenticated_session(
                authenticate_test_peer_v1(&target, &relay, now),
                now,
            ))
            .unwrap();
        let (expected, admitted) = if control {
            let initiator =
                NodeHandshakeInitiatorV1::start(&source, registration.peer_id.clone(), now, 5_000)
                    .unwrap();
            let handshake = RelayPeerHandshakeV1::Offer(initiator.offer().clone());
            let expected = ProductRelayWireMessageV1::PeerHandshakeDelivery(
                crate::duplex::RelayPeerHandshakeDeliveryV1 {
                    source_peer_id: source_registration.peer_id.clone(),
                    target_peer_id: registration.peer_id.clone(),
                    received_at_ms: now,
                    handshake: handshake.clone(),
                },
            );
            let admitted = runtime.block_on(manager.forward_peer_handshake(
                &source_registration.peer_id,
                source_registration.session_id,
                &registration.peer_id,
                handshake,
                now,
            ));
            (expected, admitted)
        } else {
            let (mut channel, _) = test_peer_channels_v1(&source, &target, now);
            let envelope = channel.seal_novorudp_frame(&test_data_frame_v1(0)).unwrap();
            let expected = ProductRelayWireMessageV1::Delivery(crate::duplex::OpaqueRelayDeliveryV1 {
                source_peer_id: source_registration.peer_id.clone(),
                target_peer_id: registration.peer_id.clone(),
                received_at_ms: now,
                envelope: envelope.clone(),
            });
            let admitted = runtime.block_on(manager.forward_opaque(
                &source_registration.peer_id,
                source_registration.session_id,
                envelope,
                now,
            ));
            (expected, admitted)
        };
        assert!(admitted.forwarded && !admitted.queued);
        Self {
            runtime,
            manager,
            registration,
            inbox: Some(inbox),
            _source_inbox: source_inbox,
            relay,
            target,
            expected_wire: crate::duplex::product_relay_wire::encode_message_v2(&expected).unwrap(),
            control,
        }
    }

    fn take(&mut self) -> crate::duplex::product_relay::RelayEncodedDeliveryV1 {
        if self.control {
            self.inbox
                .as_mut()
                .unwrap()
                .try_recv_peer_handshake_encoded()
                .unwrap()
        } else {
            self.inbox.as_mut().unwrap().try_recv_encoded().unwrap()
        }
    }

    fn snapshot(&self) -> crate::duplex::RelayRuntimeSnapshotV1 {
        self.runtime.block_on(self.manager.snapshot())
    }

    fn replace(
        &self,
    ) -> (
        crate::duplex::RelaySessionRegistrationV1,
        crate::duplex::RelaySessionInboxV1,
    ) {
        let now = now_ms_v1();
        self.runtime
            .block_on(self.manager.register_authenticated_session(
                authenticate_test_peer_v1(&self.target, &self.relay, now),
                now,
            ))
            .unwrap()
    }
}

struct QuotaObservingWriterV1<'a, S> {
    stream: &'a mut S,
    fixture: &'a EncodedDeliveryFixtureV1,
    queued_bytes: usize,
    writes: usize,
    flushes: usize,
}

impl<S> QuotaObservingWriterV1<'_, S> {
    fn assert_held(&self) {
        let snapshot = self.fixture.snapshot();
        assert_eq!(snapshot.active_queued_frame_count, 1);
        assert_eq!(snapshot.active_queued_bytes, self.queued_bytes);
        let target = snapshot
            .active_sessions
            .iter()
            .find(|session| session.session_id == self.fixture.registration.session_id)
            .unwrap();
        assert_eq!(target.queued_frame_count, 1);
        assert_eq!(target.queued_bytes, self.queued_bytes);
    }
}

impl<S: Write> Write for QuotaObservingWriterV1<'_, S> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.assert_held();
        self.writes += 1;
        let result = self.stream.write(bytes);
        self.assert_held();
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        self.assert_held();
        self.flushes += 1;
        let result = self.stream.flush();
        self.assert_held();
        result
    }
}

impl<S: Read> Read for QuotaObservingWriterV1<'_, S> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.stream.read(bytes)
    }
}

#[test]
fn daemon_encoded_inbox_idle_and_fair_service_keep_both_lane_guards_until_write() {
    struct IdleThenClose {
        idle: bool,
        close: Cursor<Vec<u8>>,
        wire: Vec<u8>,
    }
    impl Read for IdleThenClose {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if std::mem::take(&mut self.idle) {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            self.close.read(bytes)
        }
    }
    impl Write for IdleThenClose {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.wire.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    for control in [false, true] {
        for idle in [false, true] {
            let mut fixture = EncodedDeliveryFixtureV1::new(control);
            let queued_bytes = fixture.snapshot().active_queued_bytes;
            let mut close = Vec::new();
            write_masked_wire_message_v1(&mut close, &ProductRelayWireMessageV1::Close).unwrap();
            let mut socket = IdleThenClose {
                idle,
                close: Cursor::new(close),
                wire: Vec::new(),
            };
            // Move only the inbox out: observation still owns the same manager,
            // session identities and original quota, not a cloned delivery.
            let mut inbox = fixture.inbox.take().unwrap();
            let mut observed = QuotaObservingWriterV1 {
                stream: &mut socket,
                fixture: &fixture,
                queued_bytes,
                writes: 0,
                flushes: 0,
            };
            if idle {
                relay_connection_loop_v1(
                    &mut observed,
                    ProductRelayConnectionLoopV1 {
                        manager: &fixture.manager,
                        runtime: &fixture.runtime,
                        peer_id: &fixture.registration.peer_id,
                        session_id: fixture.registration.session_id,
                        inbox: &mut inbox,
                        stopping: &AtomicBool::new(false),
                        io_deadline: None,
                        read_waker: None,
                    },
                )
                .unwrap();
            } else {
                let mut prefer_control = control;
                let mut window = RelayDeliveryWindowV1::default();
                assert!(service_one_relay_inbox_v1(
                    &mut observed,
                    &fixture.manager,
                    &fixture.runtime,
                    (
                        &fixture.registration.peer_id,
                        fixture.registration.session_id
                    ),
                    &mut inbox,
                    &mut prefer_control,
                    &mut window
                )
                .unwrap());
                assert_eq!(prefer_control, !control);
                assert_eq!(window.sent, 1);
            }
            assert_eq!((observed.writes, observed.flushes), (2, 1));
            assert_eq!(fixture.snapshot().active_queued_frame_count, 0);
            let WebSocketFrameV1::Binary(actual) =
                read_websocket_frame_v1(&mut socket.wire.as_slice(), false).unwrap()
            else {
                unreachable!()
            };
            assert_eq!(actual, fixture.expected_wire);
        }
    }
}

#[test]
fn daemon_encoded_delivery_preserves_exact_wire_and_holds_quota_through_real_tls_flush() {
    for control in [false, true] {
        let mut fixture = EncodedDeliveryFixtureV1::new(control);
        let before = fixture.snapshot();
        let delivery = fixture.take();
        assert_eq!(delivery.wire_bytes(), fixture.expected_wire);
        assert_eq!(
            fixture.snapshot().active_queued_bytes,
            before.active_queued_bytes
        );
        let (mut stream, mut client, mut peer) = daemon_tls_io_fixture_v1();
        let original_deadline = stream.sock.deadline.state.lock().unwrap().deadline;
        let mut observed = QuotaObservingWriterV1 {
            stream: &mut stream,
            fixture: &fixture,
            queued_bytes: before.active_queued_bytes,
            writes: 0,
            flushes: 0,
        };
        write_encoded_relay_delivery_v1(
            &mut observed,
            &fixture.manager,
            &fixture.runtime,
            (
                &fixture.registration.peer_id,
                fixture.registration.session_id,
            ),
            delivery,
        )
        .unwrap();
        assert_eq!((observed.writes, observed.flushes), (2, 1));
        let after = fixture.snapshot();
        assert_eq!(
            (after.active_queued_frame_count, after.active_queued_bytes),
            (0, 0)
        );
        assert_eq!(
            stream.sock.deadline.state.lock().unwrap().deadline,
            original_deadline
        );
        let wire = daemon_actual_tls_wire_v1(&stream, &mut peer);
        let mut records = Cursor::new(wire);
        while records.position() < records.get_ref().len() as u64 {
            assert!(client.read_tls(&mut records).unwrap() > 0);
            client.process_new_packets().unwrap();
        }
        let WebSocketFrameV1::Binary(actual) =
            read_websocket_frame_v1(&mut client.reader(), false).unwrap()
        else {
            panic!("encoded delivery was not a binary WebSocket frame");
        };
        assert_eq!(actual, fixture.expected_wire);
        assert_eq!(
            client.reader().read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn daemon_encoded_delivery_failure_releases_guard_but_cannot_revive_tls() {
    for control in [false, true] {
        for fault in [
            ProductRelayDaemonTestWriteFaultV1::ZeroProgressTimeout,
            ProductRelayDaemonTestWriteFaultV1::TimeoutAfterProgress(7),
            ProductRelayDaemonTestWriteFaultV1::ExpireAfterProgress(7),
        ] {
            let mut fixture = EncodedDeliveryFixtureV1::new(control);
            let queued_bytes = fixture.snapshot().active_queued_bytes;
            let delivery = fixture.take();
            let (mut stream, _client, mut peer) = daemon_tls_io_fixture_v1();
            stream.sock.test_writes.fault = Some(fault);
            let mut observed = QuotaObservingWriterV1 {
                stream: &mut stream,
                fixture: &fixture,
                queued_bytes,
                writes: 0,
                flushes: 0,
            };
            assert!(write_encoded_relay_delivery_v1(
                &mut observed,
                &fixture.manager,
                &fixture.runtime,
                (
                    &fixture.registration.peer_id,
                    fixture.registration.session_id
                ),
                delivery,
            )
            .is_err());
            assert!(observed.writes > 0);
            let after = fixture.snapshot();
            assert_eq!(
                (after.active_queued_frame_count, after.active_queued_bytes),
                (0, 0)
            );
            assert_daemon_tls_terminal_v1(&mut stream);
            assert!(daemon_actual_tls_wire_v1(&stream, &mut peer).len() <= 7);
        }
    }
}

#[test]
fn daemon_encoded_delivery_rechecks_replaced_session_before_any_write() {
    for control in [false, true] {
        let mut fixture = EncodedDeliveryFixtureV1::new(control);
        let before = fixture.snapshot();
        let delivery = fixture.take();
        let (replacement, mut inbox) = fixture.replace();
        assert!(replacement.replaced_existing_session);
        assert_ne!(replacement.session_id, fixture.registration.session_id);
        let replaced = fixture.snapshot();
        assert_eq!(replaced.active_queued_frame_count, 1);
        assert_eq!(replaced.active_queued_bytes, before.active_queued_bytes);
        let current = replaced
            .active_sessions
            .iter()
            .find(|session| session.session_id == replacement.session_id)
            .unwrap();
        assert_eq!((current.queued_frame_count, current.queued_bytes), (0, 0));
        let mut wire = Vec::new();
        let error = write_encoded_relay_delivery_v1(
            &mut wire,
            &fixture.manager,
            &fixture.runtime,
            (
                &fixture.registration.peer_id,
                fixture.registration.session_id,
            ),
            delivery,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("replaced before queued delivery"));
        assert!(wire.is_empty());
        assert!(inbox.try_recv_encoded().is_err());
        assert!(inbox.try_recv_peer_handshake_encoded().is_err());
        let after = fixture.snapshot();
        assert_eq!(
            (after.active_queued_frame_count, after.active_queued_bytes),
            (0, 0)
        );
    }
}

#[test]
fn daemon_encoded_delivery_replacement_during_write_keeps_old_guard_not_new_session_quota() {
    struct ReplacingWriter<'a> {
        fixture: &'a EncodedDeliveryFixtureV1,
        queued_bytes: usize,
        replacement: Option<(
            crate::duplex::RelaySessionRegistrationV1,
            crate::duplex::RelaySessionInboxV1,
        )>,
        wire: Vec<u8>,
    }
    impl ReplacingWriter<'_> {
        fn assert_old_guard_held(&self) {
            let snapshot = self.fixture.snapshot();
            assert_eq!(
                (
                    snapshot.active_queued_frame_count,
                    snapshot.active_queued_bytes
                ),
                (1, self.queued_bytes)
            );
            if let Some((registration, _)) = &self.replacement {
                let current = snapshot
                    .active_sessions
                    .iter()
                    .find(|session| session.session_id == registration.session_id)
                    .unwrap();
                assert_eq!((current.queued_frame_count, current.queued_bytes), (0, 0));
            }
        }
    }
    impl Write for ReplacingWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.assert_old_guard_held();
            if self.replacement.is_none() {
                self.replacement = Some(self.fixture.replace());
            }
            self.assert_old_guard_held();
            self.wire.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.assert_old_guard_held();
            Ok(())
        }
    }
    for control in [false, true] {
        let mut fixture = EncodedDeliveryFixtureV1::new(control);
        let queued_bytes = fixture.snapshot().active_queued_bytes;
        let delivery = fixture.take();
        let mut writer = ReplacingWriter {
            fixture: &fixture,
            queued_bytes,
            replacement: None,
            wire: Vec::new(),
        };
        // Replacement after the pre-write check cannot retract already written
        // bytes. It must not release the old guard or transfer it to the new TLS.
        write_encoded_relay_delivery_v1(
            &mut writer,
            &fixture.manager,
            &fixture.runtime,
            (
                &fixture.registration.peer_id,
                fixture.registration.session_id,
            ),
            delivery,
        )
        .unwrap();
        let (replacement, mut inbox) = writer.replacement.take().unwrap();
        assert_ne!(replacement.session_id, fixture.registration.session_id);
        assert!(inbox.try_recv_encoded().is_err());
        assert!(inbox.try_recv_peer_handshake_encoded().is_err());
        let WebSocketFrameV1::Binary(actual) =
            read_websocket_frame_v1(&mut writer.wire.as_slice(), false).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(actual, fixture.expected_wire);
        let after = fixture.snapshot();
        assert_eq!(
            (after.active_queued_frame_count, after.active_queued_bytes),
            (0, 0)
        );
    }
}

#[test]
fn daemon_websocket_payload_is_written_from_original_slice_after_fixed_header() {
    struct SliceWriter<'a> {
        payload: &'a [u8],
        wire: Vec<u8>,
        header_len: usize,
        payload_written: usize,
    }
    impl Write for SliceWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.wire.is_empty() {
                assert!(bytes.len() <= 10);
                self.header_len = bytes.len();
                self.wire.extend_from_slice(bytes);
                return Ok(bytes.len());
            }
            assert_eq!(
                bytes.as_ptr(),
                self.payload[self.payload_written..].as_ptr()
            );
            assert_eq!(bytes, &self.payload[self.payload_written..]);
            let count = bytes.len().min(1009);
            self.payload_written += count;
            self.wire.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            assert_eq!(self.payload_written, self.payload.len());
            Ok(())
        }
    }
    for length in [
        0,
        125,
        126,
        65_535,
        65_536,
        PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
    ] {
        let payload = vec![0x5b; length];
        let mut writer = SliceWriter {
            payload: &payload,
            wire: Vec::new(),
            header_len: 0,
            payload_written: 0,
        };
        write_websocket_frame_v1(&mut writer, 0x2, &payload).unwrap();
        assert_eq!(
            writer.header_len,
            if length <= 125 {
                2
            } else if length <= 65_535 {
                4
            } else {
                10
            }
        );
        let mut wire = writer.wire.as_slice();
        let WebSocketFrameV1::Binary(actual) = read_websocket_frame_v1(&mut wire, false).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(actual, payload);
        assert!(wire.is_empty());
    }
}
