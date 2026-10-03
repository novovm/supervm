use super::super::tests::{
    delayed_test_relay_v1, read_test_client_wire_v1, test_window_deliveries_v1,
    write_fragmented_test_server_wire_v1,
};
use super::*;
use crate::duplex::{
    peer_id_from_ed25519_public_key_v1, E2eSecureChannelV1, NodeHandshakeResponderV1,
    NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0,
};

fn envelopes(count: usize, size: usize) -> (Vec<SecureNovoRudpEnvelopeV1>, E2eSecureChannelV1) {
    let source = SigningKey::from_bytes(&[218; 32]);
    let target = SigningKey::from_bytes(&[219; 32]);
    let target_id = peer_id_from_ed25519_public_key_v1(&target.verifying_key().to_bytes());
    let now = now_ms_v1();
    let initiator = NodeHandshakeInitiatorV1::start(&source, target_id, now, 30_000).unwrap();
    let responder = NodeHandshakeResponderV1::respond(
        initiator.offer(),
        &target,
        now,
        30_000,
        &mut HandshakeReplayCacheV1::default(),
    )
    .unwrap();
    let mut channel = initiator
        .complete(
            responder.response(),
            now,
            &mut HandshakeReplayCacheV1::default(),
        )
        .unwrap();
    let receiver = responder.into_channel();
    let envelopes = (0..count)
        .map(|sequence| {
            let frame = NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                channel.session_id(),
                1,
                2,
                sequence as u64,
                0,
                vec![sequence as u8; size],
            );
            channel.seal_novorudp_frame(&frame).unwrap()
        })
        .collect();
    (envelopes, receiver)
}

fn outcome(envelope: &SecureNovoRudpEnvelopeV1, wire_bytes: usize) -> RelayForwardOutcomeV1 {
    RelayForwardOutcomeV1 {
        disposition: RelayForwardDispositionV1::Forwarded,
        source_peer_id: envelope.sender_peer_id.clone(),
        target_peer_id: envelope.recipient_peer_id.clone(),
        forwarded: true,
        queued: false,
        payload_treated_opaque: true,
        envelope_session_id: Some(envelope.session_id),
        envelope_sequence: Some(envelope.sequence),
        admitted_wire_bytes: wire_bytes,
    }
}

fn until(
    pipeline: &mut ProductRelayPipelineV1,
    mut done: impl FnMut(&ProductRelayPipelineV1, &PipelineProgress) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        assert!(
            Instant::now() < deadline,
            "incremental pipeline did not progress"
        );
        let progress = pipeline.poll().unwrap();
        if done(pipeline, &progress) {
            break;
        }
        if matches!(progress, PipelineProgress::Idle) {
            pipeline.wait().unwrap();
        }
    }
}

