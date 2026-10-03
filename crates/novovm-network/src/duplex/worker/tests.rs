use super::*;
use crate::duplex::product_relay_client::ProductRelayTlsTrustV1;

pub(super) fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
pub(super) fn id(seed: u8) -> String {
    peer_id_from_ed25519_public_key_v1(&key(seed).verifying_key().to_bytes())
}

pub(super) fn config(peers: Vec<String>) -> NetworkWorkerConfig {
    NetworkWorkerConfig {
        chain_id: 77,
        relay: ProductRelayClientConfigV1 {
            endpoint: "wss://127.0.0.1:1/novovm".into(),
            expected_relay_peer_id: id(91),
            connect_timeout_ms: 1000,
            read_timeout_ms: 10,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        },
        peers,
        limits: WorkerLimits::default(),
        handshake_timeout_ms: 5000,
        reconnect_delay_ms: 50,
        heartbeat_interval_ms: 1000,
        queue_ttl_ms: 30_000,
    }
}

pub(super) fn unstarted(config: &NetworkWorkerConfig) -> NetworkWorker {
    NetworkWorker {
        shared: Arc::new(Mutex::new(Shared {
            outbound: OutboundQueues::new(&config.peers, config.limits.outbound.clone()),
            inbound: Queues::new(&config.peers, config.limits.inbound.clone()),
            status: WorkerStatus::default(),
            relay_read_waker: None,
            read_wait_probe: None,
        })),
        stop: Arc::new(AtomicBool::new(false)),
        worker: None,
        max_payload: config.limits.max_payload_bytes,
        ttl: Duration::from_millis(config.queue_ttl_ms),
    }
}

#[test]
fn status_contention_is_retryable_without_mutating_queue_or_snapshot() {
    let worker = unstarted(&config(vec![id(2)]));
    assert_eq!(
        worker.try_send(id(2), vec![7; 13]).unwrap(),
        SendAdmission::Accepted
    );
    let mut held = worker.shared.lock().unwrap();
    held.inbound
        .push(&id(2), vec![9; 5], 5, Instant::now())
        .unwrap();
    held.status.active_peers.push(id(2));
    // The same thread deliberately owns the mutex: a blocking status read
    // would deadlock, while a poisoned read must never return this `None`.
    assert!(worker.try_status().unwrap().is_none());
    assert_eq!(
        worker.status().unwrap_err().to_string(),
        "network status busy"
    );
    drop(held);
    for status in [
        worker.try_status().unwrap().unwrap(),
        worker.status().unwrap(),
    ] {
        assert_eq!(status.outbound_messages, 1);
        assert_eq!(status.outbound_bytes, 13);
        assert_eq!(status.inbound_messages, 1);
        assert_eq!(status.inbound_bytes, 5);
        assert_eq!(status.active_peers, vec![id(2)]);
    }
}

#[test]
fn poisoned_status_is_fatal_not_retryable_contention() {
    let worker = unstarted(&config(vec![id(2)]));
    let shared = worker.shared.clone();
    let poisoned = std::panic::catch_unwind(move || {
        let _held = shared.lock().unwrap();
        panic!("poison the test-only worker state");
    });
    assert!(poisoned.is_err());
    assert_eq!(
        worker.try_status().unwrap_err().to_string(),
        "network status poisoned"
    );
    assert_eq!(
        worker.status().unwrap_err().to_string(),
        "network status poisoned"
    );
}

#[derive(Default)]
struct QueueWakeCounter(std::sync::atomic::AtomicUsize);

impl std::task::Wake for QueueWakeCounter {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn queue_admission_and_waker_registration_cover_both_orders_and_replacement() {
    let config = config(vec![id(2), id(3)]);
    let mut worker = unstarted(&config);
    let old = Arc::new(QueueWakeCounter::default());
    let new = Arc::new(QueueWakeCounter::default());
    let old_waker = Waker::from(Arc::clone(&old));
    // The first accepted original predates any authenticated connection.
    assert_eq!(
        worker.try_send(id(2), vec![1]).unwrap(),
        SendAdmission::Accepted
    );
    let pending = worker
        .shared
        .lock()
        .unwrap()
        .install_relay_read_waker(old_waker.clone());
    assert_eq!(
        old.0.load(Ordering::Relaxed),
        0,
        "wake is issued outside the queue lock"
    );
    pending.unwrap().wake();
    assert_eq!(old.0.load(Ordering::Relaxed), 1);
    assert_eq!(
        worker.try_send(id(3), vec![2]).unwrap(),
        SendAdmission::Accepted
    );
    assert_eq!(old.0.load(Ordering::Relaxed), 2);
    assert!(matches!(
        worker.try_send(id(4), vec![3]).unwrap(),
        SendAdmission::Rejected { .. }
    ));
    assert_eq!(old.0.load(Ordering::Relaxed), 2);

