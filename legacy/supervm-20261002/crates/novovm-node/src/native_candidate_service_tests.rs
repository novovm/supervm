#[test]
fn candidate_workspace_execution_configured_service_rechecks_live_aoem_and_halts_on_abort() {
    use crate::native_block_seal::{
        NovNativeBlockSealStoreV1, NovNativeSealValidatorSetV1, NovNativeSealValidatorV1,
        NovNativeSealLocalProposalRequestV1, NovNativeSealQuorumCertificateV1,
        service::NovNativeSealServiceV1, service_config::NovNativeSealServiceConfigV1,
    };
    use crate::native_block_seal_overlay::{NovNativeSealEpochAuthorityV1, NovNativeSealValidatorTransportBindingV1};
    use crate::native_block_seal::tests::native_seal_round_network::with_service_test_transports;
    let _guard = PLAN_RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let chain = 98_917_310;
    with_plan_runtime(|path, params| {
        let parent = funded_candidate_parent(path, params, chain, &[[0xc3; 32]], raw_fixture(chain, 610));
        let plan = successor_plan(&parent, vec![candidate_workspace_execution_raw(chain, 0, [0xc3; 32], 31, "deposit_reserve")]);
        let input = workspace::create_v1(&plan, params).unwrap();
        workspace::execute_v1(chain, input.workspace_id, params).unwrap();
        let record = workspace::register_block_candidate_v1(chain, input.workspace_id, params).unwrap();
        let ledger_path = nov_native_block_ledger_rocksdb_path_v1(path);
        let ledger = NovNativeBlockLedgerV1::open(&ledger_path).unwrap();
        let before = candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes);
        with_service_test_transports(chain, |peers| {
            let validators: Vec<_> = peers.iter().map(|(_, key)| NovNativeSealValidatorV1::new(key.verifying_key().to_bytes(), 1).unwrap()).collect();
            let set = NovNativeSealValidatorSetV1::new(chain, 1, 1, validators.clone()).unwrap();
            let authority = NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(&ledger, set.clone(),
                peers.iter().zip(&validators).map(|((runtime, _), validator)| NovNativeSealValidatorTransportBindingV1 {
                    validator_id: validator.validator_id, transport_peer_id: runtime.startup().local_peer_id.clone(),
                }).collect()).unwrap();
            let leader = authority.expected_leader(record.height, 0).unwrap();
            let index = validators.iter().position(|v| v.validator_id == leader).unwrap();
            let (runtime, key) = peers[index];
            let root = path.with_extension("isolated-service-test");
            std::fs::create_dir_all(&root).unwrap();
            let seal_path = root.join("seal");
            let seal = NovNativeBlockSealStoreV1::open(&seal_path).unwrap();
            let proposal = seal.sign_local_proposal(&ledger, &NovNativeSealLocalProposalRequestV1 {
                chain_id: chain, block_hash: record.parent_block_hash, round: 0, justify_qc_hash: None,
            }, &set, peers[0].1).unwrap();
            let votes = peers.iter().take(3).map(|(_, key)| seal.sign_local_vote(&ledger, &proposal, &set, key).unwrap()).collect();
            let parent_qc = NovNativeSealQuorumCertificateV1::from_votes(proposal.subject, &set, votes).unwrap();
            seal.persist_local_verified_qc(&ledger, &parent_qc, &set).unwrap();
            std::fs::write(root.join("authority.json"), serde_json::to_vec(&authority).unwrap()).unwrap();
            std::fs::write(root.join("signer.hex"), to_hex(&key.to_bytes())).unwrap();
            let config_path = root.join("service.json");
            std::fs::write(&config_path, serde_json::to_vec(&serde_json::json!({
                "schema":"novovm-native-seal-service/v1", "enabled":true, "decision_v3_enabled":true,
                "isolated_workspace_id":to_hex(&input.workspace_id), "chain_id":chain, "height":record.height,
                "block_hash":to_hex(&record.block_hash), "justify_qc_hash":to_hex(&parent_qc.qc_hash),
                "authority_path":"authority.json", "signer_key_path":"signer.hex", "seal_store_path":"seal",
                "round_timeout_ms":60000, "poll_interval_ms":100, "ingress_per_source_per_second":8, "ingress_per_poll":8,
            })).unwrap()).unwrap();
            let now = std::time::Instant::now();
            let config = || NovNativeSealServiceConfigV1::load(&config_path, chain).unwrap();
            assert!(NovNativeSealServiceV1::open(config(), &ledger_path, runtime, now).is_err());
            let mut service = NovNativeSealServiceV1::open_configured(config(), &ledger_path, params, runtime, now).unwrap();
            service.poll(runtime, now).unwrap();
            assert!(!service.halted());
            let emitted = seal.load_pending_outbox(chain, leader, 128).unwrap();
            let candidate_proposal = emitted.iter().find(|entry| entry.height == record.height && entry.object_kind == "proposal")
                .expect("configured service must actually sign the isolated candidate");
            assert_eq!(seal.load_proposal(candidate_proposal.object_hash).unwrap().unwrap().subject.block_hash, record.block_hash);
            drop(service);
            let mut service = NovNativeSealServiceV1::open_configured(config(), &ledger_path, params, runtime, now).unwrap();
            service.poll(runtime, now).unwrap();
            assert_eq!(seal.load_pending_outbox(chain, leader, 128).unwrap(), emitted);
            workspace::abort_v1(chain, input.workspace_id, params).unwrap();
            assert!(service.poll(runtime, now).is_err());
            assert!(service.halted());
            assert_eq!(seal.load_pending_outbox(chain, leader, 128).unwrap(), emitted);
            assert_eq!(service.status_json()["decision_confirmed"], false);
            assert!(NovNativeSealServiceV1::open_configured(config(), &ledger_path, params, runtime, now).is_err());
        });
        assert_eq!(candidate_workspace_authority_fingerprint(path, params, chain, &plan.tx_hashes), before);
    });
}
