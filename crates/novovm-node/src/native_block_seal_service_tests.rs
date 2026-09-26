// Included after the real-WSS fixture in native_seal_round_network. These
// services own their keys/stores and use the application's event/tick shape;
// candidates are still synthetic test executions, not AOEM or physical nodes.
use crate::native_block_seal::service::NovNativeSealServiceV1;
use crate::native_block_seal::service_config::NovNativeSealServiceConfigV1;

fn write_service_config(cluster: &NetworkCluster, index: usize) -> PathBuf {
    let peer = &cluster.peers[index];
    let directory = peer.node.root.join("service-config");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("authority.json"),
        serde_json::to_vec_pretty(&cluster.authority).unwrap(),
    )
    .unwrap();
    fs::write(directory.join("signer.hex"), hex_v1(&peer.key.to_bytes())).unwrap();
    let path = directory.join("service.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "novovm-native-seal-service/v1",
            "enabled": true,
            "chain_id": cluster.authority.chain_id,
            "height": 1,
            "block_hash": hex_v1(&cluster.block_hash),
            "justify_qc_hash": null,
            "authority_path": "authority.json",
            "signer_key_path": "signer.hex",
            "seal_store_path": "../seal",
            "round_timeout_ms": ROUND_INTERVAL.as_millis() as u64,
            "poll_interval_ms": 100,
            "ingress_per_source_per_second": 8,
            "ingress_per_poll": 8
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

fn open_test_service(
    cluster: &NetworkCluster,
    index: usize,
    path: &Path,
    now: Instant,
) -> Result<NovNativeSealServiceV1> {
    let peer = &cluster.peers[index];
    NovNativeSealServiceV1::open(
        NovNativeSealServiceConfigV1::load(path, cluster.authority.chain_id)?,
        peer.node.ledger().path(),
        peer.runtime.as_ref().unwrap(),
        now,
    )
}

fn start_test_service(
    cluster: &mut NetworkCluster,
    index: usize,
    now: Instant,
) -> NovNativeSealServiceV1 {
    cluster.start_peer(index, now);
    // Never poll the fixture's lower-level adapter: only the service owns the
    // signing lifecycle under test. Opening an adapter itself makes no signature.
    cluster.peers[index].adapter.take();
    let path = write_service_config(cluster, index);
    open_test_service(cluster, index, &path, now).unwrap()
}

fn step_test_services(
    cluster: &NetworkCluster,
    services: &mut [Option<NovNativeSealServiceV1>],
    indices: &[usize],
    now: Instant,
) -> Vec<(usize, ProductMainlineOverlayInboundV1)> {
    let mut arrivals = Vec::new();
    for &index in indices {
        let peer = &cluster.peers[index];
        let runtime = peer.runtime.as_ref().unwrap();
        let service = services[index].as_mut().unwrap();
        let before = durable_seal_facts(peer.node.store());
        for event in runtime.drain_events(128) {
            match event {
                ProductMainlineOverlayEventV1::Inbound(inbound) => {
                    service.enqueue(inbound.clone());
                    arrivals.push((index, inbound));
                }
                ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                    panic!("service WSS worker failed: {error}")
                }
                _ => (),
            }
        }
        assert_eq!(
            durable_seal_facts(peer.node.store()),
            before,
            "event staging must not sign or change durable round evidence"
        );
        service.poll(runtime, now).unwrap();
        assert!(!service.halted());
        assert_eq!(service.status_json()["finalized"], false);
    }
    arrivals
}

