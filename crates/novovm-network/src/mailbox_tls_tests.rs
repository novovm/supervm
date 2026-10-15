//! Real TCP/TLS carrying the existing bound NVMBOX06 protocol, not a new server.
use super::*;
use crate::tls_transport::TlsScopeV1;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn tls_notification_wait_does_not_serialize_an_independent_put_lane() -> Result<()> {
    let _serial = crate::tls_transport::TLS_TEST_SERIAL.acquire().await?;
    let directory = tempfile::tempdir()?;
    let identity = SigningKey::from_bytes(&[63; 32]);
    let user = SigningKey::from_bytes(&[64; 32]);
    let service = MailboxServiceV1::new(
        MailboxStore::open(directory.path().join("lanes.db"), Default::default())?,
        identity.clone(),
    );
    let grant = MailboxGrant::generate()?;
    let a = TlsScopeV1::new_continuous_v4(|| Ok(()))?;
    let b = TlsScopeV1::new_continuous_v4(|| Ok(()))?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let mut clients = Vec::new();
    let mut servers = Vec::new();
    for _ in 0..2 {
        let (client, (server, _)) = tokio::try_join!(
            TcpStream::connect(listener.local_addr()?),
            listener.accept()
        )?;
        client.set_nodelay(true)?;
        server.set_nodelay(true)?;
        let (client, server) = tokio::try_join!(
            a.connect(client, b.endpoint_key()),
            b.accept(server, Some(a.endpoint_key()))
        )?;
        clients.push(CarrierStreamV1::from(client));
        servers.push(CarrierStreamV1::from(server));
    }
    let mut put_stream = clients.pop().unwrap();
    let mut watch_stream = clients.pop().unwrap();
    let mut put_server = servers.pop().unwrap();
    let mut watch_server = servers.pop().unwrap();
    let allowed = [user.verifying_key().to_bytes()];
    let host = async {
        tokio::try_join!(
            serve_mailbox_continuous_with_policy_v6(
                &mut put_server,
                &identity,
                ServicePeerPolicyV1::Pinned(&allowed),
                &service,
                || Ok(())
            ),
            serve_mailbox_continuous_with_policy_v6(
                &mut watch_server,
                &identity,
                ServicePeerPolicyV1::Pinned(&allowed),
                &service,
                || Ok(())
            )
        )?;
        Ok::<_, anyhow::Error>(())
    };
    let client = async {
        let mut put = MailboxClientSessionV2::begin_continuous_v6(
            &mut put_stream,
            &user,
            identity.verifying_key().to_bytes(),
        )
        .await?;
        let mut watch = MailboxClientSessionV2::begin_continuous_v6(
            &mut watch_stream,
            &user,
            identity.verifying_key().to_bytes(),
        )
        .await?;
        assert!(matches!(
            put.request(MailboxRequestV1::Register {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                read: *grant.read.as_bytes(),
                expires: now()? + 120,
            })
            .await?,
            MailboxReplyV1::Registered
        ));
        let query = |after| MailboxRequestV1::WatchFetchV6 {
            boxes: vec![MailboxWatchV1 {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
                after,
            }],
            wait_ms: 10_000,
        };
        let MailboxReplyV1::NoticeObjectV6 {
            notices,
            object: None,
        } = watch.request(query(None)).await?
        else {
            anyhow::bail!("initial empty snapshot missing")
        };
        let waiting = watch.request(query(Some(notices[0].cursor)));
        tokio::pin!(waiting);
        // Poll the real watch first; it must still be pending when Put starts.
        tokio::select! {
            result = &mut waiting => anyhow::bail!("watch ended before Put: {}", result.is_ok()),
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
        let writing = put.request(MailboxRequestV1::Put {
            mailbox: grant.id,
            write: *grant.write.as_bytes(),
            object: [8; 32],
            ciphertext: vec![19; 256],
            expires: now()? + 60,
        });
        let (notice, receipt) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::try_join!(&mut waiting, writing)
        })
        .await??;
        assert!(matches!(receipt, MailboxReplyV1::Stored(_)));
        assert!(matches!(
            notice,
            MailboxReplyV1::NoticeObjectV6 {
                object: Some(_),
                ..
            }
        ));
        // The watch's 10s wait did not hold the common store or block the writer.
        put.finish().await?;
        Ok::<_, anyhow::Error>(())
    };
    // Use cancellation for this two-lane fixture after the assertions; dropping
    // the clients closes actual IO. Graceful close is covered by the idle test.
    tokio::pin!(host);
    tokio::select! {
        result = client => result?,
        result = &mut host => { result?; anyhow::bail!("hosts ended before client"); }
    }
    Ok(())
}

