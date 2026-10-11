//! Fixed-key loopback fixtures only. These tests exercise the real duplex
//! daemon, signed handshakes and encrypted frames, never a production route.
use super::*;
use crate::duplex::product_relay_daemon::{
    run_product_relay_daemon_with_shutdown_v1, ProductRelayDaemonConfigV1,
    ProductRelayDaemonReportV1,
};
use std::{fs, path::PathBuf, sync::atomic::AtomicU64};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

fn must<T>(result: WorkResult<T>) -> T {
    result.unwrap_or_else(|error| panic!("{}", error.reason))
}

fn channels() -> (E2eSecureChannelV1, E2eSecureChannelV1) {
    let a = SigningKey::from_bytes(&[31; 32]);
    let b = SigningKey::from_bytes(&[32; 32]);
    let offer = NodeHandshakeInitiatorV1::start(
        &a,
        peer_id_from_ed25519_public_key_v1(&b.verifying_key().to_bytes()),
        1_000,
        10_000,
    )
    .unwrap();
    let response = NodeHandshakeResponderV1::respond(
        offer.offer(),
        &b,
        1_000,
        10_000,
        &mut HandshakeReplayCacheV1::new(8),
    )
    .unwrap();
    let sender = offer
        .complete(
            response.response(),
            1_001,
            &mut HandshakeReplayCacheV1::new(8),
        )
        .unwrap();
    (sender, response.into_channel())
}

fn envelope(
    channel: &mut E2eSecureChannelV1,
    kind: NovoRudpTransportFrameKindV0,
    sequence: u64,
    offset: u64,
    bytes: &[u8],
) -> SecureNovoRudpEnvelopeV1 {
    channel
        .seal_novorudp_frame(&NovoRudpTransportFrameV0::new(
            kind,
            channel.session_id(),
            0,
            STREAM_DOMAIN,
            sequence,
            offset,
            bytes.to_vec(),
        ))
        .unwrap()
}

fn order() -> ReceiveOrder {
    ReceiveOrder {
        sequence: 0,
        offset: 0,
        done: false,
    }
}

#[test]
fn signed_channel_requires_contiguous_sequence_offset_and_authenticated_done() {
    let (mut sender, mut receiver) = channels();
    let first = envelope(
        &mut sender,
        NovoRudpTransportFrameKindV0::Data,
        0,
        0,
        b"abc",
    );
    let second = envelope(
        &mut sender,
        NovoRudpTransportFrameKindV0::Data,
        1,
        3,
        b"defg",
    );
    let done = envelope(&mut sender, NovoRudpTransportFrameKindV0::Done, 2, 7, b"");
    let mut ordered = order();
    assert_eq!(must(ordered.open(&mut receiver, &first)).unwrap(), b"abc");
    assert_eq!(must(ordered.open(&mut receiver, &second)).unwrap(), b"defg");
    assert!(must(ordered.open(&mut receiver, &done)).is_none());
    assert!(ordered.done);
    assert!(ordered.open(&mut receiver, &done).is_err());

    // The old E2E replay window permits a new highest sequence with a hole;
    // the byte adapter must reject that before any plaintext reaches TLS.
    let (mut sender, mut receiver) = channels();
    let _first = envelope(
        &mut sender,
        NovoRudpTransportFrameKindV0::Data,
        0,
        0,
        b"abc",
    );
    let second = envelope(
        &mut sender,
        NovoRudpTransportFrameKindV0::Data,
        1,
        3,
        b"def",
    );
    assert!(order().open(&mut receiver, &second).is_err());

    let (mut sender, mut receiver) = channels();
    let wrong_offset = envelope(
        &mut sender,
        NovoRudpTransportFrameKindV0::Data,
        0,
        1,
        b"abc",
    );
    assert!(order().open(&mut receiver, &wrong_offset).is_err());
    let (mut sender, mut receiver) = channels();
    let empty_data = envelope(&mut sender, NovoRudpTransportFrameKindV0::Data, 0, 0, b"");
    assert!(order().open(&mut receiver, &empty_data).is_err());
    let (mut sender, mut receiver) = channels();
    let payload_done = envelope(&mut sender, NovoRudpTransportFrameKindV0::Done, 0, 0, b"x");
    assert!(order().open(&mut receiver, &payload_done).is_err());
}

