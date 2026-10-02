use super::*;
use crate::product_relay_client::ProductRelayTlsTrustV1;

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn id(seed: u8) -> String {
    peer_id_from_ed25519_public_key_v1(&key(seed).verifying_key().to_bytes())
}

fn config(peers: Vec<String>) -> NetworkWorkerConfig {
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

fn unstarted(config: &NetworkWorkerConfig) -> NetworkWorker {
    NetworkWorker {
        shared: Arc::new(Mutex::new(Shared {
            outbound: Queues::new(&config.peers, config.limits.outbound.clone()),
            inbound: Queues::new(&config.peers, config.limits.inbound.clone()),
            status: WorkerStatus::default(),
        })),
        stop: Arc::new(AtomicBool::new(false)),
        worker: None,
        max_payload: config.limits.max_payload_bytes,
        ttl: Duration::from_millis(config.queue_ttl_ms),
    }
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
    // The in-flight front remains charged until the network owner pops it.
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
    worker.shared.lock().unwrap().outbound.pop(&id(2));
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

fn channels(local: u8, remote: u8) -> (E2eSecureChannelV1, E2eSecureChannelV1) {
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
        use crate::product_relay_daemon::{
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
