use super::*;
use crate::novorudp::{fragments::*, *};
use crate::product_overlay::*;
use ed25519_dalek::SigningKey;
use std::time::{Duration, Instant};

fn channels() -> (E2eSecureChannelV1, E2eSecureChannelV1) {
    let a = SigningKey::from_bytes(&[31; 32]);
    let b = SigningKey::from_bytes(&[47; 32]);
    let initiator = NodeHandshakeInitiatorV1::start(
        &a,
        peer_id_from_ed25519_public_key_v1(&b.verifying_key().to_bytes()),
        1000,
        5000,
    )
    .unwrap();
    let responder = NodeHandshakeResponderV1::respond(
        initiator.offer(),
        &b,
        1100,
        5000,
        &mut HandshakeReplayCacheV1::default(),
    )
    .unwrap();
    let a = initiator
        .complete(
            responder.response(),
            1200,
            &mut HandshakeReplayCacheV1::default(),
        )
        .unwrap();
    (a, responder.into_channel())
}

fn frame(session: [u8; 16], sequence: u64, payload: Vec<u8>) -> NovoRudpTransportFrameV0 {
    NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        session,
        11,
        12,
        sequence,
        0,
        payload,
    )
}

#[test]
fn owned_and_borrowed_open_share_source_authentication_and_replay_state() -> Result<()> {
    let address = "127.0.0.1:12345".parse()?;
    let (a, b) = channels();
    let mut sender = NativeUdpSession::new(a, address)?;
    let mut receiver = NativeUdpSession::new(b, address)?;
    let expected = frame(sender.session_id(), 0, vec![8; 848]);
    let wire = sender.encode(&expected)?;
    assert!(receiver
        .open_route_owned(NativeRoute::Udp("127.0.0.1:12346".parse()?), wire.clone())
        .is_err());
    let mut tampered = wire.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(receiver
        .open_route_owned(NativeRoute::Udp(address), tampered.clone())
        .is_err());
    assert!(receiver.open(address, &tampered).is_err());
    assert_eq!(
        receiver.open_route_owned(NativeRoute::Udp(address), wire.clone())?,
        expected
    );
    assert!(receiver.open(address, &wire).is_err());
    assert!(receiver
        .open_route_owned(NativeRoute::Udp(address), wire)
        .is_err());
    let repair = sender.encode(&expected)?;
    assert_eq!(receiver.open(address, &repair)?, expected);
    assert!(receiver
        .open_route_owned(NativeRoute::Udp(address), repair)
        .is_err());
    Ok(())
}

