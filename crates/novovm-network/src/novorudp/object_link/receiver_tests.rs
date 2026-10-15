use super::*;
use crate::novorudp::endpoint::{NativeHandshakeBudget, NativePeer};
use ed25519_dalek::SigningKey;
use std::net::UdpSocket;

fn lanes() -> Vec<NativeLaneConfig> {
    [1, 2]
        .into_iter()
        .map(|stream| NativeLaneConfig {
            stream,
            domain: [stream as u8; 32],
            receive: ReassemblyLimits {
                max_message_bytes: 8192,
                messages: 2,
                bytes: 16_384,
                peer_messages: 1,
                peer_bytes: 8192,
                ttl: Duration::from_secs(5),
            },
            send: NativeSendLimits {
                messages: 2,
                bytes: 16_384,
                peer_messages: 1,
                peer_bytes: 8192,
            },
        })
        .collect()
}

fn send_policy() -> NativeSendPolicy {
    NativeSendPolicy {
        window: NovoRudpWindowConfig {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 16,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
            ..Default::default()
        },
        pacing: NovoRudpPacingProfile {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 16,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
        },
        ttl: Duration::from_secs(5),
    }
}

fn pair() -> Result<(NativeObjectLink, NativeObjectLink, Arc<UdpSocket>)> {
    let left = UdpSocket::bind("127.0.0.1:0")?;
    let right = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
    let left_address = left.local_addr()?;
    let right_address = right.local_addr()?;
    let left_key = SigningKey::from_bytes(&[81; 32]);
    let right_key = SigningKey::from_bytes(&[82; 32]);
    let budget = || NativeHandshakeBudget {
        lifetime: Duration::from_secs(5),
        retry: Duration::from_millis(10),
    };
    let now = Instant::now();
    let a = NativeEndpoint::bind(
        left,
        left_key.clone(),
        vec![NativePeer {
            key: right_key.verifying_key().to_bytes(),
            address: right_address,
        }],
        budget(),
        1000,
        now,
    )?;
    let b = NativeEndpoint::bind_shared(
        Arc::clone(&right),
        right_key,
        vec![NativePeer {
            key: left_key.verifying_key().to_bytes(),
            address: left_address,
        }],
        budget(),
        1000,
        now,
    )?;
    Ok((
        NativeObjectLink::new(a, lanes(), send_policy())?,
        NativeObjectLink::new(b, lanes(), send_policy())?,
        right,
    ))
}

#[test]
fn external_receiver_keeps_authenticated_reassembly_ack_and_lane_budgets() -> Result<()> {
    let (mut a, mut b, socket) = pair()?;
    let start = Instant::now();
    let mut received_datagrams = 0;
    while a.session_id(0).is_none() || b.session_id(0).is_none() {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "shared receiver handshake timeout"
        );
        let now = Instant::now();
        let wall = 1000 + start.elapsed().as_millis() as u64;
        assert!(a.poll(wall, now, 8)?.completed.is_empty());
        let mut calls = 0;
        let result = b.poll_with_receiver(wall, now, 8, |bytes| {
            calls += 1;
            let received = socket.recv_from(bytes);
            received_datagrams += usize::from(received.is_ok());
            received.map(Some)
        })?;
        assert!(calls <= 8);
        assert_eq!(result.rejected, 0);
        assert!(result.completed.is_empty());
        std::thread::yield_now();
    }
    assert!(received_datagrams > 0);
    let session = b.session_id(0);
    let messages = [1, 2]
        .map(|stream| {
            Ok(Arc::new(OutgoingMessage::with_profile(
                [stream as u8; 32],
                vec![stream as u8; 1800 + stream as usize * 1024],
                8192,
                FragmentProfile::NativeUdpV2,
            )?))
        })
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    for (index, message) in messages.iter().enumerate() {
        assert!(a.try_send(0, index as u64 + 1, message, Instant::now())?);
    }
    let mut delivered = BTreeSet::new();
    let mut completed = BTreeSet::new();
    let mut fragments = 0;
    let mut feedback = 0;
    while delivered.len() < 2 || completed.len() < 2 {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "shared receiver object timeout"
        );
        let now = Instant::now();
        let wall = 1000 + start.elapsed().as_millis() as u64;
        let sent = a.poll(wall, now, 8)?;
        assert!(sent.data_sent <= 8 && sent.feedback_sent <= 8);
        for done in sent.completed {
            assert_eq!(done.state, ObjectSendState::Delivered);
            assert!(Arc::ptr_eq(
                &done.body,
                &messages[(done.stream - 1) as usize]
            ));
            assert!(completed.insert(done.stream));
        }
        let mut calls = 0;
        let received = b.poll_with_receiver(wall, now, 8, |bytes| {
            calls += 1;
            socket.recv_from(bytes).map(Some)
        })?;
        assert!(calls <= 8);
        assert_eq!(received.rejected, 0);
        assert!(received.feedback_sent <= 8 && received.data_sent <= 8);
        assert!(received.session_changes.is_empty());
        fragments += received.accepted_fragments;
        feedback += received.feedback_sent;
        for stream in [1, 2] {
            if let Some(item) = b.poll_complete(stream, now, 8)? {
                assert_eq!(item.id, messages[(stream - 1) as usize].id());
                assert_eq!(
                    item.chunks.concat(),
                    vec![stream as u8; 1800 + stream as usize * 1024]
                );
                assert!(delivered.insert(stream));
            }
        }
        std::thread::yield_now();
    }
    assert!(fragments > 2 && feedback >= 2);
    assert_eq!(b.session_id(0), session);
    assert_eq!(a.outbound_usage(1)?, (0, 0));
    assert_eq!(a.outbound_usage(2)?, (0, 0));
    Ok(())
}

#[test]
fn external_receiver_preserves_poll_quantum_and_io_error_accounting() -> Result<()> {
    let (_a, mut b, _socket) = pair()?;
    let now = Instant::now();
    for quantum in [0, 65] {
        let mut calls = 0;
        assert!(b
            .poll_with_receiver(1000, now, quantum, |_| {
                calls += 1;
                Err(std::io::ErrorKind::WouldBlock.into())
            })
            .is_err());
        assert_eq!(calls, 0);
    }
    let mut calls = 0;
    let result = b.poll_with_receiver(1000, now, 3, |_| {
        calls += 1;
        Err(std::io::ErrorKind::Interrupted.into())
    })?;
    assert_eq!(calls, 3);
    assert_eq!(result.receive_errors, 3);
    assert_eq!(result.accepted_fragments, 0);
    assert!(result.completed.is_empty());
    let mut calls = 0;
    let result = b.poll_with_receiver(1000, now, 3, |_| {
        calls += 1;
        Ok(None)
    })?;
    assert_eq!(calls, 3);
    assert_eq!(result.receive_errors, 0);
    assert_eq!(result.accepted_fragments, 0);
    assert!(result.completed.is_empty());
    let mut calls = 0;
    let result = b.poll_with_receiver(1000, now, 3, |_| {
        calls += 1;
        Err(std::io::ErrorKind::WouldBlock.into())
    })?;
    assert_eq!(calls, 1);
    assert_eq!(result.receive_errors, 0);
    assert!(result.completed.is_empty());
    Ok(())
}
