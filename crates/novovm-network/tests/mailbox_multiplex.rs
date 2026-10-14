#![cfg(feature = "iroh-transport")]
//! Real loopback QUIC and SQLite fixtures; not independent-device or latency evidence.
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use futures_util::{stream::FuturesUnordered, StreamExt};
use novovm_network::{
    carrier_stream::CarrierStreamV1, iroh_transport::*, mailbox_rpc::*, mailbox_store::*,
    product_directory::sign_relay_record_v1, product_rendezvous_daemon::now_ms,
    product_rendezvous_owned::run_self_owned_node_v1, product_service_lease::*,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn config() -> IrohConfigV1 {
    IrohConfigV1::new(&[], "127.0.0.1".parse().unwrap())
        .unwrap()
        .with_bind_address("127.0.0.1:0".parse().unwrap())
        .unwrap()
}
struct Stop(Arc<AtomicBool>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[tokio::test]
async fn three_clients_six_authenticated_lanes_share_three_connections() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let journal = dir.path().join("authority.bin");
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open(dir.path().join("mailbox.db"), Limits::default())?,
        key(1),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    let guard = Stop(stop.clone());
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let trust = SelfOwnedTrustV1 {
        version: 1,
        node_pins: [key(1).verifying_key().to_bytes()].into(),
    };
    let server = run_self_owned_node_v1(
        config().with_owned_service_identity(zeroize::Zeroizing::new([90; 32]))?,
        key(1),
        (2..=4).map(|n| key(n).verifying_key().to_bytes()).collect(),
        stop,
        Some(service),
        move |endpoint, expires| {
            let now = now_ms()?;
            let record = sign_relay_record_v1(
                &key(1),
                "multiplex-fixture",
                vec![endpoint.endpoint()?],
                now,
                expires,
                1,
            )?;
            let mut book = ServiceLeaseBookV1::new_self_owned(trust, vec![record], now)?;
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
    let clients = async move {
        let _stop = guard;
        let route = IrohServiceEndpointV1::from_record(&rx.await?)?;
        let barrier = Arc::new(tokio::sync::Barrier::new(3));
        let mut jobs = FuturesUnordered::new();
        for n in 2..=4 {
            let address = route.address.clone();
            let barrier = barrier.clone();
            jobs.push(async move {
                let owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
                let before = owner.snapshot();
                let result = owner
                    .with_continuous_service_v4(
                        || Ok(()),
                        move |scope| {
                            Box::pin(async move {
                                let mut operation: CarrierStreamV1<'_> =
                                    scope.connect(&address).await?.into();
                                let connection =
                                    request_mailbox_multiplex_v5(&mut operation).await?;
                                let mut notification = connection.open().await?;
                                request_mailbox_multiplex_lane_v5(&mut notification).await?;
                                let identity = key(n);
                                let pin = key(1).verifying_key().to_bytes();
                                let (mut op, mut notify) = tokio::try_join!(
                                    MailboxClientSessionV2::begin_continuous_v4(
                                        &mut operation,
                                        &identity,
                                        pin
                                    ),
                                    MailboxClientSessionV2::begin_continuous_v4(
                                        &mut notification,
                                        &identity,
                                        pin
                                    ),
                                )?;
                                // All SIX independently authenticated sessions must coexist.
                                barrier.wait().await;
                                let grant = MailboxGrant::generate()?;
                                let expires = (now_ms()? / 1000) as i64 + 60;
                                ensure!(matches!(
                                    op.request(MailboxRequestV1::Register {
                                        mailbox: grant.id,
                                        read: *grant.read.as_bytes(),
                                        write: *grant.write.as_bytes(),
                                        expires,
                                    })
                                    .await?,
                                    MailboxReplyV1::Registered
                                ));
                                let watch = |after, wait_ms| MailboxRequestV1::Watch {
                                    boxes: vec![MailboxWatchV1 {
                                        mailbox: grant.id,
                                        read: *grant.read.as_bytes(),
                                        after,
                                    }],
                                    wait_ms,
                                };
                                let before = match notify.request(watch(None, 0)).await? {
                                    MailboxReplyV1::Notice(items) if items.len() == 1 => {
                                        items[0].cursor
                                    }
                                    _ => anyhow::bail!("initial cursor missing"),
                                };
                                let data = vec![n; 32]; // opaque fixture, not application encryption evidence
                                let put = async {
                                    tokio::time::sleep(Duration::from_millis(100)).await;
                                    match op
                                        .request(MailboxRequestV1::Put {
                                            mailbox: grant.id,
                                            write: *grant.write.as_bytes(),
                                            object: [n; 32],
                                            ciphertext: data.clone(),
                                            expires,
                                        })
                                        .await?
                                    {
                                        MailboxReplyV1::Stored(receipt) => receipt
                                            .verify(pin, grant.id, [n; 32], &data, expires)?,
                                        _ => anyhow::bail!("durable receipt missing"),
                                    }
                                    Ok::<_, anyhow::Error>(())
                                };
                                let notice = async {
                                    match notify.request(watch(Some(before), 10_000)).await? {
                                        MailboxReplyV1::Notice(items) => ensure!(
                                            items.len() == 1
                                                && items[0].pending
                                                && items[0].cursor != before
                                        ),
                                        _ => anyhow::bail!("notification missing"),
                                    }
                                    Ok::<_, anyhow::Error>(())
                                };
                                // Put must complete while sibling Watch is outstanding.
                                tokio::time::timeout(Duration::from_secs(8), async {
                                    tokio::try_join!(put, notice)
                                })
                                .await??;
                                let fetch = || MailboxRequestV1::Fetch {
                                    mailbox: grant.id,
                                    read: *grant.read.as_bytes(),
                                };
                                let digest = match op.request(fetch()).await? {
                                    MailboxReplyV1::Object {
                                        object,
                                        ciphertext,
                                        digest,
                                        expires: stored_expiry,
                                    } => {
                                        ensure!(
                                            object == [n; 32]
                                                && ciphertext == data
                                                && stored_expiry == expires
                                        );
                                        digest
                                    }
                                    _ => anyhow::bail!("stored object missing"),
                                };
                                // Normal completion releases this stream, not the sibling.
                                notify.finish().await?;
                                ensure!(matches!(
                                    op.request(MailboxRequestV1::Consume {
                                        mailbox: grant.id,
                                        read: *grant.read.as_bytes(),
                                        object: [n; 32],
                                        digest
                                    })
                                    .await?,
                                    MailboxReplyV1::Consumed
                                ));
                                ensure!(matches!(
                                    op.request(fetch()).await?,
                                    MailboxReplyV1::Empty
                                ));
                                op.finish().await?;
                                Ok::<_, anyhow::Error>(())
                            })
                        },
                    )
                    .await;
                let snapshot = owner.snapshot();
                owner.close().await?;
                result?;
                // binds_completed is PROCESS-wide (all three owners), not per client.
                ensure!(snapshot.instance == before.instance && snapshot.operations_started == 1);
                Ok::<_, anyhow::Error>(())
            });
        }
        while let Some(result) = jobs.next().await {
            result?;
        }
        Ok::<_, anyhow::Error>(())
    };
    let (server, clients) = tokio::time::timeout(Duration::from_secs(60), async {
        tokio::join!(server, clients)
    })
    .await?;
    clients?;
    ensure!(server.is_err(), "local stop should end node scope");
    Ok(())
}

#[tokio::test]
async fn multiplex_rejects_legacy_finite_bad_child_identity_and_revocation() -> Result<()> {
    for case in [
        "legacy",
        "finite",
        "child_identity",
        "child_pin",
        "child_version",
        "revoked",
        "cancelled",
    ] {
        let dir = tempfile::tempdir()?;
        let service = MailboxServiceV1::new(
            MailboxStore::open(dir.path().join("mailbox.db"), Limits::default())?,
            key(1),
        );
        let allowed = Arc::new(AtomicBool::new(true));
        let check = allowed.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (done, wait_done) = tokio::sync::oneshot::channel();
        let server = with_iroh_owned_listener_v1(
            config().with_owned_service_identity(zeroize::Zeroizing::new([90; 32]))?,
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    tx.send(scope.address()?).unwrap();
                    let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
                    let result = if case == "legacy" {
                        let result = serve_node_offer_selected_v1::<()>(&mut stream, || {
                            anyhow::bail!("unexpected old offer")
                        })
                        .await
                        .map(|_| ());
                        drop(stream);
                        result
                    } else {
                        let (kind, _) = serve_self_owned_node_selected_v4::<()>(
                            &mut stream,
                            || anyhow::bail!("unexpected old offer"),
                            || Ok(()),
                        )
                        .await?;
                        ensure!(kind == NodeServiceV1::MailboxMultiplex);
                        serve_mailbox_multiplex_v5(
                            stream,
                            &key(1),
                            &[key(2).verifying_key().to_bytes()],
                            &service,
                            || {
                                ensure!(check.load(Ordering::Acquire), "fixture revoked");
                                Ok(())
                            },
                        )
                        .await
                    };
                    // Preserve the listener while the client observes its terminal error.
                    let _ = wait_done.await;
                    ensure!(result.is_err(), "invalid multiplex session accepted");
                    Ok(())
                })
            },
        );
        let owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
        async fn operation(
            scope: &IrohScopeV1,
            case: &'static str,
            rx: tokio::sync::oneshot::Receiver<IrohAddressV1>,
            done: tokio::sync::oneshot::Sender<()>,
            allowed: Arc<AtomicBool>,
        ) -> Result<()> {
            let mut root: CarrierStreamV1<'_> = scope.connect(&rx.await?).await?.into();
            let connection = request_mailbox_multiplex_v5(&mut root).await;
            if matches!(case, "legacy" | "finite") {
                ensure!(connection.is_err(), "old or finite scope gained multiplex");
                drop(connection);
                drop(root);
                let _ = done.send(());
                return Ok(());
            }
            let connection = connection?;
            let identity = key(2);
            let pin = key(1).verifying_key().to_bytes();
            let mut op =
                MailboxClientSessionV2::begin_continuous_v4(&mut root, &identity, pin).await?;
            let mut child = connection.open().await?;
            if case == "child_version" {
                ensure!(
                    request_mailbox_continuous_v4(&mut child).await.is_err(),
                    "child silently downgraded"
                );
            } else {
                request_mailbox_multiplex_lane_v5(&mut child).await?;
                let child_identity = key(if case == "child_identity" { 3 } else { 2 });
                let child_pin = if case == "child_pin" {
                    key(4).verifying_key().to_bytes()
                } else {
                    pin
                };
                let session = MailboxClientSessionV2::begin_continuous_v4(
                    &mut child,
                    &child_identity,
                    child_pin,
                )
                .await;
                if matches!(case, "child_identity" | "child_pin") {
                    ensure!(session.is_err(), "child borrowed root identity");
                } else {
                    let mut notify = session?;
                    let grant = MailboxGrant::generate()?;
                    ensure!(matches!(
                        op.request(MailboxRequestV1::Register {
                            mailbox: grant.id,
                            read: *grant.read.as_bytes(),
                            write: *grant.write.as_bytes(),
                            expires: (now_ms()? / 1000) as i64 + 60,
                        })
                        .await?,
                        MailboxReplyV1::Registered
                    ));
                    let watch = |after, wait_ms| MailboxRequestV1::Watch {
                        boxes: vec![MailboxWatchV1 {
                            mailbox: grant.id,
                            read: *grant.read.as_bytes(),
                            after,
                        }],
                        wait_ms,
                    };
                    let before = match notify.request(watch(None, 0)).await? {
                        MailboxReplyV1::Notice(items) => items[0].cursor,
                        _ => anyhow::bail!("cursor missing"),
                    };
                    let started = Instant::now();
                    if case == "revoked" {
                        let revoke = async {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            allowed.store(false, Ordering::Release);
                        };
                        let (result, ()) =
                            tokio::join!(notify.request(watch(Some(before), 10_000)), revoke);
                        ensure!(result.is_err() && started.elapsed() < Duration::from_secs(3));
                    } else {
                        ensure!(tokio::time::timeout(
                            Duration::from_millis(100),
                            notify.request(watch(Some(before), 10_000))
                        )
                        .await
                        .is_err());
                    }
                    ensure!(
                        notify.request(watch(None, 0)).await.is_err(),
                        "poisoned child revived"
                    );
                }
            }
            drop(child);
            // A security/partial-IO failure remains terminal for the shared group.
            ensure!(
                op.request(MailboxRequestV1::Fetch {
                    mailbox: [5; 32],
                    read: [6; 32]
                })
                .await
                .is_err(),
                "sibling ignored terminal failure"
            );
            drop(op);
            drop(root);
            drop(connection);
            let _ = done.send(());
            Ok(())
        }
        let client = async {
            if case == "finite" {
                owner
                    .with_operation(
                        IrohControlV1::new(Instant::now() + Duration::from_secs(10), || Ok(()))?,
                        move |scope| Box::pin(operation(scope, case, rx, done, allowed)),
                    )
                    .await
            } else {
                owner
                    .with_continuous_service_v4(
                        || Ok(()),
                        move |scope| Box::pin(operation(scope, case, rx, done, allowed)),
                    )
                    .await
            }
        };
        let (a, b) = tokio::time::timeout(Duration::from_secs(15), async {
            tokio::join!(server, client)
        })
        .await?;
        owner.close().await?;
        a.with_context(|| format!("server {case}"))?;
        b.with_context(|| format!("client {case}"))?;
    }
    Ok(())
}
