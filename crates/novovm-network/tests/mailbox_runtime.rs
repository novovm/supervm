#![cfg(feature = "iroh-transport")]
use anyhow::{ensure, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    carrier_stream::CarrierStreamV1, iroh_transport::*, mailbox_rpc::*, mailbox_store::*,
};
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
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
    IrohControlV1::new(Instant::now() + Duration::from_secs(10), || Ok(())).unwrap()
}
async fn rpc(
    service: Arc<MailboxServiceV1>,
    client: u8,
    request: MailboxRequestV1,
) -> Result<MailboxReplyV1> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let server = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            tx.send(scope.address()?)
                .map_err(|_| anyhow::anyhow!("test address channel"))?;
            let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
            serve_mailbox_v1(
                &mut stream,
                &key(1),
                &[
                    key(2).verifying_key().to_bytes(),
                    key(3).verifying_key().to_bytes(),
                ],
                &service,
            )
            .await
        })
    });
    let client = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            let mut stream: CarrierStreamV1<'_> = scope.connect(&rx.await?).await?.into();
            request_mailbox_v1(
                &mut stream,
                &key(client),
                key(1).verifying_key().to_bytes(),
                request,
            )
            .await
        })
    });
    let (server, client) = tokio::join!(server, client);
    server?;
    client
}
fn register(grant: &MailboxGrant, expires: i64) -> MailboxRequestV1 {
    MailboxRequestV1::Register {
        mailbox: grant.id,
        write: *grant.write.as_bytes(),
        read: *grant.read.as_bytes(),
        expires,
    }
}

