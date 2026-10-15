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
