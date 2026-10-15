use super::*;
use crate::{
    mailbox_rpc::MailboxWatchV1,
    mailbox_store::MailboxStore,
    novorudp::{
        endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
        fragments::ReassemblyLimits,
        object_link::{NativeLaneConfig, NativeSendLimits, NativeSendPolicy},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    },
};
use ed25519_dalek::SigningKey;
use std::{net::UdpSocket, time::Duration};

struct Pair {
    client: NativeObjectLink,
    server: NativeObjectLink,
    start: Instant,
}
impl Pair {
    async fn new() -> Result<Self> {
        let key = SigningKey::from_bytes(&[17; 32]);
        let pin = key.verifying_key().to_bytes();
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let address = socket.local_addr()?;
        let budget = || NativeHandshakeBudget {
            lifetime: Duration::from_secs(2),
            retry: Duration::from_millis(10),
        };
        let lane = || NativeLaneConfig {
            stream: super::super::STREAM,
            domain: object_domain(),
            receive: ReassemblyLimits {
                max_message_bytes: 16_384,
                messages: 8,
                bytes: 131_072,
                peer_messages: 8,
                peer_bytes: 131_072,
                ttl: Duration::from_secs(5),
            },
            send: NativeSendLimits {
                messages: 8,
                bytes: 131_072,
                peer_messages: 8,
                peer_bytes: 131_072,
            },
        };
        let policy = || NativeSendPolicy {
            window: NovoRudpWindowConfig::default(),
            pacing: NovoRudpPacingProfile {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 32,
                batch_pause_ms: 0,
                ack_wait_ms: 10,
                no_progress_backoff: false,
            },
            ttl: Duration::from_secs(5),
        };
        let start = Instant::now();
        let server = NativeObjectLink::new(
            NativeEndpoint::listen_clients(socket, key, 1, vec![super::super::STREAM], budget())?,
            vec![lane()],
            policy(),
        )?;
        let client = NativeObjectLink::new(
            NativeEndpoint::connect_client(
                UdpSocket::bind("127.0.0.1:0")?,
                SigningKey::from_bytes(&[83; 32]),
                NativePeer { key: pin, address },
                budget(),
                1000,
                start,
            )?,
            vec![lane()],
            policy(),
        )?;
        let mut pair = Self {
            client,
            server,
            start,
        };
        while pair.client.session_id(0).is_none() || pair.server.session_id(0).is_none() {
            ensure!(
                start.elapsed() < Duration::from_secs(5),
                "handshake timeout"
            );
            pair.poll()?;
            tokio::task::yield_now().await;
        }
        Ok(pair)
    }
    fn poll(&mut self) -> Result<()> {
        let now = Instant::now();
        let wall = 1000 + now.duration_since(self.start).as_millis() as u64;
        let _ = self.client.poll(wall, now, 64)?;
        let _ = self.server.poll(wall, now, 64)?;
        Ok(())
    }
    async fn request(
        &mut self,
        caller: &NativeMailboxClient,
        prepared: &PreparedRequest,
    ) -> Result<CompletedMessage> {
        ensure!(
            caller.try_send(&mut self.client, prepared, Instant::now(), |_| Ok(()))?,
            "send full"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            ensure!(Instant::now() < deadline, "request timeout");
            self.poll()?;
            if let Some(body) =
                self.server
                    .poll_complete(super::super::STREAM, Instant::now(), 64)?
            {
                return Ok(body);
            }
            tokio::task::yield_now().await;
        }
    }
}
fn duplicate(body: &CompletedMessage) -> CompletedMessage {
    CompletedMessage {
        peer: body.peer.clone(),
        id: body.id,
        chunks: body.chunks.clone(),
    }
}
fn fetch() -> MailboxRequestV1 {
    MailboxRequestV1::Fetch {
        mailbox: [1; 32],
        read: [3; 32],
    }
}
fn encoded_body(peer: &str, bytes: Vec<u8>) -> CompletedMessage {
    // Codec/state fault injection only; the integration test supplies real
    // reassembled objects. Never presented as authenticated network traffic.
    CompletedMessage {
        peer: peer.into(),
        id: [0; 32],
        chunks: vec![bytes],
    }
}