#[test]
fn duplicate_and_mismatched_outcomes_are_terminal_not_another_ticket_completion() {
    for corrupt in [false, true] {
        let (mut originals, _) = envelopes(1, 8);
        let fixture = delayed_test_relay_v1(move |stream| {
            let (ProductRelayWireMessageV1::Data(envelope), bytes) =
                read_test_client_wire_v1(stream)?
            else {
                bail!("expected data");
            };
            let mut reply = outcome(&envelope, bytes);
            if corrupt {
                reply.admitted_wire_bytes += 1;
            }
            write_fragmented_test_server_wire_v1(
                stream,
                &ProductRelayWireMessageV1::ForwardOutcome(reply.clone()),
            )?;
            if !corrupt {
                write_fragmented_test_server_wire_v1(
                    stream,
                    &ProductRelayWireMessageV1::ForwardOutcome(reply),
                )?;
            }
            Ok(())
        });
        let mut pipeline =
            ProductRelayClientV1::connect(&SigningKey::from_bytes(&[218; 32]), &fixture.config)
                .unwrap()
                .into_pipeline()
                .unwrap();
        assert_eq!(
            pipeline
                .try_submit_envelope(originals.pop().unwrap())
                .unwrap(),
            Some(0)
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut completions = 0;
        let error = loop {
            assert!(Instant::now() < deadline);
            match pipeline.poll() {
                Ok(PipelineProgress::Forward { ticket, .. }) => {
                    assert_eq!(ticket, 0);
                    completions += 1;
                }
                Ok(PipelineProgress::Idle) => pipeline.wait().unwrap(),
                Ok(_) => {}
                Err(error) => break error,
            }
        };
        assert_eq!(completions, usize::from(!corrupt));
        assert!(!product_relay_client_read_is_idle_timeout_v1(&error));
        assert!(pipeline.client.stream.sock.terminal_error.is_some());
        assert!(pipeline.poll().is_err());
        assert!(pipeline.wait().is_err());
        drop(pipeline);
        fixture.finish();
    }
}

#[test]
fn metadata_capacity_and_oldest_deadline_remain_bounded_without_ack() {
    let (originals, _) = envelopes(MAX_FORWARDS + 1, 8);
    let fixture = delayed_test_relay_v1(|stream| {
        for _ in 0..MAX_FORWARDS {
            read_test_client_wire_v1(stream)?;
        }
        Ok(())
    });
    let mut pipeline =
        ProductRelayClientV1::connect(&SigningKey::from_bytes(&[218; 32]), &fixture.config)
            .unwrap()
            .into_pipeline()
            .unwrap();
    for (index, original) in originals.iter().take(MAX_FORWARDS).enumerate() {
        assert_eq!(
            pipeline.try_submit_envelope(original.clone()).unwrap(),
            Some(index as u64)
        );
        until(&mut pipeline, |pipeline, _| pipeline.writing.is_none());
    }
    assert_eq!(pipeline.pending.len(), MAX_FORWARDS);
    assert!(!pipeline.can_submit());
    assert_eq!(
        pipeline
            .try_submit_envelope(originals.last().unwrap().clone())
            .unwrap(),
        None
    );
    let oldest = pipeline.pending[&0].deadline.unwrap();
    pipeline.read_waker().wake_by_ref();
    assert!(pipeline.try_heartbeat().unwrap());
    until(&mut pipeline, |pipeline, _| pipeline.writing.is_none());
    assert_eq!(pipeline.pending[&0].deadline, Some(oldest));
    pipeline.pending.get_mut(&0).unwrap().deadline =
        Some(Instant::now() - Duration::from_millis(1));
    assert!(pipeline.poll().is_err());
    assert_eq!(
        pipeline.pending.len(),
        MAX_FORWARDS,
        "terminal failure does not forge acknowledgements"
    );
    assert!(pipeline.client.stream.sock.terminal_error.is_some());
    drop(pipeline);
    fixture.finish();
}

#[test]
fn handshake_is_single_flight_while_data_remains_pipelineable() {
    let fixture = delayed_test_relay_v1(|stream| {
        read_test_client_wire_v1(stream)?;
        read_test_client_wire_v1(stream)?;
        Ok(())
    });
    let mut pipeline =
        ProductRelayClientV1::connect(&SigningKey::from_bytes(&[218; 32]), &fixture.config)
            .unwrap()
            .into_pipeline()
            .unwrap();
    let ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery) =
        test_window_deliveries_v1().remove(1)
    else {
        unreachable!()
    };
    assert!(pipeline
        .try_submit_handshake(delivery.target_peer_id.clone(), delivery.handshake.clone())
        .unwrap()
        .is_some());
    until(&mut pipeline, |pipeline, _| pipeline.writing.is_none());
    assert!(!pipeline.can_submit_handshake());
    assert_eq!(
        pipeline
            .try_submit_handshake(delivery.target_peer_id, delivery.handshake)
            .unwrap(),
        None
    );
    assert!(pipeline.can_submit());
    let (mut originals, _) = envelopes(1, 8);
    assert!(pipeline
        .try_submit_envelope(originals.pop().unwrap())
        .unwrap()
        .is_some());
    until(&mut pipeline, |pipeline, _| pipeline.writing.is_none());
    assert_eq!(pipeline.pending.len(), 2);
    drop(pipeline);
    fixture.finish();
}

#[test]
fn partial_inbound_and_write_stall_deadlines_survive_other_progress() {
    for expire_read in [true, false] {
        let fixture = delayed_test_relay_v1(|_| Ok(()));
        let mut pipeline =
            ProductRelayClientV1::connect(&SigningKey::from_bytes(&[218; 32]), &fixture.config)
                .unwrap()
                .into_pipeline()
                .unwrap();
        let original = Instant::now() + Duration::from_secs(1);
        pipeline.client.stream.sock.frame_deadline = Some(original);
        assert!(pipeline.try_heartbeat().unwrap());
        assert_eq!(pipeline.client.stream.sock.frame_deadline, Some(original));
        until(&mut pipeline, |pipeline, _| pipeline.writing.is_none());
        assert_eq!(pipeline.client.stream.sock.frame_deadline, Some(original));
        let expired = Instant::now() - Duration::from_millis(1);
        if expire_read {
            pipeline.client.stream.sock.frame_deadline = Some(expired);
        } else {
            pipeline.write_stall_deadline = Some(expired);
        }
        pipeline.read_waker().wake_by_ref();
        assert!(pipeline.poll().is_err());
        assert!(pipeline.client.stream.sock.terminal_error.is_some());
        assert!(pipeline.try_heartbeat().is_err());
        drop(pipeline);
        fixture.finish();
    }
}

