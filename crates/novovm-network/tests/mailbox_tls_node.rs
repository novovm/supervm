#![cfg(all(feature = "iroh-transport", feature = "tls-transport"))]
//! Real existing node loop, TCP/TLS, QUIC and encrypted authority journal.
//! Loopback fixtures are not Android, independent-node or public reachability proof.
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::*,
    mailbox_rpc::*,
    mailbox_store::*,
    product_directory::{sign_relay_record_v1, PeerSignedRelayRecordV1},
    product_rendezvous_daemon::now_ms,
    product_rendezvous_owned::{run_self_owned_node_with_tls_v1, TlsMailboxListenerV1},
    product_service_lease::*,
    tls_transport::TlsScopeV1,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use zeroize::Zeroizing;

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn trust() -> SelfOwnedTrustV1 {
    SelfOwnedTrustV1 {
        version: 1,
        node_pins: [key(1).verifying_key().to_bytes()].into(),
    }
}
fn config() -> Result<IrohConfigV1> {
    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)
}
fn sign(
    iref: &IrohServiceEndpointV1,
    tls: Option<&TlsServiceEndpointV1>,
    seq: u64,
    at: u64,
) -> Result<PeerSignedRelayRecordV1> {
    let mut routes = vec![iref.endpoint()?];
    if let Some(tls) = tls {
        routes.push(tls.endpoint()?);
    }
    Ok(sign_relay_record_v1(
        &key(1),
        "tls-node-fixture",
        routes,
        at,
        at + 90_000,
        seq,
    )?)
}

#[test]
fn signed_tls_route_uses_existing_history_freshness_and_revocation() -> Result<()> {
    let ir = IrohServiceEndpointV1 {
        instance: [4; 32],
        address: IrohAddressV1 {
            endpoint_key: key(3).verifying_key().to_bytes(),
            direct: vec!["127.0.0.1:3000".parse()?],
            relays: vec![],
        },
    };
    let tls = TlsServiceEndpointV1 {
        address: "127.0.0.1:3001".parse()?,
        endpoint_key: key(5).verifying_key().to_bytes(),
        max_connections: 4,
    };
    let pin = key(1).verifying_key().to_bytes();
    let first = sign(&ir, Some(&tls), 1, 1000)?;
    let mut book = ServiceLeaseBookV1::new_self_owned(trust(), vec![first.clone()], 1000)?;
    book.check_tls_route_v1(pin, &tls, 1001)?;
    let mut wrong = tls.clone();
    wrong.address.set_port(3002);
    assert!(book.check_tls_route_v1(pin, &wrong, 1002).is_err());
    wrong.endpoint_key = key(6).verifying_key().to_bytes();
    assert!(book
        .check_continuous_tls_peer_v1(pin, &wrong, 1002)
        .is_err());
    // Discovery expiry cannot renew a dial, and does not break a live session.
    book.check_continuous_tls_peer_v1(pin, &tls, 92_000)?;
    assert!(book.check_tls_route_v1(pin, &tls, 92_000).is_err());
    let next = sign(&ir, Some(&tls), 2, 92_000)?;
    let mut forged = next.clone();
    forged.endpoints[1] = wrong.endpoint()?;
    assert!(book
        .replace_self_owned_records(vec![forged], 92_000)
        .is_err());
    book.replace_self_owned_records(vec![next], 92_000)?;
    book.check_tls_route_v1(pin, &tls, 92_000)?;
    let saved = book.checkpoint()?;
    let mut book = ServiceLeaseBookV1::restore_self_owned(trust(), &saved, 92_001)?;
    assert!(book
        .replace_self_owned_records(vec![first], 92_001)
        .is_err());
    // New signed address updates new dialing; unchanged key keeps live IO valid.
    let mut moved = tls.clone();
    moved.address.set_port(3003);
    book.replace_self_owned_records(vec![sign(&ir, Some(&moved), 3, 92_002)?], 92_002)?;
    book.check_continuous_tls_peer_v1(pin, &tls, 92_002)?;
    assert!(book.check_tls_route_v1(pin, &tls, 92_002).is_err());
    book.check_tls_route_v1(pin, &moved, 92_002)?;
    book.replace_self_owned_records(vec![sign(&ir, None, 4, 92_003)?], 92_003)?;
    assert!(book
        .check_continuous_tls_peer_v1(pin, &tls, 92_003)
        .is_err());
    book.revoke_self_owned(pin, 92_004)?;
    let mut restored =
        ServiceLeaseBookV1::restore_self_owned(trust(), &book.checkpoint()?, 92_005)?;
    assert!(restored
        .replace_self_owned_records(vec![sign(&ir, Some(&tls), 5, 92_005)?], 92_005)
        .is_err());

    let mut duplicate = sign(&ir, Some(&tls), 1, 1000)?;
    duplicate.endpoints[0] = tls.endpoint()?;
    assert!(IrohServiceEndpointV1::from_record(&duplicate).is_err());
    for address in [
        "0.0.0.0:443",
        "224.0.0.1:443",
        "255.255.255.255:443",
        "127.0.0.1:0",
    ] {
        let mut bad = tls.clone();
        bad.address = address.parse()?;
        assert!(bad.endpoint().is_err());
    }
    let mut weak = tls;
    weak.endpoint_key = [0; 32];
    assert!(weak.endpoint().is_err());
    Ok(())
}

