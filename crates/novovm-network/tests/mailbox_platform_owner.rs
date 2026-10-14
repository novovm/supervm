#![cfg(feature = "iroh-transport")]
//! Real localhost QUIC/SQLite fixtures. Not phone, independent-replica or E2EE evidence.
use anyhow::{ensure, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::*,
    mailbox_rpc::*,
    mailbox_store::*,
    product_directory::sign_relay_record_v1,
    product_rendezvous_daemon::now_ms,
    product_rendezvous_owned::{
        run_self_owned_mailbox_on_endpoint_v1, run_self_owned_node_on_endpoint_v1,
    },
    product_service_lease::*,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
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
fn control() -> IrohControlV1 {
    IrohControlV1::new(Instant::now() + Duration::from_secs(20), || Ok(())).unwrap()
}

#[tokio::test]
async fn contributor_hosts_shared_mailbox_while_sending_and_retains_obligations_when_disabled(
) -> Result<()> {
    contributor_roundtrip(false).await
}

#[tokio::test]
async fn open_contributor_accepts_unregistered_writer_and_independent_reader() -> Result<()> {
    contributor_roundtrip(true).await
}

async fn contributor_roundtrip(open: bool) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("mailbox.db");
    let journal = dir.path().join("authority.bin");
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open_contributor(&db, Limits::default(), true)?,
        key(1),
    ));
    ensure!(!service.contribution_status().await?.resources_available);
    service.set_resources_available(true).await?;
    let owner = IrohEndpointOwnerV1::bind(
        config().with_owned_service_identity(zeroize::Zeroizing::new([90; 32]))?,
        || Ok(()),
    )
    .await?;
    let peer = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
    let before = owner.snapshot();
    let stop = Arc::new(AtomicBool::new(false));
    let (published, record) = tokio::sync::oneshot::channel();
    let published = Mutex::new(Some(published));
    let (peer_address, outbound_address) = tokio::sync::oneshot::channel();
    let (host_ended, wait_host_ended) = tokio::sync::oneshot::channel();
    let trust = SelfOwnedTrustV1 {
        version: 1,
        node_pins: [key(1).verifying_key().to_bytes()].into(),
    };
    let activate = move |endpoint: &IrohServiceEndpointV1, expires: u64| {
        let now = now_ms()?;
        let record = sign_relay_record_v1(
            &key(1),
            "platform-fixture",
            vec![endpoint.endpoint()?],
            now,
            expires,
            1,
        )?;
        let mut book = ServiceLeaseBookV1::new_self_owned(trust, vec![record], now)?;
        let checkpoint = book.checkpoint()?;
        let file =
            ServiceLeaseFileV1::create(&journal, zeroize::Zeroizing::new([91; 32]), &checkpoint)?;
        book.attach_persistence(file, &checkpoint)?;
        Ok(Arc::new(ServiceLeaseHostV1::new(key(1), book)?))
    };
    let publish = move |value: &novovm_network::product_directory::PeerSignedRelayRecordV1| {
        if let Some(tx) = published.lock().unwrap().take() {
            let _ = tx.send(value.clone());
        }
        Ok(())
    };
    let hosting: std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + '_>> = if open {
        Box::pin(run_self_owned_mailbox_on_endpoint_v1(
            &owner,
            key(1),
            stop.clone(),
            service.clone(),
            activate,
            publish,
        ))
    } else {
        Box::pin(run_self_owned_node_on_endpoint_v1(
            &owner,
            key(1),
            [key(2).verifying_key().to_bytes()].into(),
            stop.clone(),
            Some(service.clone()),
            activate,
            publish,
        ))
    };
    let host = async {
        let result = hosting.await;
        ensure!(
            stop.load(Ordering::Acquire),
            "host ended before explicit stop"
        );
        // Existing owner-stop semantics are preserved; it is not a successful RPC.
        ensure!(result.is_err(), "stopped host silently succeeded");
        let _ = host_ended.send(());
        Ok::<_, anyhow::Error>(())
    };
    let outbound = owner.with_operation(control(), |scope| {
        Box::pin(async move {
            let mut stream = scope.connect(&outbound_address.await?).await?;
            stream.write_frame(b"before-host-stop").await?;
            ensure!(stream.read_frame().await? == b"ack");
            wait_host_ended.await?;
            stream.write_frame(b"after-host-stop").await?;
            ensure!(stream.read_frame().await? == b"ack");
            stream.finish().await?;
            Ok(())
        })
    });
    let grant = MailboxGrant::generate()?;
    let original_mailbox = grant.id;
    let original_read = Capability::from_bytes(*grant.read.as_bytes())?;
    let expires = (now_ms()? / 1000) as i64 + 60;
    let identity = key(2);
    let pin = key(1).verifying_key().to_bytes();
    let client_service = service.clone();
    let client_stop = stop.clone();
    let client = peer.with_continuous_service_v4(
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                let _ = peer_address.send(scope.address()?);
                let route = IrohServiceEndpointV1::from_record(&record.await?)?;
                let mailbox = async {
                    let mut stream: CarrierStreamV1<'_> =
                        scope.connect(&route.address).await?.into();
                    let _group = request_mailbox_multiplex_v5(&mut stream).await?;
                    let mut session =
                        MailboxClientSessionV2::begin_continuous_v4(&mut stream, &identity, pin)
                            .await?;
                    ensure!(matches!(
                        session
                            .request(MailboxRequestV1::Register {
                                mailbox: grant.id,
                                read: *grant.read.as_bytes(),
                                write: *grant.write.as_bytes(),
                                expires,
                            })
                            .await?,
                        MailboxReplyV1::Registered
                    ));
                    let put = |object| MailboxRequestV1::Put {
                        mailbox: grant.id,
                        write: *grant.write.as_bytes(),
                        object,
                        ciphertext: vec![7; 32],
                        expires,
                    };
                    match session.request(put([7; 32])).await? {
                        MailboxReplyV1::Stored(receipt) => {
                            receipt.verify(pin, grant.id, [7; 32], &[7; 32], expires)?
                        }
                        _ => anyhow::bail!("no durable storage receipt"),
                    }
                    client_service.set_resources_available(false).await?;
                    client_service.set_contribution_enabled(false).await?;
                    ensure!(matches!(
                        session.request(put([8; 32])).await?,
                        MailboxReplyV1::Rejected(MailboxRejectionV1::Quota)
                    ));
                    // A lost StoreReceipt can be recovered without creating a new obligation.
                    ensure!(matches!(
                        session.request(put([7; 32])).await?,
                        MailboxReplyV1::Stored(_)
                    ));
                    ensure!(matches!(
                        session
                            .request(MailboxRequestV1::Fetch {
                                mailbox: grant.id,
                                read: [99; 32],
                            })
                            .await?,
                        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
                    ));
                    ensure!(matches!(session.request(MailboxRequestV1::Fetch {
                mailbox: grant.id, read: *grant.read.as_bytes(),
            }).await?, MailboxReplyV1::Object { object, ciphertext, expires: actual, .. }
                if object == [7; 32] && ciphertext == [7; 32] && actual == expires));
                    session.finish().await?;
                    if open {
                        // A third key was never registered with the host. Key
                        // possession alone does NOT confer read capability.
                        let mut other: CarrierStreamV1<'_> = scope.connect(&route.address).await?.into();
                        let _other_group = request_mailbox_multiplex_v5(&mut other).await?;
                        let mut reader = MailboxClientSessionV2::begin_continuous_v4(
                            &mut other, &key(3), pin,
                        ).await?;
                        ensure!(matches!(reader.request(MailboxRequestV1::Fetch {
                            mailbox: grant.id, read: [98;32],
                        }).await?, MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)));
                        ensure!(matches!(reader.request(MailboxRequestV1::Fetch {
                            mailbox: grant.id, read: *grant.read.as_bytes(),
                        }).await?, MailboxReplyV1::Object { object, ciphertext, expires: actual, .. }
                            if object == [7;32] && ciphertext == [7;32] && actual == expires));
                        reader.finish().await?;
                    }
                    client_stop.store(true, Ordering::Release);
                    Ok::<_, anyhow::Error>(())
                };
                let echo = async {
                    let mut stream = scope.accept_inbound().await?;
                    for expected in [
                        b"before-host-stop".as_slice(),
                        b"after-host-stop".as_slice(),
                    ] {
                        ensure!(stream.read_frame().await? == expected);
                        stream.write_frame(b"ack").await?;
                    }
                    stream.finish().await?;
                    Ok::<_, anyhow::Error>(())
                };
                tokio::try_join!(mailbox, echo)?;
                Ok(())
            })
        },
    );
    let result = tokio::time::timeout(Duration::from_secs(25), async {
        tokio::try_join!(host, outbound, client)
    })
    .await;
    let snapshot = owner.snapshot();
    // A subsequent listener can acquire the same endpoint after the old one ends.
    let reacquire = owner
        .with_owned_listener_v1(|| Ok(()), |_| Box::pin(async { Ok(()) }))
        .await;
    peer.close().await?;
    owner.close().await?;
    result??;
    reacquire?;
    ensure!(
        snapshot.instance == before.instance && snapshot.binds_completed == before.binds_completed
    );
    ensure!(snapshot.active_operations == 0 && !snapshot.closing);
    drop(service);
    let mut reopened = MailboxStore::open_contributor(&db, Limits::default(), true)?;
    let status = reopened.contribution_status((now_ms()? / 1000) as i64)?;
    ensure!(
        !status.enabled
            && !status.resources_available
            && status.retained_objects == 1
            && status.retain_until == Some(expires)
    );
    let retained = reopened.fetch(&original_mailbox, &original_read, (now_ms()? / 1000) as i64)?;
    ensure!(
        retained.len() == 1,
        "retained ciphertext missing after reopen"
    );
    let retained = &retained[0];
    ensure!(
        retained.id == [7; 32] && retained.ciphertext == [7; 32] && retained.expires == expires
    );
    Ok(())
}