    worker.shared.lock().unwrap().clear_relay_read_waker();
    assert_eq!(
        worker.try_send(id(2), vec![4]).unwrap(),
        SendAdmission::Accepted
    );
    assert_eq!(old.0.load(Ordering::Relaxed), 2);
    let pending = worker
        .shared
        .lock()
        .unwrap()
        .install_relay_read_waker(Waker::from(Arc::clone(&new)));
    pending.unwrap().wake();
    assert_eq!(new.0.load(Ordering::Relaxed), 1);
    // A previously cloned notification can finish late; it cannot republish
    // the old connection or consume the replacement's queued-work obligation.
    old_waker.wake();
    assert_eq!(old.0.load(Ordering::Relaxed), 3);
    assert_eq!(new.0.load(Ordering::Relaxed), 1);
    assert_eq!(
        worker.try_send(id(3), vec![5]).unwrap(),
        SendAdmission::Accepted
    );
    assert_eq!(new.0.load(Ordering::Relaxed), 2);
    assert_eq!(worker.status().unwrap().outbound_messages, 4);
    worker.shutdown().unwrap();
    assert_eq!(
        new.0.load(Ordering::Relaxed),
        3,
        "shutdown notifies the current registration"
    );
}

#[test]
fn recv_rearm_requires_live_work_on_the_actual_active_peer_without_changing_fair_turn() {
    let config = config(vec![id(2), id(3)]);
    let worker = unstarted(&config);
    let now = Instant::now();
    let ttl = Duration::from_secs(1);
    let expired = now - ttl - Duration::from_millis(1);
    let mut peers = BTreeMap::from([(id(2), Peer::new()), (id(3), Peer::new())]);
    let counter = Arc::new(QueueWakeCounter::default());
    let mut shared = worker.shared.lock().unwrap();
    assert!(shared
        .install_relay_read_waker(Waker::from(Arc::clone(&counter)))
        .is_none());
    shared.outbound.push(&id(2), vec![1], 1, expired).unwrap();
    shared.outbound.push(&id(3), vec![2], 1, now).unwrap();
    // Intentionally stale telemetry must never make Idle/Cooldown runnable.
    shared.status.active_peers = vec![id(2), id(3)];
    assert!(shared
        .runnable_outbound_waker(&peers, now, ttl, true)
        .is_none());
    peers.get_mut(&id(3)).unwrap().phase = Phase::Cooldown(now - Duration::from_millis(1));
    assert!(shared
        .runnable_outbound_waker(&peers, now, ttl, true)
        .is_none());
    let (active, _) = channels(1, 2);
    peers.get_mut(&id(2)).unwrap().phase = Phase::Active(active);
    assert!(
        shared
            .runnable_outbound_waker(&peers, now, ttl, true)
            .is_none(),
        "expired-only active queue must sleep"
    );
    shared.outbound.push(&id(2), vec![3], 1, now).unwrap();
    let turn = shared.outbound.turn();
    assert!(shared
        .runnable_outbound_waker(&peers, now, ttl, false)
        .is_none());
    shared
        .runnable_outbound_waker(&peers, now, ttl, true)
        .unwrap()
        .wake();
    assert_eq!(counter.0.load(Ordering::Relaxed), 1);
    assert_eq!(
        shared.outbound.turn(),
        turn,
        "readiness check cannot consume the fair scheduling turn"
    );
    assert_eq!(shared.outbound.expire(now, ttl), 1);
    let (peer, entry, bytes) = shared
        .outbound
        .reserve_next(|id| id == self::id(2), 1)
        .unwrap();
    assert_eq!(bytes, vec![3]);
    assert!(
        shared
            .runnable_outbound_waker(&peers, now, ttl, true)
            .is_none(),
        "in-flight original cannot self-wake"
    );
    shared.outbound.settle_exact(&peer, entry, 1, true).unwrap();
    assert!(shared
        .runnable_outbound_waker(&peers, now, ttl, true)
        .is_none());
}

#[test]
fn queue_admission_returns_original_on_count_bytes_peer_and_contention_pressure() {
    let mut config = config(vec![id(2), id(3)]);
    config.limits.outbound = QueueLimits {
        max_messages: 2,
        max_bytes: 5,
        peer_max_messages: 1,
        peer_max_bytes: 4,
    };
    let worker = unstarted(&config);
    assert_eq!(
        worker.try_send(id(2), vec![1, 2, 3]).unwrap(),
        SendAdmission::Accepted
    );
    for (peer, bytes) in [(id(2), vec![4]), (id(3), vec![7, 8, 9])] {
        assert_eq!(
            worker.try_send(peer.clone(), bytes.clone()).unwrap(),
            SendAdmission::Backpressure(Outbound {
                peer_id: peer,
                bytes
            })
        );
    }
    assert_eq!(
        worker.try_send(id(3), vec![4, 5]).unwrap(),
        SendAdmission::Accepted
    );
    assert_eq!(worker.status().unwrap().outbound_bytes, 5);
    // In-flight originals remain charged until the exact correlated result.
    {
        let shared = worker.shared.lock().unwrap();
        let original = vec![9, 9];
        assert_eq!(
            worker.try_send(id(3), original.clone()).unwrap(),
            SendAdmission::Backpressure(Outbound {
                peer_id: id(3),
                bytes: original
            })
        );
        assert!(worker.try_recv().unwrap().is_none());
        assert_eq!(shared.outbound.count, 2);
    }
    {
        let mut shared = worker.shared.lock().unwrap();
        let (peer, entry, _) = shared
            .outbound
            .reserve_next(|id| id == self::id(2), 1)
            .unwrap();
        shared.outbound.settle_exact(&peer, entry, 1, true).unwrap();
    }
    assert_eq!(
        worker.try_send(id(2), vec![1]).unwrap(),
        SendAdmission::Accepted
    );
}

#[test]
fn outbound_rejection_and_stop_return_payload_without_false_ack() {
    let config = config(vec![id(2)]);
    let mut worker = unstarted(&config);
    for (peer, bytes, reason) in [
        (id(3), vec![1, 2], SendRejection::UnknownPeer),
        (
            id(2),
            vec![0; NETWORK_WORKER_MAX_PAYLOAD_BYTES + 1],
            SendRejection::PayloadTooLarge,
        ),
    ] {
        assert_eq!(
            worker.try_send(peer.clone(), bytes.clone()).unwrap(),
            SendAdmission::Rejected {
                message: Outbound {
                    peer_id: peer,
                    bytes
                },
                reason
            }
        );
    }
    worker.shutdown().unwrap();
    assert_eq!(
        worker.try_send(id(2), vec![7]).unwrap(),
        SendAdmission::Rejected {
            message: Outbound {
                peer_id: id(2),
                bytes: vec![7]
            },
            reason: SendRejection::Stopped,
        }
    );
    assert_eq!(worker.status().unwrap().relay_admissions, 0);
}

#[test]
fn fair_queue_turns_do_not_starve_other_peers_and_expiry_releases_both_budgets() {
    let ids = vec![id(2), id(3), id(4)];
    let mut queues = Queues::new(&ids, QueueLimits::default());
    let now = Instant::now();
    for peer in &ids {
        queues.push(peer, vec![1], 1, now).unwrap();
    }
    let mut selected = BTreeSet::new();
    for _ in 0..ids.len() {
        let peer = queues.next_peer(|_| true).unwrap();
        queues.pop(&peer).unwrap();
        queues.push(&peer, vec![2], 1, now).unwrap();
        selected.insert(peer);
    }
    assert_eq!(selected, ids.into_iter().collect());
    assert_eq!(
        queues.expire(now + Duration::from_secs(2), Duration::from_secs(1)),
        3
    );
    assert_eq!((queues.count, queues.bytes), (0, 0));
}

#[test]
fn configured_limits_and_peer_pins_are_checked_before_thread_spawn() {
    let mut config = config(vec![id(2)]);
    assert!(validate_config(&config, &key(1)).is_ok());
    config.chain_id = 0;
    assert!(validate_config(&config, &key(1)).is_err());
    config.chain_id = 77;
    config.peers.push(id(2));
    assert!(validate_config(&config, &key(1)).is_err());
    config.peers = vec![id(1)];
    assert!(validate_config(&config, &key(1)).is_err());
    config.peers = vec!["untrusted-name".into()];
    assert!(validate_config(&config, &key(1)).is_err());
    config.peers = vec![id(2)];
    config.limits.max_payload_bytes += 1;
    assert!(validate_config(&config, &key(1)).is_err());
}

#[test]
fn start_and_queue_admission_do_not_wait_on_stalled_relay_handshake() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = config(vec![id(2)]);
    config.relay.endpoint = format!("wss://{}/novovm", listener.local_addr().unwrap());
    let started = Instant::now();
    let mut worker = NetworkWorker::start(config, key(1)).unwrap();
    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(
        worker.try_send(id(2), vec![1, 2]).unwrap(),
        SendAdmission::Accepted
    );
    assert_eq!(worker.status().unwrap().relay_admissions, 0);
    worker.shutdown().unwrap();
}