#[tokio::test]
async fn actual_owned_node_shares_tls_and_quic_custody_and_stable_tls_identity() -> Result<()> {
    actual_node(false).await
}

#[tokio::test]
async fn tls_listener_survives_unreachable_optional_relay() -> Result<()> {
    actual_node(true).await
}

async fn actual_node(unreachable_relay: bool) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let journal = dir.path().join("authority.bin");
    let stop = Arc::new(AtomicBool::new(false));
    let listener = TlsMailboxListenerV1::bind_with_capacity(
        "127.0.0.1:0".parse()?,
        "127.0.0.1:0".parse()?,
        Zeroizing::new([92; 32]),
        stop.clone(),
        8,
    )
    .await?;
    let expected = listener.route().clone();
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open(dir.path().join("mailbox.db"), Limits::default())?,
        key(1),
    ));
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let carrier = if unreachable_relay {
        IrohConfigV1::new(
            &["https://unreachable-fixture.invalid/".into()],
            "127.0.0.1".parse()?,
        )?
        .with_bind_address("127.0.0.1:0".parse()?)?
    } else {
        config()?
    };
    let server = run_self_owned_node_with_tls_v1(
        carrier.with_owned_service_identity(Zeroizing::new([90; 32]))?,
        key(1),
        [key(2).verifying_key().to_bytes()].into(),
        stop.clone(),
        service,
        listener,
        move |endpoint, expires| {
            let at = now_ms()?;
            let record = sign_relay_record_v1(
                &key(1),
                "tls-node-fixture",
                vec![endpoint.endpoint()?],
                at,
                expires,
                1,
            )?;
            let mut book = ServiceLeaseBookV1::new_self_owned(trust(), vec![record], at)?;
            let bytes = book.checkpoint()?;
            let file = ServiceLeaseFileV1::create(&journal, Zeroizing::new([91; 32]), &bytes)?;
            book.attach_persistence(file, &bytes)?;
            Ok(Arc::new(ServiceLeaseHostV1::new(key(1), book)?))
        },
        move |record| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(record.clone());
            }
            Ok(())
        },
    );
    let client = async {
        let record = rx.await?;
        let mut book =
            ServiceLeaseBookV1::new_self_owned(trust(), vec![record.clone()], now_ms()?)?;
        let tls = TlsServiceEndpointV1::from_record(&record)?.context("TLS route unpublished")?;
        ensure!(tls == expected);
        let pin = key(1).verifying_key().to_bytes();
        book.check_tls_route_v1(pin, &tls, now_ms()?)?;
        let client_scope = TlsScopeV1::new_continuous_v4(|| Ok(()))?.with_connection_limit(8)?;
        let tcp = tokio::net::TcpStream::connect(tls.address).await?;
        tcp.set_nodelay(true)?;
        let mut stream: CarrierStreamV1<'_> =
            client_scope.connect(tcp, tls.endpoint_key).await?.into();
        request_mailbox_continuous_v6(&mut stream).await?;
        let identity = key(2);
        let mut session =
            MailboxClientSessionV2::begin_continuous_v6(&mut stream, &identity, pin).await?;
        let grant = MailboxGrant::generate()?;
        let expires = (now_ms()? / 1000) as i64 + 60;
        ensure!(matches!(
            session
                .request(MailboxRequestV1::Register {
                    mailbox: grant.id,
                    write: *grant.write.as_bytes(),
                    read: *grant.read.as_bytes(),
                    expires
                })
                .await?,
            MailboxReplyV1::Registered
        ));
        let data = vec![23; 200];
        let MailboxReplyV1::Stored(receipt) = session
            .request(MailboxRequestV1::Put {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                object: [7; 32],
                ciphertext: data.clone(),
                expires,
            })
            .await?
        else {
            anyhow::bail!("no StoreReceipt")
        };
        receipt.verify(pin, grant.id, [7; 32], &data, expires)?;
        // Real simultaneous TLS admissions exceed the former four-slot limit.
        // These lanes wait for their bound handshake; they cannot access data.
        // The established mailbox and QUIC lane must still make progress.
        let mut admitted = Vec::new();
        for _ in 0..5 {
            let tcp = tokio::net::TcpStream::connect(tls.address).await?;
            let mut lane: CarrierStreamV1<'_> =
                client_scope.connect(tcp, tls.endpoint_key).await?.into();
            request_mailbox_continuous_v6(&mut lane).await?;
            admitted.push(lane);
        }
        // Both real transports remain connected to the SAME running node/store.
        let route = IrohServiceEndpointV1::from_record(&record)?;
        let mailbox = grant.id;
        let read = *grant.read.as_bytes();
        let owner = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
        let result = owner
            .with_continuous_service_v4(
                || Ok(()),
                move |scope| {
                    Box::pin(async move {
                        let mut stream = scope.connect(&route.address).await?.into();
                        let _group = request_mailbox_multiplex_v6(&mut stream).await?;
                        let user = key(2);
                        let mut op =
                            MailboxClientSessionV2::begin_continuous_v6(&mut stream, &user, pin)
                                .await?;
                        let MailboxReplyV1::Object {
                            object,
                            ciphertext,
                            digest,
                            expires: actual,
                        } = op
                            .request(MailboxRequestV1::Fetch { mailbox, read })
                            .await?
                        else {
                            anyhow::bail!("shared object missing")
                        };
                        ensure!(object == [7; 32] && ciphertext == data && actual == expires);
                        ensure!(matches!(
                            op.request(MailboxRequestV1::Consume {
                                mailbox,
                                read,
                                object,
                                digest
                            })
                            .await?,
                            MailboxReplyV1::Consumed
                        ));
                        op.finish().await
                    })
                },
            )
            .await;
        owner.close().await?;
        result?;
        drop(admitted);
        ensure!(matches!(
            session
                .request(MailboxRequestV1::Fetch { mailbox, read })
                .await?,
            MailboxReplyV1::Empty
        ));
        session.finish().await?;
        Ok::<_, anyhow::Error>(())
    };
    tokio::pin!(server);
    let result = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(30), client) => result?,
        result = &mut server => { result?; anyhow::bail!("node ended before client") }
    };
    stop.store(true, Ordering::Release);
    let ended = tokio::time::timeout(Duration::from_secs(10), &mut server).await?;
    if let Err(error) = ended {
        ensure!(
            format!("{error:#}").contains("stopped by owner"),
            "{error:#}"
        );
    }
    result?;
    // Protected platform seed reuse, not a new node identity on every launch.
    let reopened = TlsMailboxListenerV1::bind(
        "127.0.0.1:0".parse()?,
        "127.0.0.1:0".parse()?,
        Zeroizing::new([92; 32]),
        Arc::new(AtomicBool::new(false)),
    )
    .await?;
    ensure!(reopened.route().endpoint_key == expected.endpoint_key);
    Ok(())
}