#[tokio::test]
async fn platform_listener_requires_stable_identity_and_exclusive_accept_owner() -> Result<()> {
    let ephemeral = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
    let result: Result<()> = ephemeral
        .with_owned_listener_v1(
            || Ok(()),
            |_| Box::pin(async { anyhow::bail!("unowned callback entered") }),
        )
        .await;
    ephemeral.close().await?;
    ensure!(result
        .unwrap_err()
        .to_string()
        .contains("protected service identity"));
    let owner = Arc::new(
        IrohEndpointOwnerV1::bind(
            config().with_owned_service_identity(zeroize::Zeroizing::new([90; 32]))?,
            || Ok(()),
        )
        .await?,
    );
    let borrowed = owner.clone();
    let result = owner
        .with_owned_listener_v1(
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    ensure!(
                        scope.connect(&scope.address()?).await.is_err(),
                        "listener gained dial authority"
                    );
                    let nested = borrowed
                        .with_owned_listener_v1(|| Ok(()), |_| Box::pin(async { Ok(()) }))
                        .await;
                    ensure!(nested
                        .unwrap_err()
                        .to_string()
                        .contains("accept already pending"));
                    let competing = borrowed
                        .with_operation(control(), |other| {
                            Box::pin(async move { other.accept_inbound().await.map(|_| ()) })
                        })
                        .await;
                    ensure!(competing
                        .unwrap_err()
                        .to_string()
                        .contains("accept already pending"));
                    let mut first = Box::pin(scope.accept_inbound());
                    tokio::select! {
                        value = &mut first => { value?; anyhow::bail!("unexpected incoming"); },
                        _ = tokio::time::sleep(Duration::from_millis(20)) => {},
                    }
                    ensure!(scope
                        .accept_inbound()
                        .await
                        .err()
                        .expect("competing accept succeeded")
                        .to_string()
                        .contains("accept already pending"));
                    Ok(())
                })
            },
        )
        .await;
    owner.close().await?;
    result
}

