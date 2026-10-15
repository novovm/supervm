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
    async fn connect(route: &NativeMailboxEndpointV7, identity: u8) -> Result<Self> {
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
