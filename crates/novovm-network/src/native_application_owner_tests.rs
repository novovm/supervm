use super::*;
use crate::{
    mailbox_rpc::MailboxRequestV1,
    mailbox_store::MailboxStore,
    mailbox_structured::native_session::NativeMailboxClient,
    native_application::{
        ApplicationFutureV1, AuthenticatedApplicationPeerV1, NativeApplicationClientV1,
        NativeApplicationHandlerV1,
    },
    novorudp::endpoint::NativePeer,
    service_concurrency::AdmissionKind,
};
use std::sync::atomic::AtomicUsize;
const APP: u64 = u64::from_be_bytes(*b"NVASAS01");
const APP_DOMAIN: [u8; 32] = [86; 32];
struct SlowApplication(Arc<AtomicUsize>, Arc<tokio::sync::Notify>);
impl NativeApplicationHandlerV1 for SlowApplication {
    fn request(&self, peer: AuthenticatedApplicationPeerV1, input: Vec<u8>) -> ApplicationFutureV1 {
        let calls = self.0.clone();
        let gate = self.1.clone();
        Box::pin(async move {
            ensure!(peer.is_current(), "retired test peer");
            calls.fetch_add(1, Ordering::SeqCst);
            gate.notified().await;
            Ok(input)
        })
    }
}
#[tokio::test(flavor = "current_thread")]
async fn application_and_mailbox_share_the_real_listener_without_blocking_each_other() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let server_key = crate::test_identity::key(87);
    let client_key = crate::test_identity::key(88);
    let stop = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(tokio::sync::Notify::new());
    let controller = ServiceConcurrencyV1::new(crate::service_concurrency::ResourceSnapshotV1 {
        cpu_parallelism: 8,
        available_memory_bytes: 8 * 1024 * 1024 * 1024,
        pressure: crate::service_concurrency::ResourcePressureV1::Normal,
    });
    let route = NativeApplicationRouteV1 {
        stream: APP,
        domain: APP_DOMAIN,
        handler: Arc::new(SlowApplication(calls.clone(), gate.clone())),
    };
    let listener = NativeMailboxListenerV7::bind_adaptive_with_application(
        "127.0.0.1:0".parse()?,
        "127.0.0.1:0".parse()?,
        server_key.clone(),
        stop.clone(),
        (Some(4), Some(64 * 1024 * 1024)),
        controller.clone(),
        route.clone(),
    )?;
    let server_address = listener.route().address;
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let endpoint = NativeEndpoint::connect_clients(
        socket,
        client_key,
        vec![NativePeer {
            key: server_key.public_key().to_vec(),
            address: server_address,
        }],
        NativeHandshakeBudget {
            lifetime: REQUEST_DEADLINE,
            retry: Duration::from_millis(20),
        },
        now_ms()?,
        Instant::now(),
    )?;
    let mut client_link = mailbox_link_with_application(endpoint, false, None, None, Some(&route))?;
    let mut store = MailboxStore::open_contributor(
        directory.path().join("mailbox.db"),
        Default::default(),
        true,
    )?;
    store.set_resources_available(true);
    let grant = store.create((now_ms()? / 1000) as i64, 600)?;
    let service = MailboxServiceV1::new(store, server_key.clone());
    service.set_resources_available(true).await?;
    let client_run = async {
        while client_link.session_id(0).is_none() {
            let _ = client_link.poll(now_ms()?, Instant::now(), 64)?;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let mut app = NativeApplicationClientV1::attach(&mut client_link, 0, APP, APP_DOMAIN)?;
        app.begin(b"instruction from B", Instant::now())?;
        let mut mailbox =
            NativeMailboxClient::attach(&mut client_link, 0, server_key.public_key().to_vec())?;
        let request = mailbox.begin(
            0,
            vec![MailboxRequestV1::Put {
                mailbox: grant.id,
                write: *grant.write.as_bytes(),
                object: [89; 32],
                ciphertext: vec![90; 4096],
                expires: (now_ms()? / 1000) as i64 + 300,
            }],
            Instant::now(),
        )?;
        let mut app_done = false;
        let mut mailbox_done = false;
        loop {
            let now = Instant::now();
            let _ = client_link.poll(now_ms()?, now, 64)?;
            if !mailbox_done {
                mailbox.try_send(&mut client_link, request, now, || Ok(()))?;
                if let Some(body) = client_link.poll_complete(STREAM, now, 32)? {
                    let (_, reply) = mailbox.receive(&client_link, body, now, || Ok(()))?;
                    ensure!(reply.len() == 1, "mailbox response missing");
                    mailbox_done = true;
                    gate.notify_one();
                }
            }
            if !app_done {
                if let Some(reply) = app.poll(&mut client_link, now)? {
                    ensure!(
                        reply == b"instruction from B",
                        "application response mismatch"
                    );
                    app_done = true;
                }
            }
            if app_done && mailbox_done {
                stop.store(true, Ordering::Release);
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    };
    let server_run = serve_link_with_application(
        listener.link,
        Readiness::Udp(listener.readiness),
        &service,
        &stop,
        Admission::Udp {
            slots: AdaptiveAdmissionV1::new(4, Some(controller.clone()), AdmissionKind::Inbound, 4),
            wire_ceiling: None,
        },
        Some(controller.clone()),
        || Ok(()),
        listener.application,
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::try_join!(client_run, server_run)
    })
    .await
    .context(format!(
        "shared owner deadline; application calls={}",
        calls.load(Ordering::SeqCst)
    ))??;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(controller.snapshot().active_work, 0);
    assert_eq!(controller.snapshot().reserved_bytes, 0);
    assert!(controller.snapshot().completed_work >= 2);
    Ok(())
}