#[test]
fn completed_send_reports_exact_secure_wire_bytes_not_payload_or_budget_denial() -> Result<()> {
    let (a, b) = channels();
    let tx = UdpSocket::bind("127.0.0.1:0")?;
    let rx = UdpSocket::bind("127.0.0.1:0")?;
    rx.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut sender = NativeUdpSession::new(a, rx.local_addr()?)?;
    let mut receiver = NativeUdpSession::new(b, tx.local_addr()?)?;
    let packet = frame(sender.session_id(), 0, vec![9; 100]);
    let sent = sender
        .try_send_limited(&tx, &packet, Instant::now(), None)?
        .unwrap();
    let mut bytes = [0; MAX_DATAGRAM_BYTES];
    let (received, source) = rx.recv_from(&mut bytes)?;
    assert_eq!(sent, received);
    assert!(sent > packet.payload.len());
    assert_eq!(receiver.open(source, &bytes[..received])?, packet);
    if let Some(snapshot) = super::super::diagnostics::native_io_cost_snapshot() {
        let streams = snapshot["traffic"]["streams"].as_array().unwrap();
        let raw = streams.iter().find(|s| s["stream"] == 0).unwrap();
        let lane = streams.iter().find(|s| s["stream"] == 11).unwrap();
        assert_eq!(raw["events"]["wire_sent"][3]["packets"], 1);
        assert_eq!(raw["events"]["wire_sent"][3]["bytes"], sent);
        assert_eq!(lane["events"]["sealed_sent"][0]["bytes"], sent);
        assert_eq!(lane["events"]["authenticated_received"][0]["bytes"], sent);
        // Same ciphertext is rejected before authenticated accounting.
        assert!(receiver.open(source, &bytes[..received]).is_err());
        let next = super::super::diagnostics::native_io_cost_snapshot().unwrap();
        assert_eq!(snapshot["traffic"], next["traffic"]);
    }
    // Actual no-capacity path, not a fake successful send or a size estimate.
    let now = Instant::now();
    let mut budget = NativeWireBudget::new(2400, now)?;
    while sender
        .try_send_limited(&tx, &packet, now, Some(&mut budget))?
        .is_some()
    {
        rx.recv_from(&mut bytes)?;
    }
    let before = budget.usage(now).sent_bytes;
    let before_traffic = super::super::diagnostics::native_io_cost_snapshot();
    assert!(sender
        .try_send_limited(&tx, &packet, now, Some(&mut budget))?
        .is_none());
    assert_eq!(budget.usage(now).sent_bytes, before);
    if let Some(before) = before_traffic {
        let after = super::super::diagnostics::native_io_cost_snapshot().unwrap();
        let raw = |s: &serde_json::Value| {
            s["traffic"]["streams"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["stream"] == 0)
                .unwrap()["events"]
                .clone()
        };
        assert_eq!(raw(&before)["wire_sent"], raw(&after)["wire_sent"]);
        assert_eq!(
            raw(&after)["send_budget_deferred"][3]["packets"]
                .as_u64()
                .unwrap(),
            raw(&before)["send_budget_deferred"][3]["packets"]
                .as_u64()
                .unwrap()
                + 1
        );
    }
    Ok(())
}

#[test]
fn deferred_partial_write_and_cancel_keep_known_and_unknown_bytes_separate() -> Result<()> {
    use crate::novorudp::carrier::NativeDatagramIo;
    struct Partial {
        cancel: bool,
        cancel_bytes: usize,
    }
    impl NativeDatagramIo for Partial {
        fn deferred_writes(&self) -> bool {
            true
        }
        fn try_receive(&self, _: &mut [u8]) -> std::io::Result<Option<(usize, NativeRoute)>> {
            Err(ErrorKind::WouldBlock.into())
        }
        fn try_send(&self, _: &[u8], _: NativeRoute) -> std::io::Result<usize> {
            if self.cancel {
                Err(ErrorKind::WouldBlock.into())
            } else {
                Ok(17)
            }
        }
        fn cancel_send(&self, wire: &[u8], _: NativeRoute) -> std::io::Result<usize> {
            Ok(self.cancel_bytes.min(wire.len()))
        }
    }
    for (cancel, cancel_bytes) in [(false, 17), (true, 17), (true, 0), (true, usize::MAX)] {
        let (channel, _) = channels();
        let mut session = NativeUdpSession::new(channel, "127.0.0.1:12345".parse()?)?;
        let now = Instant::now();
        let mut budget = NativeWireBudget::new(100_000, now)?;
        let mut payload = vec![0; 72];
        payload[40..72].fill(7);
        let frame = frame(session.session_id(), 0, payload);
        let carrier = Partial {
            cancel,
            cancel_bytes,
        };
        let sealed = || {
            super::super::diagnostics::native_io_cost_snapshot().map(|s| {
                s["traffic"]["streams"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["stream"] == 11)
                    .map_or(0, |s| {
                        s["events"]["sealed_sent"][0]["packets"].as_u64().unwrap()
                    })
            })
        };
        let before = sealed();
        let result = session.try_send_limited(&carrier, &frame, now, Some(&mut budget));
        if cancel {
            assert!(result?.is_none());
            assert_eq!(
                session
                    .cancel_send(&carrier, 11, [7; 32], false, Some(&mut budget), now)
                    .is_err(),
                cancel_bytes == 17
            );
            // A second cancellation cannot report the completed write again.
            session.cancel_send(&carrier, 11, [7; 32], false, Some(&mut budget), now)?;
        } else {
            assert!(result.is_err());
        }
        if let Some(before) = before {
            assert_eq!(
                sealed(),
                Some(before + u64::from(cancel_bytes == usize::MAX))
            );
        }
        let usage = budget.usage(now);
        if cancel_bytes == 17 {
            assert_eq!(usage.sent_bytes, 17);
            assert!(usage.uncertain_send_bytes > 0);
        } else {
            assert_eq!(usage.uncertain_send_bytes, 0);
            assert_eq!(usage.sent_bytes > 0, cancel_bytes == usize::MAX);
        }
        assert_eq!(usage.used, usage.sent_bytes + usage.uncertain_send_bytes);
        assert_eq!(usage.reserved_send_bytes, 0);
    }
    Ok(())
}