#[tokio::test]
async fn native_binding_reordering_replay_budget_and_session_separation() -> Result<()> {
    let mut pair = Pair::new().await?;
    let mut client = NativeMailboxClient::new(&pair.client, 0)?;
    let mut server = NativeMailboxServer::new(&pair.server, 0, NativeMailboxBudget::new(2)?)?;
    let now = Instant::now();
    let deadline = now + Duration::from_secs(5);
    let p1 = client.prepare(&pair.client, &[fetch()], now, deadline)?;
    let p2 = client.prepare(&pair.client, &[fetch()], now, deadline)?;
    let p3 = client.prepare(&pair.client, &[fetch()], now, deadline)?;
    // Actual UDP delivery in the opposite request order is legitimate.
    let b2 = pair.request(&client, &p2).await?;
    let copy2 = duplicate(&b2);
    let j2 = server.accept(&pair.server, b2, now, deadline)?;
    let b1 = pair.request(&client, &p1).await?;
    let copy1 = duplicate(&b1);
    let j1 = server.accept(&pair.server, b1, now, deadline)?;
    let b3 = pair.request(&client, &p3).await?;
    assert!(server
        .accept(&pair.server, duplicate(&b3), now, deadline)
        .is_err());
    drop(j2); // cancelled job releases capacity, NOT replay history
    assert!(server.accept(&pair.server, copy2, now, deadline).is_err());
    let j3 = server.accept(&pair.server, b3, now, deadline)?;
    drop((j1, j3));
    assert!(server.accept(&pair.server, copy1, now, deadline).is_err());

    // Wrong direction, unknown session and malformed high sequence cannot
    // poison the original replay window or grant cross-peer authority.
    let reverse = SessionBinding::capture(&pair.server, 0, true)?;
    assert_ne!(reverse.context(4), client.binding.context(4));
    let wrong = super::super::encode_requests(reverse.context(4), &[fetch()])?;
    assert!(server
        .accept(
            &pair.server,
            encoded_body(pair.server.peer_id(0)?, wrong.to_vec()),
            now,
            deadline
        )
        .is_err());
    let fresh = Pair::new().await?;
    assert!(client.binding.check(&fresh.client).is_err());
    let mut bad =
        super::super::encode_requests(client.binding.context(u64::MAX), &[fetch()])?.to_vec();
    bad.pop();
    assert!(server
        .accept(
            &pair.server,
            encoded_body(pair.server.peer_id(0)?, bad),
            now,
            deadline
        )
        .is_err());
    let p4 = client.prepare(&pair.client, &[fetch()], now, deadline)?;
    let b4 = pair.request(&client, &p4).await?;
    drop(server.accept(&pair.server, b4, now, deadline)?);
    Ok(())
}

#[tokio::test]
async fn native_reply_correlation_deadline_and_window_do_not_reset() -> Result<()> {
    let pair = Pair::new().await?;
    let mut client = NativeMailboxClient::new(&pair.client, 0)?;
    let now = Instant::now();
    let deadline = now + Duration::from_secs(5);
    let p = client.prepare(&pair.client, &[fetch()], now, deadline)?;
    let reply = |sequence| -> Result<_> {
        Ok(super::super::encode_replies(
            client.binding.context(sequence),
            &[MailboxReplyV1::Empty],
        )?
        .to_vec())
    };
    let good = reply(p.sequence)?;
    let mut bad = good.clone();
    bad[32] ^= 1;
    for bytes in [bad, reply(p.sequence + 1)?] {
        assert!(client
            .receive(
                &pair.client,
                encoded_body(pair.client.peer_id(0)?, bytes),
                now
            )
            .is_err());
        assert_eq!(client.pending_count(), 1);
    }
    assert!(client
        .receive(&pair.client, encoded_body("wrong peer", good.clone()), now)
        .is_err());
    assert!(client
        .receive(
            &pair.client,
            encoded_body(pair.client.peer_id(0)?, good.clone()),
            deadline
        )
        .is_err());
    assert_eq!(
        client
            .receive(
                &pair.client,
                encoded_body(pair.client.peer_id(0)?, good.clone()),
                now
            )?
            .0,
        p.sequence
    );
    assert!(client
        .receive(
            &pair.client,
            encoded_body(pair.client.peer_id(0)?, good),
            now
        )
        .is_err());
    let oldest = client.prepare(&pair.client, &[fetch()], now, deadline)?;
    for _ in 1..WINDOW {
        let request = client.prepare(&pair.client, &[fetch()], now, deadline)?;
        assert!(client.abandon(request.sequence));
    }
    assert!(client
        .prepare(&pair.client, &[fetch()], now, deadline)
        .is_err());
    assert!(client.abandon(oldest.sequence));
    let next = client.prepare(&pair.client, &[fetch()], now, deadline)?;
    assert!(next.sequence > oldest.sequence);
    for _ in 1..MAX_PENDING {
        let _ = client.prepare(&pair.client, &[fetch()], now, deadline)?;
    }
    assert!(client
        .prepare(&pair.client, &[fetch()], now, deadline)
        .is_err());
    Ok(())
}

