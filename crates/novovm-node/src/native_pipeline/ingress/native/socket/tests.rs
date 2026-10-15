use super::*;
use anyhow::Context;
use ed25519_dalek::SigningKey;
use novovm_network::novorudp::{
    endpoint::{NativeHandshakeBudget, NativePeer},
    object_sender::{NativeObjectSender, ObjectSendState},
    NovoRudpPacingProfile,
};
use std::{net::UdpSocket, sync::Arc, time::Duration};

/// Same socket lifecycle consumed by the node, not a fixture-owned handshake or
/// synthetic ACK. Assembly is deliberately withheld after all fragments arrive.
/// The first completion ACK is lost, forcing a probe and bounded DONE re-answer.
pub(crate) fn deliver(
    wire: Vec<u8>,
    limits: ApflLimits,
    policy: DirectNovFeePolicy,
) -> Result<AuthenticationRequest> {
    let left = UdpSocket::bind("127.0.0.1:0")?;
    let right = UdpSocket::bind("127.0.0.1:0")?;
    let left_addr = left.local_addr()?;
    let right_addr = right.local_addr()?;
    let a = SigningKey::from_bytes(&[23; 32]);
    let b = SigningKey::from_bytes(&[47; 32]);
    let a_key = a.verifying_key().to_bytes();
    let b_key = b.verifying_key().to_bytes();
    let start = Instant::now();
    let budget = || NativeHandshakeBudget {
        lifetime: Duration::from_secs(5),
        retry: Duration::from_millis(10),
    };
    let mut sender = NativeEndpoint::bind(
        left,
        a,
        vec![NativePeer {
            key: b_key,
            address: right_addr,
        }],
        budget(),
        1000,
        start,
    )?;
    let endpoint = NativeEndpoint::bind(
        right,
        b,
        vec![NativePeer {
            key: a_key,
            address: left_addr,
        }],
        budget(),
        1000,
        start,
    )?;
    let peer = endpoint.peer_id(0)?.to_owned();
    let domain = [0x31; 32];
    let mut input = NativeApflSocket::new(
        endpoint,
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
    while sender.session_id(0).is_none() || input.session_id(0).is_none() {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "socket handshake timeout"
        );
        let wall = 1000 + start.elapsed().as_millis() as u64;
        sender.poll(wall, Instant::now(), 8)?;
        let report = input.poll(wall, Instant::now(), 8)?;
        assert!(report.completed.is_empty());
        std::thread::yield_now();
    }
    let session = sender.session_id(0).unwrap();
    let message = Arc::new(OutgoingMessage::with_profile(
        domain,
        wire,
        limits.body_bytes,
        FragmentProfile::NativeUdpV2,
    )?);
    let mut wrong = apfl_fragment_frame(&message, session, 0)?;
    wrong.stream_id ^= 1;
    assert!(sender.try_send(0, &wrong)?);
    wrong = apfl_fragment_frame(&message, session, 0)?;
    wrong.object_id ^= 1;
    assert!(sender.try_send(0, &wrong)?);
    let mut rejected = 0;
    while rejected < 2 {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "invalid frame observation timeout"
        );
        rejected += input
            .poll(1000 + start.elapsed().as_millis() as u64, Instant::now(), 8)?
            .rejected;
    }
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
        session,
        APFL_STREAM_V1,
        config,
        pacing,
        Instant::now(),
        Duration::from_secs(5),
    )?;
    let mut accepted = 0;
    // No free assembly slot: frame receipt cannot claim complete handoff.
    while accepted < message.frame_count() {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "fragment receive timeout"
        );
        let now = Instant::now();
        let wall = 1000 + start.elapsed().as_millis() as u64;
        sender.poll_object(0, &mut outbound, now, 16)?;
        accepted += input.poll(wall, now, 64)?.accepted_fragments;
        for received in sender.poll(wall, now, 64)?.frames {
            outbound.on_ack(received.frame(), now)?;
        }
        assert_eq!(outbound.status(), ObjectSendState::Sending);
    }
    assert_eq!(input.pending_messages(), 1);
    assert!(input.reserved_bytes() > 0);
    let mut complete = None;
    let mut lost_done = false;
    let mut duplicates = 0;
    while outbound.status() == ObjectSendState::Sending {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "socket completion timeout"
        );
        let now = Instant::now();
        let wall = 1000 + start.elapsed().as_millis() as u64;
        sender.poll_object(0, &mut outbound, now, 16)?;
        duplicates += input.poll(wall, now, 64)?.duplicate_fragments;
        if let Some(item) = input.poll_request(now, 64)? {
            ensure!(complete.is_none(), "duplicate socket body handoff");
            complete = Some(item);
        }
        for received in sender.poll(wall, now, 64)?.frames {
            if !lost_done && received.frame().payload.get(48) == Some(&1) {
                lost_done = true;
                continue;
            }
            outbound.on_ack(received.frame(), now)?;
        }
        std::thread::yield_now();
    }
    assert_eq!(outbound.status(), ObjectSendState::Delivered);
    assert!(lost_done && duplicates > 0);
    assert_eq!(input.pending_messages(), 0);
    assert_eq!(input.reserved_bytes(), 0);
    let complete = complete.context("transport done without assembly handoff")?;
    assert_eq!(complete.peer, peer);
    assert_eq!(complete.object_id, message.id());
    Ok(complete.request)
}