#[tokio::test]
async fn tls_v6_idle_then_durable_put_notify_consume_on_same_session() -> Result<()> {
    let _serial = crate::tls_transport::TLS_TEST_SERIAL.acquire().await?;
    let directory = tempfile::tempdir()?;
    let identity = SigningKey::from_bytes(&[61; 32]);
    let client_identity = SigningKey::from_bytes(&[62; 32]);
    let service = MailboxServiceV1::new(
        MailboxStore::open(directory.path().join("tls.db"), Default::default())?,
        identity.clone(),
    );
    let grant = MailboxGrant::generate()?;
    let server_scope = TlsScopeV1::new_continuous_v4(|| Ok(()))?;
    let client_scope = TlsScopeV1::new_continuous_v4(|| Ok(()))?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let (client_tcp, (server_tcp, _)) = tokio::try_join!(
        TcpStream::connect(listener.local_addr()?),
        listener.accept()
    )?;
    client_tcp.set_nodelay(true)?;
    server_tcp.set_nodelay(true)?;
    let (client, server) = tokio::try_join!(
        client_scope.connect(client_tcp, server_scope.endpoint_key()),
        server_scope.accept(server_tcp, Some(client_scope.endpoint_key()))
    )?;
    let mut client = CarrierStreamV1::from(client);
    let mut server = CarrierStreamV1::from(server);
    let allowed = [client_identity.verifying_key().to_bytes()];
    let serving = serve_mailbox_continuous_with_policy_v6(
        &mut server,
        &identity,
        ServicePeerPolicyV1::Pinned(&allowed),
        &service,
        || Ok(()),
    );
    let using = async {
        let mut session = MailboxClientSessionV2::begin_continuous_v6(
            &mut client,
            &client_identity,
            identity.verifying_key().to_bytes(),
        )
        .await?;
        assert!(matches!(
            session
                .request(MailboxRequestV1::Register {
                    mailbox: grant.id,
                    write: *grant.write.as_bytes(),
                    read: *grant.read.as_bytes(),
                    expires: now()? + 300,
                })
                .await?,
            MailboxReplyV1::Registered
        ));
        // No request/heartbeat/handshake during idle, longer than request budget.
        tokio::time::sleep(Duration::from_secs(31)).await;
        assert_eq!(
            session.carrier_observation_v1()?,
            ("tls-unclassified", None)
        );
        let ciphertext = vec![17; CHUNK * 2];
        let expires = now()? + 60;
        let stored = session
            .request(MailboxRequestV1::Put {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                object: [7; 32],
                ciphertext: ciphertext.clone(),
                expires,
            })
            .await?;
        let MailboxReplyV1::Stored(receipt) = stored else {
            anyhow::bail!("no durable receipt")
        };
        receipt.verify(
            identity.verifying_key().to_bytes(),
            grant.id,
            [7; 32],
            &ciphertext,
            expires,
        )?;
        assert!(matches!(
            session
                .request(MailboxRequestV1::Fetch {
                    mailbox: grant.id,
                    read: [99; 32],
                })
                .await?,
            MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
        ));
        let notice = session
            .request(MailboxRequestV1::WatchFetchV6 {
                boxes: vec![MailboxWatchV1 {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                    after: None,
                }],
                wait_ms: 0,
            })
            .await?;
        let MailboxReplyV1::NoticeObjectV6 {
            notices,
            object: Some(object),
        } = notice
        else {
            anyhow::bail!("missing prefetched object")
        };
        assert!(notices[0].pending);
        assert_eq!(object.object, [7; 32]);
        assert_eq!(object.ciphertext, ciphertext);
        assert_eq!(object.expires, expires);
        assert!(matches!(
            session
                .request(MailboxRequestV1::Consume {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                    object: object.object,
                    digest: object.digest,
                })
                .await?,
            MailboxReplyV1::Consumed
        ));
        assert!(matches!(
            session
                .request(MailboxRequestV1::Fetch {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                })
                .await?,
            MailboxReplyV1::Empty
        ));
        session.finish().await
    };
    tokio::try_join!(serving, using)?;
    Ok(())
}
