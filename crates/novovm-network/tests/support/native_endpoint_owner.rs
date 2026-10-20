//! Borrowed mobile-owner composition uses the existing real UDP fixture client
//! and the production continuous carrier client. No external Relay is involved.
use super::*;
use novovm_network::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::{IrohConfigV1, IrohEndpointOwnerV1},
    mailbox_rpc::MailboxClientSessionV2,
    product_rendezvous_owned::run_self_owned_mailbox_on_endpoint_with_native_v7,
    product_service_lease::{
        request_mailbox_continuous_v4, IrohServiceEndpointV1, NativeCarrierMailboxEndpointV8,
    },
};

fn config() -> Result<IrohConfigV1> {
    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)
}

#[tokio::test]
async fn borrowed_l4_endpoint_shares_udp_custody_and_preserves_carrier() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let owner = IrohEndpointOwnerV1::bind(
        config()?.with_owned_service_identity(Zeroizing::new([90; 32]))?,
        || Ok(()),
    )
    .await?;
    let peer = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
    let before = owner.snapshot();
    let stop = Arc::new(AtomicBool::new(false));
    let listener = NativeMailboxListenerV7::bind(
        "127.0.0.1:0".parse()?,
        "127.0.0.1:0".parse()?,
        key(17),
        stop.clone(),
        2,
    )?;
    let expected = listener.route().clone();
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open_contributor(dir.path().join("mailbox.db"), Default::default(), true)?,
        key(17),
    ));
    service.set_resources_available(true).await?;
    let journal = dir.path().join("authority.bin");
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let mut server = Box::pin(run_self_owned_mailbox_on_endpoint_with_native_v7(
        &owner,
        key(17),
        stop.clone(),
        service.clone(),
        Some(listener),
        move |endpoint, until| {
            let now = now_ms()?;
            let record = sign_relay_record_v1(
                &key(17),
                "borrowed-native-fixture",
                vec![endpoint.endpoint()?],
                now,
                until,
                1,
            )?;
            let mut book = ServiceLeaseBookV1::new_self_owned(trust(), vec![record], now)?;
            let checkpoint = book.checkpoint()?;
            let file = ServiceLeaseFileV1::create(&journal, Zeroizing::new([91; 32]), &checkpoint)?;
            book.attach_persistence(file, &checkpoint)?;
            let host = Arc::new(ServiceLeaseHostV1::new(key(17), book)?);
            host.rotate_native_carrier_mailbox_bulk_v1(true, now, until)?;
            Ok(host)
        },
        move |record| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(record.clone());
            }
            Ok(())
        },
    ));
    let client = async {
        let record = rx.await?;
        ensure!(NativeCarrierMailboxEndpointV8::supports_bulk_v1(&record)?);
        let route =
            NativeMailboxEndpointV7::from_record(&record)?.context("missing UDP locator")?;
        ensure!(route == expected);
        let pin = key(17).verifying_key().to_bytes();
        let mut book =
            ServiceLeaseBookV1::new_self_owned(trust(), vec![record.clone()], now_ms()?)?;
        book.check_native_mailbox_route_v7(pin, &route, now_ms()?)?;
        let carrier = IrohServiceEndpointV1::from_record(&record)?;
        book.check_continuous_peer_v4(pin, carrier.address.endpoint_key, now_ms()?)?;
        let mut udp = Client::connect(&route, 83).await?;
        let expires = (now_ms()? / 1000) as i64 + 60;
        ensure!(matches!(
            udp.request(Request::Register {
                mailbox: [1; 32],
                write: [2; 32],
                read: [3; 32],
                expires,
            })
            .await?,
            Reply::Registered
        ));
        let Reply::Stored(original) = udp.request(put(expires, 4)).await? else {
            bail!("native custody missing")
        };
        original.verify(pin, [1; 32], [4; 32], &vec![9; 9000], expires)?;
        let custody = service.clone();
        peer.with_continuous_service_v4(|| Ok(()), move |scope| Box::pin(async move {
            let mut stream: CarrierStreamV1<'_> = scope.connect(&carrier.address).await?.into();
            request_mailbox_continuous_v4(&mut stream).await?;
            // A different key is not pre-enrolled. Capabilities still determine access.
            let identity = key(84);
            let mut session = MailboxClientSessionV2::begin_continuous_v4(&mut stream, &identity, pin).await?;
            ensure!(matches!(session.request(fetch(99)).await?, Reply::Rejected(MailboxRejectionV1::Unauthorized)));
            ensure!(matches!(session.request(fetch(3)).await?, Reply::Object { object, ciphertext, expires: ttl, .. }
                if object == [4;32] && ciphertext == vec![9;9000] && ttl == expires));
            let Reply::Stored(duplicate) = session.request(put(expires, 4)).await? else {
                bail!("carrier did not recover same custody")
            };
            ensure!(duplicate.signature == original.signature);
            let Reply::Stored(second) = session.request(put(expires, 5)).await? else { bail!("carrier Put failed") };
            second.verify(pin, [1;32], [5;32], &vec![9;9000], expires)?;
            ensure!(matches!(session.request(Request::Consume {
                mailbox:[1;32], read:[3;32], object:[4;32], digest: original.digest,
            }).await?, Reply::Consumed));
            // Contributor pause denies fresh obligations on BOTH paths while retained data remains.
            custody.set_contribution_enabled(false).await?;
            ensure!(matches!(session.request(put(expires, 6)).await?, Reply::Rejected(MailboxRejectionV1::Quota)));
            session.finish().await
        })).await?;
        ensure!(
            matches!(udp.request(fetch(3)).await?, Reply::Object { object, ciphertext, expires: ttl, .. }
            if object == [5;32] && ciphertext == vec![9;9000] && ttl == expires)
        );
        ensure!(matches!(
            udp.request(put(expires, 6)).await?,
            Reply::Rejected(MailboxRejectionV1::Quota)
        ));
        Ok::<(), anyhow::Error>(())
    };
    let result = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(20), client) => result?,
        result = &mut server => { result?; bail!("borrowed owner ended early") }
    };
    stop.store(true, Ordering::Release);
    let cleanup = tokio::time::timeout(Duration::from_secs(5), &mut server).await?;
    drop(server);
    peer.close().await?;
    // Ended borrowed service does not close the endpoint. Its actual owner does.
    let after = owner.snapshot();
    ensure!(after.instance == before.instance && after.binds_completed == before.binds_completed);
    ensure!(after.active_operations == 0 && !after.closing);
    owner.close().await?;
    result?;
    if let Err(error) = cleanup {
        ensure!(
            error.to_string() == "service stopped by owner",
            "unexpected cleanup: {error:#}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn borrowed_l4_native_rejects_mixed_identity_and_stop_before_activation() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let owner = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
    for (native_key, mailbox_key, shared_stop, expected) in [
        (
            17,
            17,
            false,
            "native listener requires original owner stop",
        ),
        (18, 17, true, "native node signer mismatch"),
        (17, 18, true, "mailbox node signer mismatch"),
    ] {
        let stop = Arc::new(AtomicBool::new(false));
        let listener = NativeMailboxListenerV7::bind(
            "127.0.0.1:0".parse()?,
            "127.0.0.1:0".parse()?,
            key(native_key),
            if shared_stop {
                stop.clone()
            } else {
                Arc::new(AtomicBool::new(false))
            },
            2,
        )?;
        let service = Arc::new(MailboxServiceV1::new(
            MailboxStore::open_contributor(
                dir.path()
                    .join(format!("{native_key}-{mailbox_key}-{shared_stop}.db")),
                Default::default(),
                true,
            )?,
            key(mailbox_key),
        ));
        let error = run_self_owned_mailbox_on_endpoint_with_native_v7(
            &owner,
            key(17),
            stop,
            service,
            Some(listener),
            |_, _| panic!("invalid owner must not activate"),
            |_| panic!("invalid owner must not publish"),
        )
        .await
        .unwrap_err();
        ensure!(error.to_string() == expected, "unexpected error: {error:#}");
    }
    owner.close().await?;
    Ok(())
}
