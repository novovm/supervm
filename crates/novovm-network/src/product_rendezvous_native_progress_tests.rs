//! A complete, authenticated request must not wait for an idle network timer
//! merely because bounded hash work needs another owner turn.
use super::*;
use crate::{
    mailbox_rpc::MailboxRequestV1,
    mailbox_store::MailboxStore,
    mailbox_structured::native_session::NativeMailboxClient,
    novorudp::{endpoint::NativePeer, fragments::UDP_CHUNK_BYTES},
    service_concurrency::AdmissionKind,
};

#[tokio::test(flavor = "current_thread")]
async fn drained_network_hash_progress_starts_real_work_without_idle_timer() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let server_key = SigningKey::from_bytes(&[61; 32]);
    let client_key = SigningKey::from_bytes(&[62; 32]);
    let server_socket = UdpSocket::bind("127.0.0.1:0")?;
    let client_socket = UdpSocket::bind("127.0.0.1:0")?;
    let server_address = server_socket.local_addr()?;
    let client_address = client_socket.local_addr()?;
    let peek_socket = server_socket.try_clone()?;
    let readiness_socket = server_socket.try_clone()?;
    let now = Instant::now();
    let endpoint = |socket, identity, key, address| {
        NativeEndpoint::bind(
            socket,
            identity,
            vec![NativePeer { key, address }],
            NativeHandshakeBudget {
                lifetime: REQUEST_DEADLINE,
                retry: Duration::from_millis(50),
            },
            now_ms()?,
            now,
        )
    };
    let mut server_link = mailbox_link(
        endpoint(
            server_socket,
            server_key.clone(),
            client_key.verifying_key().to_bytes(),
            client_address,
        )?,
        false,
    )?;
    let mut client_link = mailbox_link(
        endpoint(
            client_socket,
            client_key,
            server_key.verifying_key().to_bytes(),
            server_address,
        )?,
        false,
    )?;
    // This timeout guards fixture setup only, not the behavior under test.
    tokio::time::timeout(Duration::from_secs(5), async {
        while server_link.session_id(0).is_none() || client_link.session_id(0).is_none() {
            let _ = client_link.poll(now_ms()?, Instant::now(), 64)?;
            let _ = server_link.poll(now_ms()?, Instant::now(), 64)?;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;

    let mut store = MailboxStore::open_contributor(
        directory.path().join("hash-progress.db"),
        Default::default(),
        true,
    )?;
    store.set_resources_available(true);
    let grant = store.create((now_ms()? / 1000) as i64, 600)?;
    let service = MailboxServiceV1::new(store, server_key.clone());
    service.set_resources_available(true).await?;
    let mut client =
        NativeMailboxClient::attach(&mut client_link, 0, server_key.verifying_key().to_bytes())?;
    let request = client.begin(
        0,
        vec![MailboxRequestV1::Put {
            mailbox: grant.id,
            write: *grant.write.as_bytes(),
            object: [63; 32],
            ciphertext: vec![64; 32 * 1024],
            expires: (now_ms()? / 1000) as i64 + 300,
        }],
        Instant::now(),
    )?;
    ensure!(client.try_send(&mut client_link, request, Instant::now(), || Ok(()))?);
    // Transport polling authenticates/reassembles every fragment but does not
    // call poll_complete: the real owner must perform all hash/dispatch work.
    let mut accepted = 0usize;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let _ = client_link.poll(now_ms()?, Instant::now(), 64)?;
            accepted += server_link
                .poll(now_ms()?, Instant::now(), 64)?
                .accepted_fragments;
            let bytes = server_link.reserved_bytes(STREAM)?;
            if bytes > 0 && accepted == bytes.div_ceil(UDP_CHUNK_BYTES) {
                ensure!(accepted > QUANTUM, "fixture must require another hash turn");
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    let quiet = server_link.poll(now_ms()?, Instant::now(), 64)?;
    ensure!(quiet.accepted_fragments == 0 && quiet.data_sent == 0 && quiet.feedback_sent == 0);
    ensure!(server_link.pending_messages(STREAM)? == 1);
    let mut peek = [0u8; 1];
    ensure!(
        matches!(peek_socket.peek_from(&mut peek), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
    );

    let stop = AtomicBool::new(false);
    let admission = AdaptiveAdmissionV1::new(1, None, AdmissionKind::Inbound, 1);
    let readiness = Readiness::Udp(tokio::net::UdpSocket::from_std(readiness_socket)?);
    // Start with a fresh cooperative budget; no timer is advanced or awaited
    // between this single poll of the production owner and the observation.
    tokio::task::yield_now().await;
    let mut owner = Box::pin(serve_link(
        server_link,
        readiness,
        &service,
        &stop,
        Admission::Udp(admission),
        None,
        || Ok(()),
    ));
    assert!(futures_util::poll!(owner.as_mut()).is_pending());
    let timings = serde_json::to_value(service.timing_snapshot_v1())?;
    drop(owner);
    // Drain an already-started SQLite worker before the temporary store closes.
    service.set_resources_available(false).await?;
    assert_eq!(
        timings["put"]["started"], 1,
        "complete authenticated input was parked instead of advancing its hash and starting Put"
    );
    Ok(())
}