pub(super) fn channels(local: u8, remote: u8) -> (E2eSecureChannelV1, E2eSecureChannelV1) {
    let initiator =
        NodeHandshakeInitiatorV1::start(&key(local), id(remote), now_ms(), 5000).unwrap();
    let responder = NodeHandshakeResponderV1::respond(
        initiator.offer(),
        &key(remote),
        now_ms(),
        5000,
        &mut HandshakeReplayCacheV1::new(16),
    )
    .unwrap();
    let local = initiator
        .complete(
            responder.response(),
            now_ms(),
            &mut HandshakeReplayCacheV1::new(16),
        )
        .unwrap();
    (local, responder.into_channel())
}

fn delivery(
    channel: &mut E2eSecureChannelV1,
    chain: u64,
    domain: u64,
    bytes: Vec<u8>,
) -> OpaqueRelayDeliveryV1 {
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        channel.session_id(),
        chain,
        domain,
        0,
        0,
        bytes,
    );
    OpaqueRelayDeliveryV1 {
        source_peer_id: channel.local_peer_id().into(),
        target_peer_id: channel.remote_peer_id().into(),
        received_at_ms: now_ms(),
        envelope: channel.seal_novorudp_frame(&frame).unwrap(),
    }
}

#[test]
fn e2e_receive_rejects_wrong_chain_domain_replay_and_stale_generation_without_reset() {
    let config = config(vec![id(2), id(3)]);
    let worker = unstarted(&config);
    let (local, mut remote) = channels(1, 2);
    let (healthy, _) = channels(1, 3);
    let healthy_session = healthy.session_id();
    let mut peers = BTreeMap::from([
        (
            id(2),
            Peer {
                phase: Phase::Active(local),
                ..Peer::new()
            },
        ),
        (
            id(3),
            Peer {
                phase: Phase::Active(healthy),
                ..Peer::new()
            },
        ),
    ]);
    let mut preauth = Queues::new(&config.peers, config.limits.preauth.clone());
    for invalid in [
        delivery(&mut remote, 78, FRAME_DOMAIN, vec![1]),
        delivery(&mut remote, 77, 9, vec![2]),
    ] {
        receive_delivery(invalid, &config, &mut peers, &worker.shared, &mut preauth).unwrap();
    }
    let valid = delivery(&mut remote, 77, FRAME_DOMAIN, vec![3]);
    receive_delivery(
        valid.clone(),
        &config,
        &mut peers,
        &worker.shared,
        &mut preauth,
    )
    .unwrap();
    receive_delivery(valid, &config, &mut peers, &worker.shared, &mut preauth).unwrap();
    let (_, mut stale_remote) = channels(1, 2);
    receive_delivery(
        delivery(&mut stale_remote, 77, FRAME_DOMAIN, vec![4]),
        &config,
        &mut peers,
        &worker.shared,
        &mut preauth,
    )
    .unwrap();
    assert_eq!(
        worker.try_recv().unwrap().unwrap(),
        Inbound {
            peer_id: id(2),
            bytes: vec![3]
        }
    );
    assert!(worker.try_recv().unwrap().is_none());
    assert_eq!(worker.status().unwrap().invalid_frames, 4);
    assert!(peers[&id(3)].expected_session(healthy_session));
    isolate(peers.get_mut(&id(2)).unwrap(), &config);
    assert!(peers[&id(3)].expected_session(healthy_session));
}

#[test]
fn inbound_full_is_explicitly_counted_and_other_peer_keeps_budget() {
    let mut config = config(vec![id(2), id(3)]);
    config.limits.inbound = QueueLimits {
        max_messages: 2,
        max_bytes: 10,
        peer_max_messages: 1,
        peer_max_bytes: 5,
    };
    let worker = unstarted(&config);
    let (local2, mut remote2) = channels(1, 2);
    let (local3, mut remote3) = channels(1, 3);
    let mut peers = BTreeMap::from([
        (
            id(2),
            Peer {
                phase: Phase::Active(local2),
                ..Peer::new()
            },
        ),
        (
            id(3),
            Peer {
                phase: Phase::Active(local3),
                ..Peer::new()
            },
        ),
    ]);
    let mut preauth = Queues::new(&config.peers, config.limits.preauth.clone());
    for packet in [
        delivery(&mut remote2, 77, FRAME_DOMAIN, vec![1; 5]),
        delivery(&mut remote2, 77, FRAME_DOMAIN, vec![2]),
        delivery(&mut remote3, 77, FRAME_DOMAIN, vec![3; 5]),
    ] {
        receive_delivery(packet, &config, &mut peers, &worker.shared, &mut preauth).unwrap();
    }
    let status = worker.status().unwrap();
    assert_eq!(
        (
            status.inbound_messages,
            status.inbound_bytes,
            status.inbound_dropped
        ),
        (2, 10, 1)
    );
    assert_eq!(status.relay_admissions, 0);
}

