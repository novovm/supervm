#![cfg(feature = "iroh-transport")]
//! Actual loopback QUIC and the production node dispatcher/store. This does not
//! prove Android wiring, NAT reachability, autonomous relay or message E2EE.
use anyhow::{bail, ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    bound_session::{
        authenticate_structured_bound_session_observed,
        peer_time::{
            authenticate_mailbox_native_v8, request_mailbox_native_v8, PeerTimeObservationV1,
        },
    },
    carrier_stream::CarrierStreamV1,
    duplex::HandshakeReplayCacheV1,
    iroh_transport::{IrohConfigV1, IrohEndpointOwnerV1},
    mailbox_receipt::StoreReceiptV1,
    mailbox_rpc::{
        MailboxRejectionV1, MailboxReplyV1 as Reply, MailboxRequestV1 as Request, MailboxServiceV1,
        MailboxWatchV1,
    },
    mailbox_store::MailboxStore,
    mailbox_structured::{native_session::*, LIMITS, STREAM},
    novorudp::{
        carrier_io::NativeCarrierPump,
        fragments::ReassemblyLimits,
        object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
        NovoRudpPacingProfile, NovoRudpWindowConfig,
    },
    product_directory::{sign_relay_record_v1, PeerSignedRelayRecordV1},
    product_rendezvous_daemon::now_ms,
    product_rendezvous_owned::run_self_owned_node_v1,
    product_service_lease::{
        request_mailbox_native_v7, IrohServiceEndpointV1, NativeCarrierMailboxEndpointV7,
        NativeCarrierMailboxEndpointV8, SelfOwnedTrustV1, ServiceLeaseBookV1, ServiceLeaseFileV1,
        ServiceLeaseHostV1,
    },
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn pin() -> [u8; 32] {
    key(17).verifying_key().to_bytes()
}
fn config() -> Result<IrohConfigV1> {
    IrohConfigV1::new(&[], "127.0.0.1".parse()?)?.with_bind_address("127.0.0.1:0".parse()?)
}
fn trust() -> SelfOwnedTrustV1 {
    SelfOwnedTrustV1 {
        version: 1,
        node_pins: [pin()].into(),
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
fn policy() -> NativeSendPolicy {
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
    }
}

/// A bounded fixture driver of the existing SDK, not a mock executor or another
/// production scheduler. The production node executes every business request.
struct Client<'a> {
    pump: NativeCarrierPump<'a>,
    link: NativeObjectLink,
    session: NativeMailboxClient,
    received: Vec<(RequestId, Vec<Reply>)>,
}
impl<'a> Client<'a> {
    async fn attach(stream: &'a mut CarrierStreamV1<'_>, v8: bool) -> Result<Self> {
        let channel = if v8 {
            let (channel, observed) = authenticate_mailbox_native_v8(
                stream,
                &key(83),
                pin(),
                Instant::now() + Duration::from_secs(5),
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?;
            ensure!(observed.peer_identity_public_key == pin());
            ensure!(
                matches!(observed.observation, PeerTimeObservationV1::Candidate(_)),
                "loopback clock observation missing"
            );
            channel
        } else {
            request_mailbox_native_v7(stream).await?;
            let channel = authenticate_structured_bound_session_observed(
                stream,
                &key(83),
                pin(),
                true,
                &mut HandshakeReplayCacheV1::new(8),
                &mut |_, _| Ok(()),
            )
            .await?;
            channel
        };
        let (endpoint, pump) =
            NativeCarrierPump::attach_continuous_v7(stream, channel, vec![STREAM])?;
        let mut link = NativeObjectLink::new(endpoint, vec![lane()], policy())?;
        let session = NativeMailboxClient::attach(&mut link, 0, pin())?;
        Ok(Self {
            pump,
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
            "fixture output exhausted"
        );
        Ok(id)
    }
    async fn tick(&mut self) -> Result<()> {
        let now = Instant::now();
        let report = self.link.poll(now_ms()?, now, 64)?;
        ensure!(
            report.session_changes.is_empty(),
            "session unexpectedly replaced"
        );
        for done in report.completed {
            ensure!(
                self.session.transport_complete(&done).is_none(),
                "transport ended before native business reply"
            );
        }
        while let Some(body) = self.link.poll_complete(STREAM, now, 64)? {
            ensure!(
                self.received.len() < SLOTS,
                "fixture reply budget exhausted"
            );
            self.received
                .push(self.session.receive(&self.link, body, now, || Ok(()))?);
        }
        tokio::select! {
            result = self.pump.progress() => result?,
            _ = tokio::time::sleep(Duration::from_millis(1)) => {},
        }
        Ok(())
    }
    async fn until(&mut self, id: RequestId) -> Result<Reply> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(index) = self.received.iter().position(|(seen, _)| *seen == id) {
                let (_, mut replies) = self.received.remove(index);
                ensure!(replies.len() == 1, "unexpected reply cardinality");
                return Ok(replies.remove(0));
            }
            ensure!(Instant::now() < deadline, "native carrier result timeout");
            self.tick().await?;
        }
    }
    async fn request(&mut self, request: Request) -> Result<Reply> {
        let id = self.send(0, request)?;
        self.until(id).await
    }
    async fn idle(&mut self, duration: Duration) -> Result<()> {
        let until = Instant::now() + duration;
        while Instant::now() < until {
            self.tick().await?;
        }
        Ok(())
    }
}
fn fetch(read: u8) -> Request {
    Request::Fetch {
        mailbox: [1; 32],
        read: [read; 32],
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
struct Stop(Arc<AtomicBool>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

enum Case {
    Store,
    Restore(StoreReceiptV1),
    Undeclared,
    Withdraw,
}

async fn verified_record(
    client: &mut Client<'_>,
    book: &mut ServiceLeaseBookV1,
    v8: bool,
) -> Result<PeerSignedRelayRecordV1> {
    // Metadata uses the original fourth logical slot on this same native
    // session, not a fixture shortcut into the node's authority owner.
    let request = client.send(3, Request::ServiceRecordV7)?;
    let Reply::ServiceRecordV7 { record } = client.until(request).await? else {
        bail!("native carrier omitted its signed service record")
    };
    if v8 {
        let endpoint =
            NativeCarrierMailboxEndpointV8::from_record(&record)?.context("V8 marker missing")?;
        book.accept_native_record_v8(pin(), record.clone(), now_ms()?)?;
        book.check_native_carrier_route_v8(pin(), &endpoint, now_ms()?)?;
    } else {
        let endpoint =
            NativeCarrierMailboxEndpointV7::from_record(&record)?.context("V7 marker missing")?;
        book.accept_native_record_v7(pin(), record.clone(), now_ms()?)?;
        book.check_native_carrier_route_v7(pin(), &endpoint, now_ms()?)?;
    }
    Ok(record)
}

#[tokio::test]
async fn native_carrier_owner_multiplexes_and_keeps_custody_across_idle_and_restart() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let expires = (now_ms()? / 1000) as i64 + 600;
    let receipt = round(directory.path(), expires, Case::Store, false)
        .await?
        .context("no original StoreReceipt")?;
    round(directory.path(), expires, Case::Restore(receipt), false).await?;
    Ok(())
}

#[tokio::test]
async fn native_carrier_requires_explicit_capability_and_rejects_withdrawn_live_service(
) -> Result<()> {
    for case in [Case::Undeclared, Case::Withdraw] {
        let directory = tempfile::tempdir()?;
        round(
            directory.path(),
            (now_ms()? / 1000) as i64 + 600,
            case,
            false,
        )
        .await?;
    }
    Ok(())
}

async fn round(path: &Path, expires: i64, case: Case, v8: bool) -> Result<Option<StoreReceiptV1>> {
    let enabled = !matches!(case, Case::Undeclared);
    let journal = path.join("authority.bin");
    let service = Arc::new(MailboxServiceV1::new(
        MailboxStore::open_contributor(path.join("mailbox.db"), Default::default(), true)?,
        key(17),
    ));
    service.set_resources_available(true).await?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_guard = Stop(stop.clone());
    let shared_host = Arc::new(Mutex::new(None));
    let activated = shared_host.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let server = run_self_owned_node_v1(
        config()?.with_owned_service_identity(Zeroizing::new([90; 32]))?,
        key(17),
        [key(83).verifying_key().to_bytes()].into(),
        stop,
        Some(service.clone()),
        move |endpoint, until| {
            let at = now_ms()?;
            let book = if journal.exists() {
                let (file, bytes) = ServiceLeaseFileV1::open(&journal, Zeroizing::new([91; 32]))?;
                let mut book = ServiceLeaseBookV1::restore_self_owned(trust(), &bytes, at)?;
                book.attach_persistence(file, &bytes)?;
                book
            } else {
                let record = sign_relay_record_v1(
                    &key(17),
                    "carrier-fixture",
                    vec![endpoint.endpoint()?],
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
            let host = Arc::new(ServiceLeaseHostV1::new(key(17), book)?);
            host.rotate_self_owned(endpoint, at, until)?;
            if v8 {
                host.rotate_native_carrier_mailbox_v8(enabled, at, until)?;
            } else {
                host.rotate_native_carrier_mailbox_v7(enabled, at, until)?;
            }
            *activated.lock().unwrap() = Some(host.clone());
            Ok(host)
        },
        move |record| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let host = shared_host
                    .lock()
                    .unwrap()
                    .clone()
                    .context("host not activated")?;
                let _ = tx.send((record.clone(), host));
            }
            Ok(())
        },
    );
    let client = async {
        let _stop = stop_guard;
        let (record, host) = rx.await?;
        // Original signed-book verification still happens before any network use.
        let mut book =
            ServiceLeaseBookV1::new_self_owned(trust(), vec![record.clone()], now_ms()?)?;
        let route = IrohServiceEndpointV1::from_record(&record)?;
        book.check_continuous_peer_v4(pin(), route.address.endpoint_key, now_ms()?)?;
        ensure!(
            carrier_route(&record, v8)?.is_some() == enabled,
            "node changed explicit capability unexpectedly"
        );
        if enabled {
            if v8 {
                book.check_native_carrier_route_v8(
                    pin(),
                    &NativeCarrierMailboxEndpointV8::from_record(&record)?.unwrap(),
                    now_ms()?,
                )?;
            } else {
                book.check_native_carrier_route_v7(
                    pin(),
                    &NativeCarrierMailboxEndpointV7::from_record(&record)?.unwrap(),
                    now_ms()?,
                )?;
            }
        }
        let owner = IrohEndpointOwnerV1::bind(config()?, || Ok(())).await?;
        let before = owner.snapshot();
        let result = owner.with_continuous_service_v4(|| Ok(()), move |scope| Box::pin(async move {
            let mut stream: CarrierStreamV1<'_> = scope.connect(&route.address).await?.into();
            if matches!(case, Case::Undeclared) {
                ensure!(if v8 { request_mailbox_native_v8(&mut stream, Instant::now() + Duration::from_secs(3)).await.is_err() } else { request_mailbox_native_v7(&mut stream).await.is_err() },
                    "undeclared native service accepted");
                return Ok(None);
            }
            let mut client = Client::attach(&mut stream, v8).await?;
            let first_record = verified_record(&mut client, &mut book, v8).await?;
            ensure!(first_record.sequence >= record.sequence,
                "service snapshot predates published locator");
            let first_route = carrier_route(&first_record, v8)?
                .context("initial native carrier locator missing")?;
            ensure!(first_route.address.endpoint_key == route.address.endpoint_key,
                "service record changed authenticated transport identity");
            if let Case::Restore(original) = case {
                ensure!(matches!(client.request(fetch(99)).await?,
                    Reply::Rejected(MailboxRejectionV1::Unauthorized)));
                let Reply::Object { object, ciphertext, digest, expires: actual } =
                    client.request(fetch(3)).await? else { bail!("restart lost ciphertext") };
                ensure!(object == [4; 32] && ciphertext == vec![9; 9000] && actual == expires
                    && digest == original.digest);
                let Reply::Stored(recovered) = client.request(put(expires, 4)).await?
                    else { bail!("duplicate Put lost original receipt") };
                recovered.verify(pin(), [1; 32], [4; 32], &ciphertext, expires)?;
                ensure!(recovered.signature == original.signature);
                ensure!(matches!(client.request(Request::Consume {
                    mailbox: [1; 32], read: [3; 32], object, digest,
                }).await?, Reply::Consumed));
                ensure!(matches!(client.request(fetch(3)).await?, Reply::Empty));
                let before = service.contribution_status().await?;
                let Reply::Stored(historical) = client.request(put(expires, 4)).await?
                    else { bail!("consumed Put lost historical evidence") };
                historical.verify(pin(), [1; 32], [4; 32], &ciphertext, expires)?;
                ensure!(historical.signature == original.signature);
                ensure!(matches!(client.request(fetch(3)).await?, Reply::Empty));
                let after = service.contribution_status().await?;
                ensure!(before.retained_objects == 0 && after.retained_objects == 0
                    && before.retained_bytes == 0 && after.retained_bytes == 0);
                return Ok(Some(recovered));
            }
            ensure!(matches!(client.request(Request::Register {
                mailbox: [1; 32], write: [2; 32], read: [3; 32], expires,
            }).await?, Reply::Registered));
            if matches!(case, Case::Withdraw) {
                let at = now_ms()?;
                if v8 {
                    host.rotate_native_carrier_mailbox_v8(false, at, at + 90_000)?;
                    ensure!(host.check_continuous_native_carrier_v8(route.address.endpoint_key, now_ms()?).is_err());
                } else {
                    host.rotate_native_carrier_mailbox_v7(false, at, at + 90_000)?;
                    ensure!(host.check_continuous_native_carrier_v7(route.address.endpoint_key, now_ms()?).is_err());
                }
                let result = tokio::time::timeout(Duration::from_secs(3),
                    client.request(put(expires, 5))).await
                    .context("withdrawn session neither closed nor rejected the request")?;
                ensure!(matches!(result, Err(_) | Ok(Reply::Rejected(_))),
                    "withdrawn service accepted custody");
                // Inspect the original store through its original service API;
                // a closed socket alone is not proof that no Put was committed.
                ensure!(matches!(service.request_local_v1(fetch(3)).await?, Reply::Empty));
                return Ok(None);
            }
            let query = |after, wait_ms| Request::WatchFetchV6 {
                boxes: vec![MailboxWatchV1 { mailbox: [1; 32], read: [3; 32], after }], wait_ms,
            };
            let Reply::NoticeObjectV6 { notices, object: None } = client.request(query(None, 0)).await?
                else { bail!("missing initial cursor") };
            let watch = client.send(1, query(Some(notices[0].cursor), 10_000))?;
            client.idle(Duration::from_millis(40)).await?;
            ensure!(client.received.is_empty(), "watch did not wait");
            let started = Instant::now();
            let Reply::Stored(receipt) = client.request(put(expires, 4)).await?
                else { bail!("missing StoreReceipt") };
            receipt.verify(pin(), [1; 32], [4; 32], &vec![9; 9000], expires)?;
            ensure!(matches!(client.until(watch).await?, Reply::NoticeObjectV6 { object: Some(row), .. }
                if row.object == [4; 32] && row.ciphertext == vec![9; 9000] && row.expires == expires));
            ensure!(started.elapsed() < Duration::from_secs(5), "Put serialized behind Watch");
            let session = client.link.session_id(0);
            // Cross the production node's actual 60-second locator rotation,
            // also exceeding the old 30-second initial carrier IO window.
            client.idle(Duration::from_secs(62)).await?;
            let next_record = verified_record(&mut client, &mut book, v8).await?;
            let next_route = carrier_route(&next_record, v8)?
                .context("rotated native carrier locator missing")?;
            ensure!(next_record.sequence > first_record.sequence
                && next_route.instance != first_route.instance
                && next_route.address.endpoint_key == first_route.address.endpoint_key,
                "node did not publish a fresh instance on the stable native carrier");
            ensure!(client.link.session_id(0) == session, "idle replaced native session");
            ensure!(matches!(client.request(fetch(3)).await?, Reply::Object { expires: actual, .. }
                if actual == expires));
            let Reply::Stored(duplicate) = client.request(put(expires, 4)).await?
                else { bail!("post-idle duplicate lost receipt") };
            ensure!(duplicate.signature == receipt.signature);
            Ok(Some(receipt))
        })).await;
        let after = owner.snapshot();
        let closed = owner.close().await;
        ensure!(
            before.instance == after.instance
                && after.operations_started == before.operations_started + 1,
            "requests acquired extra scopes or rebuilt endpoint"
        );
        closed?;
        result
    };
    let (server, client) = tokio::time::timeout(Duration::from_secs(95), async {
        tokio::join!(server, client)
    })
    .await?;
    let result = client?;
    if let Err(error) = server {
        ensure!(
            error.to_string() == "service stopped by owner",
            "node failed: {error:#}"
        );
    }
    Ok(result)
}

fn carrier_route(
    record: &PeerSignedRelayRecordV1,
    v8: bool,
) -> Result<Option<IrohServiceEndpointV1>> {
    if v8 {
        Ok(NativeCarrierMailboxEndpointV8::from_record(record)?.map(|e| e.iroh))
    } else {
        Ok(NativeCarrierMailboxEndpointV7::from_record(record)?.map(|e| e.iroh))
    }
}

#[tokio::test]
async fn v8_native_owner_keeps_same_session_over_rotation_and_custody_across_restart() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let expires = (now_ms()? / 1000) as i64 + 600;
    let receipt = round(directory.path(), expires, Case::Store, true)
        .await?
        .context("missing receipt")?;
    round(directory.path(), expires, Case::Restore(receipt), true).await?;
    Ok(())
}

#[tokio::test]
async fn v8_native_owner_rejects_undeclared_and_withdrawn_capability() -> Result<()> {
    for case in [Case::Undeclared, Case::Withdraw] {
        let directory = tempfile::tempdir()?;
        round(
            directory.path(),
            (now_ms()? / 1000) as i64 + 600,
            case,
            true,
        )
        .await?;
    }
    Ok(())
}