#[test]
fn actual_tls_close_notify_delivers_complete_last_event_but_rejects_partial_ws() {
    for partial in [false, true] {
        let fixture = delayed_test_relay_v1(move |stream| {
            if partial {
                stream.write_all(&[0x82, 50, b'{'])?;
                stream.flush()?;
            } else {
                write_fragmented_test_server_wire_v1(
                    stream,
                    &ProductRelayWireMessageV1::HeartbeatAck,
                )?;
            }
            stream.conn.send_close_notify();
            while stream.conn.wants_write() {
                stream.conn.write_tls(&mut stream.sock)?;
            }
            Ok(())
        });
        let mut pipeline =
            ProductRelayClientV1::connect(&SigningKey::from_bytes(&[218; 32]), &fixture.config)
                .unwrap()
                .into_pipeline()
                .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut events = 0;
        loop {
            assert!(
                Instant::now() < deadline,
                "TLS close_notify became an idle loop"
            );
            match pipeline.poll() {
                Ok(PipelineProgress::Event(event))
                    if *event == ProductRelayClientEventV1::HeartbeatAck =>
                {
                    events += 1
                }
                Ok(PipelineProgress::Idle) => pipeline.wait().unwrap(),
                Ok(PipelineProgress::Progress) => {}
                Err(_) => break,
                other => panic!("unexpected close result {other:?}"),
            }
        }
        assert_eq!(events, usize::from(!partial));
        assert!(pipeline.client.stream.sock.terminal_error.is_some());
        assert!(pipeline.poll().is_err());
        drop(pipeline);
        fixture.finish();
    }
}

#[test]
fn incremental_partial_write_failure_never_resends_or_completes_an_obligation() {
    for partial in [false, true] {
        let fixture = delayed_test_relay_v1(|_| Ok(()));
        let mut pipeline =
            ProductRelayClientV1::connect(&SigningKey::from_bytes(&[218; 32]), &fixture.config)
                .unwrap()
                .into_pipeline()
                .unwrap();
        let (mut originals, _) = envelopes(1, 512);
        pipeline
            .try_submit_envelope(originals.pop().unwrap())
            .unwrap()
            .unwrap();
        pipeline.client.stream.sock.test_write_fault = Some(if partial {
            ProductRelayClientWriteFaultV1::ExpireAfterProgress(7)
        } else {
            ProductRelayClientWriteFaultV1::Timeout
        });
        assert!(pipeline.poll().is_err());
        assert_eq!(pipeline.pending.len(), 1);
        assert!(
            pipeline.pending[&0].deadline.is_none(),
            "not completely written is not awaiting ACK"
        );
        let calls = pipeline
            .client
            .stream
            .sock
            .test_io_calls
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(pipeline.poll().is_err());
        assert!(pipeline.wait().is_err());
        assert!(pipeline.try_heartbeat().is_err());
        assert_eq!(
            pipeline
                .client
                .stream
                .sock
                .test_io_calls
                .load(std::sync::atomic::Ordering::Relaxed),
            calls
        );
        drop(pipeline);
        fixture.finish();
    }
}