#[test]
fn replay_old_session_and_authenticated_wrong_frame_binding_are_terminal() {
    let (mut sender, mut receiver) = channels();
    let first = envelope(
        &mut sender,
        NovoRudpTransportFrameKindV0::Data,
        0,
        0,
        b"abc",
    );
    let mut ordered = order();
    assert!(must(ordered.open(&mut receiver, &first)).is_some());
    assert!(ordered.open(&mut receiver, &first).is_err());
    let (_, mut new_receiver) = channels();
    assert!(order().open(&mut new_receiver, &first).is_err());

    for bad_field in 0..4 {
        let (mut sender, mut receiver) = channels();
        let mut frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            sender.session_id(),
            0,
            STREAM_DOMAIN,
            0,
            0,
            vec![1],
        );
        match bad_field {
            0 => frame.stream_id = 1,
            1 => frame.object_id ^= 1,
            2 => frame.sequence = 1,
            _ => frame.kind = NovoRudpTransportFrameKindV0::Ack,
        }
        let encrypted = sender.seal_novorudp_frame(&frame).unwrap();
        assert!(order().open(&mut receiver, &encrypted).is_err());
    }
}

struct RelayFixture {
    root: PathBuf,
    config: ProductRelayClientConfigV1,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
}
impl RelayFixture {
    fn start() -> Self {
        let root = std::env::temp_dir().join(format!(
            "novovm-byte-fixture-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            FIXTURE_ID.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_path = root.join("certificate.pem");
        let key_path = root.join("tls-key.pem");
        let identity_path = root.join("identity.hex");
        let report_path = root.join("report.json");
        fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
        fs::write(&key_path, certificate.serialize_private_key_pem()).unwrap();
        fs::write(&identity_path, "47".repeat(32)).unwrap();
        let config: ProductRelayDaemonConfigV1 = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:0",
            "tls_cert_path": certificate_path,
            "tls_key_path": key_path,
            "relay_identity_key_path": identity_path,
            "report_path": report_path,
            "report_interval_ms": 10,
            "run_for_ms": 30_000,
            "max_connections": 8,
            "max_sessions": 4,
            "handshake_timeout_ms": 5_000
        }))
        .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker =
            thread::spawn(move || run_product_relay_daemon_with_shutdown_v1(config, worker_stop));
        // Construct the cleanup owner before any readiness assertion.
        let mut fixture = Self {
            root,
            config: ProductRelayClientConfigV1 {
                endpoint: String::new(),
                expected_relay_peer_id: peer_id_from_ed25519_public_key_v1(
                    &SigningKey::from_bytes(&[0x47; 32])
                        .verifying_key()
                        .to_bytes(),
                ),
                connect_timeout_ms: 4_000,
                read_timeout_ms: 4_000,
                tls_trust: ProductRelayTlsTrustV1::ExplicitCa { certificate_path },
            },
            stop,
            worker: Some(worker),
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let report = fs::read(&report_path).ok().and_then(|bytes| {
                serde_json::from_slice::<ProductRelayDaemonReportV1>(&bytes).ok()
            });
            if let Some(report) = report {
                let address: std::net::SocketAddr = report.listen_addr.parse().unwrap();
                fixture.config.endpoint = format!("wss://localhost:{}/novovm", address.port());
                break;
            }
            assert!(Instant::now() < deadline, "real relay fixture did not bind");
            thread::sleep(Duration::from_millis(5));
        }
        fixture
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap().unwrap();
        }
    }
    async fn wait_report(
        &self,
        predicate: impl Fn(&ProductRelayDaemonReportV1) -> bool,
    ) -> ProductRelayDaemonReportV1 {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let report = fs::read(self.root.join("report.json"))
                .ok()
                .and_then(|bytes| {
                    serde_json::from_slice::<ProductRelayDaemonReportV1>(&bytes).ok()
                });
            if let Some(report) = report.filter(|report| predicate(report)) {
                return report;
            }
            assert!(
                Instant::now() < deadline,
                "real relay report did not reach expected state"
            );
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}
impl Drop for RelayFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // Exact fixture-owned paths, never a recursive computed-path delete.
        for name in [
            "certificate.pem",
            "tls-key.pem",
            "identity.hex",
            "report.json",
        ] {
            let _ = fs::remove_file(self.root.join(name));
        }
        let _ = fs::remove_dir(&self.root);
    }
}

