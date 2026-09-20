// Real loopback WSS and authenticated Product Overlay, with four independent
// durable seal stores. Executed candidates are synthetic fixtures, not AOEM replay.
// The round timer is advanced by a controlled monotonic test clock.
use super::*;
use crate::native_block_seal::round_driver::NovNativeSealRoundDriverV1;
use crate::native_block_seal::round_overlay::NovNativeSealRoundOverlayV1;
use crate::native_block_seal_overlay::{
    NovNativeSealEpochAuthorityV1, NovNativeSealValidatorTransportBindingV1,
};
use crate::product_mainline_overlay::{
    ProductMainlineOverlayConfigV1, ProductMainlineOverlayEventV1, ProductMainlineOverlayInboundV1,
    ProductMainlineOverlayPayloadClassV1, ProductMainlineOverlayPeerConfigV1,
    ProductMainlineOverlayResourceLimitsV1, ProductMainlineOverlayRoleV1,
    ProductMainlineOverlayRuntimeV1,
};
use crate::product_node_overlay::{ProductBootstrapSourceV1, ProductNodeOverlayConfigV1};
use crate::product_relay_client::{ProductRelayClientConfigV1, ProductRelayTlsTrustV1};
use crate::product_relay_daemon::{
    run_product_relay_daemon_with_shutdown_v1, ProductRelayDaemonConfigV1,
};
use novovm_network::{
    peer_id_from_ed25519_public_key_v1, sign_bootstrap_manifest_v1, sign_relay_record_v1,
    BootstrapSourceKindV1, RelayEndpointV1, RelayTransportV1, SignedBootstrapManifestV1,
};
use std::{
    collections::BTreeSet,
    net::TcpListener,
    sync::{atomic::AtomicBool, Arc},
    thread,
    time::{Duration, Instant},
};

const ROUND_INTERVAL: Duration = Duration::from_secs(60);
const NETWORK_DEADLINE: Duration = Duration::from_secs(30);

struct RelayGuard {
    stopping: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Result<()>>>,
}

impl RelayGuard {
    fn start(config: ProductRelayDaemonConfigV1) -> Self {
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopping);
        Self {
            stopping,
            worker: Some(thread::spawn(move || {
                run_product_relay_daemon_with_shutdown_v1(config, stop)
            })),
        }
    }

    fn stop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .expect("loopback relay thread")
                .expect("loopback relay shutdown");
        }
    }
}

impl Drop for RelayGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

struct NetworkPeer {
    node: TestNodeV1,
    key: SigningKey,
    peer_id: String,
    runtime: Option<ProductMainlineOverlayRuntimeV1>,
    adapter: Option<NovNativeSealRoundOverlayV1>,
    authenticated_sources: BTreeSet<String>,
    received: usize,
}

