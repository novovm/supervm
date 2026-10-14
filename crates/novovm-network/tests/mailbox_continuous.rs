#![cfg(feature = "iroh-transport")]
//! Real localhost QUIC fixtures. These are neither Android nor anonymity evidence.
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    carrier_stream::CarrierStreamV1, iroh_transport::*, mailbox_rpc::*, mailbox_store::*,
    product_directory::sign_relay_record_v1, product_rendezvous_daemon::now_ms,
    product_rendezvous_owned::run_self_owned_node_v1, product_service_lease::*,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
fn fetch(grant: &MailboxGrant) -> MailboxRequestV1 {
    MailboxRequestV1::Fetch {
        mailbox: grant.id,
        read: *grant.read.as_bytes(),
    }
}
fn register(grant: &MailboxGrant) -> MailboxRequestV1 {
    MailboxRequestV1::Register {
        mailbox: grant.id,
        write: *grant.write.as_bytes(),
        read: *grant.read.as_bytes(),
        expires: (now_ms().unwrap() / 1000) as i64 + 600,
    }
}
async fn cursor(
    session: &mut MailboxClientSessionV2<'_, '_>,
    grant: &MailboxGrant,
) -> Result<MailboxCursorV1> {
    match session
        .request(MailboxRequestV1::Watch {
            boxes: vec![MailboxWatchV1 {
                mailbox: grant.id,
                read: *grant.read.as_bytes(),
                after: None,
            }],
            wait_ms: 0,
        })
        .await?
    {
        MailboxReplyV1::Notice(notices) if notices.len() == 1 => Ok(notices[0].cursor),
        _ => anyhow::bail!("missing initial cursor"),
    }
}
struct Stop(Arc<AtomicBool>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[tokio::test]
async fn continuous_mailbox_survives_locator_rotation_expiry_and_more_than_64_requests(
) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let journal = dir.path().join("authority.bin");
    let database = dir.path().join("mailbox.db");
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open(&database, Limits::default())?,
        key(1),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    let guard = Stop(stop.clone());
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let epochs = Arc::new(AtomicUsize::new(0));
    let published = epochs.clone();
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
                "continuous-fixture",
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
            published.fetch_add(1, Ordering::Relaxed);
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(record.clone());
            }
            Ok(())
        },
    );
    let client = async move {
        let _stop = guard;
        let record = rx.await?;
        let original_expiry = record.expires_at_ms;
        let route = IrohServiceEndpointV1::from_record(&record)?;
        let book = Arc::new(Mutex::new(ServiceLeaseBookV1::new_self_owned(
            trust,
            vec![record],
            now_ms()?,
        )?));
        let owner = IrohEndpointOwnerV1::bind(config(), || Ok(())).await?;
        let before = owner.snapshot();
        let check_book = book.clone();
        let transport = route.address.endpoint_key;
        owner.with_continuous_service_v4(move || {
            check_book.lock().unwrap().check_continuous_peer_v4(key(1).verifying_key().to_bytes(), transport, now_ms()?)
        }, move |scope| Box::pin(async move {
            let started = Instant::now();
            let mut stream: CarrierStreamV1<'_> = scope.connect(&route.address).await?.into();
            request_mailbox_continuous_v4(&mut stream).await?;
            let mut session = MailboxClientSessionV2::begin_continuous_v4(&mut stream, &key(2), key(1).verifying_key().to_bytes()).await?;
            let grant = MailboxGrant::generate()?;
            ensure!(matches!(session.request(register(&grant)).await?, MailboxReplyV1::Registered));
            // Exercise the shared rate budget beyond the old lifetime cap.
            for _ in 0..70 { ensure!(matches!(session.request(fetch(&grant)).await?, MailboxReplyV1::Empty)); }
            let after = cursor(&mut session, &grant).await?;
            while started.elapsed() < Duration::from_secs(131) {
                ensure!(matches!(session.request(MailboxRequestV1::Watch {
                    boxes: vec![MailboxWatchV1 { mailbox: grant.id, read: *grant.read.as_bytes(), after: Some(after) }],
                    wait_ms: 10_000,
                }).await?, MailboxReplyV1::Notice(_)));
            }
            ensure!(now_ms()? > original_expiry && epochs.load(Ordering::Relaxed) >= 3, "did not cross locator epochs");
            // Stale record is NOT renewed or used as a fresh lease.
            ensure!(book.lock().unwrap().routes(now_ms()?)?.is_empty());
            let data = vec![77; 32]; // fixture bytes, not a claim of application E2EE
            let expires = (now_ms()? / 1000) as i64 + 60;
            let receipt = session.request(MailboxRequestV1::Put { mailbox: grant.id, write: *grant.write.as_bytes(),
                object: [42; 32], ciphertext: data.clone(), expires }).await?;
            match receipt { MailboxReplyV1::Stored(receipt) => receipt.verify(key(1).verifying_key().to_bytes(), grant.id, [42; 32], &data, expires)?,
                _ => anyhow::bail!("missing durable store receipt") }
            ensure!(matches!(session.request(fetch(&grant)).await?, MailboxReplyV1::Object { ciphertext, .. } if ciphertext == data));
            // A capability error is a rejected operation, not a global license.
            ensure!(matches!(session.request(MailboxRequestV1::Fetch { mailbox: grant.id, read: [99; 32] }).await?, MailboxReplyV1::Rejected(MailboxRejectionV1::Unauthorized)));
            ensure!(matches!(session.request(fetch(&grant)).await?, MailboxReplyV1::Object { .. }));
            eprintln!("continuous fixture: elapsed_ms={} declaration_generations={} initial_fetches=70 scopes=1", started.elapsed().as_millis(), epochs.load(Ordering::Relaxed));
            session.finish().await
        })).await?;
        let after = owner.snapshot();
        ensure!(
            before.instance == after.instance
                && after.operations_started == before.operations_started + 1,
            "owner rebuilt scope"
        );
        owner.close().await?;
        Ok::<_, anyhow::Error>(())
    };
    let (a, b) = tokio::time::timeout(Duration::from_secs(165), async {
        tokio::join!(server, client)
    })
    .await?;
    b?;
    if let Err(error) = a {
        ensure!(
            error.to_string() == "service stopped by owner",
            "unexpected node failure: {error:#}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn continuous_rejects_wrong_identity_legacy_finite_scope_revocation_and_cancellation(
) -> Result<()> {
    for case in [
        "identity",
        "node_pin",
        "legacy",
        "old_selector",
        "finite",
        "revoked",
        "cancelled",
    ] {
        let dir = tempfile::tempdir()?;
        let service = Arc::new(MailboxServiceV1::new(
            MailboxStore::open(dir.path().join("box.db"), Limits::default())?,
            key(1),
        ));
        let allowed = Arc::new(AtomicBool::new(true));
        let check = allowed.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (done, wait_done) = tokio::sync::oneshot::channel();
        let server = with_iroh_owned_listener_v1(
            config().with_owned_service_identity(zeroize::Zeroizing::new([90; 32]))?,
            || Ok(()),
            move |scope| {
                Box::pin(async move {
                    let _ = tx.send(scope.address()?);
                    let mut stream: CarrierStreamV1<'_> = scope.accept_inbound().await?.into();
                    let result = if case == "old_selector" {
                        serve_node_offer_selected_v1::<()>(&mut stream, || {
                            anyhow::bail!("old selector must not issue V4 offer")
                        })
                        .await
                        .map(|_| ())
                    } else {
                        serve_mailbox_continuous_v4(
                            &mut stream,
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
                    drop(stream);
                    let _ = wait_done.await;
                    ensure!(result.is_err(), "invalid session accepted");
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
            let mut stream: CarrierStreamV1<'_> = scope.connect(&rx.await?).await?.into();
            if case == "old_selector" {
                ensure!(
                    request_mailbox_continuous_v4(&mut stream).await.is_err(),
                    "old version accepted continuous selector"
                );
                drop(stream);
                let _ = done.send(());
                return Ok(());
            }
            let identity = key(if case == "identity" { 3 } else { 2 });
            let node_pin = key(if case == "node_pin" { 4 } else { 1 })
                .verifying_key()
                .to_bytes();
            let result = if case == "legacy" {
                MailboxClientSessionV2::begin(&mut stream, &identity, node_pin).await
            } else {
                MailboxClientSessionV2::begin_continuous_v4(&mut stream, &identity, node_pin).await
            };
            if matches!(case, "identity" | "node_pin" | "legacy" | "finite") {
                ensure!(result.is_err(), "invalid negotiation accepted");
                drop(result);
                drop(stream);
                let _ = done.send(());
                return Ok(());
            }
            let mut session = result?;
            let grant = MailboxGrant::generate()?;
            ensure!(matches!(
                session.request(register(&grant)).await?,
                MailboxReplyV1::Registered
            ));
            let after = cursor(&mut session, &grant).await?;
            let watch = MailboxRequestV1::Watch {
                boxes: vec![MailboxWatchV1 {
                    mailbox: grant.id,
                    read: *grant.read.as_bytes(),
                    after: Some(after),
                }],
                wait_ms: 10_000,
            };
            let started = Instant::now();
            if case == "revoked" {
                let revoke = async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    allowed.store(false, Ordering::Release);
                };
                let (result, ()) = tokio::join!(session.request(watch), revoke);
                ensure!(
                    result.is_err() && started.elapsed() < Duration::from_secs(3),
                    "revocation blocked behind Watch"
                );
            } else {
                ensure!(
                    tokio::time::timeout(Duration::from_millis(100), session.request(watch))
                        .await
                        .is_err()
                );
            }
            ensure!(
                session.request(fetch(&grant)).await.is_err(),
                "cancelled or revoked session revived"
            );
            drop(session);
            drop(stream);
            let _ = done.send(());
            Ok(())
        }
        // Function pointers/closures retain the actual finite vs owner scope.
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
        a.with_context(|| format!("server {case}"))?;
        b.with_context(|| format!("client {case}"))?;
        owner.close().await?;
    }
    Ok(())
}
