//! External consumer test: only public network APIs, no node/validator/AOEM,
//! HTTP, Iroh or Relay. Opaque binary payloads are NOT APFL mailbox acceptance.
use anyhow::{ensure, Result};
use ed25519_dalek::SigningKey;
use novovm_network::novorudp::{
    endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
    fragments::{FragmentProfile, OutgoingMessage, ReassemblyLimits},
    object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
    object_sender::ObjectSendState,
    NovoRudpPacingProfile, NovoRudpWindowConfig,
};
use std::{
    net::UdpSocket,
    sync::Arc,
    time::{Duration, Instant},
};

const STREAM: u64 = 17;
const DOMAIN: [u8; 32] = [71; 32];
fn budget() -> NativeHandshakeBudget {
    NativeHandshakeBudget {
        lifetime: Duration::from_millis(500),
        retry: Duration::from_millis(10),
    }
}
fn lane() -> NativeLaneConfig {
    NativeLaneConfig {
        stream: STREAM,
        domain: DOMAIN,
        receive: ReassemblyLimits {
            max_message_bytes: 16_384,
            messages: 2,
            bytes: 32_768,
            peer_messages: 1,
            peer_bytes: 16_384,
            ttl: Duration::from_secs(5),
        },
        send: NativeSendLimits {
            messages: 2,
            bytes: 32_768,
            peer_messages: 1,
            peer_bytes: 16_384,
        },
    }
}
fn policy() -> NativeSendPolicy {
    NativeSendPolicy {
        window: NovoRudpWindowConfig {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 32,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
            ..Default::default()
        },
        pacing: NovoRudpPacingProfile {
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 32,
            batch_pause_ms: 0,
            ack_wait_ms: 10,
            no_progress_backoff: false,
        },
        ttl: Duration::from_secs(5),
    }
}
fn object(bytes: Vec<u8>) -> Result<Arc<OutgoingMessage>> {
    Ok(Arc::new(OutgoingMessage::with_profile(
        DOMAIN,
        bytes,
        16_384,
        FragmentProfile::NativeUdpV2,
    )?))
}

#[test]
fn unenrolled_application_uses_one_native_session_for_bidirectional_objects() -> Result<()> {
    let listener = UdpSocket::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server_key = SigningKey::from_bytes(&[17; 32]);
    let endpoint =
        NativeEndpoint::listen_clients(listener, server_key.clone(), 1, vec![STREAM], budget())?;
    assert_eq!(endpoint.peer_count(), 0);
    let mut server = NativeObjectLink::new(endpoint, vec![lane()], policy())?;
    let origin = Instant::now();
    let endpoint = NativeEndpoint::connect_client(
        UdpSocket::bind("127.0.0.1:0")?,
        SigningKey::from_bytes(&[83; 32]),
        NativePeer {
            key: server_key.verifying_key().to_bytes(),
            address,
        },
        budget(),
        1000,
        origin,
    )?;
    let mut client = NativeObjectLink::new(endpoint, vec![lane()], policy())?;
    let mut original_session = None;
    for round in 0..3u8 {
        let input = (0..12_000)
            .map(|i| (i as u8).wrapping_add(round))
            .collect::<Vec<_>>();
        let output = input.iter().rev().copied().collect::<Vec<_>>();
        let request = object(input.clone())?;
        let response = object(output.clone())?;
        let (mut sent, mut received, mut replied, mut request_done, mut response_done) =
            (false, false, false, false, false);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !(received && replied && request_done && response_done) {
            ensure!(
                Instant::now() < deadline,
                "public native client roundtrip timeout, round={round}"
            );
            // Later rounds are beyond the original handshake expiry; no renewal.
            let now = Instant::now() + Duration::from_secs(2 * u64::from(round));
            let wall = 1000 + now.duration_since(origin).as_millis() as u64;
            for done in client.poll(wall, now, 64)?.completed {
                assert_eq!(done.state, ObjectSendState::Delivered);
                assert!(Arc::ptr_eq(&done.body, &request));
                request_done = true;
            }
            for done in server.poll(wall, now, 64)?.completed {
                assert_eq!(done.state, ObjectSendState::Delivered);
                assert!(Arc::ptr_eq(&done.body, &response));
                response_done = true;
            }
            if !sent {
                sent = client.try_send(0, STREAM, &request, now)?;
            }
            if let Some(body) = server.poll_complete(STREAM, now, 64)? {
                assert!(!received);
                assert_eq!(body.id, request.id());
                assert_eq!(body.chunks.concat(), input);
                assert_eq!(server.peer_count(), 1);
                assert!(server.try_send(0, STREAM, &response, now)?);
                received = true;
            }
            if let Some(body) = client.poll_complete(STREAM, now, 64)? {
                assert!(!replied);
                assert_eq!(body.id, response.id());
                assert_eq!(body.chunks.concat(), output);
                replied = true;
            }
            std::thread::yield_now();
        }
        let session = client.session_id(0).unwrap();
        assert_eq!(server.session_id(0), Some(session));
        if let Some(old) = original_session {
            assert_eq!(session, old);
        }
        original_session = Some(session);
        assert_eq!(client.outbound_usage(STREAM)?, (0, 0));
        assert_eq!(server.outbound_usage(STREAM)?, (0, 0));
    }
    Ok(())
}

#[test]
fn empty_pinned_policy_is_not_open_and_listener_requires_explicit_limits() -> Result<()> {
    let key = SigningKey::from_bytes(&[17; 32]);
    assert!(NativeEndpoint::bind(
        UdpSocket::bind("127.0.0.1:0")?,
        key.clone(),
        vec![],
        budget(),
        1000,
        Instant::now()
    )
    .is_err());
    for (clients, streams) in [
        (0, vec![STREAM]),
        (65, vec![STREAM]),
        (1, vec![]),
        (1, vec![0]),
    ] {
        assert!(NativeEndpoint::listen_clients(
            UdpSocket::bind("127.0.0.1:0")?,
            key.clone(),
            clients,
            streams,
            budget()
        )
        .is_err());
    }
    Ok(())
}