#[test]
fn preauth_accepts_only_exact_pending_session_and_is_bounded() {
    let mut config = config(vec![id(2)]);
    config.limits.preauth = QueueLimits {
        max_messages: 1,
        max_bytes: 1024,
        peer_max_messages: 1,
        peer_max_bytes: 1024,
    };
    let worker = unstarted(&config);
    let initiator = NodeHandshakeInitiatorV1::start(&key(1), id(2), now_ms(), 5000).unwrap();
    let responder = NodeHandshakeResponderV1::respond(
        initiator.offer(),
        &key(2),
        now_ms(),
        5000,
        &mut HandshakeReplayCacheV1::new(16),
    )
    .unwrap();
    let response = responder.response().clone();
    let mut remote = responder.into_channel();
    let mut peers = BTreeMap::from([(
        id(2),
        Peer {
            phase: Phase::Handshaking {
                initiator,
                deadline: Instant::now() + Duration::from_secs(5),
            },
            ..Peer::new()
        },
    )]);
    let mut preauth = Queues::new(&config.peers, config.limits.preauth.clone());
    receive_delivery(
        delivery(&mut remote, 77, FRAME_DOMAIN, vec![1]),
        &config,
        &mut peers,
        &worker.shared,
        &mut preauth,
    )
    .unwrap();
    receive_delivery(
        delivery(&mut remote, 77, FRAME_DOMAIN, vec![2]),
        &config,
        &mut peers,
        &worker.shared,
        &mut preauth,
    )
    .unwrap();
    assert_eq!(preauth.count, 1);
    assert_eq!(worker.status().unwrap().inbound_dropped, 1);
    let peer = peers.get_mut(&id(2)).unwrap();
    let Phase::Handshaking { initiator, .. } = std::mem::replace(&mut peer.phase, Phase::Idle)
    else {
        unreachable!()
    };
    peer.phase = Phase::Active(
        initiator
            .complete(&response, now_ms(), &mut peer.replay)
            .unwrap(),
    );
    let packet = preauth.pop(&id(2)).unwrap();
    receive_delivery(packet, &config, &mut peers, &worker.shared, &mut preauth).unwrap();
    assert_eq!(worker.try_recv().unwrap().unwrap().bytes, vec![1]);
    assert_eq!((preauth.count, preauth.bytes), (0, 0));
}

struct RelayFixture {
    endpoint: String,
    cert: std::path::PathBuf,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
}

