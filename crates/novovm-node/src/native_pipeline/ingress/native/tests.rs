use super::*;
use anyhow::Context;
use ed25519_dalek::SigningKey;
use novovm_network::{
    novorudp::{
        handshake::NativeHandshake,
        object_ack::encode_object_ack,
        object_sender::{NativeObjectSender, ObjectSendState},
        NovoRudpPacingProfile,
    },
    product_overlay::HandshakeReplayCacheV1,
};
use std::{io::ErrorKind, net::UdpSocket, sync::Arc, time::Duration};

fn pump_handshake(
    socket: &UdpSocket,
    handshake: &mut NativeHandshake,
    cache: &mut HandshakeReplayCacheV1,
    start: Instant,
) -> Result<()> {
    let mut buffer = [0; 1201];
    for _ in 0..8 {
        match socket.recv_from(&mut buffer) {
            Ok((count, source)) => {
                let result = handshake.receive(
                    source,
                    &buffer[..count],
                    1000 + start.elapsed().as_millis() as u64,
                    Instant::now(),
                    cache,
                );
                if !handshake.is_ready() {
                    result?;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Actual original-node ingress consumers with native UDP handshake, bounded
/// send/ACK/repair polls and APFL assembly. No test-created channel/ACK shortcut;
/// consensus and the executable's continuous lifecycle remain separate.
pub(crate) fn deliver(
    wire: Vec<u8>,
    limits: ApflLimits,
    policy: DirectNovFeePolicy,
) -> Result<AuthenticationRequest> {
    let left = UdpSocket::bind("127.0.0.1:0")?;
    let right = UdpSocket::bind("127.0.0.1:0")?;
    left.set_nonblocking(true)?;
    right.set_nonblocking(true)?;
    let a = SigningKey::from_bytes(&[23; 32]);
    let b = SigningKey::from_bytes(&[47; 32]);
    let a_key = a.verifying_key().to_bytes();
    let b_key = b.verifying_key().to_bytes();
    let start = Instant::now();
    let mut left_handshake = NativeHandshake::start(
        a,
        b_key,
        right.local_addr()?,
        1000,
        start,
        Duration::from_secs(5),
        Duration::from_millis(10),
    )?;
    let mut right_handshake = NativeHandshake::start(
        b,
        a_key,
        left.local_addr()?,
        1000,
        start,
        Duration::from_secs(5),
        Duration::from_millis(10),
    )?;
    let mut left_cache = HandshakeReplayCacheV1::default();
    let mut right_cache = HandshakeReplayCacheV1::default();
    while !(left_handshake.is_ready() && right_handshake.is_ready()) {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "native test handshake timeout"
        );
        let now = Instant::now();
        let wall = 1000 + start.elapsed().as_millis() as u64;
        left_handshake.try_send(&left, wall, now)?;
        right_handshake.try_send(&right, wall, now)?;
        pump_handshake(&left, &mut left_handshake, &mut left_cache, start)?;
        pump_handshake(&right, &mut right_handshake, &mut right_cache, start)?;
        std::thread::yield_now();
    }
    let mut sender = left_handshake
        .take_session()
        .context("left session missing")?;
    let mut receiver = right_handshake
        .take_session()
        .context("right session missing")?;
    let peer = receiver.remote_peer_id().to_owned();
    let domain = [0x31; 32];
    let message = Arc::new(OutgoingMessage::with_profile(
        domain,
        wire,
        limits.body_bytes,
        FragmentProfile::NativeUdpV2,
    )?);
    let now = Instant::now();
    let mut input = NativeApflReceiver::new(
        domain,
        vec![peer.clone()],
        ReassemblyLimits {
            max_message_bytes: limits.body_bytes,
            messages: 1,
            bytes: limits.body_bytes,
            peer_messages: 1,
            peer_bytes: limits.body_bytes,
            ttl: Duration::from_secs(5),
        },
        limits,
        policy,
    )?;
    // Authenticated transport alone must not admit another stream/object.
    let mut wrong = apfl_fragment_frame(&message, sender.session_id(), 0)?;
    wrong.stream_id ^= 1;
    assert!(input
        .accept_datagram(
            &mut receiver,
            left.local_addr()?,
            &sender.encode(&wrong)?,
            now
        )
        .is_err());
    wrong = apfl_fragment_frame(&message, sender.session_id(), 0)?;
    wrong.object_id ^= 1;
    assert!(input
        .accept_datagram(
            &mut receiver,
            left.local_addr()?,
            &sender.encode(&wrong)?,
            now
        )
        .is_err());
    assert_eq!(input.reserved_bytes(), 0);
    let config = NovoRudpWindowConfig {
        packet_copies: 1,
        tail_packet_copies: 1,
        batch_size: 16,
        batch_pause_ms: 0,
        ack_wait_ms: 10,
        no_progress_backoff: false,
        ..Default::default()
    };
    let pacing = NovoRudpPacingProfile {
        packet_copies: 1,
        tail_packet_copies: 1,
        batch_size: 16,
        batch_pause_ms: 0,
        ack_wait_ms: 10,
        no_progress_backoff: false,
    };
    let mut outbound = NativeObjectSender::new(
        message.clone(),
        sender.session_id(),
        APFL_STREAM_V1,
        config,
        pacing,
        now,
        Duration::from_secs(5),
    )?;
    let mut complete: Option<NativeApflInput> = None;
    let mut buffer = [0; 1201];
    let mut received = 0usize;
    let mut epoch = 0;
    let mut lost_done = false;
    let mut repaired_after_done = false;
    while outbound.status() == ObjectSendState::Sending {
        ensure!(
            now.elapsed() < Duration::from_secs(5),
            "native input completion timeout"
        );
        let current = Instant::now();
        outbound.poll_send(&left, &mut sender, current, 16)?;
        for _ in 0..64 {
            match right.recv_from(&mut buffer) {
                Ok((count, source)) => {
                    received += 1;
                    // Lose one real data datagram. No per-chunk fresh ACK wait.
                    if received == 7 {
                        continue;
                    }
                    let admitted =
                        input.accept_datagram(&mut receiver, source, &buffer[..count], current)?;
                    if complete.is_some() {
                        assert_eq!(admitted, FragmentAdmission::Duplicate);
                        repaired_after_done = true;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        if let Some(item) = input.poll_request(current, 64)? {
            assert!(
                complete.is_none(),
                "lost DONE delivered the same input twice"
            );
            complete = Some(item);
        }
        epoch += 1;
        let ack =
            input.transport_feedback(&peer, message.id(), receiver.session_id(), epoch, current)?;
        if let Some(ack) = ack {
            if !ack.receiver_done || !lost_done || repaired_after_done {
                receiver.try_send(
                    &right,
                    &encode_object_ack(message.id(), APFL_STREAM_V1, &ack)?,
                )?;
            }
        }
        for _ in 0..64 {
            let Some(frame) = sender.try_receive(&left)? else {
                break;
            };
            // Lose the first actual encrypted completion, then withhold retries
            // until a sender probe arrives. The receiver, not this test, owns DONE.
            if complete.is_some() && !lost_done && frame.payload.get(48) == Some(&1) {
                lost_done = true;
                continue;
            }
            outbound.on_ack(&frame, current)?;
        }
        std::thread::yield_now();
    }
    assert_eq!(outbound.status(), ObjectSendState::Delivered);
    assert!(lost_done && repaired_after_done);
    let complete = complete.context("delivered without a complete input")?;
    assert_eq!(complete.object_id, message.id());
    assert_eq!(complete.peer, peer);
    assert_eq!(input.pending_messages(), 0);
    assert_eq!(input.reserved_bytes(), 0);
    Ok(complete.request)
}