#[test]
fn native_binding_rejects_source_replay_tamper_and_old_session_before_body_admission() -> Result<()>
{
    let address = "127.0.0.1:12345".parse()?;
    let (a, b) = channels();
    let mut a = NativeUdpSession::new(a, address)?;
    let mut b = NativeUdpSession::new(b, address)?;
    let inner = frame(a.session_id(), 0, vec![8; 848]);
    let wire = a.encode(&inner)?;
    assert!(wire.len() <= MAX_DATAGRAM_BYTES);
    assert!(b.open("127.0.0.1:12346".parse()?, &wire).is_err());
    let mut bad = wire.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(b.open(address, &bad).is_err());
    assert_eq!(b.open(address, &wire)?, inner);
    assert!(b.open(address, &wire).is_err());
    // A repair gets a fresh secure sequence, preserving the object sequence.
    let repair = a.encode(&inner)?;
    assert_eq!(b.open(address, &repair)?, inner);
    assert!(a.encode(&frame([0; 16], 0, vec![1])).is_err());
    assert!(a.encode(&frame(a.session_id(), 0, vec![1; 1200])).is_err());
    let (_, replacement) = channels();
    assert!(NativeUdpSession::new(replacement, address)?
        .open(address, &repair)
        .is_err());
    Ok(())
}