#[tokio::test]
async fn existing_self_owned_listener_dispatches_mailbox_with_verified_offer() -> Result<()> {
    use novovm_network::{
        product_directory::sign_relay_record_v1, product_rendezvous_daemon::now_ms,
        product_rendezvous_owned::run_self_owned_node_v1, product_service_lease::*,
    };
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    };
    let directory = tempfile::tempdir()?;
    let journal = directory.path().join("service.journal");
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open(directory.path().join("mailbox.db"), Limits::default())?,
        key(1),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    struct Stop(Arc<AtomicBool>);
    impl Drop for Stop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let guard = Stop(stop.clone());
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let trust = SelfOwnedTrustV1 {
        version: 1,
        node_pins: [key(1).verifying_key().to_bytes()].into(),
    };
    let server_trust = trust.clone();
    let server = run_self_owned_node_v1(
        config().with_owned_service_identity(zeroize::Zeroizing::new([90; 32]))?,
        key(1),
        [key(2).verifying_key().to_bytes()].into(),
        stop,
        Some(service),
        move |endpoint, expires| {
            let now = now_ms()?;
            let record = sign_relay_record_v1(
                &key(1),
                "mailbox-runtime-fixture",
                vec![endpoint.endpoint()?],
                now,
                expires,
                1,
            )?;
            let mut book = ServiceLeaseBookV1::new_self_owned(server_trust, vec![record], now)?;
            let checkpoint = book.checkpoint()?;
            let file = ServiceLeaseFileV1::create(
                &journal,
                zeroize::Zeroizing::new([91; 32]),
                &checkpoint,
            )?;
            book.attach_persistence(file, &checkpoint)?;
            Ok(Arc::new(ServiceLeaseHostV1::new(key(1), book)?))
        },
        move |record| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(record.clone());
            }
            Ok(())
        },
    );
    let client = async move {
        let _stop = guard;
        let record = rx.await?;
        let route = IrohServiceEndpointV1::from_record(&record)?;
        let mut book = ServiceLeaseBookV1::new_self_owned(trust, vec![record], now_ms()?)?;
        let endpoint = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
        let grant = MailboxGrant::generate()?;
        let expiry = (now_ms()? / 1000) as i64 + 300;
        let data = vec![77; 32];
        let requests = vec![
            register(&grant, expiry + 60),
            MailboxRequestV1::Fetch {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
            },
            MailboxRequestV1::Put {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                object: [42; 32],
                ciphertext: data.clone(),
                expires: expiry,
            },
            MailboxRequestV1::Watch {
                boxes: vec![MailboxWatchV1 {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                    after: None,
                }],
                wait_ms: 0,
            },
            MailboxRequestV1::Fetch {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
            },
            MailboxRequestV1::Consume {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
                object: [42; 32],
                digest: Sha256::digest(&data).into(),
            },
            MailboxRequestV1::Fetch {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
            },
        ];
        for (index, request) in requests.into_iter().enumerate() {
            let route = route.clone();
            let (reply, returned_book) = endpoint
                .with_reusable_operation(control(), 5, move |scope| {
                    Box::pin(async move {
                        let raw = scope.connect(&route.address).await?;
                        ensure!(
                            raw.connection_reused() == (index > 0),
                            "connection was rebuilt"
                        );
                        let mut stream: CarrierStreamV1<'_> = raw.into();
                        let offer = request_mailbox_offer_v2(&mut stream).await?;
                        book.accept_offer(
                            offer,
                            key(1).verifying_key().to_bytes(),
                            route.address.endpoint_key,
                            route.instance,
                            now_ms()?,
                        )?;
                        let reply = request_mailbox_v1(
                            &mut stream,
                            &key(2),
                            key(1).verifying_key().to_bytes(),
                            request,
                        )
                        .await?;
                        Ok((reply, book))
                    })
                })
                .await?;
            book = returned_book;
            match (index, reply) {
                (0, MailboxReplyV1::Registered)
                | (1 | 6, MailboxReplyV1::Empty)
                | (5, MailboxReplyV1::Consumed) => {}
                (2, MailboxReplyV1::Stored(r)) => r.verify(
                    key(1).verifying_key().to_bytes(),
                    grant.id,
                    [42; 32],
                    &data,
                    expiry,
                )?,
                (3, MailboxReplyV1::Notice(n)) => ensure!(
                    n.len() == 1 && n[0].pending && n[0].cursor.sequence == 1,
                    "cursor mismatch"
                ),
                (4, MailboxReplyV1::Object { ciphertext, .. }) => {
                    ensure!(ciphertext == data, "payload changed")
                }
                _ => anyhow::bail!("unexpected mailbox response"),
            }
        }
        // Explicit multi-request mode on the SAME transport after legacy streams.
        endpoint
            .with_reusable_operation(control(), 5, move |scope| {
                Box::pin(async move {
                    let raw = scope.connect(&route.address).await?;
                    ensure!(raw.connection_reused(), "session rebuilt connection");
                    let mut stream: CarrierStreamV1<'_> = raw.into();
                    let offer = request_mailbox_offer_v2(&mut stream).await?;
                    book.accept_offer(
                        offer,
                        key(1).verifying_key().to_bytes(),
                        route.address.endpoint_key,
                        route.instance,
                        now_ms()?,
                    )?;
                    let mut session = MailboxClientSessionV2::begin(
                        &mut stream,
                        &key(2),
                        key(1).verifying_key().to_bytes(),
                    )
                    .await?;
                    for _ in 0..3 {
                        ensure!(
                            matches!(
                                session
                                    .request(MailboxRequestV1::Fetch {
                                        mailbox: grant.id,
                                        read: *grant.read.as_bytes(),
                                    })
                                    .await?,
                                MailboxReplyV1::Empty
                            ),
                            "session response differs"
                        );
                    }
                    session.finish().await
                })
            })
            .await?;
        endpoint.close().await?;
        Ok::<_, anyhow::Error>(())
    };
    let (a, b) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(server, client)
    })
    .await?;
    b?;
    // The explicit owner-stop gate can win against the maintenance tick; it is
    // expected only after both authenticated operations above completed.
    if let Err(error) = a {
        ensure!(
            error
                .chain()
                .any(|e| e.to_string() == "service stopped by owner"),
            "unexpected node exit: {error:#}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn l3_l4_same_rpc_durable_offline_fetch_restart_duplicate_revoke_and_full() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("mailbox.db");
    let limits = Limits {
        object_bytes: 65536,
        mailbox_bytes: 65536,
        total_bytes: 65536,
        ..Limits::default()
    };
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open(&path, limits)?,
        key(1),
    ));
    let now = (novovm_network::product_rendezvous_daemon::now_ms()? / 1000) as i64;
    let grant = MailboxGrant::generate()?;
    assert!(matches!(
        rpc(service.clone(), 3, register(&grant, now + 600)).await?,
        MailboxReplyV1::Registered
    ));
    // Lost registration ACK repeats original capabilities rather than creating a second mailbox.
    assert!(matches!(
        rpc(service.clone(), 3, register(&grant, now + 600)).await?,
        MailboxReplyV1::Registered
    ));
    // A failed durable write must never issue a StoreReceipt. Retrying the same
    // exterior after repairing storage is allowed, not a replacement message.
    let raw = rusqlite::Connection::open(&path)?;
    raw.execute_batch("CREATE TRIGGER reject_put BEFORE INSERT ON envelopes BEGIN SELECT RAISE(ABORT,'fixture storage failure'); END;")?;
    assert!(matches!(
        rpc(
            service.clone(),
            2,
            MailboxRequestV1::Put {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                object: [88; 32],
                ciphertext: vec![1],
                expires: now + 300
            }
        )
        .await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Storage)
    ));
    raw.execute_batch("DROP TRIGGER reject_put;")?;
    drop(raw);
    let payload = vec![177; 65536]; // Transport/storage fixture, not an E2EE claim.
    let object = [88; 32];
    let expires = now + 300;
    let put = || MailboxRequestV1::Put {
        mailbox: grant.id,
        write: *grant.write.as_bytes(),
        object,
        ciphertext: payload.clone(),
        expires,
    };
    // Recipient has no socket/runtime session during either Put.
    for _ in 0..2 {
        let MailboxReplyV1::Stored(receipt) = rpc(service.clone(), 2, put()).await? else {
            anyhow::bail!("not stored")
        };
        receipt.verify(
            key(1).verifying_key().to_bytes(),
            grant.id,
            object,
            &payload,
            expires,
        )?;
        assert!(receipt
            .verify(
                key(2).verifying_key().to_bytes(),
                grant.id,
                object,
                &payload,
                expires
            )
            .is_err());
        assert!(receipt
            .verify(
                key(1).verifying_key().to_bytes(),
                grant.id,
                object,
                &payload,
                expires + 1
            )
            .is_err());
    }
    assert!(matches!(
        rpc(
            service.clone(),
            2,
            MailboxRequestV1::Put {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                object: [89; 32],
                ciphertext: vec![1],
                expires
            }
        )
        .await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Quota)
    ));
    drop(service);
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open(&path, limits)?,
        key(1),
    ));
    // Sender is gone. Receiver repeatedly fetches after node restart; no destructive fetch.
    for _ in 0..2 {
        let MailboxReplyV1::Object {
            object: id,
            ciphertext,
            expires: expiry,
            ..
        } = rpc(
            service.clone(),
            3,
            MailboxRequestV1::Fetch {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
            },
        )
        .await?
        else {
            anyhow::bail!("missing stored object")
        };
        ensure!(
            id == object && ciphertext == payload && expiry == expires,
            "reopen differs"
        );
    }
    assert!(matches!(
        rpc(
            service.clone(),
            3,
            MailboxRequestV1::Fetch {
                mailbox: grant.id,
                read: *grant.write.as_bytes()
            }
        )
        .await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    assert!(matches!(
        rpc(
            service.clone(),
            3,
            MailboxRequestV1::Consume {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
                object,
                digest: [0; 32]
            }
        )
        .await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Conflict)
    ));
    for _ in 0..2 {
        assert!(matches!(
            rpc(
                service.clone(),
                3,
                MailboxRequestV1::Consume {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                    object,
                    digest: Sha256::digest(&payload).into()
                }
            )
            .await?,
            MailboxReplyV1::Consumed
        ));
    }
    assert!(matches!(
        rpc(service.clone(), 2, put()).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Consumed)
    ));
    assert!(matches!(
        rpc(
            service.clone(),
            3,
            MailboxRequestV1::Revoke {
                mailbox: grant.id,
                read: *grant.read.as_bytes()
            }
        )
        .await?,
        MailboxReplyV1::Revoked
    ));
    assert!(matches!(
        rpc(service, 2, put()).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
    ));
    Ok(())
}