struct Workers(Vec<RelayByteStreamCompletionV1>);
impl Workers {
    async fn completed(&self) {
        for worker in &self.0 {
            worker
                .wait_finished(Instant::now() + Duration::from_secs(3))
                .await
                .unwrap();
            assert!(worker.is_finished());
        }
    }
    async fn failed(&self) {
        for worker in &self.0 {
            assert!(worker
                .wait_finished(Instant::now() + Duration::from_secs(3))
                .await
                .is_err());
            assert!(worker.is_finished());
        }
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        // Assertion failures also cancel and join these exact test workers.
        for completion in &self.0 {
            completion.cancel();
        }
        for completion in &self.0 {
            let worker = completion
                .shared
                .worker
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(worker) = worker {
                let _ = worker.join();
            }
        }
    }
}

fn control(deadline: Instant, revoked: Arc<AtomicBool>) -> RelayByteStreamControlV1 {
    RelayByteStreamControlV1::new(deadline, move || {
        if revoked.load(Ordering::Acquire) {
            Err(io::ErrorKind::PermissionDenied.into())
        } else {
            Ok(())
        }
    })
    .unwrap()
}
async fn pair(
    fixture: &RelayFixture,
    deadline: Instant,
    revoked: Arc<AtomicBool>,
) -> (RelayByteStreamV1, RelayByteStreamV1, Workers) {
    let a = SigningKey::from_bytes(&[31; 32]);
    let b = SigningKey::from_bytes(&[32; 32]);
    let (a_key, b_key) = (a.verifying_key().to_bytes(), b.verifying_key().to_bytes());
    let (left, right) = tokio::try_join!(
        RelayByteStreamV1::connect(
            a,
            fixture.config.clone(),
            b_key,
            true,
            control(deadline, revoked.clone())
        ),
        RelayByteStreamV1::connect(
            b,
            fixture.config.clone(),
            a_key,
            false,
            control(deadline, revoked)
        ),
    )
    .unwrap();
    let workers = Workers(vec![left.completion(), right.completion()]);
    (left, right, workers)
}

