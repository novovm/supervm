// Included inside product_relay_daemon::tests. The physical peer stops reading
// a real large TLS delivery, then sends application traffic in the other
// direction. Raw read-ahead alone cannot satisfy the third-recipient oracle.
use anyhow::ensure;

fn run_daemon_duplex_owner_v1(
    stream: TestDaemonTlsStreamV1,
    context: ProductRelayConnectionLoopV1<'_>,
) -> Result<()> {
    relay_incremental_connection_loop_v1(stream, context)
}

fn write_duplex_client_without_reading_v1(
    client: &mut TestClientWebSocketV1,
    wire: &[u8],
) -> Result<()> {
    // StreamOwned::write may service TLS reads as a side effect. Do not let the
    // test accidentally drain the very direction whose backpressure it needs.
    client.conn.writer().write_all(wire)?;
    while client.conn.wants_write() {
        ensure!(
            client.conn.write_tls(&mut client.sock)? > 0,
            "peer TLS write made no progress"
        );
    }
    client.sock.flush()?;
    Ok(())
}

fn read_duplex_client_message_v1(
    client: &mut TestClientWebSocketV1,
) -> Result<(ProductRelayWireMessageV1, Vec<u8>)> {
    let WebSocketFrameV1::Binary(bytes) = read_websocket_frame_v1(client, false)? else {
        bail!("expected a binary relay delivery/reply");
    };
    Ok((serde_json::from_slice(&bytes)?, bytes))
}