#[test]
#[ignore = "real socket backpressure/latency: run release --include-ignored --test-threads=1"]
fn real_wss_large_read_progresses_during_unfinished_write_with_fixed_socket_buffers() {
    const PAYLOAD: usize = 192 * 1024;
    let (incoming, mut channel) = envelopes(7, PAYLOAD);
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        channel.session_id(),
        1,
        2,
        0,
        0,
        vec![99; PAYLOAD],
    );
    let outgoing = channel.seal_novorudp_frame(&frame).unwrap();
    let expected_outgoing = outgoing.clone();
    let fixture = delayed_test_relay_v1(move |stream| {
        let socket = socket2::SockRef::from(&stream.sock);
        // Keep the send buffer small to require actual write WouldBlock below.
        // Original Linux runs with post-handshake 4KiB receive buffers stalled
        // with TCP receive-window/retransmit evidence. The receive-only 64KiB
        // A/B passes while retaining the same real-backpressure assertions.
        socket.set_send_buffer_size(4096)?;
        socket.set_recv_buffer_size(64 * 1024)?;
        for envelope in incoming {
            let wire = ProductRelayWireMessageV1::Delivery(OpaqueRelayDeliveryV1 {
                source_peer_id: envelope.sender_peer_id.clone(),
                target_peer_id: envelope.recipient_peer_id.clone(),
                received_at_ms: now_ms_v1(),
                envelope,
            });
            let payload = encode_message_v2(&wire)?;
            assert!(payload.len() <= PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
            let mut bytes = vec![0x82, 127];
            bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
            bytes.extend_from_slice(&payload);
            stream.write_all(&bytes)?;
            stream.flush()?;
        }
        let (ProductRelayWireMessageV1::Data(actual), bytes) = read_test_client_wire_v1(stream)?
        else {
            bail!("expected complete original after opposite-direction progress");
        };
        assert_eq!(actual, expected_outgoing);
        write_fragmented_test_server_wire_v1(
            stream,
            &ProductRelayWireMessageV1::ForwardOutcome(outcome(&actual, bytes)),
        )?;
        let (credit, _) = read_test_client_wire_v1(stream)?;
        assert!(matches!(
            credit,
            ProductRelayWireMessageV1::DeliveryConsumedV1 { through: 7 }
        ));
        Ok(())
    });
    let client =
        ProductRelayClientV1::connect(&SigningKey::from_bytes(&[219; 32]), &fixture.config)
            .unwrap();
    client
        .stream
        .sock
        .inner
        .set_test_buffer_sizes(4096, 64 * 1024)
        .unwrap();
    let mut pipeline = client.into_pipeline().unwrap();
    assert!(pipeline.try_submit_envelope(outgoing).unwrap().is_some());
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut received = 0;
    let mut completed = false;
    let mut during_write = false;
    let mut stalled = false;
    while received != 7 || !completed || pipeline.client.delivery_consumed_reported != 7 {
        assert!(
            Instant::now() < deadline,
            "full-duplex incremental socket stalled"
        );
        let writing_before = pipeline.writing.is_some();
        let started = Instant::now();
        let progress = pipeline.poll().unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "poll performed a blocking socket wait"
        );
        stalled |= pipeline.write_stall_deadline.is_some();
        match progress {
            PipelineProgress::Event(event) => {
                let ProductRelayClientEventV1::Delivery(delivery) = *event else {
                    panic!("unexpected event")
                };
                let frame = channel.open_novorudp_frame(&delivery.envelope).unwrap();
                assert_eq!(frame.sequence, received);
                assert_eq!(frame.payload, vec![received as u8; PAYLOAD]);
                received += 1;
                during_write |= writing_before;
            }
            PipelineProgress::Forward { ticket, .. } => {
                assert_eq!(ticket, 0);
                assert!(!completed);
                completed = true;
            }
            PipelineProgress::Progress => {}
            PipelineProgress::Idle => pipeline.wait().unwrap(),
        }
        assert!(pipeline.client.read_buffer.capacity() <= MAX_READ_BUFFER);
    }
    assert!(stalled, "fixture did not force actual write WouldBlock");
    assert!(
        during_write,
        "inbound parsing was postponed until the outbound frame finished"
    );
    drop(pipeline);
    fixture.finish();
}

