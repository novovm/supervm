//! Real native UDP/SQLite evidence correlation, not Android or delivery proof.
use super::*;
use novovm_network::novorudp::diagnostics::NativeTransferDiagnosticsV1;

fn correlated_response(
    server: &NativeTransferDiagnosticsV1,
    client: &NativeTransferDiagnosticsV1,
    request_tag: [u8; 32],
) -> Result<()> {
    let server = server.snapshot().context("server observer unavailable")?;
    let request = server
        .objects
        .iter()
        .find(|record| record.tag == request_tag)
        .context("authenticated request evidence missing")?;
    ensure!(request.fragments > 0 && request.hash_chunks > 0);
    ensure!(
        request.verified_us.is_some()
            && request.work_entered_us.is_some()
            && request.work_finished_us.is_some()
            && request.response_queued_us.is_some(),
        "request lifecycle evidence incomplete"
    );
    let response_tag = request
        .response_tag
        .context("response association missing")?;
    let client = client.snapshot().context("client observer unavailable")?;
    ensure!(
        client
            .objects
            .iter()
            .any(|record| { record.tag == response_tag && record.response_validated_us.is_some() }),
        "correlated response was not validated by the client"
    );
    // Each observer has its own monotonic origin. Association uses tags only;
    // timestamps from the two snapshots are never subtracted or ordered.
    Ok(())
}

#[tokio::test]
async fn native_evidence_correlates_verified_io_work_and_reply_without_claiming_commit(
) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let stop = Arc::new(AtomicBool::new(false));
    let listener = NativeMailboxListenerV7::bind(
        "127.0.0.1:0".parse()?,
        "127.0.0.1:0".parse()?,
        key(17),
        stop.clone(),
        4,
    )?;
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open_contributor(
            directory.path().join("evidence.db"),
            Default::default(),
            true,
        )?,
        key(17),
    ));
    service.set_resources_available(true).await?;
    // Enable before the production listener borrows its transport.
    let server_evidence = service.enable_native_diagnostics_v1();
    let journal = directory.path().join("authority.bin");
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let server = run_self_owned_native_mailbox_v7(
        listener,
        key(17),
        stop.clone(),
        service.clone(),
        move |route, until| {
            let at = now_ms()?;
            let record = sign_relay_record_v1(
                &key(17),
                "native-evidence-fixture",
                vec![route.endpoint()?],
                at,
                until,
                1,
            )?;
            let mut book = ServiceLeaseBookV1::new_self_owned(trust(), vec![record], at)?;
            let checkpoint = book.checkpoint()?;
            let file = ServiceLeaseFileV1::create(&journal, Zeroizing::new([91; 32]), &checkpoint)?;
            book.attach_persistence(file, &checkpoint)?;
            Ok(Arc::new(ServiceLeaseHostV1::new(key(17), book)?))
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
        let route =
            NativeMailboxEndpointV7::from_record(&record)?.context("native route missing")?;
        let mut client = Client::connect(&route, 84).await?;
        let client_evidence = Arc::new(NativeTransferDiagnosticsV1::new());
        client.link.set_diagnostics_v1(client_evidence.clone());
        let expires = (now_ms()? / 1000) as i64 + 300;
        let register = client.send(
            0,
            Request::Register {
                mailbox: [81; 32],
                write: [82; 32],
                read: [83; 32],
                expires,
            },
        )?;
        let register_tag = client
            .session
            .request_transfer_tag_v1(register)
            .context("register tag")?;
        ensure!(matches!(client.until(register).await?, Reply::Registered));
        correlated_response(&server_evidence, &client_evidence, register_tag)?;

        let ciphertext = vec![85; 32 * 1024];
        let put = client.send(
            0,
            Request::Put {
                mailbox: [81; 32],
                write: [82; 32],
                object: [86; 32],
                ciphertext: ciphertext.clone(),
                expires,
            },
        )?;
        let put_tag = client
            .session
            .request_transfer_tag_v1(put)
            .context("Put tag")?;
        let Reply::Stored(receipt) = client.until(put).await? else {
            bail!("authenticated Put did not return a StoreReceipt")
        };
        receipt.verify(
            key(17).verifying_key().to_bytes(),
            [81; 32],
            [86; 32],
            &ciphertext,
            expires,
        )?;
        correlated_response(&server_evidence, &client_evidence, put_tag)?;
        ensure!(server_evidence
            .snapshot()
            .unwrap()
            .objects
            .iter()
            .any(|r| r.tag == put_tag && r.stored_receipt_created_us.is_some()));
        let before = service.contribution_status().await?;

        let rejected = client.send(
            0,
            Request::Put {
                mailbox: [81; 32],
                write: [99; 32],
                object: [87; 32],
                ciphertext: vec![88; 1024],
                expires,
            },
        )?;
        let rejected_tag = client
            .session
            .request_transfer_tag_v1(rejected)
            .context("rejected Put tag")?;
        ensure!(matches!(
            client.until(rejected).await?,
            Reply::Rejected(MailboxRejectionV1::Unauthorized)
        ));
        // WorkFinished/ResponseValidated also occur for a legitimate rejection:
        // neither stage means that custody was created or a commit succeeded.
        correlated_response(&server_evidence, &client_evidence, rejected_tag)?;
        ensure!(server_evidence
            .snapshot()
            .unwrap()
            .objects
            .iter()
            .any(|r| r.tag == rejected_tag && r.stored_receipt_created_us.is_none()));
        let after = service.contribution_status().await?;
        ensure!(
            before.retained_objects == after.retained_objects
                && before.retained_bytes == after.retained_bytes
        );
        ensure!(
            matches!(client.request(Request::Fetch { mailbox: [81;32], read: [83;32] }).await?,
            Reply::Object { object, ciphertext: bytes, .. } if object == [86;32] && bytes == ciphertext)
        );
        Ok::<_, anyhow::Error>(())
    };
    tokio::pin!(server);
    let result = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(25), client) => result?,
        result = &mut server => { result?; bail!("native owner ended before evidence check") }
    };
    stop.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(5), &mut server).await??;
    result
}
