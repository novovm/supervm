#[test]
fn native_seal_service_v3_split_prepare_rounds_recover_without_resigning_timed_out_round() {
    exercise_split_prepare_rounds(6, false, 1);
}

#[test]
fn native_seal_service_v3_split_prepare_rounds_timeout_restart_and_late_decision_vote() {
    exercise_split_prepare_rounds(9, true, 1);
}

#[test]
fn native_seal_service_v3_two_prepared_two_timeout_recover_after_restart() {
    exercise_split_prepare_rounds(9, true, 2);
}

#[test]
fn native_seal_service_v3_two_prepared_two_timeout_late_qc_keeps_fence() {
    exercise_split_prepare_rounds(6, false, 2);
}

fn exercise_split_prepare_rounds(late_kind: u8, restart_timeout: bool, prepared_count: usize) {
    use crate::native_block_seal::commit_v3::decision_target_v3;
    let mut cluster = NetworkCluster::new(9_782_350 + u64::from(late_kind));
    let initial = cluster.started;
    let prepared = cluster.initial_leader();
    let prepared_peers = (0..prepared_count)
        .map(|offset| (prepared + offset) % 4)
        .collect::<Vec<_>>();
    let advancing = (0..4)
        .filter(|index| !prepared_peers.contains(index))
        .collect::<Vec<_>>();
    let mut services = (0..4)
        .map(|index| Some(start_v3_service(&mut cluster, index, initial)))
        .collect::<Vec<_>>();
    let mut late = std::collections::BTreeMap::new();
    let started = Instant::now();
    loop {
        for (index, service) in services.iter_mut().enumerate() {
            let runtime = cluster.peers[index].runtime.as_ref().unwrap();
            let service = service.as_mut().unwrap();
            for event in runtime.drain_events(128) {
                match event {
                    ProductMainlineOverlayEventV1::Inbound(inbound) => {
                        let kind = inbound.frame.payload.get(10).copied();
                        if kind == Some(4)
                            || (index == prepared && kind == Some(5))
                            || (prepared_peers.contains(&index) && kind == Some(6))
                        {
                            assert!(service.enqueue(inbound));
                        } else if !prepared_peers.contains(&index) && kind == Some(late_kind) {
                            late.entry(index).or_insert(inbound);
                        }
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => panic!("{error}"),
                    _ => (),
                }
            }
            service.poll(runtime, initial + started.elapsed()).unwrap();
        }
        if late.len() == advancing.len()
            && prepared_peers
                .iter()
                .all(|&index| services[index].as_ref().unwrap().status_json()["prepared"] == true)
        {
            break;
        }
        assert!(
            started.elapsed() < NETWORK_DEADLINE,
            "initial prepare partition"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        services[prepared].as_ref().unwrap().status_json()["prepared"],
        true
    );
    for &index in &advancing {
        assert_eq!(
            services[index].as_ref().unwrap().status_json()["prepared"],
            false
        );
    }
    let mut advanced_at = initial + ROUND_INTERVAL + started.elapsed();
    for &index in &advancing {
        let peer = &cluster.peers[index];
        let runtime = peer.runtime.as_ref().unwrap();
        services[index]
            .as_mut()
            .unwrap()
            .poll(runtime, advanced_at)
            .unwrap();
        if restart_timeout {
            services[index].take();
            let path = peer.node.root.join("service-config/service.json");
            services[index] = Some(open_test_service(&cluster, index, &path, advanced_at).unwrap());
        }
        let service = services[index].as_mut().unwrap();
        let timeout = peer
            .node
            .store()
            .load_local_timeout(
                peer.node.ledger(),
                &cluster.authority.validator_set,
                1,
                0,
                validator_id_v1(peer.key.verifying_key().as_bytes()),
            )
            .unwrap()
            .unwrap();
        let before = durable_seal_facts(peer.node.store());
        assert!(service.enqueue(late.remove(&index).unwrap()));
        assert_eq!(durable_seal_facts(peer.node.store()), before);
        service
            .poll(runtime, advanced_at + Duration::from_millis(100))
            .unwrap();
        assert!(!service.halted());
        assert_eq!(service.status_json()["prepared"], false);
        assert_eq!(durable_seal_facts(peer.node.store()), before);
        assert_eq!(
            peer.node
                .store()
                .load_local_timeout(
                    peer.node.ledger(),
                    &cluster.authority.validator_set,
                    1,
                    0,
                    validator_id_v1(peer.key.verifying_key().as_bytes()),
                )
                .unwrap()
                .unwrap(),
            timeout
        );
    }
    let original_locks = prepared_peers
        .iter()
        .map(|&index| split_decision_locks(cluster.peers[index].node.store()))
        .collect::<Vec<_>>();
    if prepared_count == 2 {
        let mut partition_received = BTreeSet::new();
        let partition_started = Instant::now();
        while partition_received.len() < 4 {
            for (index, service) in services.iter_mut().enumerate() {
                let runtime = cluster.peers[index].runtime.as_ref().unwrap();
                let service = service.as_mut().unwrap();
                let group = if prepared_peers.contains(&index) {
                    &prepared_peers
                } else {
                    &advancing
                };
                let expected_kind = if prepared_peers.contains(&index) {
                    9
                } else {
                    1
                };
                for event in runtime.drain_events(128) {
                    match event {
                        ProductMainlineOverlayEventV1::Inbound(inbound)
                            if inbound.frame.payload.get(10) == Some(&expected_kind)
                                && group.iter().any(|&source| {
                                    cluster.peers[source].peer_id == inbound.source_peer_id
                                }) =>
                        {
                            if service.enqueue(inbound) {
                                partition_received.insert(index);
                            }
                        }
                        ProductMainlineOverlayEventV1::WorkerFailed(error) => panic!("{error}"),
                        _ => (),
                    }
                }
                let now = if prepared_peers.contains(&index) {
                    initial + started.elapsed()
                } else {
                    advanced_at + Duration::from_secs(1) + partition_started.elapsed()
                };
                service.poll(runtime, now).unwrap();
                assert_eq!(service.status_json()["round"], 0);
                assert_eq!(service.status_json()["decision_confirmed"], false);
                assert_eq!(
                    service.status_json()["prepared"],
                    prepared_peers.contains(&index)
                );
            }
            assert!(
                partition_started.elapsed() < NETWORK_DEADLINE,
                "2+2 partition delivery"
            );
            thread::sleep(Duration::from_millis(10));
        }
        advanced_at += partition_started.elapsed() + Duration::from_secs(2);
        for &index in &prepared_peers {
            let runtime = cluster.peers[index].runtime.as_ref().unwrap();
            let now = advanced_at + Duration::from_millis(200);
            services[index]
                .as_mut()
                .unwrap()
                .poll(runtime, now)
                .unwrap();
            let timeout = cluster.peers[index]
                .node
                .store()
                .load_local_timeout(
                    cluster.peers[index].node.ledger(),
                    &cluster.authority.validator_set,
                    1,
                    0,
                    validator_id_v1(cluster.peers[index].key.verifying_key().as_bytes()),
                )
                .unwrap();
            assert!(
                timeout.is_some(),
                "prepared minority must participate in timeout"
            );
            services[index].take();
            cluster.peers[index].node.reopen_store();
            let path = cluster.peers[index]
                .node
                .root
                .join("service-config/service.json");
            services[index] = Some(open_test_service(&cluster, index, &path, now).unwrap());
        }
    }
    let round_started = Instant::now();
    let mut reopened_after_tc = false;
    loop {
        for (index, service) in services.iter_mut().enumerate() {
            let runtime = cluster.peers[index].runtime.as_ref().unwrap();
            let service = service.as_mut().unwrap();
            for event in runtime.drain_events(128) {
                match event {
                    ProductMainlineOverlayEventV1::Inbound(inbound) => {
                        let kind = inbound.frame.payload.get(10).copied();
                        if !matches!(kind, Some(9 | 10))
                            && (prepared_count == 2
                                || (index != prepared
                                    && inbound.source_peer_id != cluster.peers[prepared].peer_id))
                        {
                            service.enqueue(inbound);
                        }
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => panic!("{error}"),
                    _ => (),
                }
            }
            service
                .poll(
                    runtime,
                    advanced_at + Duration::from_secs(1) + round_started.elapsed(),
                )
                .unwrap();
        }
        if prepared_count == 2
            && !reopened_after_tc
            && prepared_peers
                .iter()
                .all(|&index| services[index].as_ref().unwrap().status_json()["round"] == 1)
        {
            for &index in &prepared_peers {
                let before = durable_seal_facts(cluster.peers[index].node.store());
                services[index].take();
                cluster.peers[index].node.reopen_store();
                let path = cluster.peers[index]
                    .node
                    .root
                    .join("service-config/service.json");
                services[index] = Some(
                    open_test_service(
                        &cluster,
                        index,
                        &path,
                        advanced_at + Duration::from_secs(1) + round_started.elapsed(),
                    )
                    .unwrap(),
                );
                assert_eq!(
                    durable_seal_facts(cluster.peers[index].node.store()),
                    before
                );
            }
            reopened_after_tc = true;
        }
        if advancing
            .iter()
            .all(|&index| services[index].as_ref().unwrap().status_json()["prepared"] == true)
        {
            break;
        }
        assert!(
            round_started.elapsed() < NETWORK_DEADLINE,
            "split prepare convergence: {:?}",
            services
                .iter()
                .map(|service| service.as_ref().unwrap().status_json())
                .collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(prepared_count == 1 || reopened_after_tc);
    for (index, service) in services.iter().enumerate() {
        let status = service.as_ref().unwrap().status_json();
        assert_eq!(
            status["round"],
            if prepared_count == 1 && index == prepared {
                0
            } else {
                1
            }
        );
        assert_eq!(status["prepared"], true);
        assert_eq!(status["decision_confirmed"], false);
    }
    for (&index, original) in prepared_peers.iter().zip(&original_locks) {
        assert_eq!(
            &split_decision_locks(cluster.peers[index].node.store()),
            original
        );
        if prepared_count == 2 {
            let before = durable_seal_facts(cluster.peers[index].node.store());
            services[index].take();
            assert_advanced_prepared_evidence_required(
                &cluster,
                index,
                advanced_at + Duration::from_secs(1) + round_started.elapsed(),
            );
            cluster.peers[index].node.reopen_store();
            let path = cluster.peers[index]
                .node
                .root
                .join("service-config/service.json");
            services[index] = Some(
                open_test_service(
                    &cluster,
                    index,
                    &path,
                    advanced_at + Duration::from_secs(1) + round_started.elapsed(),
                )
                .unwrap(),
            );
            assert_eq!(
                durable_seal_facts(cluster.peers[index].node.store()),
                before
            );
        }
    }
    let decision_locks = cluster
        .peers
        .iter()
        .map(|peer| split_decision_locks(peer.node.store()))
        .collect::<Vec<_>>();
    assert!(decision_locks.iter().all(|locks| locks.len() == 1));
    let offline = *advancing.last().unwrap();
    let active = (0..4).filter(|index| *index != offline).collect::<Vec<_>>();
    services[offline].take();
    cluster.peers[offline].runtime.take().unwrap().shutdown();
    let mut now = advanced_at + Duration::from_secs(2) + round_started.elapsed();
    let minority = [prepared, advancing[0]];
    let mut minority_received = BTreeSet::new();
    let minority_started = Instant::now();
    loop {
        for &index in &minority {
            let runtime = cluster.peers[index].runtime.as_ref().unwrap();
            let service = services[index].as_mut().unwrap();
            for event in runtime.drain_events(128) {
                match event {
                    ProductMainlineOverlayEventV1::Inbound(inbound)
                        if inbound.frame.payload.get(10) == Some(&9)
                            && minority.iter().any(|&source| {
                                cluster.peers[source].peer_id == inbound.source_peer_id
                            }) =>
                    {
                        if service.enqueue(inbound) {
                            minority_received.insert(index);
                        }
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => panic!("{error}"),
                    _ => (),
                }
            }
            service
                .poll(runtime, now + minority_started.elapsed())
                .unwrap();
            assert_eq!(service.status_json()["decision_confirmed"], false);
        }
        if minority_received.len() == 2 {
            break;
        }
        assert!(
            minority_started.elapsed() < NETWORK_DEADLINE,
            "minority delivery"
        );
        thread::sleep(Duration::from_millis(10));
    }
    now += minority_started.elapsed() + Duration::from_secs(1);
    let decision_started = Instant::now();
    loop {
        for &index in &active {
            let runtime = cluster.peers[index].runtime.as_ref().unwrap();
            let service = services[index].as_mut().unwrap();
            for event in runtime.drain_events(128) {
                match event {
                    ProductMainlineOverlayEventV1::Inbound(inbound)
                        if matches!(inbound.frame.payload.get(10), Some(9 | 10))
                            && inbound.source_peer_id != cluster.peers[offline].peer_id =>
                    {
                        service.enqueue(inbound);
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => panic!("{error}"),
                    _ => (),
                }
            }
            service
                .poll(runtime, now + decision_started.elapsed())
                .unwrap();
        }
        if active.iter().all(|&index| {
            services[index].as_ref().unwrap().status_json()["decision_confirmed"] == true
        }) {
            break;
        }
        assert!(
            decision_started.elapsed() < NETWORK_DEADLINE,
            "cross-round decisions did not converge"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let mut target = None;
    for &index in &active {
        let peer = &cluster.peers[index];
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
        assert_eq!(
            certificate.prepare.subject.round,
            if prepared_peers.contains(&index) {
                0
            } else {
                1
            }
        );
        let actual =
            decision_target_v3(&certificate.prepare, &cluster.authority.validator_set).unwrap();
        assert_eq!(*target.get_or_insert(actual), actual);
        assert_eq!(certificate.votes.len(), 3);
        assert!(certificate.votes.iter().all(|vote| vote.validator_id
            != validator_id_v1(cluster.peers[offline].key.verifying_key().as_bytes())));
        assert_eq!(
            split_decision_locks(peer.node.store()),
            decision_locks[index]
        );
        let before = durable_seal_facts(peer.node.store());
        services[index].take();
        cluster.peers[index].node.reopen_store();
        let path = cluster.peers[index]
            .node
            .root
            .join("service-config/service.json");
        let restarted_at = now + decision_started.elapsed() + Duration::from_secs(1);
        let mut restarted = open_test_service(&cluster, index, &path, restarted_at).unwrap();
        restarted
            .poll(cluster.peers[index].runtime.as_ref().unwrap(), restarted_at)
            .unwrap();
        assert_eq!(restarted.status_json()["decision_confirmed"], true);
        assert_eq!(
            durable_seal_facts(cluster.peers[index].node.store()),
            before
        );
        services[index] = Some(restarted);
    }
    cluster.assert_unfinalized();
}

fn assert_advanced_prepared_evidence_required(
    cluster: &NetworkCluster,
    index: usize,
    now: Instant,
) {
    let peer = &cluster.peers[index];
    let store = peer.node.store();
    let locks = split_decision_locks(store);
    let original: serde_json::Value = serde_json::from_slice(&locks[0].1).unwrap();
    let original_qc_hash: [u8; 32] =
        serde_json::from_value(original["prepare_qc_hash"].clone()).unwrap();
    let original_qc = store.load_qc(original_qc_hash).unwrap().unwrap();
    let marker = format!(
        "{}/decision-v3-vote-hash",
        String::from_utf8(locks[0].0.clone()).unwrap()
    );
    let path = peer.node.root.join("service-config/service.json");
    for record_key in [
        locks[0].0.clone(),
        marker.as_bytes().to_vec(),
        qc_object_key_v1(&original_qc.qc_hash).into_bytes(),
        proposal_object_key_v1(&original_qc.proposal_hash).into_bytes(),
    ] {
        let record = store.db.get(&record_key).unwrap().unwrap();
        store.db.delete(&record_key).unwrap();
        let corrupted = durable_seal_facts(store);
        assert!(open_test_service(cluster, index, &path, now).is_err());
        assert_eq!(durable_seal_facts(store), corrupted);
        store.db.put(&record_key, &record).unwrap();
    }
    let marker_record = store.db.get(marker.as_bytes()).unwrap().unwrap();
    store.db.delete(&locks[0].0).unwrap();
    store.db.delete(marker.as_bytes()).unwrap();
    let corrupted = durable_seal_facts(store);
    assert!(open_test_service(cluster, index, &path, now).is_err());
    assert_eq!(durable_seal_facts(store), corrupted);
    store.db.put(&locks[0].0, &locks[0].1).unwrap();
    store.db.put(marker.as_bytes(), &marker_record).unwrap();
    assert_eq!(split_decision_locks(store), locks);
}

fn split_decision_locks(store: &NovNativeBlockSealStoreV1) -> Vec<(Vec<u8>, Vec<u8>)> {
    durable_seal_facts(store)
        .into_iter()
        .filter(|(_, value)| {
            serde_json::from_slice::<serde_json::Value>(value)
                .ok()
                .is_some_and(|value| value["schema"] == "novovm-native-seal-decision-lock/v3")
        })
        .collect()
}