#[test]
#[ignore = "real socket backpressure/latency: run release --include-ignored --test-threads=1"]
fn real_wss_eight_large_inflight_forwards_receive_seven_deliveries_before_reverse_acks() {
    const PAYLOAD: usize = 192 * 1024;
    const OUTGOING: usize = 8;
    const INCOMING: usize = 7;
    assert_eq!(MAX_FORWARDS, OUTGOING);
    let remote = SigningKey::from_bytes(&[218; 32]);
    let local = SigningKey::from_bytes(&[219; 32]);
    let local_id = peer_id_from_ed25519_public_key_v1(&local.verifying_key().to_bytes());
    let now = now_ms_v1();
    let initiator = NodeHandshakeInitiatorV1::start(&remote, local_id, now, 30_000).unwrap();
    let responder = NodeHandshakeResponderV1::respond(
        initiator.offer(),
        &local,
        now,
        30_000,
        &mut HandshakeReplayCacheV1::default(),
    )
    .unwrap();
    let mut remote_channel = initiator
        .complete(
            responder.response(),
            now,
            &mut HandshakeReplayCacheV1::default(),
        )
        .unwrap();
    let mut local_channel = responder.into_channel();
    let frame = |session, sequence, byte| {
        NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            session,
            1,
            2,
            sequence,
            0,
            vec![byte; PAYLOAD],
        )
    };
    let incoming: Vec<_> = (0..INCOMING)
        .map(|sequence| {
            remote_channel
                .seal_novorudp_frame(&frame(
                    remote_channel.session_id(),
                    sequence as u64,
                    sequence as u8,
                ))
                .unwrap()
        })
        .collect();
    let mut outgoing: std::collections::VecDeque<_> = (0..OUTGOING)
        .map(|sequence| {
            local_channel
                .seal_novorudp_frame(&frame(
                    local_channel.session_id(),
                    sequence as u64,
                    128 + sequence as u8,
                ))
                .unwrap()
        })
        .collect();
    let fixture = delayed_test_relay_v1(move |stream| {
        let socket = socket2::SockRef::from(&stream.sock);
        socket.set_send_buffer_size(4096)?;
        socket.set_recv_buffer_size(64 * 1024)?;
        // Write all inbound data before reading any client Data or credit.
        // The client must drain this direction while its own write is blocked.
        for envelope in incoming {
            let payload = encode_message_v2(&ProductRelayWireMessageV1::Delivery(
                OpaqueRelayDeliveryV1 {
                    source_peer_id: envelope.sender_peer_id.clone(),
                    target_peer_id: envelope.recipient_peer_id.clone(),
                    received_at_ms: now_ms_v1(),
                    envelope,
                },
            ))?;
            assert!(payload.len() <= PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
            let mut wire = vec![0x82, 127];
            wire.extend_from_slice(&(payload.len() as u64).to_be_bytes());
            wire.extend_from_slice(&payload);
            stream.write_all(&wire)?;
            stream.flush()?;
        }
        let mut replies = Vec::new();
        let mut credit_received = false;
        while replies.len() != OUTGOING || !credit_received {
            let (message, wire_bytes) = read_test_client_wire_v1(stream)?;
            match message {
                ProductRelayWireMessageV1::Data(envelope) => {
                    let sequence = replies.len();
                    assert!(sequence < OUTGOING, "unexpected duplicate outgoing Data");
                    let opened = remote_channel.open_novorudp_frame(&envelope)?;
                    assert_eq!(
                        opened,
                        frame(
                            remote_channel.session_id(),
                            sequence as u64,
                            128 + sequence as u8
                        )
                    );
                    replies.push(outcome(&envelope, wire_bytes));
                }
                ProductRelayWireMessageV1::DeliveryConsumedV1 { through } => {
                    assert_eq!(through, INCOMING as u64);
                    assert!(!credit_received, "duplicate cumulative credit");
                    credit_received = true;
                }
                other => bail!("unexpected mixed large-frame client message: {other:?}"),
            }
        }
        // No forward ACK exists until eight actual, decryptable Data frames
        // and the original cumulative credit have traversed this TLS socket.
        for reply in replies.into_iter().rev() {
            write_fragmented_test_server_wire_v1(
                stream,
                &ProductRelayWireMessageV1::ForwardOutcome(reply),
            )?;
        }
        Ok(())
    });
    let client = ProductRelayClientV1::connect(&local, &fixture.config).unwrap();
    client
        .stream
        .sock
        .inner
        .set_test_buffer_sizes(4096, 64 * 1024)
        .unwrap();
    let mut pipeline = client.into_pipeline().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut submitted = 0;
    let mut received = 0;
    let mut completed = 0;
    let mut peak_pending = 0;
    let mut stalled = false;
    let mut during_write = false;
    while completed != OUTGOING
        || received != INCOMING
        || pipeline.client.delivery_consumed_reported != INCOMING as u64
    {
        assert!(Instant::now() < deadline, "mixed large-frame pipeline exceeded original 3s gate: submitted={submitted} received={received} completed={completed}");
        if submitted < OUTGOING && pipeline.can_submit() {
            let original = outgoing.pop_front().unwrap();
            assert_eq!(
                pipeline.try_submit_envelope(original).unwrap(),
                Some(submitted as u64)
            );
            submitted += 1;
            peak_pending = peak_pending.max(pipeline.pending.len());
        }
        let writing_before = pipeline.writing.is_some();
        let pending_before = pipeline.pending.len();
        let started = Instant::now();
        let progress = pipeline.poll().unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "poll performed a blocking socket wait"
        );
        stalled |= pipeline.write_stall_deadline.is_some();
        match progress {
            PipelineProgress::Event(event) => {
                let ProductRelayClientEventV1::Delivery(delivery) = *event else {
                    panic!("unexpected mixed large-frame event")
                };
                assert!(received < INCOMING, "duplicate inbound delivery");
                let opened = local_channel
                    .open_novorudp_frame(&delivery.envelope)
                    .unwrap();
                assert_eq!(
                    opened,
                    frame(local_channel.session_id(), received as u64, received as u8)
                );
                received += 1;
                during_write |= writing_before;
            }
            PipelineProgress::Forward { ticket, outcome } => {
                assert_eq!(submitted, OUTGOING);
                assert_eq!(received, INCOMING);
                assert_eq!(pending_before, OUTGOING - completed);
                assert_eq!(ticket, (OUTGOING - 1 - completed) as u64);
                assert_eq!(outcome.envelope_sequence, Some(ticket));
                assert!(outcome.forwarded && !outcome.queued);
                completed += 1;
            }
            PipelineProgress::Progress => {}
            PipelineProgress::Idle => pipeline.wait().unwrap(),
        }
        assert!(pipeline.pending.len() <= OUTGOING);
        assert!(pipeline.client.read_buffer.capacity() <= MAX_READ_BUFFER);
    }
    assert_eq!(
        (submitted, peak_pending, completed),
        (OUTGOING, OUTGOING, OUTGOING)
    );
    assert!(outgoing.is_empty() && pipeline.pending.is_empty());
    assert_eq!(pipeline.client.delivery_consumed, INCOMING as u64);
    assert_eq!(pipeline.client.delivery_consumed_reported, INCOMING as u64);
    assert!(stalled, "fixture did not force actual write WouldBlock");
    assert!(
        during_write,
        "inbound delivery waited for the outbound frame to finish"
    );
    drop(pipeline);
    fixture.finish();
}