#[tokio::test]
async fn admission_rejects_insecure_outer_tls_wrong_keys_and_revoked_control() {
    let _serial = SERIAL.lock().await;
    let deadline = Instant::now() + Duration::from_secs(2);
    let config = ProductRelayClientConfigV1 {
        endpoint: "wss://127.0.0.1:1/novovm".into(),
        expected_relay_peer_id: "not-used".into(),
        connect_timeout_ms: 1_000,
        read_timeout_ms: 1_000,
        tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
    };
    let key = SigningKey::from_bytes(&[31; 32]);
    let remote = SigningKey::from_bytes(&[32; 32]).verifying_key().to_bytes();
    assert!(RelayByteStreamV1::connect(
        key.clone(),
        config.clone(),
        remote,
        true,
        control(deadline, Arc::new(AtomicBool::new(false)))
    )
    .await
    .is_err());
    let mut strict = config;
    strict.tls_trust = ProductRelayTlsTrustV1::NativeWebPki;
    for remote in [[0; 32], key.verifying_key().to_bytes()] {
        assert!(RelayByteStreamV1::connect(
            key.clone(),
            strict.clone(),
            remote,
            true,
            control(deadline, Arc::new(AtomicBool::new(false)))
        )
        .await
        .is_err());
    }
    assert!(RelayByteStreamControlV1::new(
        deadline,
        || Err(io::ErrorKind::PermissionDenied.into())
    )
    .is_err());
    assert!(RelayByteStreamControlV1::new(Instant::now(), || Ok(())).is_err());
    assert_eq!(OWNERS.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn real_duplex_large_bidirectional_backpressure_recovers_without_byte_loss() {
    let _serial = SERIAL.lock().await;
    let fixture = RelayFixture::start();
    let (left, right, workers) = pair(
        &fixture,
        Instant::now() + Duration::from_secs(15),
        Arc::new(AtomicBool::new(false)),
    )
    .await;
    let left_state = left.shared.clone();
    let right_state = right.shared.clone();
    let (mut left_read, mut left_write) = tokio::io::split(left);
    let (mut right_read, mut right_write) = tokio::io::split(right);
    let payload_left: Vec<u8> = (0..RELAY_BYTE_STREAM_QUEUE_BYTES_V1 * 3 + 127)
        .map(|i| (i % 251) as u8)
        .collect();
    let payload_right: Vec<u8> = (0..RELAY_BYTE_STREAM_QUEUE_BYTES_V1 * 3 + 319)
        .map(|i| (i % 239) as u8)
        .collect();
    let left_send = async {
        left_write.write_all(&payload_left).await.unwrap();
        left_write.shutdown().await.unwrap();
    };
    let right_send = async {
        right_write.write_all(&payload_right).await.unwrap();
        right_write.shutdown().await.unwrap();
    };
    let receive_both = async {
        // Delay reads until BOTH actual encrypted receive queues are full;
        // then let readers consume concurrently with pending writers.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let full_left = left_state.state().inbound_bytes == RELAY_BYTE_STREAM_QUEUE_BYTES_V1;
            let full_right = right_state.state().inbound_bytes == RELAY_BYTE_STREAM_QUEUE_BYTES_V1;
            if full_left && full_right {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "bidirectional queues never reached controlled backpressure"
            );
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        let mut from_left = Vec::new();
        let mut from_right = Vec::new();
        let (a, b) = tokio::join!(
            left_read.read_to_end(&mut from_right),
            right_read.read_to_end(&mut from_left)
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(from_left, payload_left);
        assert_eq!(from_right, payload_right);
    };
    tokio::join!(left_send, right_send, receive_both);
    drop((left_read, left_write, right_read, right_write));
    workers.completed().await;
    assert_eq!(OWNERS.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn authenticated_half_close_preserves_other_direction_and_immediate_reopen() {
    let _serial = SERIAL.lock().await;
    let fixture = RelayFixture::start();
    for _ in 0..3 {
        let (mut left, mut right, workers) = pair(
            &fixture,
            Instant::now() + Duration::from_secs(8),
            Arc::new(AtomicBool::new(false)),
        )
        .await;
        left.write_all(b"request").await.unwrap();
        left.shutdown().await.unwrap();
        let mut request = Vec::new();
        right.read_to_end(&mut request).await.unwrap();
        assert_eq!(request, b"request");
        assert!(!workers.0[0].is_finished(), "remote Done has not been sent");
        right
            .write_all(b"response after authenticated peer EOF")
            .await
            .unwrap();
        right.shutdown().await.unwrap();
        drop(right); // The local Done result may precede the other worker's EOF.
        let mut response = Vec::new();
        left.read_to_end(&mut response).await.unwrap();
        assert_eq!(response, b"response after authenticated peer EOF");
        drop(left);
        workers.completed().await;
    }
    assert_eq!(OWNERS.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn cancellation_wakes_pending_read_and_buffered_revocation_never_releases_bytes() {
    let _serial = SERIAL.lock().await;
    let fixture = RelayFixture::start();
    let revoked = Arc::new(AtomicBool::new(false));
    let (mut left, mut right, workers) = pair(
        &fixture,
        Instant::now() + Duration::from_secs(8),
        revoked.clone(),
    )
    .await;
    left.write_all(b"must remain inaccessible after revocation")
        .await
        .unwrap();
    left.flush().await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while right.shared.state().inbound_bytes == 0 {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    revoked.store(true, Ordering::Release);
    let mut untouched = [0u8; 64];
    assert!(right.read(&mut untouched).await.is_err());
    assert_eq!(untouched, [0; 64]);
    revoked.store(false, Ordering::Release); // A stale callback cannot resurrect it.
    assert!(right.read(&mut untouched).await.is_err());
    drop((left, right));
    workers.failed().await;

    let (left, mut right, workers) = pair(
        &fixture,
        Instant::now() + Duration::from_secs(8),
        Arc::new(AtomicBool::new(false)),
    )
    .await;
    let cancel = right.completion();
    let reading = async {
        let mut byte = [0];
        right.read(&mut byte).await
    };
    let cancelling = async {
        tokio::time::sleep(Duration::from_millis(40)).await;
        cancel.cancel();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(reading, cancelling)
    })
    .await
    .expect("cancel did not wake pending reader");
    assert!(result.is_err());
    drop((left, right));
    workers.failed().await;
}

#[tokio::test]
async fn real_relay_disconnect_is_error_not_authenticated_eof() {
    let _serial = SERIAL.lock().await;
    let mut fixture = RelayFixture::start();
    let (mut left, mut right, workers) = pair(
        &fixture,
        Instant::now() + Duration::from_secs(8),
        Arc::new(AtomicBool::new(false)),
    )
    .await;
    fixture.stop();
    let mut a = [0];
    let mut b = [0];
    let (left_result, right_result) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(left.read(&mut a), right.read(&mut b))
    })
    .await
    .expect("relay disconnect remained pending");
    assert!(left_result.is_err());
    assert!(right_result.is_err());
    drop((left, right));
    workers.failed().await;
}

#[tokio::test]
async fn absent_peer_uses_original_deadline_and_releases_worker_permit() {
    let _serial = SERIAL.lock().await;
    let fixture = RelayFixture::start();
    let deadline = Instant::now() + Duration::from_millis(700);
    let result = RelayByteStreamV1::connect(
        SigningKey::from_bytes(&[31; 32]),
        fixture.config.clone(),
        SigningKey::from_bytes(&[32; 32]).verifying_key().to_bytes(),
        true,
        control(deadline, Arc::new(AtomicBool::new(false))),
    )
    .await;
    assert!(result.is_err());
    assert!(
        Instant::now() >= deadline,
        "offline queued offer was treated as rejection"
    );
    let cleanup = Instant::now() + Duration::from_secs(1);
    while OWNERS.load(Ordering::Acquire) != 0 {
        assert!(Instant::now() < cleanup);
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[tokio::test]
async fn cancelling_pending_connect_releases_actual_workers_and_global_admission_is_four() {
    use std::future::{poll_fn, Future};
    let _serial = SERIAL.lock().await;
    let fixture = RelayFixture::start();
    let deadline = Instant::now() + Duration::from_secs(10);
    let absent_peer = SigningKey::from_bytes(&[200; 32])
        .verifying_key()
        .to_bytes();
    let mut pending = Box::pin(RelayByteStreamV1::connect(
        SigningKey::from_bytes(&[40; 32]),
        fixture.config.clone(),
        absent_peer,
        true,
        control(deadline, Arc::new(AtomicBool::new(false))),
    ));
    poll_fn(|cx| {
        assert!(pending.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    fixture
        .wait_report(|report| {
            report.relay_runtime.registered_session_total == 1
                && report.relay_runtime.queued_frame_total >= 1
        })
        .await;
    assert_eq!(OWNERS.load(Ordering::Acquire), 1);
    // The constructor is still waiting for the absent remote identity. Its
    // cancellation guard must close the already-authenticated WSS worker.
    drop(pending);
    let cleanup = Instant::now() + Duration::from_secs(2);
    while OWNERS.load(Ordering::Acquire) != 0 {
        assert!(
            Instant::now() < cleanup,
            "cancelled constructor leaked its worker permit"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    fixture
        .wait_report(|report| report.active_connection_count == 0)
        .await;
    // Observe the real daemon over another maintenance interval: cancellation
    // must not result in a reconnect under the same or a replacement session.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let report = fixture
        .wait_report(|report| report.active_connection_count == 0)
        .await;
    assert_eq!(report.relay_runtime.registered_session_total, 1);

    let mut pending: Vec<_> = (41..45)
        .map(|seed| {
            Box::pin(RelayByteStreamV1::connect(
                SigningKey::from_bytes(&[seed; 32]),
                fixture.config.clone(),
                absent_peer,
                true,
                control(deadline, Arc::new(AtomicBool::new(false))),
            ))
        })
        .collect();
    poll_fn(|cx| {
        for connect in &mut pending {
            assert!(connect.as_mut().poll(cx).is_pending());
        }
        Poll::Ready(())
    })
    .await;
    fixture
        .wait_report(|report| {
            report.relay_runtime.registered_session_total == 5
                && report.relay_runtime.queued_frame_total >= 5
        })
        .await;
    assert_eq!(
        OWNERS.load(Ordering::Acquire),
        RELAY_BYTE_STREAM_MAX_OWNERS_V1
    );
    let fifth = RelayByteStreamV1::connect(
        SigningKey::from_bytes(&[45; 32]),
        fixture.config.clone(),
        absent_peer,
        true,
        control(deadline, Arc::new(AtomicBool::new(false))),
    )
    .await;
    match fifth {
        Ok(_) => panic!("fifth real worker was admitted"),
        Err(error) => assert_eq!(
            error.to_string(),
            "relay byte-stream owner capacity exhausted"
        ),
    }
    drop(pending);
    let cleanup = Instant::now() + Duration::from_secs(2);
    while OWNERS.load(Ordering::Acquire) != 0 {
        assert!(
            Instant::now() < cleanup,
            "cancelled workers did not release quota"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    let report = fixture
        .wait_report(|report| report.active_connection_count == 0)
        .await;
    assert_eq!(report.relay_runtime.registered_session_total, 5);
}

#[cfg(feature = "tls-transport")]
#[tokio::test]
async fn actual_tls_finish_drops_io_then_both_workers_drain_done_before_success() {
    use crate::tls_transport::{TlsControlV1, TlsScopeV1};
    let _serial = SERIAL.lock().await;
    let fixture = RelayFixture::start();
    for _ in 0..3 {
        let deadline = Instant::now() + Duration::from_secs(8);
        let (left, right, workers) =
            pair(&fixture, deadline, Arc::new(AtomicBool::new(false))).await;
        let left_tls = TlsScopeV1::new(TlsControlV1::new(deadline, || Ok(())).unwrap()).unwrap();
        let right_tls = TlsScopeV1::new(TlsControlV1::new(deadline, || Ok(())).unwrap()).unwrap();
        let (mut a, mut b) = tokio::try_join!(
            left_tls.connect(left, right_tls.endpoint_key()),
            right_tls.accept(right, Some(left_tls.endpoint_key())),
        )
        .unwrap();
        a.write_frame(b"real pinned TLS over actual duplex WSS")
            .await
            .unwrap();
        assert_eq!(
            b.read_frame().await.unwrap(),
            b"real pinned TLS over actual duplex WSS"
        );
        b.write_frame(b"authenticated opposite direction")
            .await
            .unwrap();
        assert_eq!(
            a.read_frame().await.unwrap(),
            b"authenticated opposite direction"
        );
        let (a_result, b_result) = tokio::join!(a.finish(), b.finish());
        a_result.unwrap();
        b_result.unwrap();
        drop((a, b)); // Tls finish has already dropped its byte IO internally.
        workers.completed().await;
    }
    assert_eq!(OWNERS.load(Ordering::Acquire), 0);
}
