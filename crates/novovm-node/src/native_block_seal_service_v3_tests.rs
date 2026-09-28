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
    let completed = loop {
        let now = base + began.elapsed();
        let active = if !returning {
            survivors.as_slice()
        } else {
            &[0, 1, 2, 3]
        };
        step_test_services(&cluster, &mut services, active, now);
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
            "V3 service deadline: {:?}",
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
