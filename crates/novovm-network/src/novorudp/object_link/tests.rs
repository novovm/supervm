use super::*;
use crate::novorudp::endpoint::{NativeHandshakeBudget, NativePeer};
use ed25519_dalek::SigningKey;
use std::net::UdpSocket;

const CONTROL: u64 = 1;
const BODY: u64 = 2;
fn configs() -> Vec<NativeLaneConfig> {
    [CONTROL, BODY]
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
fn policy() -> NativeSendPolicy {
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
        ttl: Duration::from_secs(2),
    }
}
fn message(stream: u64, value: u8, size: usize) -> Result<Arc<OutgoingMessage>> {
    Ok(Arc::new(OutgoingMessage::with_profile(
        [stream as u8; 32],
        vec![value; size],
        8192,
        FragmentProfile::NativeUdpV2,
    )?))
}
fn pair_and_third() -> Result<(NativeObjectLink, NativeObjectLink, NativeObjectLink)> {
    let sockets = (0..3)
        .map(|_| UdpSocket::bind("127.0.0.1:0"))
        .collect::<std::io::Result<Vec<_>>>()?;
    let addresses = sockets
        .iter()
        .map(UdpSocket::local_addr)
        .collect::<std::io::Result<Vec<_>>>()?;
    let keys = [23, 47, 71].map(|seed| SigningKey::from_bytes(&[seed; 32]));
    let now = Instant::now();
    let mut links = Vec::new();
    for (i, socket) in sockets.into_iter().enumerate() {
        let peers = if i == 0 { vec![1, 2] } else { vec![0] };
        let peers = peers
            .into_iter()
            .map(|n| NativePeer {
                key: keys[n].verifying_key().to_bytes(),
                address: addresses[n],
            })
            .collect();
        let endpoint = NativeEndpoint::bind(
            socket,
            keys[i].clone(),
            peers,
            NativeHandshakeBudget {
                lifetime: Duration::from_secs(2),
                retry: Duration::from_millis(10),
            },
            1000,
            now,
        )?;
        links.push(NativeObjectLink::new(endpoint, configs(), policy())?);
    }
    while links
        .iter()
        .any(|l| (0..l.peer_count()).any(|p| l.session_id(p).is_none()))
    {
        ensure!(
            now.elapsed() < Duration::from_secs(2),
            "native link handshake timeout"
        );
        for link in &mut links {
            let result = link.poll(1000 + now.elapsed().as_millis() as u64, Instant::now(), 8)?;
            assert!(result.completed.is_empty());
        }
        std::thread::yield_now();
    }
    let c = links.pop().unwrap();
    let b = links.pop().unwrap();
    let a = links.pop().unwrap();
    Ok((a, b, c))
}

#[test]
fn body_backpressure_and_offline_peer_do_not_consume_control_lane_or_other_peer_quota() -> Result<()>
{
    let (mut a, mut b, c) = pair_and_third()?;
    drop(c); // Peer went offline AFTER the original authenticated handshake.
    let body = message(BODY, 11, 4096)?;
    let other = message(BODY, 12, 4096)?;
    let control = message(CONTROL, 21, 1800)?; // Multi-fragment QC-sized object.
    let reply = message(CONTROL, 22, 200)?;
    let start = Instant::now();
    assert!(a.try_send(1, BODY, &body, start)?);
    assert!(!a.try_send(1, BODY, &other, start)?);
    assert!(a.try_send(0, BODY, &body, start)?);
    assert!(!a.try_send(0, BODY, &other, start)?);
    assert_eq!(a.outbound_usage(BODY)?, (2, 8192));
    assert!(a.try_send(0, CONTROL, &control, start)?);
    // Duplicate enqueue retains the exact existing reservation/cursor.
    assert!(a.try_send(0, CONTROL, &control, start)?);
    assert_eq!(a.outbound_usage(CONTROL)?, (1, 1800));
    let mut got_control = false;
    let mut got_reply = false;
    let mut control_done = false;
    let mut reply_done = false;
    while !(got_control && got_reply && control_done && reply_done) {
        ensure!(
            start.elapsed() < Duration::from_secs(1),
            "healthy control starved by body/offline peer"
        );
        let now = Instant::now();
        let wall = 1100 + start.elapsed().as_millis() as u64;
        for done in a.poll(wall, now, 8)?.completed {
            assert_eq!(done.stream, CONTROL);
            assert_eq!(done.state, ObjectSendState::Delivered);
            assert!(Arc::ptr_eq(&done.body, &control));
            control_done = true;
        }
        for done in b.poll(wall, now, 8)?.completed {
            assert_eq!(done.stream, CONTROL);
            assert_eq!(done.state, ObjectSendState::Delivered);
            assert!(Arc::ptr_eq(&done.body, &reply));
            reply_done = true;
        }
        if let Some(item) = b.poll_complete(CONTROL, now, 1)? {
            assert_eq!(item.id, control.id());
            assert_eq!(item.chunks.concat(), vec![21; 1800]);
            assert!(!got_control);
            got_control = true;
            assert!(b.try_send(0, CONTROL, &reply, now)?);
        }
        if let Some(item) = a.poll_complete(CONTROL, now, 1)? {
            assert_eq!(item.id, reply.id());
            assert_eq!(item.chunks.concat(), vec![22; 200]);
            assert!(!got_reply);
            got_reply = true;
        }
        // Deliberately never take the healthy receiver's body assembly slot.
        assert_eq!(a.outbound_usage(BODY)?.0, 2);
        std::thread::yield_now();
    }
    let later = start + Duration::from_secs(3);
    let expired = a.poll(4100, later, 64)?.completed;
    assert_eq!(expired.len(), 2);
    for item in expired {
        assert_eq!(item.state, ObjectSendState::Expired);
        assert_eq!(item.stream, BODY);
        assert!(Arc::ptr_eq(&item.body, &body));
    }
    assert_eq!(a.outbound_usage(BODY)?, (0, 0));
    assert_eq!(a.outbound_usage(CONTROL)?, (0, 0));
    Ok(())
}

#[test]
fn scope_profile_and_quantum_reject_without_reserving_or_relabeling_bytes() -> Result<()> {
    let (mut a, _b, _c) = pair_and_third()?;
    let now = Instant::now();
    let body = message(BODY, 11, 100)?;
    assert!(a.try_send(0, CONTROL, &body, now).is_err());
    assert!(a.try_send(8, BODY, &body, now).is_err());
    assert!(a.try_send(0, 99, &body, now).is_err());
    let legacy = Arc::new(OutgoingMessage::new([BODY as u8; 32], vec![1; 100], 8192)?);
    assert!(a.try_send(0, BODY, &legacy, now).is_err());
    assert!(a.poll(1000, now, 0).is_err());
    assert!(a.poll(1000, now, 65).is_err());
    assert_eq!(a.outbound_usage(BODY)?, (0, 0));
    Ok(())
}
