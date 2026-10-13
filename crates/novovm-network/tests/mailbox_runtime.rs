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
        with_iroh_scope_v1(config(), control(), move |scope| {
            Box::pin(async move {
                let grant = MailboxGrant::generate()?;
                for request in [
                    register(&grant, (now_ms()? / 1000) as i64 + 600),
                    MailboxRequestV1::Fetch {
                        mailbox: grant.id,
                        read: *grant.read.as_bytes(),
                    },
                ] {
                    let mut stream: CarrierStreamV1<'_> =
                        scope.connect(&route.address).await?.into();
                    let offer = request_mailbox_offer_v1(&mut stream).await?;
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
                    ensure!(
                        matches!(reply, MailboxReplyV1::Registered | MailboxReplyV1::Empty),
                        "host dispatch failed"
                    );
                }
                Ok(())
            })
        })
        .await
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