impl RelayFixture {
    fn start() -> Self {
        use crate::duplex::product_relay_daemon::{
            run_product_relay_daemon_with_shutdown_v1, ProductRelayDaemonConfigV1,
            ProductRelayDaemonReportV1,
        };
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/runtime-rebuild/network-worker")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&root).unwrap();
        let certificate =
            rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()])
                .unwrap();
        let cert = root.join("cert.pem");
        let tls_key = root.join("tls-key.pem");
        let identity = root.join("identity.hex");
        let report = root.join("report.json");
        std::fs::write(&cert, certificate.serialize_pem().unwrap()).unwrap();
        std::fs::write(&tls_key, certificate.serialize_private_key_pem()).unwrap();
        std::fs::write(&identity, "5b".repeat(32)).unwrap();
        let daemon: ProductRelayDaemonConfigV1 = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:0", "tls_cert_path": cert, "tls_key_path": tls_key,
            "relay_identity_key_path": identity, "report_path": report,
            "report_interval_ms": 10, "max_connections": 16, "max_sessions": 8,
        }))
        .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let daemon_stop = Arc::clone(&stop);
        let worker =
            thread::spawn(move || run_product_relay_daemon_with_shutdown_v1(daemon, daemon_stop));
        let mut fixture = Self {
            endpoint: String::new(),
            cert,
            stop,
            worker: Some(worker),
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(report) = std::fs::read(&report)
                .ok()
                .and_then(|raw| serde_json::from_slice::<ProductRelayDaemonReportV1>(&raw).ok())
            {
                let addr: std::net::SocketAddr = report.listen_addr.parse().unwrap();
                fixture.endpoint = format!("wss://127.0.0.1:{}/novovm", addr.port());
                return fixture;
            }
            assert!(Instant::now() < deadline, "relay fixture did not start");
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for RelayFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // Preserve workspace-local artifacts for diagnosis; no temp-dir writes.
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "network worker condition timed out"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "real WSS read-wake timing; run release with --include-ignored --test-threads=1"]
fn real_wss_outbound_admission_wakes_a_known_idle_socket_read() {
    let relay = RelayFixture::start();
    let make_config = |peer| {
        let mut config = config(vec![id(peer)]);
        config.relay.endpoint = relay.endpoint.clone();
        config.relay.tls_trust = ProductRelayTlsTrustV1::ExplicitCa {
            certificate_path: relay.cert.clone(),
        };
        config.heartbeat_interval_ms = 5000;
        config
    };
    let mut sender_config = make_config(42);
    sender_config.relay.read_timeout_ms = 1000;
    let mut sender = NetworkWorker::start(sender_config, key(41)).unwrap();
    let mut receiver = NetworkWorker::start(make_config(41), key(42)).unwrap();
    wait_until(|| {
        sender
            .status()
            .is_ok_and(|status| status.active_peers.len() == 1)
            && receiver
                .status()
                .is_ok_and(|status| status.active_peers.len() == 1)
    });
    let initial = sender.status().unwrap();
    let probe = Arc::clone(
        sender
            .shared
            .lock()
            .unwrap()
            .read_wait_probe
            .as_ref()
            .unwrap(),
    );
    let previous = probe.entries.load(Ordering::Acquire);
    // Wait for a fresh genuine read-WouldBlock → Poll window, not a guessed
    // sleep after handshake. The observer never stalls or replaces real I/O.
    wait_until(|| {
        probe.polling.load(Ordering::Acquire) && probe.entries.load(Ordering::Acquire) > previous
    });
    let payload = b"admitted-after-real-read-park".to_vec();
    let mut pending = Outbound {
        peer_id: id(42),
        bytes: payload.clone(),
    };
    let started = Instant::now();
    loop {
        match sender.try_send(pending.peer_id, pending.bytes).unwrap() {
            SendAdmission::Accepted => break,
            SendAdmission::Backpressure(original) => {
                pending = original;
                assert!(
                    started.elapsed() < Duration::from_millis(250),
                    "local admission stayed busy"
                );
                thread::yield_now();
            }
            SendAdmission::Rejected { reason, .. } => {
                panic!("read-wake fixture rejected: {reason:?}")
            }
        }
    }
    let mut observed = None;
    wait_until(|| {
        if let Some(message) = receiver.try_recv().unwrap() {
            assert_eq!(message.peer_id, id(41));
            assert_eq!(message.bytes, payload);
            observed = Some(started.elapsed());
            true
        } else {
            false
        }
    });
    wait_until(|| {
        sender
            .status()
            .is_ok_and(|status| status.relay_admissions == initial.relay_admissions + 1)
    });
    let previous = probe.entries.load(Ordering::Acquire);
    wait_until(|| {
        probe.polling.load(Ordering::Acquire) && probe.entries.load(Ordering::Acquire) > previous
    });
    let burst_started = Instant::now();
    let expected: Vec<_> = (0..24)
        .map(|index| format!("coalesced-ready-tail-{index}").into_bytes())
        .collect();
    for bytes in &expected {
        let mut pending = Outbound {
            peer_id: id(42),
            bytes: bytes.clone(),
        };
        loop {
            match sender.try_send(pending.peer_id, pending.bytes).unwrap() {
                SendAdmission::Accepted => break,
                SendAdmission::Backpressure(original) => {
                    pending = original;
                    assert!(burst_started.elapsed() < Duration::from_millis(250));
                    thread::yield_now();
                }
                SendAdmission::Rejected { reason, .. } => panic!("burst rejected: {reason:?}"),
            }
        }
    }
    let mut received = Vec::new();
    wait_until(|| {
        while let Some(message) = receiver.try_recv().unwrap() {
            assert_eq!(message.peer_id, id(41));
            received.push(message.bytes);
        }
        received.len() >= expected.len()
    });
    let burst_elapsed = burst_started.elapsed();
    assert_eq!(
        received, expected,
        "coalesced notifications must preserve every FIFO original"
    );
    wait_until(|| {
        sender
            .status()
            .is_ok_and(|status| status.relay_admissions == initial.relay_admissions + 25)
    });
    assert_eq!(
        sender.status().unwrap().active_sessions,
        initial.active_sessions
    );
    let previous = probe.entries.load(Ordering::Acquire);
    wait_until(|| {
        probe.polling.load(Ordering::Acquire) && probe.entries.load(Ordering::Acquire) > previous
    });
    let stop_started = Instant::now();
    sender.shutdown().unwrap();
    let stop_elapsed = stop_started.elapsed();
    assert!(!probe.polling.load(Ordering::Acquire));
    {
        let shared = sender.shared.lock().unwrap();
        assert!(shared.relay_read_waker.is_none());
        assert!(shared.read_wait_probe.is_none());
    }
    receiver.shutdown().unwrap();
    assert!(receiver.try_recv().unwrap().is_none());
    let final_status = sender.status().unwrap();
    assert_eq!(final_status.relay_reconnects, initial.relay_reconnects);
    assert_eq!(final_status.outbound_expired, initial.outbound_expired);
    let elapsed = observed.unwrap();
    eprintln!("real post-park admission/decryption latency: {elapsed:?}; 24 FIFO originals after coalesced wakes: {burst_elapsed:?}; ordinary idle shutdown: {stop_elapsed:?}");
    assert!(
        elapsed < Duration::from_millis(250),
        "outbound queue notification did not interrupt the 1s socket idle read: {elapsed:?}"
    );
    assert!(burst_elapsed < Duration::from_millis(250), "burst tail paid another full idle read after its notification was consumed: {burst_elapsed:?}");
    assert!(
        stop_elapsed < Duration::from_millis(250),
        "ordinary idle shutdown failed to wake Poll: {stop_elapsed:?}"
    );
}