#[test]
#[ignore = "real socket backpressure and original 100ms write budget: run release --include-ignored --test-threads=1"]
fn daemon_dispatches_inbound_application_while_large_delivery_is_write_blocked() -> Result<()> {
    let runtime = Arc::new(
        TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()?,
    );
    let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default())?;
    let relay_key = SigningKey::from_bytes(&[171; 32]);
    let sender_key = SigningKey::from_bytes(&[172; 32]);
    let peer_key = SigningKey::from_bytes(&[173; 32]);
    let recipient_key = SigningKey::from_bytes(&[174; 32]);
    let now = now_ms_v1();
    let (sender, _sender_inbox) = runtime.block_on(manager.register_authenticated_session(
        authenticate_test_peer_v1(&sender_key, &relay_key, now),
        now,
    ))?;
    let (peer, mut peer_inbox) = runtime.block_on(manager.register_authenticated_session(
        authenticate_test_peer_v1(&peer_key, &relay_key, now),
        now,
    ))?;
    let (recipient, mut recipient_inbox) =
        runtime.block_on(manager.register_authenticated_session(
            authenticate_test_peer_v1(&recipient_key, &relay_key, now),
            now,
        ))?;
    let (mut sender_channel, mut peer_channel) = test_peer_channels_v1(&sender_key, &peer_key, now);
    let small_frame = test_data_frame_v1(0);
    let large_frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        [0x72; 16],
        1,
        2,
        1,
        3,
        vec![0x6d; 192 * 1024],
    );
    let mut expected_large_wire = Vec::new();
    for (index, frame) in [&small_frame, &large_frame].into_iter().enumerate() {
        let envelope = sender_channel.seal_novorudp_frame(frame)?;
        if index == 1 {
            expected_large_wire = serde_json::to_vec(&ProductRelayWireMessageV1::Delivery(
                crate::OpaqueRelayDeliveryV1 {
                    source_peer_id: sender.peer_id.clone(),
                    target_peer_id: peer.peer_id.clone(),
                    received_at_ms: now,
                    envelope: envelope.clone(),
                },
            ))?;
        }
        let outcome = runtime.block_on(manager.forward_opaque(
            &sender.peer_id,
            sender.session_id,
            envelope,
            now,
        ));
        ensure!(
            outcome.forwarded && !outcome.queued,
            "test delivery was not active"
        );
    }
    ensure!(
        expected_large_wire.len() > 512 * 1024,
        "fixture lacks a genuinely large wire frame"
    );
    let baseline = runtime.block_on(manager.snapshot());
    ensure!(
        baseline.active_queued_frame_count == 2,
        "fixture delivery count"
    );

    let (mut inbound_sender, mut inbound_recipient) =
        test_peer_channels_v1(&peer_key, &recipient_key, now);
    let inbound_frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        [0x73; 16],
        1,
        2,
        19,
        0,
        b"application ingress during blocked daemon egress".to_vec(),
    );
    let inbound_envelope = inbound_sender.seal_novorudp_frame(&inbound_frame)?;
    let data_wire_bytes =
        serde_json::to_vec(&ProductRelayWireMessageV1::Data(inbound_envelope.clone()))?.len();
    let messages = [
        ProductRelayWireMessageV1::Heartbeat,
        ProductRelayWireMessageV1::Data(inbound_envelope),
        // The first small delivery is consumed before this is sent. Credit 1
        // must be accepted while delivery 2 is still physically unfinished.
        ProductRelayWireMessageV1::DeliveryConsumedV1 { through: 1 },
    ];
    let mut requests = Vec::new();
    let mut request_wire_bytes = 0u64;
    for message in &messages {
        request_wire_bytes += serde_json::to_vec(message)?.len() as u64;
        write_masked_wire_message_v1(&mut requests, message)?;
    }

    let (mut daemon, client_tls, socket) = daemon_tls_io_fixture_v1();
    daemon.sock.inner.set_test_buffer_sizes(4096, 64 * 1024)?;
    daemon
        .sock
        .inner
        .set_read_timeout(Some(Duration::from_millis(100)))?;
    daemon
        .sock
        .inner
        .set_write_timeout(Some(Duration::from_millis(100)))?;
    daemon.sock.inner.enable_duplex_read_ahead();
    daemon.sock.test_writes.capture = false;
    // Discard only the fixture's completed-handshake deadline; the production
    // loop now starts and owns its unchanged 10-second frame deadline.
    daemon.sock.deadline.clear_v1()?;
    let stopping = Arc::clone(&daemon.sock.deadline.stopping);
    let io_deadline = daemon.sock.deadline.clone();
    let read_waker = daemon.sock.inner.read_waker();
    let would_block = daemon.sock.inner.write_would_block_probe();
    ensure!(
        would_block.load(Ordering::Acquire) == 0,
        "fixture started with a write stall"
    );
    let peer_socket = socket2::SockRef::from(&socket);
    peer_socket.set_send_buffer_size(4096)?;
    peer_socket.set_recv_buffer_size(64 * 1024)?;
    socket.set_nodelay(true)?;
    socket.set_write_timeout(Some(Duration::from_millis(100)))?;
    let mut client = rustls::StreamOwned::new(client_tls, socket);
    let daemon_manager = manager.clone();
    let daemon_runtime = Arc::clone(&runtime);
    let daemon_stop = Arc::clone(&stopping);
    let daemon_waker = read_waker.clone();
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
    let owner = thread::spawn(move || {
        let result = run_daemon_duplex_owner_v1(
            daemon,
            ProductRelayConnectionLoopV1 {
                manager: &daemon_manager,
                runtime: &daemon_runtime,
                peer_id: &peer.peer_id,
                session_id: peer.session_id,
                inbox: &mut peer_inbox,
                stopping: &daemon_stop,
                io_deadline: Some(&io_deadline),
                read_waker: Some(&daemon_waker),
            },
        );
        daemon_runtime.block_on(daemon_manager.disconnect(&peer.peer_id, peer.session_id));
        let _ = finished_tx.send(result.map_err(|error| format!("{error:#}")));
    });

    let result = (|| -> Result<()> {
        let (first, _) = read_duplex_client_message_v1(&mut client)?;
        let ProductRelayWireMessageV1::Delivery(first) = first else {
            bail!("first daemon message was not the small delivery");
        };
        ensure!(
            peer_channel.open_novorudp_frame(&first.envelope)? == small_frame,
            "small delivery changed"
        );
        let gate = Instant::now() + Duration::from_secs(3);
        while would_block.load(Ordering::Acquire) == 0 {
            ensure!(
                Instant::now() < gate,
                "fixture never reached actual socket write WouldBlock"
            );
            if let Ok(exited) = finished_rx.try_recv() {
                bail!("daemon exited before physical backpressure: {exited:?}");
            }
            thread::yield_now();
        }
        let stalled_at = Instant::now();
        let stalled = runtime.block_on(manager.snapshot());
        ensure!(stalled.active_queued_frame_count == 1 && stalled.active_queued_bytes == expected_large_wire.len(),
            "physical write stall did not retain the unfinished large delivery: frames={} bytes={} expected={}",
            stalled.active_queued_frame_count, stalled.active_queued_bytes, expected_large_wire.len());
        eprintln!("daemon app-duplex before reverse: real_write_would_block={} unfinished_delivery_bytes={} held_frames={} held_bytes={}",
            would_block.load(Ordering::Acquire), expected_large_wire.len(),
            stalled.active_queued_frame_count, stalled.active_queued_bytes);
        // From this point until the third-recipient oracle succeeds, no code
        // reads the peer socket, decrypts pending outbound TLS, or returns its
        // receive capacity to the daemon. Only a small reverse write is made.
        write_duplex_client_without_reading_v1(&mut client, &requests)?;
        let mut forwarded = None;
        let observed = loop {
            if forwarded.is_none() {
                forwarded = recipient_inbox.try_recv().ok();
            }
            let snapshot = runtime.block_on(manager.snapshot());
            if forwarded.is_some()
                && snapshot.admitted_wire_bytes_total
                    == baseline.admitted_wire_bytes_total + request_wire_bytes
            {
                break snapshot;
            }
            if let Ok(exited) = finished_rx.try_recv() {
                bail!("application ingress did not progress while daemon write was blocked: raw_would_block={} held_frames={} held_bytes={} incoming_delivery={} daemon={exited:?}",
                    would_block.load(Ordering::Acquire), snapshot.active_queued_frame_count,
                    snapshot.active_queued_bytes, forwarded.is_some());
            }
            ensure!(
                Instant::now() < gate,
                "application ingress waited for unfinished daemon egress"
            );
            thread::yield_now();
        };
        let forwarded = forwarded.expect("checked actual recipient delivery");
        ensure!(
            forwarded.source_peer_id == first.target_peer_id
                && forwarded.target_peer_id == recipient.peer_id,
            "reverse route changed"
        );
        ensure!(
            inbound_recipient.open_novorudp_frame(&forwarded.envelope)? == inbound_frame,
            "third recipient did not recover original authenticated plaintext"
        );
        ensure!(
            observed.rejected_frame_total == 0,
            "reverse heartbeat/data/credit was rejected"
        );
        ensure!(observed.active_queued_frame_count == 1 && observed.active_queued_bytes == expected_large_wire.len(),
            "outbound delivery completed/released before application ingress: frames={} bytes={} expected={}",
            observed.active_queued_frame_count, observed.active_queued_bytes, expected_large_wire.len());
        eprintln!("daemon app-duplex: real_write_would_block={} unfinished_delivery_bytes={} held_frames={} third_recipient_plaintext_bytes={} ingress_wait_ms={:.3}",
            would_block.load(Ordering::Acquire), expected_large_wire.len(), observed.active_queued_frame_count,
            inbound_frame.payload.len(), stalled_at.elapsed().as_secs_f64() * 1000.0);

        // Only now permit physical read progress. The original frame must
        // arrive exactly once, followed by one reply for each reverse request.
        let (large, large_bytes) = read_duplex_client_message_v1(&mut client)?;
        ensure!(
            large_bytes == expected_large_wire,
            "large wire changed, duplicated, or interleaved"
        );
        let ProductRelayWireMessageV1::Delivery(large) = large else {
            bail!("expected the unfinished large delivery before control replies");
        };
        ensure!(
            peer_channel.open_novorudp_frame(&large.envelope)? == large_frame,
            "large E2E plaintext changed"
        );
        let mut heartbeats = 0;
        let mut outcomes = 0;
        for _ in 0..2 {
            match read_duplex_client_message_v1(&mut client)?.0 {
                ProductRelayWireMessageV1::HeartbeatAck => heartbeats += 1,
                ProductRelayWireMessageV1::ForwardOutcome(outcome) => {
                    ensure!(
                        outcome.forwarded
                            && !outcome.queued
                            && outcome.admitted_wire_bytes == data_wire_bytes,
                        "reverse forward outcome did not match original admission"
                    );
                    outcomes += 1;
                }
                other => {
                    bail!("unexpected/duplicate daemon message after large delivery: {other:?}")
                }
            }
        }
        ensure!(
            heartbeats == 1 && outcomes == 1,
            "duplicate/missing request replies"
        );
        ensure!(
            recipient_inbox.try_recv().is_err(),
            "duplicate third-recipient delivery"
        );
        let mut finish_wire = Vec::new();
        let mut finish_bytes = 0;
        for message in [
            ProductRelayWireMessageV1::DeliveryConsumedV1 { through: 2 },
            ProductRelayWireMessageV1::Close,
        ] {
            finish_bytes += serde_json::to_vec(&message)?.len() as u64;
            write_masked_wire_message_v1(&mut finish_wire, &message)?;
        }
        write_duplex_client_without_reading_v1(&mut client, &finish_wire)?;
        let exited = finished_rx
            .recv_timeout(Duration::from_secs(3))
            .context("daemon did not finish after consumed credit and Close")?;
        exited.map_err(|error| anyhow::anyhow!(error))?;
        let final_state = runtime.block_on(manager.snapshot());
        ensure!(
            final_state.admitted_wire_bytes_total
                == baseline.admitted_wire_bytes_total + request_wire_bytes + finish_bytes,
            "input admission was duplicated or lost"
        );
        ensure!(
            final_state.rejected_frame_total == 0
                && final_state.queued_frame_count == 0
                && final_state.queued_bytes == 0,
            "failure/guard residue after complete duplex exchange"
        );
        ensure!(
            Instant::now() < gate,
            "duplex exchange exceeded original 3-second fixture gate"
        );
        Ok(())
    })();
    // Cleanup runs on the expected RED failure too, without leaving a worker
    // consuming another test's physical connection or executor resources.
    stopping.store(true, Ordering::Release);
    let _ = client.sock.shutdown(Shutdown::Both);
    read_waker.wake_by_ref();
    owner
        .join()
        .map_err(|_| anyhow::anyhow!("daemon duplex owner panicked"))?;
    result
}