struct NetworkCluster {
    peers: Vec<NetworkPeer>,
    authority: NovNativeSealEpochAuthorityV1,
    block_hash: [u8; 32],
    bootstrap: ProductBootstrapSourceV1,
    bootstrap_signer: [u8; 32],
    relay_override: ProductRelayClientConfigV1,
    relay: RelayGuard,
    started: Instant,
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn durable_seal_facts(store: &NovNativeBlockSealStoreV1) -> Vec<(Vec<u8>, Vec<u8>)> {
    store
        .db
        .iterator(IteratorMode::Start)
        .map(|entry| {
            let (key, value) = entry.unwrap();
            (key.to_vec(), value.to_vec())
        })
        .collect()
}

impl NetworkCluster {
    fn new(chain_id: u64) -> Self {
        let mut peers = Vec::new();
        let mut expected_authority = None;
        let mut block_hash = [0; 32];
        for index in 0..4 {
            let (node, block, keys, set) =
                genesis_fixture_v1(&format!("round-wss-{index}"), chain_id);
            let bindings = keys
                .iter()
                .map(|key| NovNativeSealValidatorTransportBindingV1 {
                    validator_id: validator_id_v1(key.verifying_key().as_bytes()),
                    transport_peer_id: peer_id_from_ed25519_public_key_v1(
                        key.verifying_key().as_bytes(),
                    ),
                })
                .collect();
            let authority = NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(
                node.ledger(),
                set,
                bindings,
            )
            .unwrap();
            if let Some(expected) = &expected_authority {
                assert_eq!(expected, &authority);
                assert_eq!(block_hash, block.header.block_hash);
            } else {
                expected_authority = Some(authority);
                block_hash = block.header.block_hash;
            }
            let key = keys.into_iter().nth(index).unwrap();
            let peer_id = peer_id_from_ed25519_public_key_v1(key.verifying_key().as_bytes());
            peers.push(NetworkPeer {
                node,
                key,
                peer_id,
                runtime: None,
                adapter: None,
                authenticated_sources: BTreeSet::new(),
                received: 0,
            });
        }
        assert!(peers.iter().enumerate().all(|(index, peer)| peers[..index]
            .iter()
            .all(|other| other.node.root != peer.node.root)));
        let root = peers[0].node.root.join("loopback-relay");
        fs::create_dir_all(&root).unwrap();
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_path = root.join("cert.pem");
        let tls_key_path = root.join("tls-key.pem");
        let identity_path = root.join("identity.hex");
        fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
        fs::write(&tls_key_path, certificate.serialize_private_key_pem()).unwrap();
        fs::write(&identity_path, hex_v1(&[113; 32])).unwrap();
        let relay_key = SigningKey::from_bytes(&[113; 32]);
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(relay_key.verifying_key().as_bytes());
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let report_path = root.join("report.json");
        let daemon_config: ProductRelayDaemonConfigV1 = serde_json::from_value(serde_json::json!({
            "bind_addr": format!("127.0.0.1:{port}"),
            "tls_cert_path": certificate_path,
            "tls_key_path": tls_key_path,
            "relay_identity_key_path": identity_path,
            "report_path": report_path,
            "report_interval_ms": 20,
            "session_queue_capacity": 128,
            "offline_queue_per_peer": 128,
            "offline_queue_per_source": 256,
            "offline_queue_total": 512,
            "session_ttl_ms": 10000,
            "rate_limit_frames": 10000,
            "max_frames_per_window": 20000,
            "rate_limit_window_ms": 1000
        }))
        .unwrap();
        let relay = RelayGuard::start(daemon_config);
        let wait = Instant::now();
        while !report_path.exists() {
            assert!(
                wait.elapsed() < Duration::from_secs(5),
                "loopback relay startup deadline"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let now = wall_ms();
        let bootstrap_key = SigningKey::from_bytes(&[114; 32]);
        let record = sign_relay_record_v1(
            &relay_key,
            "round-loopback-relay",
            vec![RelayEndpointV1 {
                transport: RelayTransportV1::Wss443,
                uri: "wss://localhost:443/novovm".into(),
                priority: 1,
                max_sessions: 16,
                max_bytes_per_minute: 64 * 1024 * 1024,
            }],
            now.saturating_sub(1000),
            now + 300_000,
            1,
        )
        .unwrap();
        let mut manifest = SignedBootstrapManifestV1 {
            version: 1,
            manifest_id: "round-loopback-manifest".into(),
            issued_at_ms: now.saturating_sub(1000),
            expires_at_ms: now + 300_000,
            candidate_limit: 1,
            full_raw_ip_directory_embedded: false,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: vec![record],
            signatures: Vec::new(),
        };
        sign_bootstrap_manifest_v1(&mut manifest, &bootstrap_key).unwrap();
        Self {
            peers,
            authority: expected_authority.unwrap(),
            block_hash,
            bootstrap: ProductBootstrapSourceV1 {
                source_kind: BootstrapSourceKindV1::EmbeddedInstall,
                priority: 1,
                manifest,
            },
            bootstrap_signer: *bootstrap_key.verifying_key().as_bytes(),
            relay_override: ProductRelayClientConfigV1 {
                endpoint: format!("wss://127.0.0.1:{port}/novovm"),
                expected_relay_peer_id: relay_peer_id,
                connect_timeout_ms: 1000,
                read_timeout_ms: 25,
                tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
            },
            relay,
            started: Instant::now(),
        }
    }

    fn initial_leader(&self) -> usize {
        let id = self.authority.scheduled_leader_v1(1, 0).unwrap();
        self.peers
            .iter()
            .position(|peer| validator_id_v1(peer.key.verifying_key().as_bytes()) == id)
            .unwrap()
    }

    fn start_peer(&mut self, index: usize, now: Instant) {
        let remote_peers = self
            .peers
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(other, peer)| ProductMainlineOverlayPeerConfigV1 {
                peer_id: peer.peer_id.clone(),
                metric_peer_id: 8800 + other as u64,
            })
            .collect();
        let peer = &mut self.peers[index];
        assert!(peer.runtime.is_none());
        let key_path = peer.node.root.join("network-identity.hex");
        fs::write(&key_path, hex_v1(&peer.key.to_bytes())).unwrap();
        let config = ProductMainlineOverlayConfigV1 {
            chain_id: self.authority.chain_id,
            role: ProductMainlineOverlayRoleV1::Duplex,
            identity_key_path: key_path,
            delivery_journal_path: None,
            overlay: ProductNodeOverlayConfigV1 {
                cache_path: peer.node.root.join("bootstrap-cache.json"),
                trusted_signer_public_keys: vec![self.bootstrap_signer],
                minimum_bootstrap_signatures: 1,
                embedded_sources: vec![self.bootstrap.clone()],
                cooldown_base_ms: Some(10),
                cooldown_max_ms: Some(100),
            },
            target_peer_id: None,
            expected_source_peer_id: None,
            peers: remote_peers,
            connect_timeout_ms: 1000,
            read_timeout_ms: 25,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
            channel_capacity: 128,
            resource_limits: ProductMainlineOverlayResourceLimitsV1::default(),
            metric_peer_id: 8800 + index as u64,
            reconnect_base_delay_ms: 20,
            reconnect_max_delay_ms: 200,
        };
        let runtime = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            config,
            wall_ms(),
            self.relay_override.clone(),
        )
        .expect("start real loopback WSS node runtime");
        let driver = NovNativeSealRoundDriverV1::open(
            peer.node.ledger(),
            peer.node.store(),
            self.authority.clone(),
            self.block_hash,
            None,
            validator_id_v1(peer.key.verifying_key().as_bytes()),
            now,
            ROUND_INTERVAL,
        )
        .unwrap();
        peer.adapter = Some(NovNativeSealRoundOverlayV1::attach(driver, &runtime).unwrap());
        peer.runtime = Some(runtime);
    }