#[tokio::test]
async fn session_preserves_chunk_counters_capabilities_revocation_and_budget() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open(directory.path().join("session.db"), Limits::default())?,
        key(1),
    ));
    let grant = MailboxGrant::generate()?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let server = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            tx.send(scope.address()?)
                .map_err(|_| anyhow::anyhow!("fixture address"))?;
            let mut stream = scope.accept_inbound().await?.into();
            serve_mailbox_session_v2(
                &mut stream,
                &key(1),
                &[key(2).verifying_key().to_bytes()],
                &service,
                || Ok(()),
            )
            .await
        })
    });
    let client = with_iroh_scope_v1(config(), control(), move |scope| {
        Box::pin(async move {
            let mut stream = scope.connect(&rx.await?).await?.into();
            let mut session = MailboxClientSessionV2::begin(
                &mut stream,
                &key(2),
                key(1).verifying_key().to_bytes(),
            )
            .await?;
            let expires = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs()
                + 300) as i64;
            ensure!(matches!(
                session.request(register(&grant, expires + 60)).await?,
                MailboxReplyV1::Registered
            ));
            let data = vec![177; 65536];
            for _ in 0..2 {
                let MailboxReplyV1::Stored(receipt) = session
                    .request(MailboxRequestV1::Put {
                        mailbox: grant.id,
                        write: *grant.write.as_bytes(),
                        object: [42; 32],
                        ciphertext: data.clone(),
                        expires,
                    })
                    .await?
                else {
                    anyhow::bail!("missing store receipt")
                };
                receipt.verify(
                    key(1).verifying_key().to_bytes(),
                    grant.id,
                    [42; 32],
                    &data,
                    expires,
                )?;
            }
            ensure!(matches!(
                session
                    .request(MailboxRequestV1::Fetch {
                        mailbox: grant.id,
                        read: *grant.write.as_bytes(),
                    })
                    .await?,
                MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
            ));
            let MailboxReplyV1::Object { ciphertext, .. } = session
                .request(MailboxRequestV1::Fetch {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                })
                .await?
            else {
                anyhow::bail!("chunked reply missing")
            };
            ensure!(ciphertext == data);
            ensure!(matches!(
                session
                    .request(MailboxRequestV1::Revoke {
                        mailbox: grant.id,
                        read: *grant.read.as_bytes(),
                    })
                    .await?,
                MailboxReplyV1::Revoked
            ));
            while session.remaining_requests() > 0 {
                ensure!(matches!(
                    session
                        .request(MailboxRequestV1::Fetch {
                            mailbox: grant.id,
                            read: *grant.read.as_bytes(),
                        })
                        .await?,
                    MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)
                ));
            }
            assert!(session
                .request(MailboxRequestV1::Fetch {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes()
                })
                .await
                .is_err());
            session.finish().await
        })
    });
    let (server, client) = tokio::join!(server, client);
    server?;
    client?;
    Ok(())
}

#[tokio::test]
async fn old_service_rejects_session_negotiation_without_fallback() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open(dir.path().join("old.db"), Limits::default())?,
        key(1),
    ));
    assert!(matches!(
        rpc(service, 2, MailboxRequestV1::BeginSession).await?,
        MailboxReplyV1::Rejected(MailboxRejectionV1::Invalid)
    ));
    Ok(())
}
