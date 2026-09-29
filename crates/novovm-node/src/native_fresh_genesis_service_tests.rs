// One configured live signer over a real WSS mesh; not four executing nodes.
fn exercise_fresh_genesis_service(
    path: &Path,
    params: &serde_json::Value,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    id: [u8; 32],
) {
    exercise_fresh_candidate_service(path, params, compiled, id, None);
}

fn exercise_fresh_candidate_service(
    path: &Path,
    params: &serde_json::Value,
    compiled: &crate::tx_ingress::fresh_genesis::CompiledFreshGenesisV1,
    id: [u8; 32],
    parent: Option<[u8; 32]>,
) {
    use crate::native_block_seal::tests::native_seal_round_network::with_service_test_transports;
    use crate::native_block_seal::{
        service::NovNativeSealServiceV1 as Service,
        service_config::NovNativeSealServiceConfigV1 as Config, NovNativeBlockSealStoreV1 as Seal,
    };
    use crate::native_block_seal_overlay::{
        NovNativeSealEpochAuthorityV1 as Authority,
        NovNativeSealValidatorTransportBindingV1 as Binding,
    };
    let chain = compiled.identity().chain_id();
    let pin = compiled.config_commitment();
    let artifact = workspace::load_block_artifact_v1(chain, id, params)
        .unwrap()
        .unwrap();
    with_service_test_transports(chain, |peers| {
        let bindings = peers
            .iter()
            .map(|(runtime, key)| Binding {
                validator_id: compiled
                    .validator_set()
                    .validators
                    .iter()
                    .find(|v| v.public_key == key.verifying_key().to_bytes())
                    .unwrap()
                    .validator_id,
                transport_peer_id: runtime.startup().local_peer_id.clone(),
            })
            .collect();
        let derive = |view: &crate::native_block_ledger::NovNativeBlockLedgerV1| {
            let (config, _) = view.fresh_genesis_seal_config_v1(chain)?.unwrap();
            Authority::derive_operator_pinned_fresh_genesis_epoch(config, pin, bindings)
        };
        let authority = match parent {
            Some(parent) => workspace::with_verified_finalized_successor_v1(
                chain, parent, id, pin, params, derive,
            ),
            None => {
                workspace::with_verified_genesis_block_candidate_v1(chain, id, pin, params, derive)
            }
        }
        .unwrap();
        let height = artifact.block().header.height;
        let leader = authority.expected_leader(height, 0).unwrap();
        let (runtime, key) = peers
            .iter()
            .find(|(_, key)| {
                authority
                    .validator_set
                    .validator(leader)
                    .unwrap()
                    .public_key
                    == key.verifying_key().to_bytes()
            })
            .unwrap();
        let root = path.with_extension(format!("fresh-service-{height}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("authority.json"),
            serde_json::to_vec(&authority).unwrap(),
        )
        .unwrap();
        fs::write(root.join("signer.hex"), to_hex(&key.to_bytes())).unwrap();
        let config_path = root.join("service.json");
        let config = serde_json::json!({
            "schema":"novovm-native-seal-service/v1", "enabled":true, "decision_v3_enabled":true,
            "fresh_genesis_config_commitment":to_hex(&pin), "isolated_workspace_id":to_hex(&id),
            "finalized_parent_workspace_id":parent.map(|id| to_hex(&id)),
            "chain_id":chain, "height":height, "block_hash":to_hex(&artifact.block().header.block_hash),
            "authority_path":"authority.json", "signer_key_path":"signer.hex", "seal_store_path":"seal",
            "round_timeout_ms":300000, "poll_interval_ms":100,
            "ingress_per_source_per_second":16, "ingress_per_poll":32,
        });
        fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        let load = || Config::load(&config_path, chain).unwrap();
        let ledger_path = nov_native_block_ledger_rocksdb_path_v1(path);
        let now = std::time::Instant::now();
        assert!(Service::open(load(), &ledger_path, runtime, now).is_err());
        let mut bad = config.clone();
        bad["fresh_genesis_config_commitment"] = serde_json::json!("99".repeat(32));
        fs::write(&config_path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(Service::open_configured(load(), &ledger_path, params, runtime, now).is_err());
        assert!(!root.join("seal").exists());
        if parent.is_some() {
            let mut bad = config.clone();
            bad["finalized_parent_workspace_id"] = serde_json::json!("99".repeat(32));
            fs::write(&config_path, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(Service::open_configured(load(), &ledger_path, params, runtime, now).is_err());
            assert!(!root.join("seal").exists());
        }
        fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        let mut service =
            Service::open_configured(load(), &ledger_path, params, runtime, now).unwrap();
        service.poll(runtime, now).unwrap();
        let seal = Seal::open(&root.join("seal")).unwrap();
        let emitted = seal.load_pending_outbox(chain, leader, 128).unwrap();
        let proposal = emitted
            .iter()
            .find(|entry| entry.height == height && entry.object_kind == "proposal")
            .unwrap();
        let subject = seal
            .load_proposal(proposal.object_hash)
            .unwrap()
            .unwrap()
            .subject;
        assert_eq!(subject.genesis_block_hash, compiled.identity().anchor());
        assert_eq!(subject.block_hash, artifact.block().header.block_hash);
        assert!(!service.halted());
        drop(service);
        let mut service =
            Service::open_configured(load(), &ledger_path, params, runtime, now).unwrap();
        service.poll(runtime, now).unwrap();
        assert_eq!(
            seal.load_pending_outbox(chain, leader, 128).unwrap(),
            emitted
        );
        if parent.is_some() {
            assert!(service
                .complete_fresh_publication(runtime, now)
                .unwrap()
                .is_none());
            assert_eq!(service.status_json()["finalized"], false);
            return;
        }
        workspace::abort_v1(chain, id, params).unwrap();
        assert!(service.poll(runtime, now).is_err());
        assert!(service.halted());
        assert_eq!(
            seal.load_pending_outbox(chain, leader, 128).unwrap(),
            emitted
        );
        assert_eq!(service.status_json()["decision_confirmed"], false);
        assert!(Service::open_configured(load(), &ledger_path, params, runtime, now).is_err());
    });
}