#[test]
fn native_udp_large_object_loss_reorder_uses_original_window_repair_without_execution_ack(
) -> Result<()> {
    let left = UdpSocket::bind("127.0.0.1:0")?;
    let right = UdpSocket::bind("127.0.0.1:0")?;
    left.set_nonblocking(true)?;
    right.set_nonblocking(true)?;
    let (a, b) = channels();
    let mut a = NativeUdpSession::new(a, right.local_addr()?)?;
    let mut b = NativeUdpSession::new(b, left.local_addr()?)?;
    let peer = b.remote_peer_id().to_owned();
    let domain = [0x31; 32];
    let bytes: Vec<_> = (0..256 * 1024).map(|i| (i % 251) as u8).collect();
    let outgoing = OutgoingMessage::with_profile(
        domain,
        bytes.clone(),
        1024 * 1024,
        FragmentProfile::NativeUdpV2,
    )?;
    let now = Instant::now();
    let mut receive = Reassembler::with_profile(
        domain,
        vec![peer.clone()],
        ReassemblyLimits {
            max_message_bytes: 1024 * 1024,
            messages: 2,
            bytes: 2 * 1024 * 1024,
            peer_messages: 1,
            peer_bytes: 1024 * 1024,
            ttl: Duration::from_secs(30),
        },
        FragmentProfile::NativeUdpV2,
    )?;
    let session_id = a.session_id();
    let send = |index: usize,
                receive: &mut Reassembler,
                a: &mut NativeUdpSession,
                b: &mut NativeUdpSession|
     -> Result<()> {
        let mut inner = frame(a.session_id(), index as u64, outgoing.frame(index)?);
        inner.object_id = u64::from_be_bytes(outgoing.id()[..8].try_into()?);
        ensure!(a.try_send(&left, &inner)?, "loopback send backpressure");
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(opened) = b.try_receive(&right)? {
                ensure!(
                    opened.stream_id == 11 && opened.object_id == inner.object_id,
                    "wrong object"
                );
                receive.push(&peer, &opened.payload, now)?;
                return Ok(());
            }
            ensure!(Instant::now() < until, "native loopback receive timeout");
            std::thread::yield_now();
        }
    };
    // No transaction pool, AOEM or ledger is present. Initial fanout does not
    // wait for any ACK. Missing indexes are deliberately not sent.
    for start in (0..outgoing.frame_count()).step_by(64) {
        for index in (start..(start + 64).min(outgoing.frame_count())).rev() {
            if index % 17 != 3 {
                send(index, &mut receive, &mut a, &mut b)?;
            }
        }
    }
    assert!(receive.poll_complete(now, 64)?.is_none());
    let mut state = NovoRudpSenderState::new();
    let config = NovoRudpWindowConfig::default();
    let pacing = NovoRudpPacingProfile::default();
    let exchange_ack = |ack: &NovoRudpAckFrame,
                        a: &mut NativeUdpSession,
                        b: &mut NativeUdpSession|
     -> Result<NovoRudpAckFrame> {
        let frame = object_ack::encode_object_ack(outgoing.id(), 11, ack)?;
        ensure!(b.try_send(&right, &frame)?, "ACK send backpressure");
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(frame) = a.try_receive(&left)? {
                return object_ack::decode_object_ack(
                    &frame,
                    session_id,
                    11,
                    outgoing.id(),
                    outgoing.frame_count() as u64,
                );
            }
            ensure!(Instant::now() < until, "ACK receive timeout");
            std::thread::yield_now();
        }
    };
    let mut epoch = 0;
    loop {
        let (total, missing) = receive.missing_ranges(&peer, outgoing.id()).unwrap();
        if missing.is_empty() {
            break;
        }
        epoch += 1;
        let window = select_first_missing_window(&missing, total, &config).unwrap();
        let ack = NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id,
                epoch,
                sequence: None,
                window_id: Some(window.window_id),
            },
            expected_total: total,
            receiver_done: false,
            missing_count: missing_count(&missing),
            current_window: Some(window.range),
            current_window_missing_ranges: window.missing_ranges,
        };
        let ack = exchange_ack(&ack, &mut a, &mut b)?;
        let NovoRudpSenderRepairDecision::Repair(plan) =
            sender_repair_decision_from_ack(&mut state, &ack, &config, &pacing)
        else {
            panic!("original repair selector did not request missing chunks");
        };
        assert!(matches!(
            sender_repair_decision_from_ack(&mut state, &ack, &config, &pacing),
            NovoRudpSenderRepairDecision::StaleAck { .. }
        ));
        for range in plan.window.missing_ranges {
            for index in range.start..=range.end_inclusive {
                send(index as usize, &mut receive, &mut a, &mut b)?;
            }
        }
    }
    let mut completed = None;
    for _ in 0..=outgoing.frame_count().div_ceil(64) {
        if let Some(message) = receive.poll_complete(now, 64)? {
            completed = Some(message);
            break;
        }
    }
    let completed = completed.expect("complete object must finish within its hash quantum budget");
    assert_eq!(completed.chunks.concat(), bytes);
    assert_eq!(completed.id, outgoing.id());
    assert_eq!(receive.reserved_bytes(), 0);
    assert!(receive.missing_ranges(&peer, outgoing.id()).is_none());
    let done = NovoRudpAckFrame {
        header: NovoRudpFrameHeader {
            version: 1,
            kind: NovoRudpFrameKind::Ack,
            session_id,
            epoch: epoch + 1,
            sequence: None,
            window_id: None,
        },
        expected_total: outgoing.frame_count() as u64,
        receiver_done: true,
        missing_count: 0,
        current_window: None,
        current_window_missing_ranges: vec![],
    };
    let done = exchange_ack(&done, &mut a, &mut b)?;
    assert_eq!(
        sender_repair_decision_from_ack(&mut state, &done, &config, &pacing),
        NovoRudpSenderRepairDecision::ReceiverDone
    );
    // ACK now crosses real encrypted UDP too. Continuous multi-peer scheduling,
    // automatic retransmission timers and node publication remain separate tests.
    Ok(())
}