#[test]
#[ignore = "real WSS connection-generation regression; run release with --include-ignored --test-threads=1"]
fn real_wss_replacement_rebinds_outbound_wake_and_ignores_late_old_notification() {
    let relay = RelayFixture::start();
    let make_config = |peer| {
        let mut config = config(vec![id(peer)]);
        config.relay.endpoint = relay.endpoint.clone();
        config.relay.tls_trust = ProductRelayTlsTrustV1::ExplicitCa {
            certificate_path: relay.cert.clone(),
        };
        config.heartbeat_interval_ms = 5000;
        config
    };
    let mut sender_config = make_config(44);
    sender_config.relay.read_timeout_ms = 1000;
    let mut sender = NetworkWorker::start(sender_config.clone(), key(43)).unwrap();
    let mut receiver = NetworkWorker::start(make_config(43), key(44)).unwrap();
    wait_until(|| {
        sender
            .status()
            .is_ok_and(|status| status.active_peers.len() == 1)
            && receiver
                .status()
                .is_ok_and(|status| status.active_peers.len() == 1)
    });
    let initial = sender.status().unwrap();
    let receiver_initial = receiver.status().unwrap();
    let (old_waker, old_probe) = {
        let shared = sender.shared.lock().unwrap();
        (
            shared.relay_read_waker.as_ref().unwrap().clone(),
            Arc::clone(shared.read_wait_probe.as_ref().unwrap()),
        )
    };
    // A real authenticated replacement revokes the old connection. The worker
    // itself must recover/rebind; no test setter swaps its production socket.
    let replacement = ProductRelayClientV1::connect(&key(43), &sender_config.relay).unwrap();
    drop(replacement);
    wait_until(|| {
        sender.status().is_ok_and(|status| {
            status.relay_reconnects > initial.relay_reconnects && status.active_peers.len() == 1
        })
    });
    let (new_waker, new_probe) = {
        let shared = sender.shared.lock().unwrap();
        (
            shared.relay_read_waker.as_ref().unwrap().clone(),
            Arc::clone(shared.read_wait_probe.as_ref().unwrap()),
        )
    };
    assert!(!old_waker.will_wake(&new_waker));
    assert!(!Arc::ptr_eq(&old_probe, &new_probe));
    assert!(!old_probe.polling.load(Ordering::Acquire));
    let previous = new_probe.entries.load(Ordering::Acquire);
    wait_until(|| {
        new_probe.polling.load(Ordering::Acquire)
            && new_probe.entries.load(Ordering::Acquire) > previous
    });
    old_waker.wake();
    let payload = b"survives-late-old-connection-wake".to_vec();
    let mut pending = Some(Outbound {
        peer_id: id(44),
        bytes: payload.clone(),
    });
    let started = Instant::now();
    wait_until(|| {
        let message = pending.take().unwrap();
        match sender.try_send(message.peer_id, message.bytes).unwrap() {
            SendAdmission::Accepted => true,
            SendAdmission::Backpressure(original) => {
                pending = Some(original);
                false
            }
            SendAdmission::Rejected { reason, .. } => panic!("replacement rejected: {reason:?}"),
        }
    });
    wait_until(|| {
        if let Some(message) = receiver.try_recv().unwrap() {
            assert_eq!(message.peer_id, id(43));
            assert_eq!(message.bytes, payload);
            true
        } else {
            false
        }
    });
    let elapsed = started.elapsed();
    wait_until(|| {
        sender
            .status()
            .is_ok_and(|status| status.relay_admissions == initial.relay_admissions + 1)
    });
    assert_eq!(
        receiver.status().unwrap().relay_reconnects,
        receiver_initial.relay_reconnects
    );
    sender.shutdown().unwrap();
    receiver.shutdown().unwrap();
    assert!(receiver.try_recv().unwrap().is_none());
    assert!(sender.shared.lock().unwrap().relay_read_waker.is_none());
    assert!(sender.shared.lock().unwrap().read_wait_probe.is_none());
    assert!(
        elapsed < Duration::from_millis(250),
        "replacement failed to wake current socket: {elapsed:?}"
    );
}

#[test]
#[ignore = "real WSS latency regression; run release with --include-ignored --test-threads=1"]
fn real_wss_queued_payload_burst_does_not_pay_idle_read_timeout_per_message() {
    let relay = RelayFixture::start();
    let sender_id = id(21);
    let receiver_id = id(22);
    let mut sender_config = config(vec![receiver_id.clone()]);
    sender_config.relay.endpoint = relay.endpoint.clone();
    sender_config.relay.tls_trust = ProductRelayTlsTrustV1::ExplicitCa {
        certificate_path: relay.cert.clone(),
    };
    // Exercise a legal idle-read setting, without changing queue limits or TTL.
    sender_config.relay.read_timeout_ms = 250;
    sender_config.heartbeat_interval_ms = 1000;
    let mut receiver_config = config(vec![sender_id.clone()]);
    receiver_config.relay.endpoint = relay.endpoint.clone();
    receiver_config.relay.tls_trust = ProductRelayTlsTrustV1::ExplicitCa {
        certificate_path: relay.cert.clone(),
    };
    let mut sender = NetworkWorker::start(sender_config, key(21)).unwrap();
    let mut receiver = NetworkWorker::start(receiver_config, key(22)).unwrap();

    // Complete real WSS and E2E handshakes before starting the admission clock.
    let mut initial = None;
    wait_until(|| {
        let (Ok(sent), Ok(received)) = (sender.status(), receiver.status()) else {
            return false;
        };
        if sent.active_sessions.contains_key(&receiver_id)
            && received.active_sessions.contains_key(&sender_id)
        {
            initial = Some((sent, received));
            true
        } else {
            false
        }
    });
    let (initial_sender, initial_receiver) = initial.unwrap();
    let expected: Vec<_> = (0..8)
        .map(|index| format!("queued-burst-payload-{index}").into_bytes())
        .collect();
    let target_admissions = initial_sender.relay_admissions + expected.len() as u64;
    let started = Instant::now();
    let deadline = started + Duration::from_secs(1);
    for bytes in &expected {
        let mut pending = Outbound {
            peer_id: receiver_id.clone(),
            bytes: bytes.clone(),
        };
        loop {
            match sender.try_send(pending.peer_id, pending.bytes).unwrap() {
                SendAdmission::Accepted => break,
                SendAdmission::Backpressure(original) => {
                    // Only retry an unaccepted original; never duplicate a send.
                    pending = original;
                    assert!(Instant::now() < deadline, "burst admission stayed busy");
                    thread::sleep(Duration::from_millis(1));
                }
                SendAdmission::Rejected { reason, .. } => {
                    panic!("burst payload rejected: {reason:?}");
                }
            }
        }
    }

    let mut admitted_in_time = None;
    let mut observed_admissions = initial_sender.relay_admissions;
    while Instant::now() < deadline {
        if let Ok(status) = sender.status() {
            observed_admissions = status.relay_admissions;
            if observed_admissions == target_admissions {
                let elapsed = started.elapsed();
                if elapsed < Duration::from_secs(1) {
                    admitted_in_time = Some(elapsed);
                }
                break;
            }
        }
        thread::sleep(Duration::from_millis(5));
    }

    // Also finish the real decrypt/readback when the timing regression is red.
    // Relay disposition is not a peer-delivery ACK or a transaction TPS result.
    let mut received = Vec::new();
    wait_until(|| {
        while let Some(message) = receiver.try_recv().unwrap() {
            assert_eq!(message.peer_id, sender_id);
            received.push(message.bytes);
        }
        received.len() >= expected.len()
            && sender
                .status()
                .is_ok_and(|status| status.relay_admissions == target_admissions)
    });
    sender.shutdown().unwrap();
    receiver.shutdown().unwrap();
    while let Some(message) = receiver.try_recv().unwrap() {
        assert_eq!(message.peer_id, sender_id);
        received.push(message.bytes);
    }
    assert_eq!(
        received, expected,
        "decrypted payloads must be exact and unique"
    );
    let final_sender = sender.status().unwrap();
    let final_receiver = receiver.status().unwrap();
    assert_eq!(final_sender.relay_admissions, target_admissions);
    assert_eq!(
        final_sender.relay_reconnects,
        initial_sender.relay_reconnects
    );
    assert_eq!(
        final_receiver.relay_reconnects,
        initial_receiver.relay_reconnects
    );
    assert!(
        admitted_in_time.is_some(),
        "8 accepted payloads paid repeated idle reads: only {} relay admissions observed within 1s; all 8 later decrypted exactly; sender={final_sender:?}, receiver={final_receiver:?}",
        observed_admissions - initial_sender.relay_admissions,
    );
    eprintln!(
        "real queued burst: 8 relay admissions in {:?}; 8 exact peer decryptions, no reconnect",
        admitted_in_time.unwrap()
    );
}

