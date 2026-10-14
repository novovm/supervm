//! Real QUIC idle/first-request lifecycle. No clock advancement or keepalive RPC.
use super::*;
use crate::iroh_transport::{with_iroh_owned_listener_v1, IrohConfigV1, IrohEndpointOwnerV1};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn config() -> IrohConfigV1 {
    IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
        .unwrap()
        .with_bind_address("127.0.0.1:0".parse().unwrap())
        .unwrap()
}
#[tokio::test]
async fn v6_idle_beyond_request_deadline_reuses_authenticated_session() -> Result<()> {
    exercise(false).await
}
#[tokio::test]
async fn v6_idle_still_observes_owner_revocation() -> Result<()> {
    exercise(true).await
}
async fn exercise(revoke: bool) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let service = MailboxServiceV1::new(
        MailboxStore::open(dir.path().join("idle.db"), Default::default())?,
        key(1),
    );
    let grant = MailboxGrant::generate()?;
    assert!(matches!(
        service
            .request_local_v1(MailboxRequestV1::Register {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                read: *grant.read.as_bytes(),
                expires: now()? + 300,
            })
            .await?,
        MailboxReplyV1::Registered
    ));
    let active = Arc::new(AtomicBool::new(true));
    let host = active.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let server = with_iroh_owned_listener_v1(
        config().with_owned_service_identity(Zeroizing::new([83; 32]))?,
        move || {
            ensure!(host.load(Ordering::Acquire), "fixture owner revoked");
            Ok(())
        },
        move |scope| {
            Box::pin(async move {
                tx.send(scope.address()?)
                    .map_err(|_| anyhow::anyhow!("fixture address"))?;
                let mut stream = scope.accept_inbound().await?.into();
                serve_session(
                    &mut stream,
                    &key(1),
                    ServicePeerPolicyV1::Pinned(&[key(2).verifying_key().to_bytes()]),
                    &service,
                    || Ok(()),
                    SessionMode::Prefetch,
                )
                .await
            })
        },
    );
    let owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
    let client = owner.with_continuous_service_v4(
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                let mut stream = scope.connect(&rx.await?).await?.into();
                let mut session = MailboxClientSessionV2::begin_continuous_v6(
                    &mut stream,
                    &key(2),
                    key(1).verifying_key().to_bytes(),
                )
                .await?;
                let (path, rtt) = session.carrier_observation_v1()?;
                assert_eq!(path, "direct");
                assert!(rtt.is_some(), "real QUIC selected path has no RTT");
                if revoke {
                    active.store(false, Ordering::Release);
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    assert!(session
                        .request(MailboxRequestV1::Fetch {
                            mailbox: grant.id,
                            read: *grant.read.as_bytes(),
                        })
                        .await
                        .is_err());
                    assert!(session.carrier_observation_v1().is_err());
                    assert!(
                        session
                            .request(MailboxRequestV1::Fetch {
                                mailbox: grant.id,
                                read: *grant.read.as_bytes(),
                            })
                            .await
                            .is_err(),
                        "revoked stream revived"
                    );
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_secs(31)).await;
                assert_eq!(session.carrier_observation_v1()?.0, "direct");
                let data = vec![5; CHUNK * 2];
                let expires = now()? + 60;
                let reply = session
                    .request(MailboxRequestV1::Put {
                        mailbox: grant.id,
                        write: *grant.write.as_bytes(),
                        object: [7; 32],
                        ciphertext: data.clone(),
                        expires,
                    })
                    .await?;
                let MailboxReplyV1::Stored(receipt) = reply else {
                    anyhow::bail!("missing durable receipt")
                };
                receipt.verify(
                    key(1).verifying_key().to_bytes(),
                    grant.id,
                    [7; 32],
                    &data,
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
                // Same authenticated stream, preserved counters and exact original object.
                let reply = session
                    .request(MailboxRequestV1::Fetch {
                        mailbox: grant.id,
                        read: *grant.read.as_bytes(),
                    })
                    .await?;
                let MailboxReplyV1::Object {
                    object,
                    ciphertext,
                    expires: stored_expiry,
                    ..
                } = reply
                else {
                    anyhow::bail!("original object missing after idle")
                };
                assert_eq!(object, [7; 32]);
                assert_eq!(ciphertext, data);
                assert_eq!(stored_expiry, expires);
                session.finish().await
            })
        },
    );
    let (server, client) = tokio::join!(server, client);
    client?;
    if revoke {
        assert!(server.is_err());
    } else {
        server?;
    }
    owner.close().await?;
    Ok(())
}