#[test]
fn host_rotation_preserves_tls_until_explicit_removal_and_persists_it() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("host.bin");
    let ir = IrohServiceEndpointV1 {
        instance: [4; 32],
        address: IrohAddressV1 {
            endpoint_key: key(3).verifying_key().to_bytes(),
            direct: vec!["127.0.0.1:3000".parse()?],
            relays: vec![],
        },
    };
    let tls = TlsServiceEndpointV1 {
        address: "127.0.0.1:3001".parse()?,
        endpoint_key: key(5).verifying_key().to_bytes(),
        max_connections: 4,
    };
    let mut book =
        ServiceLeaseBookV1::new_self_owned(trust(), vec![sign(&ir, None, 1, 1000)?], 1000)?;
    let bytes = book.checkpoint()?;
    let file = ServiceLeaseFileV1::create(&path, Zeroizing::new([91; 32]), &bytes)?;
    book.attach_persistence(file, &bytes)?;
    let host = ServiceLeaseHostV1::new(key(1), book)?;
    host.rotate_self_owned_with_tls_v1(&ir, Some(&tls), 1001, 91_001)?;
    let updated = host.rotate_self_owned(&ir, 1002, 91_002)?;
    ensure!(TlsServiceEndpointV1::from_record(&updated)?.as_ref() == Some(&tls));
    drop(host);
    let (file, bytes) = ServiceLeaseFileV1::open(&path, Zeroizing::new([91; 32]))?;
    let mut book = ServiceLeaseBookV1::restore_self_owned(trust(), &bytes, 1003)?;
    book.attach_persistence(file, &bytes)?;
    book.check_tls_route_v1(key(1).verifying_key().to_bytes(), &tls, 1003)?;
    let host = ServiceLeaseHostV1::new(key(1), book)?;
    let removed = host.rotate_self_owned_with_tls_v1(&ir, None, 1004, 91_004)?;
    ensure!(TlsServiceEndpointV1::from_record(&removed)?.is_none());
    assert!(host.check_continuous_tls_peer_v1(&tls, 1004).is_err());
    Ok(())
}