#[test]
#[ignore = "real WSS large-frame regression; run release with --include-ignored --test-threads=1"]
fn real_wss_three_worker_large_duplex_fanin_crosses_delivery_windows_without_loss() {
    const MESSAGES_PER_ROUTE: usize = 24;
    const PAYLOAD_BYTES: usize = 192 * 1024;
    fn payload(source: u8, target: u8, sequence: usize) -> Vec<u8> {
        (0..PAYLOAD_BYTES)
            .map(|index| {
                ((index + source as usize * 31 + target as usize * 7 + sequence * 17) % 251) as u8
            })
            .collect()
    }

    let relay = RelayFixture::start();
    let seeds = [31, 32, 33];
    let ids: Vec<_> = seeds.into_iter().map(id).collect();
    let configs: Vec<_> = (0..seeds.len())
        .map(|index| {
            let mut config = config(
                ids.iter()
                    .enumerate()
                    .filter(|(other, _)| *other != index)
                    .map(|(_, peer)| peer.clone())
                    .collect(),
            );
            config.relay.endpoint = relay.endpoint.clone();
            config.relay.tls_trust = ProductRelayTlsTrustV1::ExplicitCa {
                certificate_path: relay.cert.clone(),
            };
            assert!(PAYLOAD_BYTES <= config.limits.max_payload_bytes);
            // Twenty-four maximum-sized payloads exceed the default per-peer
            // queue, so the producer must respect real backpressure. Do not
            // enlarge the queue, TTL, socket timeout, or the 20-second gate.
            assert!(MESSAGES_PER_ROUTE * PAYLOAD_BYTES > config.limits.outbound.peer_max_bytes);
            config
        })
        .collect();
    let mut workers: Vec<_> = configs
        .iter()
        .zip(seeds)
        .map(|(config, seed)| NetworkWorker::start(config.clone(), key(seed)).unwrap())
        .collect();
    let mut initial = None;
    wait_until(|| {
        let Ok(statuses) = workers
            .iter()
            .map(NetworkWorker::status)
            .collect::<Result<Vec<_>>>()
        else {
            return false;
        };
        if statuses
            .iter()
            .all(|status| status.relay_connected && status.active_sessions.len() == 2)
        {
            initial = Some(statuses);
            true
        } else {
            false
        }
    });
    let initial = initial.unwrap();
    // Two leaves feed the same hub while the hub concurrently sends a full
    // stream to each leaf. Every direction exceeds the shared delivery window;
    // these are real worker WSS/E2E frames, not scripted plaintext I/O.
    let routes = [(0usize, 1usize), (0, 2), (1, 0), (2, 0)];
    assert!(
        MESSAGES_PER_ROUTE as u64 > crate::duplex::product_relay::PRODUCT_RELAY_DELIVERY_WINDOW_V1
    );
    let mut submitted = [0usize; 4];
    let mut received = [0usize; 4];
    let mut pending: [Option<Outbound>; 4] = std::array::from_fn(|_| None);
    let mut seen = BTreeSet::new();
    let started = Instant::now();
    let mut printed = false;
    wait_until(|| {
        for (route, &(source, target)) in routes.iter().enumerate() {
            if submitted[route] == MESSAGES_PER_ROUTE {
                continue;
            }
            let message = pending[route].take().unwrap_or_else(|| Outbound {
                peer_id: ids[target].clone(),
                bytes: payload(seeds[source], seeds[target], submitted[route]),
            });
            match workers[source]
                .try_send(message.peer_id, message.bytes)
                .unwrap()
            {
                SendAdmission::Accepted => submitted[route] += 1,
                // The exact unaccepted original is retained; an Accepted item
                // is never recreated or resubmitted by this producer.
                SendAdmission::Backpressure(original) => pending[route] = Some(original),
                SendAdmission::Rejected { reason, .. } => {
                    panic!("large duplex rejected: {reason:?}")
                }
            }
        }
        for (target, worker) in workers.iter().enumerate() {
            while let Some(message) = worker.try_recv().unwrap() {
                let source = ids
                    .iter()
                    .position(|peer| peer == &message.peer_id)
                    .expect("configured source");
                let route = routes
                    .iter()
                    .position(|pair| *pair == (source, target))
                    .expect("active payload route");
                let sequence = received[route];
                assert!(
                    sequence < MESSAGES_PER_ROUTE,
                    "duplicate or extra delivery on route {route}"
                );
                assert!(
                    message.bytes == payload(seeds[source], seeds[target], sequence),
                    "exact per-route FIFO, route {route}, sequence {sequence}"
                );
                assert!(seen.insert((route, sequence)), "duplicate large payload");
                received[route] += 1;
            }
        }
        let statuses: Vec<_> = workers.iter().map(NetworkWorker::status).collect();
        for (index, status) in statuses.iter().enumerate() {
            if let Ok(status) = status {
                assert_eq!(
                    status.relay_reconnects, initial[index].relay_reconnects,
                    "large duplex relay reconnect: {statuses:?}"
                );
                assert_eq!(
                    status.inbound_dropped, initial[index].inbound_dropped,
                    "large duplex dropped plaintext: {statuses:?}"
                );
                assert_eq!(
                    status.outbound_expired, initial[index].outbound_expired,
                    "large duplex expired plaintext: {statuses:?}"
                );
                assert_eq!(
                    status.invalid_frames, initial[index].invalid_frames,
                    "large duplex invalid frame: {statuses:?}"
                );
                assert_eq!(
                    status.active_sessions, initial[index].active_sessions,
                    "healthy E2E sessions must remain unchanged: {statuses:?}"
                );
                assert!(status.outbound_bytes <= configs[index].limits.outbound.max_bytes);
                assert!(status.inbound_bytes <= configs[index].limits.inbound.max_bytes);
            }
        }
        if !printed && started.elapsed() > Duration::from_secs(19) {
            eprintln!("large duplex nearing unchanged 20s deadline: submitted={submitted:?}, received={received:?}, statuses={statuses:?}");
            printed = true;
        }
        submitted.iter().all(|count| *count == MESSAGES_PER_ROUTE)
            && received.iter().all(|count| *count == MESSAGES_PER_ROUTE)
            && statuses.iter().enumerate().all(|(index, status)| {
                let outgoing = routes.iter().filter(|(source, _)| *source == index).count()
                    * MESSAGES_PER_ROUTE;
                status.as_ref().is_ok_and(|status| {
                    status.outbound_messages == 0
                        && status.relay_admissions
                            == initial[index].relay_admissions + outgoing as u64
                })
            })
    });
    assert!(pending.iter().all(Option::is_none));
    assert_eq!(seen.len(), routes.len() * MESSAGES_PER_ROUTE);
    let elapsed = started.elapsed();
    for worker in &mut workers {
        worker.shutdown().unwrap();
    }
    for (index, worker) in workers.iter().enumerate() {
        assert!(
            worker.try_recv().unwrap().is_none(),
            "extra delivery after completion"
        );
        let status = worker.status().unwrap();
        assert_eq!(status.relay_reconnects, initial[index].relay_reconnects);
        assert_eq!(status.inbound_dropped, initial[index].inbound_dropped);
        assert_eq!(status.outbound_expired, initial[index].outbound_expired);
        assert_eq!(status.invalid_frames, initial[index].invalid_frames);
    }
    eprintln!("real WSS/E2E large duplex fan-in: 4 routes x {MESSAGES_PER_ROUTE} x {PAYLOAD_BYTES} exact bytes in {elapsed:?}; unchanged bounded queues, sessions, no reconnect/drop");
}