#[test]
fn daemon_bad_json_or_credit_closes_before_following_heartbeat_and_releases_guard() -> Result<()> {
    for invalid_credit in [false, true] {
        let mut fixture = EncodedDeliveryFixtureV1::new(false);
        let before = fixture.snapshot();
        ensure!(
            before.active_queued_frame_count == 1 && before.active_queued_bytes != 0,
            "negative fixture must own a real queued delivery guard"
        );
        let bad_payload = if invalid_credit {
            serde_json::to_vec(&ProductRelayWireMessageV1::DeliveryConsumedV1 {
                through: u64::MAX,
            })?
        } else {
            b"{".to_vec()
        };
        ensure!(
            bad_payload.len() <= 125,
            "negative fixture requires a short masked frame"
        );
        let mask = [0x11, 0x23, 0x35, 0x47];
        let mut requests = vec![0x82, 0x80 | bad_payload.len() as u8];
        requests.extend_from_slice(&mask);
        requests.extend(
            bad_payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
        write_masked_wire_message_v1(&mut requests, &ProductRelayWireMessageV1::Heartbeat)?;

        let (mut daemon, client_tls, socket) = daemon_tls_io_fixture_v1();
        daemon
            .sock
            .inner
            .set_read_timeout(Some(Duration::from_millis(100)))?;
        daemon
            .sock
            .inner
            .set_write_timeout(Some(Duration::from_millis(100)))?;
        daemon.sock.inner.enable_duplex_read_ahead();
        daemon.sock.test_writes.capture = false;
        daemon.sock.deadline.clear_v1()?;
        let stopping = Arc::clone(&daemon.sock.deadline.stopping);
        let deadline = daemon.sock.deadline.clone();
        let waker = daemon.sock.inner.read_waker();
        socket.set_nodelay(true)?;
        socket.set_write_timeout(Some(Duration::from_millis(100)))?;
        let mut client = rustls::StreamOwned::new(client_tls, socket);
        // Both complete masked protocol frames traverse the same actual TLS
        // write before the owner starts. The trailing heartbeat is not absent
        // or waiting for another network write when the first frame fails.
        write_duplex_client_without_reading_v1(&mut client, &requests)?;

        let manager = fixture.manager.clone();
        let observer = TokioRuntimeBuilder::new_current_thread().build()?;
        let owner_stop = Arc::clone(&stopping);
        let owner_waker = waker.clone();
        let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);
        let owner = thread::spawn(move || {
            let mut inbox = fixture
                .inbox
                .take()
                .expect("original negative-fixture inbox");
            let result = run_daemon_duplex_owner_v1(
                daemon,
                ProductRelayConnectionLoopV1 {
                    manager: &fixture.manager,
                    runtime: &fixture.runtime,
                    peer_id: &fixture.registration.peer_id,
                    session_id: fixture.registration.session_id,
                    inbox: &mut inbox,
                    stopping: &owner_stop,
                    io_deadline: Some(&deadline),
                    read_waker: Some(&owner_waker),
                },
            );
            fixture.runtime.block_on(fixture.manager.disconnect(
                &fixture.registration.peer_id,
                fixture.registration.session_id,
            ));
            drop(inbox);
            let _ = finished_tx.send(result.map_err(|error| format!("{error:#}")));
        });
        let result = (|| -> Result<()> {
            let terminal = finished_rx
                .recv_timeout(Duration::from_secs(3))
                .context("invalid protocol did not terminate incremental daemon")?
                .expect_err("malformed JSON and future credit must fail closed");
            ensure!(
                terminal.contains(if invalid_credit {
                    "invalid relay delivery consumption watermark"
                } else {
                    "decode incremental relay message"
                }),
                "wrong terminal cause: {terminal}"
            );
            let after = observer.block_on(manager.snapshot());
            ensure!(
                after.admitted_wire_bytes_total
                    == before.admitted_wire_bytes_total + bad_payload.len() as u64,
                "bad input was charged twice or following Heartbeat was executed"
            );
            ensure!(
                after.rejected_wire_bytes_total
                    == before.rejected_wire_bytes_total + bad_payload.len() as u64
                    && after.rejected_frame_total == before.rejected_frame_total + 1
                    && after.protocol_rejected_frame_total
                        == before.protocol_rejected_frame_total + 1,
                "protocol rejection was lost or counted more than once"
            );
            ensure!(
                after.disconnected_session_total == before.disconnected_session_total + 1
                    && after.active_queued_frame_count == 0
                    && after.active_queued_bytes == 0
                    && after.offline_queued_frame_count == 0
                    && after.offline_queued_bytes == 0,
                "terminal owner retained or migrated the original delivery guard"
            );
            // The error closes the owned transport, not merely its manager
            // registration. No HeartbeatAck or queued delivery may escape.
            match client.sock.read(&mut [0u8; 16]) {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionReset
                            | io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::BrokenPipe
                    ) => {}
                other => {
                    bail!("invalid-input TLS socket remained usable or produced a reply: {other:?}")
                }
            }
            Ok(())
        })();
        stopping.store(true, Ordering::Release);
        let _ = client.sock.shutdown(Shutdown::Both);
        waker.wake_by_ref();
        owner
            .join()
            .map_err(|_| anyhow::anyhow!("invalid-protocol daemon owner panicked"))?;
        result?;
    }
    Ok(())
}
