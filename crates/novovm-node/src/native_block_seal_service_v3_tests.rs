// Real WSS event/tick service integration; no direct test-driven V3 signing.
fn start_v3_service(
    cluster: &mut NetworkCluster,
    index: usize,
    now: Instant,
) -> NovNativeSealServiceV1 {
    cluster.start_peer_with_decision(index, now, false, true);
    cluster.peers[index].adapter.take();
    let path = write_service_config(cluster, index);
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["decision_v3_enabled"] = serde_json::json!(true);
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    open_test_service(cluster, index, &path, now).unwrap()
}

#[test]
fn native_seal_service_v3_real_wss_confirmation_restart_and_mode_pin() {
    run_v3_service_scenario(false);
}

#[test]
fn native_seal_service_v3_real_wss_failover_and_returning_leader() {
    run_v3_service_scenario(true);
}

#[test]
fn native_seal_service_v3_recovers_prepare_from_decision_vote_only() {
    recover_v3_with_lost_prepare(9);
}

#[test]
fn native_seal_service_v3_recovers_prepare_from_final_certificate_only() {
    recover_v3_with_lost_prepare(10);
}

#[test]
fn native_seal_service_v3_retains_distinct_decision_votes_in_first_poll() {
    let mut cluster = NetworkCluster::new(9_782_341);
    let initial = cluster.started;
    let delayed = (cluster.initial_leader() + 1) % 4;
    let active = (0..4).filter(|index| *index != delayed).collect::<Vec<_>>();
    let mut services = (0..4)
        .map(|index| Some(start_v3_service(&mut cluster, index, initial)))
        .collect::<Vec<_>>();
    let before = durable_seal_facts(cluster.peers[delayed].node.store());
    let started = Instant::now();
    let mut votes = std::collections::BTreeMap::new();
    while votes.len() < 2 {
        step_test_services(
            &cluster,
            &mut services,
            &active,
            initial + started.elapsed(),
        );
        for event in cluster.peers[delayed]
            .runtime
            .as_ref()
            .unwrap()
            .drain_events(128)
        {
            match event {
                ProductMainlineOverlayEventV1::Inbound(inbound)
                    if inbound.frame.payload.get(10) == Some(&9) =>
                {
                    votes
                        .entry(inbound.source_peer_id.clone())
                        .or_insert(inbound);
                }
                ProductMainlineOverlayEventV1::WorkerFailed(error) => panic!("{error}"),
                _ => (),
            }
        }
        assert!(
            started.elapsed() < NETWORK_DEADLINE,
            "two distinct decision votes required"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let peer = &cluster.peers[delayed];
    let runtime = peer.runtime.as_ref().unwrap();
    let service = services[delayed].as_mut().unwrap();
    assert_eq!(service.status_json()["prepared"], false);
    for inbound in votes.into_values().take(2) {
        assert!(service.enqueue(inbound));
    }
    assert_eq!(durable_seal_facts(peer.node.store()), before);
    let now = initial + started.elapsed();
    service.poll(runtime, now).unwrap();
    assert_eq!(service.status_json()["prepared"], true);
    assert_eq!(service.status_json()["accepted_ingress"], 2);
    assert_eq!(service.status_json()["dropped_ingress"], 0);
    assert_eq!(service.status_json()["decision_confirmed"], false);
    service.poll(runtime, now + Duration::from_secs(1)).unwrap();
    assert_eq!(service.status_json()["decision_confirmed"], true);
    let certificate = peer
        .node
        .store()
        .load_decision_certificate_by_height_v3(
            cluster.authority.chain_id,
            cluster.authority.epoch,
            1,
        )
        .unwrap()
        .unwrap();
    certificate
        .verify(&cluster.authority.validator_set)
        .unwrap();
    assert_eq!(certificate.votes.len(), 3);
    cluster.assert_unfinalized();
}

fn recover_v3_with_lost_prepare(kind: u8) {
    let mut cluster = NetworkCluster::new(9_782_320 + u64::from(kind));
    let initial = cluster.started;
    let delayed = (cluster.initial_leader() + 1) % 4;
    let active = (0..4).filter(|index| *index != delayed).collect::<Vec<_>>();
    let mut services = (0..4)
        .map(|index| Some(start_v3_service(&mut cluster, index, initial)))
        .collect::<Vec<_>>();
    let started = Instant::now();
    let mut filtered = 0;
    let mut admitted = 0;
    let peer = &cluster.peers[delayed];
    let mut verifier = NovNativeSealRoundDriverV1::open_with_decision_mode(
        peer.node.ledger(),
        peer.node.store(),
        cluster.authority.clone(),
        cluster.block_hash,
        None,
        validator_id_v1(peer.key.verifying_key().as_bytes()),
        initial,
        ROUND_INTERVAL,
        false,
        true,
    )
    .unwrap();
    let mut invalid_signature_rejected = false;
    loop {
        let now = initial + started.elapsed();
        step_test_services(&cluster, &mut services, &active, now);
        let peer = &cluster.peers[delayed];
        let runtime = peer.runtime.as_ref().unwrap();
        let service = services[delayed].as_mut().unwrap();
        let before = durable_seal_facts(peer.node.store());
        for event in runtime.drain_events(128) {
            match event {
                ProductMainlineOverlayEventV1::Inbound(inbound) => {
                    if inbound.frame.payload.get(10) == Some(&kind) {
                        if !invalid_signature_rejected {
                            use crate::native_block_seal::round_message::NovNativeSealRoundMessageV1 as Message;
                            let mut damaged = crate::native_block_seal::round_wire::decode_nov_native_seal_round_wire_v1(
                                &inbound.frame.payload, &cluster.authority, 1, &inbound.source_peer_id,
                            ).unwrap();
                            match &mut damaged {
                                Message::DecisionVoteV3 { vote, .. } => vote.signature[0] ^= 1,
                                Message::DecisionCertificateV3 { decision, .. } => {
                                    decision.votes[0].signature[0] ^= 1
                                }
                                _ => unreachable!(),
                            }
                            assert!(verifier
                                .ingest_authenticated(
                                    peer.node.ledger(),
                                    peer.node.store(),
                                    &inbound.source_peer_id,
                                    damaged,
                                )
                                .is_err());
                            assert!(!verifier.status().prepared);
                            assert_eq!(durable_seal_facts(peer.node.store()), before);
                            invalid_signature_rejected = true;
                        }
                        admitted += u64::from(service.enqueue(inbound));
                    } else {
                        filtered += 1;
                    }
                }
                ProductMainlineOverlayEventV1::WorkerFailed(error) => panic!("{error}"),
                _ => (),
            }
        }
        assert_eq!(durable_seal_facts(peer.node.store()), before);
        service.poll(runtime, now).unwrap();
        if services
            .iter()
            .all(|service| service.as_ref().unwrap().status_json()["decision_confirmed"] == true)
        {
            break;
        }
        assert!(
            started.elapsed() < NETWORK_DEADLINE,
            "kind={kind}, delayed={:?}",
            services[delayed].as_ref().unwrap().status_json()
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(filtered > 0 && admitted > 0);
    assert!(invalid_signature_rejected);
    let before = durable_seal_facts(cluster.peers[delayed].node.store());
    services[delayed].take();
    cluster.peers[delayed].runtime.take().unwrap().shutdown();
    cluster.peers[delayed].node.reopen_store();
    let now = initial + started.elapsed();
    let mut restarted = start_v3_service(&mut cluster, delayed, now);
    restarted
        .poll(cluster.peers[delayed].runtime.as_ref().unwrap(), now)
        .unwrap();
    assert_eq!(restarted.status_json()["decision_confirmed"], true);
    assert_eq!(
        durable_seal_facts(cluster.peers[delayed].node.store()),
        before
    );
    cluster.assert_unfinalized();
}

#[test]
fn native_seal_service_v3_cannot_upgrade_an_existing_prepare_binding() {
    let mut cluster = NetworkCluster::new(9_782_312);
    let now = cluster.started;
    drop(start_test_service(&mut cluster, 0, now));
    let before = durable_seal_facts(cluster.peers[0].node.store());
    let path = write_service_config(&cluster, 0);
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["decision_v3_enabled"] = serde_json::json!(true);
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    assert!(open_test_service(&cluster, 0, &path, now).is_err());
    assert_eq!(durable_seal_facts(cluster.peers[0].node.store()), before);
}

fn run_v3_service_scenario(failover: bool) {
    let mut cluster = NetworkCluster::new(9_782_310 + u64::from(failover));
    let initial = cluster.started;
    let mut services = (0..4)
        .map(|index| Some(start_v3_service(&mut cluster, index, initial)))
        .collect::<Vec<_>>();
    let began = Instant::now();
    let survivors = (0..4)
        .filter(|i| *i != cluster.initial_leader())
        .collect::<Vec<_>>();
    let base = if failover {
        initial + ROUND_INTERVAL
    } else {
        initial
    };
    let mut returning = !failover;
    let mut stage_started = Instant::now();
    let mut steps = 0u64;
    let mut max_step = Duration::ZERO;
    let completed = loop {
        let now = base + began.elapsed();
        let active = if !returning {
            survivors.as_slice()
        } else {
            &[0, 1, 2, 3]
        };
        let step_started = Instant::now();
        step_test_services(&cluster, &mut services, active, now);
        steps += 1;
        max_step = max_step.max(step_started.elapsed());
        if !returning
            && survivors
                .iter()
                .all(|&i| services[i].as_ref().unwrap().status_json()["decision_confirmed"] == true)
        {
            returning = true;
            stage_started = Instant::now();
        }
        if services
            .iter()
            .all(|s| s.as_ref().unwrap().status_json()["decision_confirmed"] == true)
        {
            break now;
        }
        assert!(
            stage_started.elapsed() < NETWORK_DEADLINE,
            "V3 service deadline: returning={returning} elapsed={:?} steps={steps} max_step={max_step:?} peers={:?}",
            stage_started.elapsed(),
            services
                .iter()
                .map(|s| s.as_ref().unwrap().status_json())
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(10));
    };
    for (index, service) in services.iter().enumerate() {
        let status = service.as_ref().unwrap().status_json();
        assert_eq!(status["decision_v3_enabled"], true);
        assert_eq!(status["commit_v2_enabled"], false);
        assert_eq!(status["phase"], "DecisionConfirmedV3");
        assert!(status["decision_certificate_hash"].is_string());
        if failover {
            assert!(status["round"].as_u64().unwrap() > 0);
        }
        assert_eq!(status["scope"], "single_height_decision_v3_experimental");
        cluster.peers[index]
            .node
            .store()
            .load_decision_certificate_by_height_v3(cluster.authority.chain_id, 1, 1)
            .unwrap()
            .unwrap()
            .verify(&cluster.authority.validator_set)
            .unwrap();
    }
    services[0].take();
    cluster.peers[0].runtime.take().unwrap().shutdown();
    cluster.peers[0].node.reopen_store();
    let before = durable_seal_facts(cluster.peers[0].node.store());
    let now = completed + Duration::from_millis(1);
    let mut restarted = start_v3_service(&mut cluster, 0, now);
    // Open never signs or claims confirmation before the explicit local tick.
    assert_eq!(durable_seal_facts(cluster.peers[0].node.store()), before);
    assert_eq!(restarted.status_json()["decision_confirmed"], false);
    restarted
        .poll(cluster.peers[0].runtime.as_ref().unwrap(), now)
        .unwrap();
    assert_eq!(restarted.status_json()["decision_confirmed"], true);
    assert_eq!(durable_seal_facts(cluster.peers[0].node.store()), before);
    drop(restarted);

    // Changing configuration cannot downgrade this height to prepare or V2.
    for v2 in [false, true] {
        let path = write_service_config_with_commit(&cluster, 0, v2);
        assert!(open_test_service(&cluster, 0, &path, now).is_err());
        assert_eq!(durable_seal_facts(cluster.peers[0].node.store()), before);
    }
    let path = write_service_config_with_commit(&cluster, 0, true);
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["decision_v3_enabled"] = serde_json::json!(true);
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    assert!(NovNativeSealServiceConfigV1::load(&path, cluster.authority.chain_id).is_err());
    assert_eq!(durable_seal_facts(cluster.peers[0].node.store()), before);
    // Missing archive evidence must suppress a previously confirmed service.
    let mut repaired_config = config;
    repaired_config["commit_v2_enabled"] = serde_json::json!(false);
    fs::write(&path, serde_json::to_vec(&repaired_config).unwrap()).unwrap();
    let mut service = open_test_service(&cluster, 0, &path, now).unwrap();
    service
        .poll(cluster.peers[0].runtime.as_ref().unwrap(), now)
        .unwrap();
    assert_eq!(service.status_json()["decision_confirmed"], true);
    let slot =
        crate::native_block_seal::commit::certificate_height_key(cluster.authority.chain_id, 1, 1);
    cluster.peers[0]
        .node
        .store()
        .db
        .delete(format!("{slot}/decision-v3-certificate-hash"))
        .unwrap();
    assert!(service
        .poll(
            cluster.peers[0].runtime.as_ref().unwrap(),
            now + Duration::from_secs(1)
        )
        .is_err());
    assert_eq!(service.status_json()["decision_confirmed"], false);
    assert!(service.status_json()["decision_certificate_hash"].is_null());
    assert!(service.halted());
    cluster.assert_unfinalized();
}