#[tokio::test]
async fn listener_cancel_releases_reservation_but_owner_revocation_cannot_be_revived() -> Result<()>
{
    let allowed = Arc::new(AtomicBool::new(true));
    let check = allowed.clone();
    let owner = IrohEndpointOwnerV1::bind(
        config().with_owned_service_identity(zeroize::Zeroizing::new([90; 32]))?,
        move || {
            ensure!(check.load(Ordering::Acquire), "owner revoked");
            Ok(())
        },
    )
    .await?;
    let cancelled = tokio::time::timeout(
        Duration::from_millis(30),
        owner.with_owned_listener_v1(
            || Ok(()),
            |scope| Box::pin(async move { scope.accept_inbound().await.map(|_| ()) }),
        ),
    )
    .await;
    ensure!(cancelled.is_err() && owner.snapshot().active_operations == 0);
    owner
        .with_owned_listener_v1(|| Ok(()), |_| Box::pin(async { Ok(()) }))
        .await?;
    let (entered, wait_entered) = tokio::sync::oneshot::channel();
    let listener = owner.with_owned_listener_v1(
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                let _ = entered.send(());
                scope.accept_inbound().await.map(|_| ())
            })
        },
    );
    let revoke = async {
        wait_entered.await?;
        allowed.store(false, Ordering::Release);
        Ok::<_, anyhow::Error>(())
    };
    let (result, revoked) = tokio::join!(listener, revoke);
    revoked?;
    ensure!(result.is_err() && owner.snapshot().active_operations == 0);
    allowed.store(true, Ordering::Release);
    ensure!(owner
        .with_owned_listener_v1(|| Ok(()), |_| Box::pin(async { Ok(()) }))
        .await
        .is_err());
    owner.close().await?;
    ensure!(owner.snapshot().closed);
    Ok(())
}

#[tokio::test]
async fn endpoint_close_joins_its_active_listener_before_socket_barrier() -> Result<()> {
    let owner = IrohEndpointOwnerV1::bind(
        config().with_owned_service_identity(zeroize::Zeroizing::new([90; 32]))?,
        || Ok(()),
    )
    .await?;
    let (entered, wait_entered) = tokio::sync::oneshot::channel();
    let listener = owner.with_owned_listener_v1(
        || Ok(()),
        move |scope| {
            Box::pin(async move {
                let _ = entered.send(());
                scope.accept_inbound().await.map(|_| ())
            })
        },
    );
    let close = async {
        wait_entered.await?;
        owner.close().await
    };
    let (result, closed) = tokio::time::timeout(Duration::from_secs(6), async {
        tokio::join!(listener, close)
    })
    .await?;
    ensure!(result.is_err());
    closed?;
    ensure!(owner.snapshot().closed && owner.snapshot().active_operations == 0);
    Ok(())
}
