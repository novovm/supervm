//! Same native protocol and node owner on actual TCP/TLS, no QUIC client.
use super::*;
use novovm_network::{
    product_rendezvous_owned::{run_self_owned_node_with_tls_v1, TlsMailboxListenerV1},
    product_service_lease::TlsServiceEndpointV1,
    tls_transport::TlsScopeV1,
};

#[tokio::test]
async fn native_tls_custody_survives_restart_and_rejects_withdrawal() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let expires = (now_ms()? / 1000) as i64 + 600;
    let receipt = round_tls(directory.path(), expires, true, None)
        .await?
        .context("receipt")?;
    round_tls(directory.path(), expires, true, Some(receipt)).await?;
    Ok(())
}

#[tokio::test]
async fn legacy_tls_does_not_implicitly_enable_native_business() -> Result<()> {
    let directory = tempfile::tempdir()?;
    round_tls(
        directory.path(),
        (now_ms()? / 1000) as i64 + 600,
        false,
        None,
    )
    .await?;
    Ok(())
}

async fn round_tls(
    root: &Path,
    expires: i64,
    declared: bool,
    original: Option<StoreReceiptV1>,
) -> Result<Option<StoreReceiptV1>> {
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open(root.join("custody.sqlite"), Default::default())?,
        key(17),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    let guard = Stop(stop.clone());
    let listener = TlsMailboxListenerV1::bind_with_capacity(
        "127.0.0.1:0".parse()?,
        "127.0.0.1:0".parse()?,
        Zeroizing::new([119; 32]),
        stop.clone(),
        8,
    )
    .await?;
    let listener = if declared {
        listener.with_native_mailbox_v8()
    } else {
        listener
    };
    let captured = Arc::new(Mutex::new(None));
    let activated = captured.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let history = root.join("authority.bin");
    let server = run_self_owned_node_with_tls_v1(
        config()?.with_owned_service_identity(Zeroizing::new([90; 32]))?,
        key(17),
        [key(83).verifying_key().to_bytes()].into(),
        stop,
        service.clone(),
        listener,
        move |endpoint, until| {
            let at = now_ms()?;
            let book = if history.exists() {
                let (file, bytes) = ServiceLeaseFileV1::open(&history, Zeroizing::new([91; 32]))?;
                let mut book = ServiceLeaseBookV1::restore_self_owned(trust(), &bytes, at)?;
                book.attach_persistence(file, &bytes)?;
                book
            } else {
                let record = sign_relay_record_v1(
                    &key(17),
                    "native-tls-fixture",
                    vec![endpoint.endpoint()?],
                    at,
                    until,
                    1,
                )?;
                let mut book = ServiceLeaseBookV1::new_self_owned(trust(), vec![record], at)?;
                let bytes = book.checkpoint()?;
                let file = ServiceLeaseFileV1::create(&history, Zeroizing::new([91; 32]), &bytes)?;
                book.attach_persistence(file, &bytes)?;
                book
            };
            let host = Arc::new(ServiceLeaseHostV1::new(key(17), book)?);
            host.rotate_self_owned(endpoint, at, until)?;
            host.rotate_native_carrier_mailbox_v8(true, at, until)?;
            *activated.lock().unwrap() = Some(host.clone());
            Ok(host)
        },
        move |record| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send((
                    record.clone(),
                    captured.lock().unwrap().clone().context("host")?,
                ));
            }
            Ok(())
        },
    );
    let client = async {
        let _guard = guard;
        let (record, host) = rx.await?;
        let mut book =
            ServiceLeaseBookV1::new_self_owned(trust(), vec![record.clone()], now_ms()?)?;
        let route = TlsServiceEndpointV1::from_record(&record)?.context("TLS missing")?;
        ensure!(route.native_mailbox_v8 == declared);
        book.check_tls_route_v1(pin(), &route, now_ms()?)?;
        let scope = TlsScopeV1::new_continuous_v4(|| Ok(()))?;
        // TLS transport pin is checked before the independent service identity.
        let wrong = tokio::net::TcpStream::connect(route.address).await?;
        ensure!(
            scope
                .connect(wrong, key(99).verifying_key().to_bytes())
                .await
                .is_err(),
            "wrong TLS identity accepted"
        );
        let tcp = tokio::net::TcpStream::connect(route.address).await?;
        tcp.set_nodelay(true)?;
        let mut stream = CarrierStreamV1::from(scope.connect(tcp, route.endpoint_key).await?);
        if !declared {
            ensure!(book
                .check_continuous_native_tls_v8(pin(), &route, now_ms()?)
                .is_err());
            ensure!(
                request_mailbox_native_v8(&mut stream, Instant::now() + Duration::from_secs(3))
                    .await
                    .is_err(),
                "sibling QUIC native capability enabled legacy TLS"
            );
            return Ok(None);
        }
        book.check_continuous_native_tls_v8(pin(), &route, now_ms()?)?;
        let mut client = Client::attach(&mut stream, true).await?;
        let current = verified_record(&mut client, &mut book, true).await?;
        ensure!(TlsServiceEndpointV1::from_record(&current)?.as_ref() == Some(&route));
        if let Some(receipt) = original {
            ensure!(matches!(
                client.request(fetch(99)).await?,
                Reply::Rejected(MailboxRejectionV1::Unauthorized)
            ));
            ensure!(
                matches!(client.request(fetch(3)).await?, Reply::Object { ciphertext, expires: actual, .. }
                if ciphertext == vec![9; 9000] && actual == expires)
            );
            let Reply::Stored(duplicate) = client.request(put(expires, 4)).await? else {
                bail!("duplicate")
            };
            duplicate.verify(pin(), [1; 32], [4; 32], &vec![9; 9000], expires)?;
            ensure!(
                duplicate.signature == receipt.signature,
                "restart changed receipt"
            );
            let at = now_ms()?;
            host.rotate_native_carrier_mailbox_v8(false, at, at + 90_000)?;
            ensure!(host
                .check_continuous_native_tls_v8(&route, now_ms()?)
                .is_err());
            let result =
                tokio::time::timeout(Duration::from_secs(3), client.request(put(expires, 5)))
                    .await?;
            ensure!(
                matches!(result, Err(_) | Ok(Reply::Rejected(_))),
                "withdrawn TLS accepted Put"
            );
            // Inspect original store, not just socket status; original ciphertext remains.
            ensure!(
                matches!(service.request_local_v1(Request::Fetch { mailbox: [1;32], read: [3;32] }).await?,
                Reply::Object { object, .. } if object == [4;32])
            );
            ensure!(
                service.contribution_status().await?.retained_objects == 1,
                "withdrawn Put mutated custody"
            );
            return Ok(Some(receipt));
        }
        ensure!(matches!(
            client
                .request(Request::Register {
                    mailbox: [1; 32],
                    write: [2; 32],
                    read: [3; 32],
                    expires,
                })
                .await?,
            Reply::Registered
        ));
        let query = |after, wait_ms| Request::WatchFetchV6 {
            boxes: vec![MailboxWatchV1 {
                mailbox: [1; 32],
                read: [3; 32],
                after,
            }],
            wait_ms,
        };
        let Reply::NoticeObjectV6 {
            notices,
            object: None,
        } = client.request(query(None, 0)).await?
        else {
            bail!("initial catchup")
        };
        let watch = client.send(1, query(Some(notices[0].cursor), 10_000))?;
        client.idle(Duration::from_millis(40)).await?;
        ensure!(client.received.is_empty());
        let started = Instant::now();
        let Reply::Stored(receipt) = client.request(put(expires, 4)).await? else {
            bail!("store")
        };
        receipt.verify(pin(), [1; 32], [4; 32], &vec![9; 9000], expires)?;
        ensure!(
            matches!(client.until(watch).await?, Reply::NoticeObjectV6 {object:Some(row), ..}
            if row.object == [4;32] && row.expires == expires)
        );
        ensure!(
            started.elapsed() < Duration::from_secs(5),
            "TLS Put blocked on Watch"
        );
        eprintln!(
            "native TLS Put+Notify {} ms; no QUIC client",
            started.elapsed().as_millis()
        );
        Ok(Some(receipt))
    };
    let (server, client) = tokio::time::timeout(Duration::from_secs(25), async {
        tokio::join!(server, client)
    })
    .await?;
    let result = client?;
    if let Err(error) = server {
        ensure!(
            error.to_string() == "service stopped by owner",
            "node: {error:#}"
        );
    }
    Ok(result)
}