#[test]
fn real_worker_retains_originals_and_serves_inbound_before_reverse_order_acks() {
    use crate::duplex::worker::{
        NetworkWorker, NetworkWorkerConfig, Outbound, SendAdmission, WorkerLimits,
    };
    let local = SigningKey::from_bytes(&[218; 32]);
    let remote = SigningKey::from_bytes(&[219; 32]);
    let remote_id = peer_id_from_ed25519_public_key_v1(&remote.verifying_key().to_bytes());
    let local_id = peer_id_from_ed25519_public_key_v1(&local.verifying_key().to_bytes());
    let (arrived, arrival) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let fixture = delayed_test_relay_v1(move |stream| {
        let (
            ProductRelayWireMessageV1::PeerHandshake {
                target_peer_id,
                handshake: RelayPeerHandshakeV1::Offer(offer),
            },
            bytes,
        ) = read_test_client_wire_v1(stream)?
        else {
            bail!("worker did not send its real peer offer");
        };
        assert_eq!(offer.initiator_peer_id, local_id);
        let responder = NodeHandshakeResponderV1::respond(
            &offer,
            &remote,
            now_ms_v1(),
            5_000,
            &mut HandshakeReplayCacheV1::default(),
        )?;
        let response = responder.response().clone();
        let mut channel = responder.into_channel();
        // A real E2E response may arrive before the offer's relay ACK.
        write_fragmented_test_server_wire_v1(
            stream,
            &ProductRelayWireMessageV1::PeerHandshakeDelivery(RelayPeerHandshakeDeliveryV1 {
                source_peer_id: target_peer_id.clone(),
                target_peer_id: local_id.clone(),
                received_at_ms: now_ms_v1(),
                handshake: RelayPeerHandshakeV1::Response(response),
            }),
        )?;
        write_fragmented_test_server_wire_v1(
            stream,
            &ProductRelayWireMessageV1::ForwardOutcome(RelayForwardOutcomeV1 {
                disposition: RelayForwardDispositionV1::Forwarded,
                source_peer_id: local_id.clone(),
                target_peer_id: target_peer_id.clone(),
                forwarded: true,
                queued: false,
                payload_treated_opaque: true,
                envelope_session_id: None,
                envelope_sequence: None,
                admitted_wire_bytes: bytes,
            }),
        )?;
        let mut replies = Vec::new();
        while replies.len() != 7 {
            let (wire, bytes) = read_test_client_wire_v1(stream)?;
            match wire {
                ProductRelayWireMessageV1::Data(envelope) => {
                    let index = replies.len();
                    let frame = channel.open_novorudp_frame(&envelope)?;
                    assert_eq!(frame.sequence, index as u64);
                    assert_eq!(frame.payload, vec![index as u8; 32]);
                    replies.push(outcome(&envelope, bytes));
                    let reverse = NovoRudpTransportFrameV0::new(
                        NovoRudpTransportFrameKindV0::Data,
                        channel.session_id(),
                        77,
                        u64::from_le_bytes(*b"NVNET001"),
                        index as u64,
                        0,
                        vec![index as u8; 48],
                    );
                    write_fragmented_test_server_wire_v1(
                        stream,
                        &ProductRelayWireMessageV1::Delivery(OpaqueRelayDeliveryV1 {
                            source_peer_id: target_peer_id.clone(),
                            target_peer_id: local_id.clone(),
                            received_at_ms: now_ms_v1(),
                            envelope: channel.seal_novorudp_frame(&reverse)?,
                        }),
                    )?;
                }
                ProductRelayWireMessageV1::DeliveryConsumedV1 { through } => assert_eq!(through, 7),
                _ => bail!("unexpected worker wire while collecting the real data window"),
            }
        }
        arrived.send(())?;
        released.recv_timeout(Duration::from_secs(3))?;
        for reply in replies.into_iter().rev() {
            write_fragmented_test_server_wire_v1(
                stream,
                &ProductRelayWireMessageV1::ForwardOutcome(reply),
            )?;
        }
        Ok(())
    });
    let mut worker = NetworkWorker::start(
        NetworkWorkerConfig {
            chain_id: 77,
            relay: fixture.config.clone(),
            peers: vec![remote_id.clone()],
            limits: WorkerLimits::default(),
            handshake_timeout_ms: 5_000,
            reconnect_delay_ms: 50,
            heartbeat_interval_ms: 5_000,
            queue_ttl_ms: 30_000,
        },
        local,
    )
    .unwrap();
    for index in 0..7u8 {
        let mut original = Outbound {
            peer_id: remote_id.clone(),
            bytes: vec![index; 32],
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match worker.try_send(original.peer_id, original.bytes).unwrap() {
                SendAdmission::Accepted => break,
                SendAdmission::Backpressure(returned) => {
                    original = returned;
                    std::thread::yield_now();
                }
                result => panic!("unexpected original admission {result:?}"),
            }
            assert!(Instant::now() < deadline);
        }
    }
    arrival.recv_timeout(Duration::from_secs(3)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Ok(status) = worker.status() {
            break status;
        }
        assert!(Instant::now() < deadline, "status remained unavailable");
        std::thread::yield_now();
    };
    assert_eq!(status.outbound_messages, 7);
    assert_eq!(status.outbound_bytes, 7 * 32);
    assert_eq!(status.relay_admissions, 0);
    assert!(status.peak_pending_forwards >= 7);
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut inbound = 0;
    while inbound != 7 {
        assert!(Instant::now() < deadline);
        if let Some(message) = worker.try_recv().unwrap() {
            assert_eq!(message.peer_id, remote_id);
            assert_eq!(message.bytes, vec![inbound as u8; 48]);
            inbound += 1;
        } else {
            std::thread::yield_now();
        }
    }
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        assert!(
            Instant::now() < deadline,
            "worker did not settle reversed acknowledgements"
        );
        let Ok(status) = worker.status() else {
            std::thread::yield_now();
            continue;
        };
        if status.relay_admissions == 7 && status.pending_forwards == 0 {
            assert_eq!(status.outbound_messages, 0);
            assert_eq!(status.outbound_bytes, 0);
            assert_eq!(status.relay_reconnects, 1);
            assert_eq!(status.outbound_expired, 0);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not settle reversed acknowledgements: {status:?}"
        );
        std::thread::yield_now();
    }
    worker.shutdown().unwrap();
    fixture.finish();
}