    fn step(
        &mut self,
        active: &[usize],
        now: Instant,
    ) -> Vec<(usize, ProductMainlineOverlayInboundV1)> {
        let mut arrivals = Vec::new();
        for &index in active {
            let peer = &mut self.peers[index];
            let runtime = peer.runtime.as_ref().unwrap();
            let adapter = peer.adapter.as_mut().unwrap();
            for event in runtime.drain_events(128) {
                match event {
                    ProductMainlineOverlayEventV1::Inbound(inbound) => {
                        assert_eq!(
                            inbound.payload_class,
                            ProductMainlineOverlayPayloadClassV1::NativeSeal
                        );
                        let before = durable_seal_facts(peer.node.store());
                        adapter
                            .ingest(peer.node.ledger(), peer.node.store(), runtime, &inbound)
                            .expect("authenticated WSS round message accepted or harmlessly stale");
                        assert_eq!(
                            durable_seal_facts(peer.node.store()),
                            before,
                            "network ingress cannot write a vote, timeout or round transition"
                        );
                        peer.authenticated_sources
                            .insert(inbound.source_peer_id.clone());
                        peer.received += 1;
                        arrivals.push((index, inbound));
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                        panic!("network worker failed: {error}")
                    }
                    _ => (),
                }
            }
            adapter
                .poll(
                    peer.node.ledger(),
                    peer.node.store(),
                    &peer.key,
                    runtime,
                    now,
                )
                .expect("bounded round-overlay scheduler tick");
        }
        arrivals
    }