fn prepare_test_services(
    cluster: &NetworkCluster,
    services: &mut [Option<NovNativeSealServiceV1>],
    indices: &[usize],
    base: Instant,
) -> Instant {
    let wall_start = Instant::now();
    loop {
        let now = base + wall_start.elapsed();
        step_test_services(cluster, services, indices, now);
        if indices
            .iter()
            .all(|&index| services[index].as_ref().unwrap().status_json()["prepared"] == true)
        {
            return now;
        }
        assert!(
            wall_start.elapsed() < NETWORK_DEADLINE,
            "service WSS prepare deadline; statuses: {:?}",
            indices
                .iter()
                .map(|&index| services[index].as_ref().unwrap().status_json())
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn assert_service_prepare(
    cluster: &NetworkCluster,
    services: &[Option<NovNativeSealServiceV1>],
    indices: &[usize],
    round: u64,
) {
    for &index in indices {
        let service = services[index].as_ref().unwrap();
        let status = service.status_json();
        assert_eq!(status["enabled"], true);
        assert_eq!(status["ok"], true);
        assert_eq!(status["prepared"], true);
        assert_eq!(status["round"], round);
        assert_eq!(status["height"], 1);
        assert_eq!(status["chain_id"], cluster.authority.chain_id);
        assert_eq!(status["block_hash"], hex_v1(&cluster.block_hash));
        assert_eq!(
            status["local_validator_id"],
            hex_v1(&validator_id_v1(
                cluster.peers[index].key.verifying_key().as_bytes()
            ))
        );
        assert_eq!(status["finalized"], false);
        assert!(!status["qc_hash"].is_null());
        let qcs = cluster.peers[index]
            .node
            .store()
            .load_qcs_by_height(cluster.authority.chain_id, cluster.authority.epoch, 1)
            .unwrap();
        assert!(!qcs.is_empty());
        for qc in qcs {
            qc.verify(&cluster.authority.validator_set).unwrap();
            assert_eq!(qc.subject.block_hash, cluster.block_hash);
            assert_eq!(qc.subject.round, round);
            assert!(qc.signed_weight >= 3);
        }
        assert!(status["processed_ingress"].as_u64().unwrap() > 0);
        let rendered = serde_json::to_string(&status).unwrap();
        assert!(
            !rendered.contains(&hex_v1(&cluster.peers[index].key.to_bytes())),
            "status must never serialize a private signer seed"
        );
    }
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_service_real_wss_failover_restart_and_returning_leader() {
    let mut cluster = NetworkCluster::new(9_782_201);
    let leader = cluster.initial_leader();
    let active = (0..4).filter(|&index| index != leader).collect::<Vec<_>>();
    let mut services = (0..4).map(|_| None).collect::<Vec<_>>();
    let initial = cluster.started;
    for &index in &active {
        services[index] = Some(start_test_service(&mut cluster, index, initial));
    }
    assert!(cluster.peers[leader].runtime.is_none());
    let completion =
        prepare_test_services(&cluster, &mut services, &active, initial + ROUND_INTERVAL);
    assert_service_prepare(&cluster, &services, &active, 1);

    let restarted = active[0];
    let previous_qc = services[restarted].as_ref().unwrap().status_json()["qc_hash"].clone();
    let before_restart = durable_seal_facts(cluster.peers[restarted].node.store());
    services[restarted].take();
    cluster.peers[restarted].runtime.take().unwrap().shutdown();
    cluster.peers[restarted].node.reopen_store();
    let recovery = completion + Duration::from_millis(1);
    services[restarted] = Some(start_test_service(&mut cluster, restarted, recovery));
    let status = services[restarted].as_ref().unwrap().status_json();
    assert_eq!(status["prepared"], true);
    assert_eq!(status["qc_hash"], previous_qc);
    assert_eq!(
        durable_seal_facts(cluster.peers[restarted].node.store()),
        before_restart,
        "recovery reuses durable evidence without a fresh signature"
    );

    services[leader] = Some(start_test_service(&mut cluster, leader, recovery));
    prepare_test_services(&cluster, &mut services, &[0, 1, 2, 3], recovery);
    // The restarted node may need no incoming evidence to be prepared; the
    // durable QC checks above establish recovery independently of counters.
    assert_service_prepare(&cluster, &services, &[leader], 1);
    for service in services.iter().flatten() {
        assert_eq!(service.status_json()["prepared"], true);
        assert!(!service.halted());
    }
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_service_two_of_four_queue_limits_and_bad_peer_isolation() {
    let mut cluster = NetworkCluster::new(9_782_202);
    let leader = cluster.initial_leader();
    let active = (0..4)
        .filter(|&index| index != leader)
        .take(2)
        .collect::<Vec<_>>();
    let mut services = (0..4).map(|_| None).collect::<Vec<_>>();
    let initial = cluster.started;
    for &index in &active {
        services[index] = Some(start_test_service(&mut cluster, index, initial));
    }
    let wall_start = Instant::now();
    let mut captured = None;
    while wall_start.elapsed() < Duration::from_secs(3) {
        let arrivals = step_test_services(
            &cluster,
            &mut services,
            &active,
            initial + ROUND_INTERVAL + wall_start.elapsed(),
        );
        if captured.is_none() {
            captured = arrivals.into_iter().next();
        }
        thread::sleep(Duration::from_millis(10));
    }
    let (recipient, authentic) = captured.expect("an actual authenticated WSS event");
    for &index in &active {
        let status = services[index].as_ref().unwrap().status_json();
        assert_eq!(status["round"], 0);
        assert_eq!(status["prepared"], false);
        assert!(status["qc_hash"].is_null());
        assert!(status["processed_ingress"].as_u64().unwrap() > 0);
    }
    let service = services[recipient].as_mut().unwrap();
    let runtime = cluster.peers[recipient].runtime.as_ref().unwrap();
    let before = durable_seal_facts(cluster.peers[recipient].node.store());
    let status_before = service.status_json();
    let mut accepted = 0;
    for counter in 0..40 {
        let mut bad = authentic.clone();
        bad.frame.sequence = bad.frame.sequence.wrapping_add(1);
        bad.object_hash[31] = counter;
        accepted += usize::from(service.enqueue(bad));
    }
    assert!(
        accepted <= 4,
        "one source must have a fixed four-frame staging limit"
    );
    assert!(accepted > 0);
    let other_sources = cluster
        .peers
        .iter()
        .filter(|peer| {
            peer.peer_id != authentic.source_peer_id
                && peer.peer_id != cluster.peers[recipient].peer_id
        })
        .map(|peer| peer.peer_id.clone())
        .collect::<Vec<_>>();
    for source in other_sources {
        for counter in 0..4 {
            let mut other_bad = authentic.clone();
            other_bad.source_peer_id = source.clone();
            other_bad.object_hash[31] = counter;
            other_bad.frame.sequence = other_bad.frame.sequence.wrapping_add(1);
            assert!(
                service.enqueue(other_bad),
                "one filled source queue cannot consume another pinned peer's queue"
            );
        }
    }
    let mut transaction = authentic.clone();
    transaction.payload_class = ProductMainlineOverlayPayloadClassV1::NativeTransaction;
    assert!(!service.enqueue(transaction));
    let mut unknown = authentic.clone();
    unknown.source_peer_id = "unauthenticated-source".into();
    assert!(!service.enqueue(unknown));
    let mut oversize = authentic.clone();
    oversize.frame.payload.resize(
        crate::native_block_seal::round_wire::NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1 + 1,
        0,
    );
    assert!(!service.enqueue(oversize));
    assert_eq!(
        durable_seal_facts(cluster.peers[recipient].node.store()),
        before
    );
    let staged = service.status_json();
    assert_eq!(staged["queued_ingress"], 12);
    assert!(staged["dropped_ingress"].as_u64().unwrap() >= 36);
    assert_eq!(
        staged["processed_ingress"],
        status_before["processed_ingress"]
    );
    let tick = initial + ROUND_INTERVAL + wall_start.elapsed() + Duration::from_secs(1);
    service.poll(runtime, tick).unwrap();
    assert!(
        !service.halted(),
        "invalid remote evidence cannot halt an honest owner"
    );
    assert_eq!(service.status_json()["round"], 0);
    assert_eq!(service.status_json()["prepared"], false);
    assert!(service.status_json()["rejected_ingress"].as_u64().unwrap() > 0);
    assert_eq!(
        service.status_json()["processed_ingress"].as_u64().unwrap()
            - status_before["processed_ingress"].as_u64().unwrap(),
        8,
        "a twelve-frame backlog gets only the configured eight verifications per poll"
    );
    assert_eq!(service.status_json()["queued_ingress"], 4);
    let after_budget = service.status_json();
    service
        .poll(runtime, tick + Duration::from_millis(10))
        .unwrap();
    assert_eq!(
        service.status_json()["processed_ingress"],
        after_budget["processed_ingress"],
        "calls inside the minimum poll interval cannot multiply the crypto budget"
    );
    assert_eq!(service.status_json()["queued_ingress"], 4);
    assert_eq!(
        durable_seal_facts(cluster.peers[recipient].node.store()),
        before
    );

    // Bring back a third survivor, not the old leader: rejected floods neither
    // contribute quorum weight nor poison the normal protocol state.
    let third = (0..4)
        .find(|index| *index != leader && !active.contains(index))
        .unwrap();
    services[third] = Some(start_test_service(&mut cluster, third, initial));
    let survivors = [active[0], active[1], third];
    prepare_test_services(
        &cluster,
        &mut services,
        &survivors,
        tick + Duration::from_millis(11),
    );
    assert_service_prepare(&cluster, &services, &survivors, 1);
}

#[test]
fn native_seal_service_rejects_wrong_candidate_identity_and_missing_ledger_before_signing() {
    let mut cluster = NetworkCluster::new(9_782_203);
    let index = cluster.initial_leader();
    cluster.start_peer(index, cluster.started);
    cluster.peers[index].adapter.take();
    let path = write_service_config(&cluster, index);
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let before = durable_seal_facts(cluster.peers[index].node.store());
    for field in ["block_hash", "height"] {
        let mut bad = original.clone();
        bad[field] = match field {
            "block_hash" => serde_json::json!("ab".repeat(32)),
            "height" => serde_json::json!(2),
            _ => unreachable!(),
        };
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(open_test_service(&cluster, index, &path, cluster.started).is_err());
        assert_eq!(
            durable_seal_facts(cluster.peers[index].node.store()),
            before
        );
    }
    fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    let key_path = path.parent().unwrap().join("signer.hex");
    let other = (index + 1) % cluster.peers.len();
    fs::write(&key_path, hex_v1(&cluster.peers[other].key.to_bytes())).unwrap();
    assert!(open_test_service(&cluster, index, &path, cluster.started).is_err());
    assert_eq!(
        durable_seal_facts(cluster.peers[index].node.store()),
        before
    );
    fs::write(&key_path, hex_v1(&cluster.peers[index].key.to_bytes())).unwrap();
    let missing_ledger = cluster.peers[index]
        .node
        .root
        .join("must-not-create-ledger");
    let config = NovNativeSealServiceConfigV1::load(&path, cluster.authority.chain_id).unwrap();
    assert!(NovNativeSealServiceV1::open(
        config,
        &missing_ledger,
        cluster.peers[index].runtime.as_ref().unwrap(),
        cluster.started,
    )
    .is_err());
    assert!(!missing_ledger.exists());
    assert_eq!(
        durable_seal_facts(cluster.peers[index].node.store()),
        before
    );

    // A fully materialized, parent-continuous candidate observed from a peer
    // still lacks this node's local execution ownership. Refuse it before even
    // initializing the configured seal store, both while active and aborted.
    let remote_ledger = cluster.peers[other].node.ledger();
    let remote_parent = remote_ledger
        .load_by_hash(cluster.authority.chain_id, cluster.block_hash)
        .unwrap()
        .unwrap();
    let observed = commit_block_v1(
        remote_ledger,
        cluster.authority.chain_id,
        2,
        Some(&remote_parent),
        0x52,
    );
    let observed_hash = observed.header.block_hash;
    cluster.peers[index]
        .node
        .ledger()
        .register_observed_unsealed_candidate(observed)
        .unwrap();
    let new_store = cluster.peers[index].node.root.join("must-not-create-seal");
    let mut observed_config = original;
    observed_config["height"] = serde_json::json!(2);
    observed_config["block_hash"] = serde_json::json!(hex_v1(&observed_hash));
    observed_config["seal_store_path"] = serde_json::json!("../must-not-create-seal");
    fs::write(&path, serde_json::to_vec(&observed_config).unwrap()).unwrap();
    assert!(open_test_service(&cluster, index, &path, cluster.started).is_err());
    assert!(!new_store.exists());
    cluster.peers[index]
        .node
        .ledger()
        .abort_unselected_candidate_branch(
            cluster.authority.chain_id,
            observed_hash,
            "service admission test: observed execution was not selected",
        )
        .unwrap();
    assert!(open_test_service(&cluster, index, &path, cluster.started).is_err());
    assert!(!new_store.exists());
    assert_eq!(
        durable_seal_facts(cluster.peers[index].node.store()),
        before
    );
}

#[test]
fn native_seal_service_wrong_runtime_and_backwards_clock_halt_before_signing() {
    let mut cluster = NetworkCluster::new(9_782_204);
    let initial = cluster.started;
    let mut first = start_test_service(&mut cluster, 0, initial);
    let mut second = start_test_service(&mut cluster, 1, initial);
    let before_first = durable_seal_facts(cluster.peers[0].node.store());
    assert!(first
        .poll(cluster.peers[1].runtime.as_ref().unwrap(), initial)
        .is_err());
    assert!(first.halted());
    assert_eq!(first.status_json()["ok"], false);
    assert_eq!(first.status_json()["prepared"], false);
    assert_eq!(first.status_json()["finalized"], false);
    assert_eq!(
        durable_seal_facts(cluster.peers[0].node.store()),
        before_first
    );
    assert!(first
        .poll(
            cluster.peers[0].runtime.as_ref().unwrap(),
            initial + ROUND_INTERVAL
        )
        .is_err());
    assert_eq!(
        durable_seal_facts(cluster.peers[0].node.store()),
        before_first
    );

    let before_second = durable_seal_facts(cluster.peers[1].node.store());
    assert!(second
        .poll(
            cluster.peers[1].runtime.as_ref().unwrap(),
            initial - Duration::from_millis(1),
        )
        .is_err());
    assert!(second.halted());
    assert_eq!(
        durable_seal_facts(cluster.peers[1].node.store()),
        before_second
    );
}

#[test]
fn native_seal_service_durable_qc_loss_halts_and_suppresses_prepared_status() {
    let mut cluster = NetworkCluster::new(9_782_205);
    let initial = cluster.started;
    let mut services = (0..4)
        .map(|index| Some(start_test_service(&mut cluster, index, initial)))
        .collect::<Vec<_>>();
    let completed = prepare_test_services(&cluster, &mut services, &[0, 1, 2, 3], initial);
    assert_service_prepare(&cluster, &services, &[0, 1, 2, 3], 0);
    let index = 0;
    let qc = cluster.peers[index]
        .node
        .store()
        .load_qcs_by_height(cluster.authority.chain_id, cluster.authority.epoch, 1)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    // Corrupt only this test database; completed telemetry must be continuously
    // backed by durable evidence, not held true by an in-memory result cache.
    cluster.peers[index]
        .node
        .store()
        .db
        .delete(qc_object_key_v1(&qc.qc_hash).as_bytes())
        .unwrap();
    let before = durable_seal_facts(cluster.peers[index].node.store());
    let service = services[index].as_mut().unwrap();
    assert!(service
        .poll(
            cluster.peers[index].runtime.as_ref().unwrap(),
            completed + Duration::from_secs(1),
        )
        .is_err());
    assert!(service.halted());
    let status = service.status_json();
    assert_eq!(status["ok"], false);
    assert_eq!(status["prepared"], false);
    assert!(status["qc_hash"].is_null());
    assert!(!status["last_error"].is_null());
    assert_eq!(status["finalized"], false);
    assert_eq!(
        durable_seal_facts(cluster.peers[index].node.store()),
        before
    );
    services[index].take();
    cluster.peers[index].node.reopen_store();
    let config = cluster.peers[index]
        .node
        .root
        .join("service-config/service.json");
    assert!(
        open_test_service(&cluster, index, &config, completed + Duration::from_secs(2),).is_err()
    );
    assert_eq!(
        durable_seal_facts(cluster.peers[index].node.store()),
        before
    );
    cluster.assert_unfinalized();
}