#[test]
fn real_wss_three_worker_duplex_and_peer_restart_preserve_healthy_session() {
    let relay = RelayFixture::start();
    let seeds = [11, 12, 13];
    let configs: Vec<_> = seeds
        .iter()
        .map(|seed| {
            let mut config = config(
                seeds
                    .iter()
                    .filter(|other| *other != seed)
                    .map(|seed| id(*seed))
                    .collect(),
            );
            config.relay.endpoint = relay.endpoint.clone();
            config.relay.tls_trust = ProductRelayTlsTrustV1::ExplicitCa {
                certificate_path: relay.cert.clone(),
            };
            config
        })
        .collect();
    let mut workers: Vec<_> = configs
        .iter()
        .zip(seeds)
        .map(|(config, seed)| NetworkWorker::start(config.clone(), key(seed)).unwrap())
        .collect();
    let ready_started = Instant::now();
    let mut printed = false;
    wait_until(|| {
        if !printed && ready_started.elapsed() > Duration::from_secs(19) {
            eprintln!(
                "worker readiness status: {:?}",
                workers
                    .iter()
                    .map(|worker| worker.status())
                    .collect::<Vec<_>>()
            );
            printed = true;
        }
        workers.iter().all(|worker| {
            worker
                .status()
                .is_ok_and(|status| status.active_peers.len() == 2)
        })
    });
    let healthy_session = workers[0].status().unwrap().active_sessions[&id(12)];
    for (index, worker) in workers.iter().enumerate() {
        for peer in &configs[index].peers {
            wait_until(|| {
                matches!(
                    worker.try_send(peer.clone(), vec![seeds[index]]).unwrap(),
                    SendAdmission::Accepted
                )
            });
        }
    }
    let mut received = vec![BTreeSet::new(); 3];
    wait_until(|| {
        for (index, worker) in workers.iter().enumerate() {
            while let Some(inbound) = worker.try_recv().unwrap() {
                received[index].insert((inbound.peer_id, inbound.bytes));
            }
        }
        received.iter().all(|messages| messages.len() == 2)
    });
    workers[2].shutdown().unwrap();
    workers[2] = NetworkWorker::start(configs[2].clone(), key(13)).unwrap();
    wait_until(|| {
        workers[2]
            .status()
            .is_ok_and(|status| status.active_peers.len() == 2)
    });
    assert_eq!(
        workers[0].status().unwrap().active_sessions[&id(12)],
        healthy_session
    );
    wait_until(|| {
        matches!(
            workers[2].try_send(id(11), vec![99]).unwrap(),
            SendAdmission::Accepted
        )
    });
    wait_until(|| {
        workers[0]
            .try_recv()
            .unwrap()
            .is_some_and(|message| message.peer_id == id(13) && message.bytes == vec![99])
    });
    for worker in &mut workers {
        worker.shutdown().unwrap();
    }
}
