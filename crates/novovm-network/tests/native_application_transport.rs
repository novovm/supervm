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

// This exercises the shared codec against the SAME durable mailbox owner over
// real UDP. It is not a production driver: mobile routing/reconnect and the
// application's live contact/revocation authority are not implemented here.
#[cfg(feature = "iroh-transport")]
#[tokio::test]
async fn structured_mailbox_over_authenticated_udp_returns_verified_custody() -> Result<()> {
    use novovm_network::{
        mailbox_rpc::{MailboxReplyV1 as Reply, MailboxRequestV1 as Request, MailboxServiceV1},
        mailbox_store::MailboxStore,
        mailbox_structured::{
            decode_replies, encode_requests, execute_local_batch, STREAM as MAIL,
        },
    };
    use zeroize::Zeroizing;
    let directory = tempfile::tempdir()?;
    let key = SigningKey::from_bytes(&[17; 32]);
    let pin = key.verifying_key().to_bytes();
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(
            directory.path().join("mailbox.db"),
            Default::default(),
            true,
        )?,
        key.clone(),
    );
    service.set_resources_available(true).await?;
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let address = socket.local_addr()?;
    let mut config = lane();
    config.stream = MAIL;
    let mut server = NativeObjectLink::new(
        NativeEndpoint::listen_clients(socket, key, 1, vec![MAIL], budget())?,
        vec![config.clone()],
        policy(),
    )?;
    let origin = Instant::now();
    let mut client = NativeObjectLink::new(
        NativeEndpoint::connect_client(
            UdpSocket::bind("127.0.0.1:0")?,
            SigningKey::from_bytes(&[83; 32]),
            NativePeer { key: pin, address },
            budget(),
            1000,
            origin,
        )?,
        vec![config],
        policy(),
    )?;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64
        + 600;
    let mut session = None;
    for round in 0..2u8 {
        // A context is not authentication by itself. The fixture associates it
        // with this exact request and checks peer/session before owner effects.
        let context = [31 + round; 32];
        let requests = if round == 0 {
            vec![
                Request::Register {
                    mailbox: [1; 32],
                    write: [2; 32],
                    read: [3; 32],
                    expires,
                },
                Request::Put {
                    mailbox: [1; 32],
                    write: [2; 32],
                    object: [4; 32],
                    ciphertext: vec![7; 9000],
                    expires,
                },
            ]
        } else {
            vec![Request::Fetch {
                mailbox: [1; 32],
                read: [3; 32],
            }]
        };
        let encoded = encode_requests(context, &requests)?;
        let request = object(encoded.to_vec())?;
        let mut sent = false;
        let mut handled = false;
        let mut response = None;
        let mut received = None;
        let mut request_done = false;
        let mut response_done = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !(request_done && response_done && received.is_some()) {
            ensure!(Instant::now() < deadline, "native mailbox fixture timeout");
            let now = Instant::now();
            let wall = 1000 + now.duration_since(origin).as_millis() as u64;
            for done in client.poll(wall, now, 64)?.completed {
                assert_eq!(done.state, ObjectSendState::Delivered);
                assert!(Arc::ptr_eq(&done.body, &request));
                request_done = true;
            }
            for done in server.poll(wall, now, 64)?.completed {
                assert_eq!(done.state, ObjectSendState::Delivered);
                assert!(Arc::ptr_eq(&done.body, response.as_ref().unwrap()));
                response_done = true;
            }
            if !sent {
                sent = client.try_send(0, MAIL, &request, now)?;
            }
            if let Some(body) = server.poll_complete(MAIL, now, 64)? {
                ensure!(
                    !handled && body.id == request.id(),
                    "unexpected mailbox request"
                );
                ensure!(body.peer == server.peer_id(0)?, "mailbox request peer");
                let bound = server.session_id(0).unwrap();
                assert_eq!(Some(bound), client.session_id(0));
                if let Some(previous) = session {
                    assert_eq!(bound, previous);
                }
                session = Some(bound);
                let bytes = Zeroizing::new(body.chunks.concat());
                let encoded = execute_local_batch(&service, &bytes, context, || {
                    ensure!(
                        server.session_id(0) == Some(bound),
                        "fixture authority changed"
                    );
                    Ok(())
                })
                .await?;
                let reply = object(encoded.to_vec())?;
                assert!(server.try_send(0, MAIL, &reply, Instant::now())?);
                response = Some(reply);
                handled = true;
            }
            if let Some(body) = client.poll_complete(MAIL, now, 64)? {
                ensure!(received.is_none(), "duplicate mailbox response");
                assert_eq!(body.id, response.as_ref().unwrap().id());
                let bytes = Zeroizing::new(body.chunks.concat());
                assert!(decode_replies(&bytes, [99; 32], requests.len()).is_err());
                received = Some(decode_replies(&bytes, context, requests.len())?);
            }
            tokio::task::yield_now().await;
        }
        let replies = received.unwrap();
        if round == 0 {
            assert!(matches!(replies[0], Reply::Registered));
            let Reply::Stored(receipt) = &replies[1] else {
                anyhow::bail!("missing custody evidence")
            };
            receipt.verify(pin, [1; 32], [4; 32], &vec![7; 9000], expires)?;
            assert!(receipt
                .verify(pin, [1; 32], [4; 32], &vec![8; 9000], expires)
                .is_err());
        } else {
            assert!(
                matches!(&replies[0],Reply::Object{object,ciphertext,expires:e,..}
                if *object==[4;32]&&ciphertext==&vec![7;9000]&&*e==expires)
            );
        }
        assert_eq!(client.outbound_usage(MAIL)?, (0, 0));
        assert_eq!(server.outbound_usage(MAIL)?, (0, 0));
    }
    Ok(())
}
