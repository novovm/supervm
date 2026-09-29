#[test]
fn candidate_workspace_execution_configured_services_real_aoem_quorum_and_restart() {
    use crate::native_block_seal::tests::native_seal_round_network::with_service_test_transports;
    use crate::native_block_seal::{
        service::NovNativeSealServiceV1, service_config::NovNativeSealServiceConfigV1,
        NovNativeBlockSealStoreV1, NovNativeSealLocalProposalRequestV1,
        NovNativeSealQuorumCertificateV1, NovNativeSealValidatorSetV1, NovNativeSealValidatorV1,
    };
    use crate::native_block_seal_overlay::{
        NovNativeSealEpochAuthorityV1, NovNativeSealValidatorTransportBindingV1,
    };
    use crate::product_mainline_overlay::ProductMainlineOverlayEventV1;
    use std::time::{Duration, Instant};
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let chain = 98_917_311;
    with_plan_runtime(|base_path, base_params| {
        let root = base_path.with_extension("isolated-quorum-test");
        std::fs::create_dir_all(&root).unwrap();
        let mut nodes = Vec::new();
        for index in 0..4 {
            let directory = root.join(format!("node-{index}"));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("native.json");
            let mut params = base_params.clone();
            params["native_execution_store_path"] = serde_json::json!(path);
            params["aoem_state_namespace"] = serde_json::json!(format!("isolated-quorum-{index}"));
            params["aoem_owned_state_db_path"] = serde_json::json!(directory.join("aoem-owned"));
            let parent = funded_candidate_parent(
                &path,
                &params,
                chain,
                &[[0xc4; 32]],
                raw_fixture(chain, 611),
            );
            let plan = successor_plan(
                &parent,
                vec![candidate_workspace_execution_raw(
                    chain,
                    0,
                    [0xc4; 32],
                    31,
                    "deposit_reserve",
                )],
            );
            let input = workspace::create_v1(&plan, &params).unwrap();
            workspace::execute_v1(chain, input.workspace_id, &params).unwrap();
            let record =
                workspace::register_block_candidate_v1(chain, input.workspace_id, &params).unwrap();
            let before =
                candidate_workspace_authority_fingerprint(&path, &params, chain, &plan.tx_hashes);
            nodes.push((
                directory,
                path,
                params,
                input.workspace_id,
                record,
                plan.tx_hashes,
                before,
            ));
        }
        assert!(nodes
            .iter()
            .all(|node| node.4.block_hash == nodes[0].4.block_hash));
        // Fingerprints include process-wide ingress caches. Capture all baselines
        // after all four independent executions, not between fixture setup writes.
        for node in &mut nodes {
            node.6 = candidate_workspace_authority_fingerprint(&node.1, &node.2, chain, &node.5);
        }
        with_service_test_transports(chain, |peers| {
            let validators: Vec<_> = peers
                .iter()
                .map(|(_, key)| {
                    NovNativeSealValidatorV1::new(key.verifying_key().to_bytes(), 1).unwrap()
                })
                .collect();
            let set = NovNativeSealValidatorSetV1::new(chain, 1, 1, validators.clone()).unwrap();
            let ledger =
                NovNativeBlockLedgerV1::open(&nov_native_block_ledger_rocksdb_path_v1(&nodes[0].1))
                    .unwrap();
            let authority = NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(
                &ledger,
                set.clone(),
                peers
                    .iter()
                    .zip(&validators)
                    .map(
                        |((runtime, _), v)| NovNativeSealValidatorTransportBindingV1 {
                            validator_id: v.validator_id,
                            transport_peer_id: runtime.startup().local_peer_id.clone(),
                        },
                    )
                    .collect(),
            )
            .unwrap();
            let leader = validators
                .iter()
                .position(|v| {
                    v.validator_id == authority.expected_leader(nodes[0].4.height, 0).unwrap()
                })
                .unwrap();
            let mut active = vec![leader];
            active.extend((0..4).filter(|i| *i != leader));
            let now = Instant::now();
            let mut services = Vec::new();
            for (index, (directory, path, params, id, record, _, _)) in nodes.iter().enumerate() {
                let ledger_path = nov_native_block_ledger_rocksdb_path_v1(path);
                let ledger = NovNativeBlockLedgerV1::open(&ledger_path).unwrap();
                let seal = NovNativeBlockSealStoreV1::open(&directory.join("seal")).unwrap();
                let proposal = seal
                    .sign_local_proposal(
                        &ledger,
                        &NovNativeSealLocalProposalRequestV1 {
                            chain_id: chain,
                            block_hash: record.parent_block_hash,
                            round: 0,
                            justify_qc_hash: None,
                        },
                        &set,
                        peers[0].1,
                    )
                    .unwrap();
                let votes = peers
                    .iter()
                    .take(3)
                    .map(|(_, key)| seal.sign_local_vote(&ledger, &proposal, &set, key).unwrap())
                    .collect();
                let parent_qc =
                    NovNativeSealQuorumCertificateV1::from_votes(proposal.subject, &set, votes)
                        .unwrap();
                seal.persist_local_verified_qc(&ledger, &parent_qc, &set)
                    .unwrap();
                std::fs::write(
                    directory.join("authority.json"),
                    serde_json::to_vec(&authority).unwrap(),
                )
                .unwrap();
                std::fs::write(
                    directory.join("signer.hex"),
                    to_hex(&peers[index].1.to_bytes()),
                )
                .unwrap();
                let config_path = directory.join("service.json");
                std::fs::write(&config_path, serde_json::to_vec(&serde_json::json!({
                    "schema":"novovm-native-seal-service/v1", "enabled":true, "decision_v3_enabled":true,
                    "isolated_workspace_id":to_hex(id), "chain_id":chain, "height":record.height,
                    "block_hash":to_hex(&record.block_hash), "justify_qc_hash":to_hex(&parent_qc.qc_hash),
                    "authority_path":"authority.json", "signer_key_path":"signer.hex", "seal_store_path":"seal",
                    "round_timeout_ms":60000, "poll_interval_ms":100, "ingress_per_source_per_second":16, "ingress_per_poll":32,
                })).unwrap()).unwrap();
                let config = NovNativeSealServiceConfigV1::load(&config_path, chain).unwrap();
                services.push(
                    NovNativeSealServiceV1::open_configured(
                        config,
                        &ledger_path,
                        params,
                        peers[index].0,
                        now,
                    )
                    .unwrap(),
                );
            }
            let step = |services: &mut Vec<NovNativeSealServiceV1>, indexes: &[usize]| {
                for &index in indexes {
                    for event in peers[index].0.drain_events(128) {
                        if let ProductMainlineOverlayEventV1::Inbound(inbound) = event {
                            services[index].enqueue(inbound);
                        }
                    }
                    services[index]
                        .poll(peers[index].0, Instant::now())
                        .unwrap();
                }
            };
            let began = Instant::now();
            while began.elapsed() < Duration::from_secs(2) {
                step(&mut services, &active[..2]);
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(services
                .iter()
                .all(|service| service.status_json()["decision_confirmed"] == false));
            let began = Instant::now();
            loop {
                step(&mut services, &active[..3]);
                if active[..3]
                    .iter()
                    .all(|&i| services[i].status_json()["decision_confirmed"] == true)
                {
                    break;
                }
                assert!(
                    began.elapsed() < Duration::from_secs(40),
                    "isolated quorum deadline: {:?}",
                    services.iter().map(|s| s.status_json()).collect::<Vec<_>>()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            let hashes: Vec<_> = active[..3]
                .iter()
                .map(|&i| services[i].status_json()["decision_certificate_hash"].clone())
                .collect();
            assert!(hashes
                .iter()
                .all(|hash| hash == &hashes[0] && !hash.is_null()));
            drop(services);
            for &index in &active[..3] {
                let (directory, path, params, _, record, _, _) = &nodes[index];
                let config =
                    NovNativeSealServiceConfigV1::load(&directory.join("service.json"), chain)
                        .unwrap();
                let mut service = NovNativeSealServiceV1::open_configured(
                    config,
                    &nov_native_block_ledger_rocksdb_path_v1(path),
                    params,
                    peers[index].0,
                    Instant::now(),
                )
                .unwrap();
                service.poll(peers[index].0, Instant::now()).unwrap();
                let status = service.status_json();
                assert_eq!(status["decision_certificate_hash"], hashes[0]);
                assert_eq!(status["finalized"], false);
                let seal = NovNativeBlockSealStoreV1::open(&directory.join("seal")).unwrap();
                let certificate = seal
                    .load_decision_certificate_by_height_v3(chain, 1, record.height)
                    .unwrap()
                    .unwrap();
                certificate.verify(&set).unwrap();
            }
        });
        for (_, path, params, _, _, hashes, before) in &nodes {
            let after = candidate_workspace_authority_fingerprint(path, params, chain, hashes);
            for (field, expected) in before.as_object().unwrap() {
                assert!(
                    after.get(field) == Some(expected),
                    "authority changed: {} field={field}",
                    path.display()
                );
            }
        }
    });
}