    fn assert_prepared(&self, indices: &[usize], round: u64) {
        for &index in indices {
            let peer = &self.peers[index];
            let adapter = peer.adapter.as_ref().unwrap();
            let qc = adapter.prepared_qc().expect("actual durable prepare QC");
            qc.verify(&self.authority.validator_set).unwrap();
            assert_eq!(qc.subject.block_hash, self.block_hash);
            assert_eq!(qc.subject.round, round);
            assert!(qc.signed_weight >= 3);
            assert_eq!(
                peer.node.store().load_qc(qc.qc_hash).unwrap().as_ref(),
                Some(qc)
            );
            assert!(adapter.status().prepared);
            assert!(!adapter.status().finalized);
            assert!(peer.received > 0);
        }
        self.assert_unfinalized();
    }

    fn assert_unfinalized(&self) {
        for peer in &self.peers {
            let candidate = peer
                .node
                .ledger()
                .load_candidate_record(self.authority.chain_id, self.block_hash)
                .unwrap()
                .unwrap();
            assert!(!candidate.chain_canonical && !candidate.fork_choice_selected);
            assert!(!candidate.proof_sealed && !candidate.safe && !candidate.finalized);
        }
    }

    fn prepare_over_network(&mut self, indices: &[usize], base: Instant) -> Instant {
        let wall_start = Instant::now();
        loop {
            let now = base + wall_start.elapsed();
            self.step(indices, now);
            if indices.iter().all(|&index| {
                self.peers[index]
                    .adapter
                    .as_ref()
                    .unwrap()
                    .status()
                    .prepared
            }) {
                return now;
            }
            assert!(
                wall_start.elapsed() < NETWORK_DEADLINE,
                "WSS quorum deadline; statuses: {:?}",
                indices
                    .iter()
                    .map(|&index| self.peers[index].adapter.as_ref().unwrap().status())
                    .collect::<Vec<_>>()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for NetworkCluster {
    fn drop(&mut self) {
        for peer in &mut self.peers {
            if let Some(mut runtime) = peer.runtime.take() {
                runtime.shutdown();
            }
        }
        self.relay.stop();
    }
}

#[test]
fn native_seal_round_network_real_wss_failover_and_returning_leader() {
    let mut cluster = NetworkCluster::new(9_781_601);
    let leader = cluster.initial_leader();
    let active = (0..4).filter(|&index| index != leader).collect::<Vec<_>>();
    for &index in &active {
        cluster.start_peer(index, cluster.started);
    }
    assert!(
        cluster.peers[leader].runtime.is_none(),
        "original leader is genuinely offline"
    );
    let completion = cluster.prepare_over_network(&active, cluster.started + ROUND_INTERVAL);
    cluster.assert_prepared(&active, 1);
    for &index in &active {
        assert!(cluster.peers[index].authenticated_sources.len() >= 2);
    }
    let restarted = active[0];
    let original_qc = cluster.peers[restarted]
        .adapter
        .as_ref()
        .unwrap()
        .prepared_qc()
        .unwrap()
        .clone();
    cluster.peers[restarted].runtime.take().unwrap().shutdown();
    cluster.peers[restarted].adapter.take();
    cluster.peers[restarted].node.reopen_store();
    let recovery_time = completion + Duration::from_millis(1);
    cluster.start_peer(restarted, recovery_time);
    assert_eq!(
        cluster.peers[restarted]
            .adapter
            .as_ref()
            .unwrap()
            .prepared_qc(),
        Some(&original_qc),
        "actual transport restart reopens the already durable prepare QC without new signatures"
    );
    cluster.assert_prepared(&active, 1);
    cluster.start_peer(leader, recovery_time);
    cluster.prepare_over_network(&[0, 1, 2, 3], recovery_time);
    cluster.assert_prepared(&[0, 1, 2, 3], 1);
}

#[test]
fn native_seal_round_network_two_of_four_cannot_advance_or_prepare() {
    let mut cluster = NetworkCluster::new(9_781_602);
    let leader = cluster.initial_leader();
    let active = (0..4)
        .filter(|&index| index != leader)
        .take(2)
        .collect::<Vec<_>>();
    for &index in &active {
        cluster.start_peer(index, cluster.started);
    }
    let started = Instant::now();
    let mut captured = None;
    while started.elapsed() < Duration::from_secs(3) {
        let arrivals = cluster.step(
            &active,
            cluster.started + ROUND_INTERVAL + started.elapsed(),
        );
        if captured.is_none() {
            captured = arrivals.into_iter().next();
        }
        thread::sleep(Duration::from_millis(10));
    }
    let (recipient, authentic) =
        captured.expect("capture a genuinely authenticated WSS inbound event");
    let peer = &mut cluster.peers[recipient];
    let runtime = peer.runtime.as_ref().unwrap();
    let adapter = peer.adapter.as_mut().unwrap();
    let before = durable_seal_facts(peer.node.store());
    let status = adapter.status();
    for case in 0..10 {
        let mut corrupt = authentic.clone();
        match case {
            0 => corrupt.frame.stream_id += 1,
            1 => corrupt.frame.session_id[0] ^= 1,
            2 => corrupt.frame.sequence += 1,
            3 => corrupt.frame.ack_epoch = 1,
            4 => corrupt.frame.object_id ^= 1,
            5 => corrupt.source_peer_id = "unauthenticated-unknown-peer".into(),
            6 => corrupt.payload_sha256[0] ^= 1,
            7 => corrupt.object_hash[16] ^= 1,
            8 => corrupt.delivery_id[0] ^= 1,
            9 => corrupt.frame.payload[0] ^= 1,
            _ => unreachable!(),
        }
        assert!(
            adapter
                .ingest(peer.node.ledger(), peer.node.store(), runtime, &corrupt)
                .is_err(),
            "corrupted authenticated metadata case {case} must not enter the driver cache"
        );
        assert_eq!(adapter.status(), status);
        assert_eq!(durable_seal_facts(peer.node.store()), before);
    }
    assert!(
        !adapter
            .ingest(peer.node.ledger(), peer.node.store(), runtime, &authentic)
            .unwrap(),
        "verified exact retransmission is suppressed without a signature or durable ACK"
    );
    let mut unrelated = authentic.clone();
    unrelated.payload_class = ProductMainlineOverlayPayloadClassV1::NativeTransaction;
    assert!(!adapter
        .ingest(peer.node.ledger(), peer.node.store(), runtime, &unrelated)
        .unwrap());
    assert_eq!(durable_seal_facts(peer.node.store()), before);
    for &index in &active {
        let peer = &cluster.peers[index];
        let adapter = peer.adapter.as_ref().unwrap();
        assert!(
            peer.received > 0,
            "actual peer timeout evidence crossed WSS"
        );
        assert_eq!(adapter.status().round, 0);
        assert!(!adapter.status().prepared);
        assert!(adapter.prepared_qc().is_none());
    }
    cluster.assert_unfinalized();
    let other = *active.iter().find(|&&index| index != recipient).unwrap();
    let wrong_runtime = cluster.peers[other].runtime.take().unwrap();
    let now = cluster.started + ROUND_INTERVAL + started.elapsed();
    let peer = &mut cluster.peers[recipient];
    let adapter = peer.adapter.as_mut().unwrap();
    let before = durable_seal_facts(peer.node.store());
    let status = adapter.status();
    assert!(adapter
        .ingest(
            peer.node.ledger(),
            peer.node.store(),
            &wrong_runtime,
            &authentic
        )
        .is_err());
    assert_eq!(
        adapter.status(),
        status,
        "bad ingress runtime cannot halt a healthy owner"
    );
    assert_eq!(durable_seal_facts(peer.node.store()), before);
    assert!(adapter
        .poll(
            peer.node.ledger(),
            peer.node.store(),
            &peer.key,
            &wrong_runtime,
            now
        )
        .is_err());
    assert_eq!(
        adapter.status().phase,
        crate::native_block_seal::round_driver::NovNativeSealRoundDriverPhaseV1::Halted
    );
    assert!(!adapter.status().prepared);
    assert!(adapter.prepared_qc().is_none());
    assert_eq!(
        durable_seal_facts(peer.node.store()),
        before,
        "local scheduler swapping the owner runtime must halt before a new signature"
    );
    cluster.peers[other].runtime = Some(wrong_runtime);
}
