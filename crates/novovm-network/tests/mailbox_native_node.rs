#![cfg(feature = "iroh-transport")]
//! Real UDP listener and original durable mailbox/authority owners. Loopback
//! fixtures do not prove Android wiring, public reachability or device delivery.
use anyhow::{bail, ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    mailbox_receipt::StoreReceiptV1,
    mailbox_rpc::{
        MailboxRejectionV1, MailboxReplyV1 as Reply, MailboxRequestV1 as Request, MailboxServiceV1,
        MailboxWatchV1,
    },
    mailbox_store::MailboxStore,
    mailbox_structured::{native_session::*, LIMITS, STREAM},
    novorudp::{
        endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
        fragments::ReassemblyLimits,
        object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    },
    product_directory::sign_relay_record_v1,
    product_rendezvous_daemon::now_ms,
    product_rendezvous_owned::{run_self_owned_native_mailbox_v7, NativeMailboxListenerV7},
    product_service_lease::{
        NativeMailboxEndpointV7, SelfOwnedTrustV1, ServiceLeaseBookV1, ServiceLeaseFileV1,
        ServiceLeaseHostV1,
    },
};
use std::{
    net::UdpSocket,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

fn key(value: u8) -> SigningKey {
    SigningKey::from_bytes(&[value; 32])
}
fn trust() -> SelfOwnedTrustV1 {
    SelfOwnedTrustV1 {
        version: 1,
        node_pins: [key(17).verifying_key().to_bytes()].into(),
    }
}
fn lane() -> NativeLaneConfig {
    NativeLaneConfig {
        stream: STREAM,
        domain: domain(),
        receive: ReassemblyLimits {
            max_message_bytes: LIMITS.wire_bytes,
            messages: 4,
            bytes: LIMITS.wire_bytes * 4,
            peer_messages: 4,
            peer_bytes: LIMITS.wire_bytes * 4,
            ttl: Duration::from_secs(10),
        },
        send: NativeSendLimits {
            messages: 4,
            bytes: LIMITS.wire_bytes * 4,
            peer_messages: 4,
            peer_bytes: LIMITS.wire_bytes * 4,
        },
    }
}

/// Only the test client uses a 1 ms bounded pump. The server is the borrowed
/// production listener future, never the old fixture's hand-written executor.
struct Client {
    link: NativeObjectLink,
    session: NativeMailboxClient,
    received: Vec<(RequestId, Vec<Reply>)>,
}
impl Client {
    async fn connect_link(
        route: &NativeMailboxEndpointV7,
        identity: u8,
    ) -> Result<NativeObjectLink> {
        let pin = key(17).verifying_key().to_bytes();
        let mut link = NativeObjectLink::new(
            NativeEndpoint::connect_client(
                UdpSocket::bind("127.0.0.1:0")?,
                key(identity),
                NativePeer {
                    key: pin,
                    address: route.address,
                },
                NativeHandshakeBudget {
                    lifetime: Duration::from_secs(5),
                    retry: Duration::from_millis(50),
                },
                now_ms()?,
                Instant::now(),
            )?,
            vec![lane()],
            NativeSendPolicy {
                window: NovoRudpWindowConfig {
                    batch_pause_ms: 0,
                    ack_wait_ms: 100,
                    packet_copies: 1,
                    tail_packet_copies: 1,
                    ..Default::default()
                },
                pacing: NovoRudpPacingProfile {
                    packet_copies: 1,
                    tail_packet_copies: 1,
                    batch_size: 32,
                    batch_pause_ms: 0,
                    ack_wait_ms: 100,
                    no_progress_backoff: false,
                },
                ttl: Duration::from_secs(10),
            },
        )?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while link.session_id(0).is_none() {
            ensure!(Instant::now() < deadline, "native node handshake timeout");
            let report = link.poll(now_ms()?, Instant::now(), 64)?;
            ensure!(
                report.session_changes.is_empty() && report.completed.is_empty(),
                "unexpected handshake generation/work"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        Ok(link)
    }
    async fn connect(route: &NativeMailboxEndpointV7, identity: u8) -> Result<Self> {
        let mut link = Self::connect_link(route, identity).await?;
        let pin = key(17).verifying_key().to_bytes();
        let session = NativeMailboxClient::attach(&mut link, 0, pin)?;
        Ok(Self {
            link,
            session,
            received: Vec::new(),
        })
    }
    fn send(&mut self, slot: usize, request: Request) -> Result<RequestId> {
        let id = self.session.begin(slot, vec![request], Instant::now())?;
        ensure!(
            self.session
                .try_send(&mut self.link, id, Instant::now(), || Ok(()))?,
            "test client output budget exhausted"
        );
        Ok(id)
    }
    async fn tick(&mut self) -> Result<()> {
        let now = Instant::now();
        for done in self.link.poll(now_ms()?, now, 64)?.completed {
            ensure!(
                self.session.transport_complete(&done).is_none(),
                "native request transport ended without mailbox result"
            );
        }
        while let Some(body) = self.link.poll_complete(STREAM, now, 64)? {
            self.received
                .push(self.session.receive(&self.link, body, now, || Ok(()))?);
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
        Ok(())
    }
    async fn until(&mut self, id: RequestId) -> Result<Reply> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(index) = self.received.iter().position(|(seen, _)| *seen == id) {
                let (_, mut replies) = self.received.remove(index);
                ensure!(replies.len() == 1, "unexpected native reply count");
                return Ok(replies.remove(0));
            }
            ensure!(Instant::now() < deadline, "native mailbox result timeout");
            self.tick().await?;
        }
    }
    async fn request(&mut self, request: Request) -> Result<Reply> {
        let id = self.send(0, request)?;
        self.until(id).await
    }
}

fn put(expires: i64, object: u8) -> Request {
    Request::Put {
        mailbox: [1; 32],
        write: [2; 32],
        object: [object; 32],
        ciphertext: vec![9; 9000],
        expires,
    }
}
fn fetch(read: u8) -> Request {
    Request::Fetch {
        mailbox: [1; 32],
        read: [read; 32],
    }
}

#[tokio::test]
async fn native_owner_multiplexes_watch_and_put_and_restores_custody() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let expires = (now_ms()? / 1000) as i64 + 600;
    let original = round(directory.path(), expires, None).await?;
    let recovered = round(directory.path(), expires, Some(&original)).await?;
    assert_eq!(original.signature, recovered.signature);
    Ok(())
}

async fn round(
    path: &Path,
    expires: i64,
    prior: Option<&StoreReceiptV1>,
) -> Result<StoreReceiptV1> {
    let stop = Arc::new(AtomicBool::new(false));
    let listener = NativeMailboxListenerV7::bind(
        "127.0.0.1:0".parse()?,
        "127.0.0.1:0".parse()?,
        key(17),
        stop.clone(),
        4,
    )?;
    let expected = listener.route().clone();
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open_contributor(path.join("mailbox.db"), Default::default(), true)?,
        key(17),
    ));
    if prior.is_none() {
        service.set_resources_available(true).await?;
    } else {
        // The durable OFF choice and runtime-unavailable initialization survive
        // process ownership turnover. This does not erase retained obligations.
        ensure!(!service.contribution_status().await?.enabled);
    }
    let journal = path.join("authority.bin");
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let server = run_self_owned_native_mailbox_v7(
        listener,
        key(17),
        stop.clone(),
        service.clone(),
        move |route, until| {
            let at = now_ms()?;
            let mut book = if journal.exists() {
                let (file, bytes) = ServiceLeaseFileV1::open(&journal, Zeroizing::new([91; 32]))?;
                let mut book = ServiceLeaseBookV1::restore_self_owned(trust(), &bytes, at)?;
                book.attach_persistence(file, &bytes)?;
                book
            } else {
                let record = sign_relay_record_v1(
                    &key(17),
                    "native-node-fixture",
                    vec![route.endpoint()?],
                    at,
                    until,
                    1,
                )?;
                let mut book = ServiceLeaseBookV1::new_self_owned(trust(), vec![record], at)?;
                let bytes = book.checkpoint()?;
                let file = ServiceLeaseFileV1::create(&journal, Zeroizing::new([91; 32]), &bytes)?;
                book.attach_persistence(file, &bytes)?;
                book
            };
            book.check_continuous_native_peer_v7(key(17).verifying_key().to_bytes(), at)?;
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
        let pin = key(17).verifying_key().to_bytes();
        let mut book =
            ServiceLeaseBookV1::new_self_owned(trust(), vec![record.clone()], now_ms()?)?;
        let route =
            NativeMailboxEndpointV7::from_record(&record)?.context("native route missing")?;
        ensure!(route == expected);
        book.check_native_mailbox_route_v7(pin, &route, now_ms()?)?;
        let mut receiver = Client::connect(&route, 83).await?;
        if let Some(original) = prior {
            // No sender connection is needed to recover the previously committed
            // ciphertext. Wrong capabilities still fail on the authenticated link.
            ensure!(matches!(
                receiver.request(fetch(99)).await?,
                Reply::Rejected(MailboxRejectionV1::Unauthorized)
            ));
            let Reply::Object {
                object,
                ciphertext,
                digest,
                expires: actual,
            } = receiver.request(fetch(3)).await?
            else {
                bail!("native restart lost retained object")
            };
            ensure!(object == [4; 32] && ciphertext == vec![9; 9000] && actual == expires);
            ensure!(digest == original.digest);
            ensure!(matches!(
                receiver.request(put(expires, 5)).await?,
                Reply::Rejected(MailboxRejectionV1::Quota)
            ));
            service.set_contribution_enabled(true).await?;
            service.set_resources_available(true).await?;
            let Reply::Stored(receipt) = receiver.request(put(expires, 4)).await? else {
                bail!("native duplicate Put did not recover receipt")
            };
            receipt.verify(pin, [1; 32], [4; 32], &ciphertext, expires)?;
            ensure!(receipt.signature == original.signature);
            ensure!(matches!(
                receiver
                    .request(Request::Consume {
                        mailbox: [1; 32],
                        read: [3; 32],
                        object,
                        digest,
                    })
                    .await?,
                Reply::Consumed
            ));
            ensure!(matches!(receiver.request(fetch(3)).await?, Reply::Empty));
            ensure!(matches!(
                receiver.request(put(expires, 4)).await?,
                Reply::Rejected(MailboxRejectionV1::Consumed)
            ));
            return Ok(receipt);
        }
        ensure!(matches!(
            receiver
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
        } = receiver.request(query(None, 0)).await?
        else {
            bail!("native initial snapshot")
        };
        let watch = receiver.send(1, query(Some(notices[0].cursor), 10_000))?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(40) {
            receiver.tick().await?;
        }
        ensure!(
            receiver.received.is_empty(),
            "Watch should still be waiting"
        );
        // A second long wait from this SAME authenticated peer must be rejected
        // without latching the request slot or holding all ordinary work slots.
        let excess = receiver.send(2, query(Some(notices[0].cursor), 10_000))?;
        ensure!(matches!(
            receiver.until(excess).await?,
            Reply::Rejected(MailboxRejectionV1::Quota)
        ));
        let ordinary = receiver.send(2, fetch(3))?;
        ensure!(ordinary.sequence == excess.sequence + 1);
        ensure!(matches!(receiver.until(ordinary).await?, Reply::Empty));
        let mut sender = Client::connect(&route, 84).await?;
        let second_watch = sender.send(1, query(Some(notices[0].cursor), 10_000))?;
        let second_started = Instant::now();
        while second_started.elapsed() < Duration::from_millis(40) {
            sender.tick().await?;
        }
        ensure!(sender.received.is_empty(), "second peer Watch should wait");
        // Two peers now hold the global waiting-Watch budget. A third peer is
        // admitted normally, but cannot consume another long-wait work slot.
        let mut third = Client::connect(&route, 85).await?;
        let third_watch = third.send(1, query(Some(notices[0].cursor), 10_000))?;
        ensure!(matches!(
            third.until(third_watch).await?,
            Reply::Rejected(MailboxRejectionV1::Quota)
        ));
        let third_fetch = third.send(1, fetch(3))?;
        ensure!(third_fetch.sequence == third_watch.sequence + 1);
        ensure!(matches!(third.until(third_fetch).await?, Reply::Empty));
        // Put on the SAME session as a waiting Watch must still run immediately;
        // both admitted observers are awakened by the original durable commit.
        let Reply::Stored(receipt) = sender.request(put(expires, 4)).await? else {
            bail!("native node did not return storage evidence")
        };
        receipt.verify(pin, [1; 32], [4; 32], &vec![9; 9000], expires)?;
        ensure!(matches!(
            sender.until(second_watch).await?,
            Reply::NoticeObjectV6 { object: Some(row), .. }
                if row.object == [4; 32] && row.ciphertext == vec![9; 9000]
                    && row.expires == expires
        ));
        let Reply::NoticeObjectV6 {
            object: Some(row), ..
        } = receiver.until(watch).await?
        else {
            bail!("native node did not notify retained object")
        };
        ensure!(row.object == [4; 32] && row.ciphertext == vec![9; 9000] && row.expires == expires);
        eprintln!(
            "native listener Watch + concurrent Put + Fetch elapsed_ms={}",
            start.elapsed().as_millis()
        );
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "Put serialized behind ten-second Watch"
        );
        // Closing contribution rejects NEW custody but the same existing secure
        // session continues to read already committed ciphertext.
        service.set_contribution_enabled(false).await?;
        ensure!(matches!(
            sender.request(put(expires, 5)).await?,
            Reply::Rejected(MailboxRejectionV1::Quota)
        ));
        ensure!(matches!(
            receiver.request(fetch(3)).await?,
            Reply::Object { .. }
        ));
        Ok::<_, anyhow::Error>(receipt)
    };
    tokio::pin!(server);
    let result = tokio::select! {
        outcome = tokio::time::timeout(Duration::from_secs(25), client) => outcome?,
        outcome = &mut server => { outcome?; bail!("native node ended before client") }
    };
    stop.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(5), &mut server).await??;
    result
}