#[tokio::test]
async fn native_watch_and_put_jobs_progress_together_and_preserve_custody() -> Result<()> {
    let mut pair = Pair::new().await?;
    let mut client = NativeMailboxClient::new(&pair.client, 0)?;
    let budget = NativeMailboxBudget::new(2)?;
    let mut server = NativeMailboxServer::new(&pair.server, 0, budget.clone())?;
    let directory = tempfile::tempdir()?;
    let key = SigningKey::from_bytes(&[17; 32]);
    let pin = key.verifying_key().to_bytes();
    let service = MailboxServiceV1::new(
        MailboxStore::open_contributor(
            directory.path().join("mailbox.db"),
            Default::default(),
            true,
        )?,
        key,
    );
    service.set_resources_available(true).await?;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64
        + 600;
    let registered = service
        .request_local_v1(MailboxRequestV1::Register {
            mailbox: [1; 32],
            write: [2; 32],
            read: [3; 32],
            expires,
        })
        .await?;
    assert!(matches!(registered, MailboxReplyV1::Registered));
    let initial = service
        .request_local_v1(MailboxRequestV1::Watch {
            boxes: vec![MailboxWatchV1 {
                mailbox: [1; 32],
                read: [3; 32],
                after: None,
            }],
            wait_ms: 0,
        })
        .await?;
    let MailboxReplyV1::Notice(notices) = initial else {
        anyhow::bail!("no initial cursor")
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let watch = client.prepare(
        &pair.client,
        &[MailboxRequestV1::Watch {
            boxes: vec![MailboxWatchV1 {
                mailbox: [1; 32],
                read: [3; 32],
                after: Some(notices[0].cursor),
            }],
            wait_ms: 2000,
        }],
        Instant::now(),
        deadline,
    )?;
    let put = client.prepare(
        &pair.client,
        &[MailboxRequestV1::Put {
            mailbox: [1; 32],
            write: [2; 32],
            object: [4; 32],
            ciphertext: vec![7; 9000],
            expires,
        }],
        Instant::now(),
        deadline,
    )?;
    let wb = pair.request(&client, &watch).await?;
    let wj = server.accept(&pair.server, wb, Instant::now(), deadline)?;
    let pb = pair.request(&client, &put).await?;
    let pj = server.accept(&pair.server, pb, Instant::now(), deadline)?;
    let (wr, pr) = {
        let wf = wj.execute(&service, |binding| binding.check(&pair.server));
        tokio::pin!(wf);
        tokio::select! {
            _ = &mut wf => anyhow::bail!("Watch did not wait on empty cursor"),
            _ = tokio::time::sleep(Duration::from_millis(30)) => {}
        }
        // Same original mailbox owner, parallel jobs, no new task scheduler.
        let (w, p) = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(
                wf,
                pj.execute(&service, |binding| binding.check(&pair.server))
            )
        })
        .await
        .context("Put blocked behind Watch")?;
        (w?, p?)
    };
    assert_eq!(budget.0.available_permits(), 0); // includes replies under backpressure
    assert!(pr.try_send(&mut pair.server, Instant::now(), |_| Ok(()))?);
    assert!(wr.try_send(&mut pair.server, Instant::now(), |_| Ok(()))?);
    let mut received = 0;
    while received < 2 {
        ensure!(Instant::now() < deadline, "native reply timeout");
        pair.poll()?;
        if let Some(body) = pair
            .client
            .poll_complete(super::super::STREAM, Instant::now(), 64)?
        {
            let (sequence, replies) = client.receive(&pair.client, body, Instant::now())?;
            if sequence == put.sequence {
                let MailboxReplyV1::Stored(receipt) = &replies[0] else {
                    anyhow::bail!("missing StoreReceipt")
                };
                receipt.verify(pin, [1; 32], [4; 32], &vec![7; 9000], expires)?;
            } else {
                assert_eq!(sequence, watch.sequence);
                assert!(matches!(&replies[0], MailboxReplyV1::Notice(n) if n[0].pending));
            }
            received += 1;
        }
        tokio::task::yield_now().await;
    }
    drop((wr, pr));
    assert_eq!(budget.0.available_permits(), 2);
    // Revocation is checked before owner effects; a live transport is not authority.
    let prepared = client.prepare(&pair.client, &[fetch()], Instant::now(), deadline)?;
    let bytes = pair.request(&client, &prepared).await?;
    let job = server.accept(&pair.server, bytes, Instant::now(), deadline)?;
    assert!(job
        .execute(&service, |_| anyhow::bail!("revoked fixture"))
        .await
        .is_err());
    assert_eq!(budget.0.available_permits(), 2);
    Ok(())
}