/// The existing caller owns and polls ONE link; request handles only wake it.
/// This bounded test pump is not another production runtime or mobile scheduler.
async fn drive_requests(
    mut link: NativeObjectLink,
    mut driver: novovm_network::mailbox_structured::native_requests::NativeMailboxDriver,
) -> Result<()> {
    use novovm_network::mailbox_structured::native_requests::NativeRequestErrorKind;
    let wake = driver.wake();
    let session = link.session_id(0);
    loop {
        let now = Instant::now();
        driver.progress(&mut link, now)?;
        let report = link.poll(now_ms()?, now, 64)?;
        ensure!(
            link.session_id(0) == session,
            "async handle rebound the session"
        );
        if !report.session_changes.is_empty() {
            driver.invalidate(NativeRequestErrorKind::Session);
            bail!("test connection generation changed");
        }
        for done in report.completed {
            driver.transport_complete(&done);
        }
        while let Some(body) = link.poll_complete(STREAM, now, 64)? {
            driver.receive(&link, body, now)?;
        }
        driver.progress(&mut link, now)?;
        tokio::select! {
            _ = wake.notified() => {},
            _ = tokio::time::sleep(Duration::from_millis(1)) => {},
        }
    }
}

#[tokio::test]
async fn native_async_handles_share_one_session_without_watch_serialization() -> Result<()> {
    use novovm_network::mailbox_structured::native_requests::{
        NativeMailboxDriver, NativeRequestAuthority, NativeRequestErrorKind,
    };
    let directory = tempfile::tempdir()?;
    let expires = (now_ms()? / 1000) as i64 + 600;
    let stop = Arc::new(AtomicBool::new(false));
    let listener = NativeMailboxListenerV7::bind(
        "127.0.0.1:0".parse()?,
        "127.0.0.1:0".parse()?,
        key(17),
        stop.clone(),
        4,
    )?;
    let route = listener.route().clone();
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open_contributor(
            directory.path().join("async.db"),
            Default::default(),
            true,
        )?,
        key(17),
    ));
    service.set_resources_available(true).await?;
    let journal = directory.path().join("async-authority.bin");
    let server = run_self_owned_native_mailbox_v7(
        listener,
        key(17),
        stop.clone(),
        service,
        move |route, until| {
            let at = now_ms()?;
            let record = sign_relay_record_v1(
                &key(17),
                "native-async-fixture",
                vec![route.endpoint()?],
                at,
                until,
                1,
            )?;
            let mut book = ServiceLeaseBookV1::new_self_owned(trust(), vec![record], at)?;
            let bytes = book.checkpoint()?;
            let file = ServiceLeaseFileV1::create(&journal, Zeroizing::new([92; 32]), &bytes)?;
            book.attach_persistence(file, &bytes)?;
            Ok(Arc::new(ServiceLeaseHostV1::new(key(17), book)?))
        },
        |_| Ok(()),
    );
    let client = async {
        let pin = key(17).verifying_key().to_bytes();
        let mut stopped_link = Client::connect_link(&route, 87).await?;
        let (stopped_driver, stopped_handle) =
            NativeMailboxDriver::attach(&mut stopped_link, 0, pin)?;
        let abandoned = stopped_handle.request(
            0,
            vec![fetch(3)],
            Instant::now() + Duration::from_secs(5),
            Arc::new(|| Ok(())),
        );
        drop(stopped_driver);
        assert_eq!(
            abandoned
                .await
                .err()
                .context("expected typed request failure")?
                .kind,
            NativeRequestErrorKind::Stopped
        );
        drop(stopped_link);
        let mut rejected_link = Client::connect_link(&route, 88).await?;
        let (mut rejected_driver, rejected_handle) =
            NativeMailboxDriver::attach(&mut rejected_link, 0, pin)?;
        let rejected_pending = rejected_handle.request(
            0,
            vec![fetch(3)],
            Instant::now() + Duration::from_secs(5),
            Arc::new(|| Ok(())),
        );
        rejected_driver.invalidate(NativeRequestErrorKind::Protocol);
        drop(rejected_driver);
        assert_eq!(
            rejected_pending
                .await
                .err()
                .context("expected protocol stop")?
                .kind,
            NativeRequestErrorKind::Protocol
        );
        assert_eq!(
            rejected_handle
                .request(
                    1,
                    vec![fetch(3)],
                    Instant::now() + Duration::from_secs(5),
                    Arc::new(|| Ok(()))
                )
                .await
                .err()
                .context("expected retained protocol stop")?
                .kind,
            NativeRequestErrorKind::Protocol
        );
        drop(rejected_link);
        let mut cancel_link = Client::connect_link(&route, 89).await?;
        let (mut cancel_driver, cancel_handle) =
            NativeMailboxDriver::attach(&mut cancel_link, 0, pin)?;
        let unsent = cancel_handle.request(
            0,
            vec![fetch(3)],
            Instant::now() + Duration::from_secs(5),
            Arc::new(|| Ok(())),
        );
        cancel_driver.progress(&mut cancel_link, Instant::now())?;
        assert_eq!(cancel_link.outbound_usage(STREAM)?.0, 1);
        drop(unsent);
        // Deliberately no driver.progress: Drop must cancel the real retained
        // transport object, not merely a second local scheduler flag.
        let report = cancel_link.poll(now_ms()?, Instant::now(), 64)?;
        assert_eq!(report.data_sent, 0);
        assert!(report.completed.iter().any(|c| matches!(
            c.state,
            novovm_network::novorudp::object_sender::ObjectSendState::Cancelled
        )));
        for done in report.completed {
            cancel_driver.transport_complete(&done);
        }
        let partial = cancel_handle.request(
            1,
            vec![put(expires, 9)],
            Instant::now() + Duration::from_secs(5),
            Arc::new(|| Ok(())),
        );
        cancel_driver.progress(&mut cancel_link, Instant::now())?;
        let sent = cancel_link.poll(now_ms()?, Instant::now(), 1)?;
        assert!(sent.data_sent > 0);
        drop(partial);
        let stopped = cancel_link.poll(now_ms()?, Instant::now(), 64)?;
        assert_eq!(stopped.data_sent, 0);
        assert!(stopped.completed.iter().any(|c| matches!(
            c.state,
            novovm_network::novorudp::object_sender::ObjectSendState::Cancelled
        )));
        assert_eq!(cancel_link.outbound_usage(STREAM)?, (0, 0));
        drop(cancel_driver);
        drop(cancel_link);
        let mut link = Client::connect_link(&route, 86).await?;
        ensure!(link.session_id(0).is_some(), "native session missing");
        let (driver, handle) = NativeMailboxDriver::attach(&mut link, 0, pin)?;
        let check: NativeRequestAuthority = Arc::new(|| Ok(()));
        let until = || Instant::now() + Duration::from_secs(5);
        // The queue's caller deadline elapses even before the owner gets CPU;
        // it must not be restarted later when the socket driver is polled.
        let queued_timeout = handle.request(
            3,
            vec![fetch(3)],
            Instant::now() + Duration::from_millis(20),
            check.clone(),
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(
            queued_timeout
                .await
                .err()
                .context("expected queued deadline")?
                .kind,
            NativeRequestErrorKind::Deadline
        );
        // Admission is synchronous and bounded even if these futures are never polled.
        let queued = handle.request(3, vec![fetch(3)], until(), check.clone());
        let capacity = handle
            .request(3, vec![fetch(3)], until(), check.clone())
            .await
            .err()
            .context("expected typed request failure")?;
        assert_eq!(capacity.kind, NativeRequestErrorKind::Capacity);
        drop(queued); // no wire operation, so no server/client sequence consumption
        let first = handle.request(
            3,
            vec![Request::Register {
                mailbox: [1; 32],
                write: [2; 32],
                read: [3; 32],
                expires,
            }],
            until(),
            check.clone(),
        );
        let operations = async {
            assert!(matches!(first.await?.as_slice(), [Reply::Registered]));
            let initial = handle
                .request(
                    0,
                    vec![Request::WatchFetchV6 {
                        boxes: vec![MailboxWatchV1 {
                            mailbox: [1; 32],
                            read: [3; 32],
                            after: None,
                        }],
                        wait_ms: 0,
                    }],
                    until(),
                    check.clone(),
                )
                .await?;
            let Reply::NoticeObjectV6 {
                notices,
                object: None,
            } = &initial[0]
            else {
                bail!("missing original async cursor");
            };
            let start = Instant::now();
            let watch = handle.request(
                1,
                vec![Request::WatchFetchV6 {
                    boxes: vec![MailboxWatchV1 {
                        mailbox: [1; 32],
                        read: [3; 32],
                        after: Some(notices[0].cursor),
                    }],
                    wait_ms: 10_000,
                }],
                until(),
                check.clone(),
            );
            // Let the owner send the waiting watch first; Put must not await it.
            tokio::time::sleep(Duration::from_millis(40)).await;
            let stored = handle
                .request(2, vec![put(expires, 7)], until(), check.clone())
                .await?;
            let Reply::Stored(receipt) = &stored[0] else {
                bail!("no async storage receipt");
            };
            receipt.verify(pin, [1; 32], [7; 32], &vec![9; 9000], expires)?;
            let notified = watch.await?;
            assert!(
                matches!(&notified[0], Reply::NoticeObjectV6 { object: Some(row), .. }
                if row.object == [7; 32] && row.expires == expires && row.ciphertext == vec![9;9000])
            );
            ensure!(
                start.elapsed() < Duration::from_secs(5),
                "async handles serialized ten-second Watch"
            );
            // Independent business actions across successive request futures reuse
            // the same application/session slots; transport DONE never completes one.
            for _ in 0..3 {
                let result = handle
                    .request(0, vec![fetch(3)], until(), check.clone())
                    .await?;
                assert!(
                    matches!(&result[0], Reply::Object { object, expires: actual, .. }
                    if *object == [7;32] && *actual == expires)
                );
            }
            let consumed = handle
                .request(
                    0,
                    vec![Request::Consume {
                        mailbox: [1; 32],
                        read: [3; 32],
                        object: [7; 32],
                        digest: receipt.digest,
                    }],
                    until(),
                    check.clone(),
                )
                .await?;
            assert!(matches!(&consumed[0], Reply::Consumed));
            let snapshot = handle
                .request(
                    0,
                    vec![Request::Watch {
                        boxes: vec![MailboxWatchV1 {
                            mailbox: [1; 32],
                            read: [3; 32],
                            after: None,
                        }],
                        wait_ms: 0,
                    }],
                    until(),
                    check.clone(),
                )
                .await?;
            let Reply::Notice(empty) = &snapshot[0] else {
                bail!("no empty cursor");
            };
            assert!(!empty[0].pending);
            let cancelled = handle.request(
                1,
                vec![Request::Watch {
                    boxes: vec![MailboxWatchV1 {
                        mailbox: [1; 32],
                        read: [3; 32],
                        after: Some(empty[0].cursor),
                    }],
                    wait_ms: 10_000,
                }],
                until(),
                check.clone(),
            );
            tokio::time::sleep(Duration::from_millis(40)).await;
            drop(cancelled); // active request slot is retired, other slots remain valid
            tokio::time::sleep(Duration::from_millis(10)).await;
            let second = handle
                .request(2, vec![put(expires, 8)], until(), check.clone())
                .await?;
            assert!(matches!(&second[0], Reply::Stored(_)));
            // This Put releases the server's already admitted canceled Watch;
            // its late authenticated reply must not poison unrelated slots.
            tokio::time::sleep(Duration::from_millis(40)).await;
            assert!(
                matches!(&handle.request(0, vec![fetch(3)], until(), check.clone()).await?[0],
                Reply::Object {object, ..} if *object == [8;32])
            );
            // Invalid authority is not converted into a transient network error.
            let denied: NativeRequestAuthority = Arc::new(|| bail!("test caller revoked"));
            let rejected = handle
                .request(3, vec![fetch(3)], until(), denied)
                .await
                .err()
                .context("expected typed request failure")?;
            assert_eq!(rejected.kind, NativeRequestErrorKind::Authority);
            // A request already out of time cannot reset its deadline on admission.
            let late = handle
                .request(2, vec![fetch(3)], Instant::now(), check.clone())
                .await
                .err()
                .context("expected typed request failure")?;
            assert_eq!(late.kind, NativeRequestErrorKind::Deadline);
            Ok::<_, anyhow::Error>(())
        };
        tokio::select! {
            result = operations => result,
            result = drive_requests(link, driver) => { result?; bail!("native request owner ended") }
        }
    };
    tokio::pin!(server);
    let result = tokio::select! {
        outcome = tokio::time::timeout(Duration::from_secs(15), client) => outcome?,
        outcome = &mut server => { outcome?; bail!("native node ended before async client") }
    };
    stop.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(5), &mut server).await??;
    result
}
