// First height: one signer. Successor: four services with separate seal stores
// over real WSS, sharing verified AOEM execution; not four executing nodes.
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
        let load = || {
            let mut config = Config::load(&config_path, chain).unwrap();
            config.receive_successors = parent.is_some();
            config.follow_finalized_tip = parent.is_some();
            config
        };
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
            assert_eq!(service.status_json()["body_delivery_targets"], 3);
            exercise_automatic_body_delivery(
                &peers,
                &authority,
                height,
                &artifact.block().body.raw_txs,
                &runtime.startup().local_peer_id,
                |now| service.poll(runtime, now),
            );
            assert_eq!(service.status_json()["finalized"], false);
            use crate::product_mainline_overlay::ProductMainlineOverlayEventV1;
            use std::time::{Duration, Instant};
            let leader_index = peers
                .iter()
                .position(|(_, candidate)| candidate.verifying_key() == key.verifying_key())
                .unwrap();
            let mut services = vec![(leader_index, service, config_path.clone())];
            for (index, (_, signer)) in peers.iter().enumerate() {
                if index == leader_index {
                    continue;
                }
                let directory = root.join(format!("validator-{index}"));
                fs::create_dir_all(&directory).unwrap();
                fs::write(
                    directory.join("authority.json"),
                    serde_json::to_vec(&authority).unwrap(),
                )
                .unwrap();
                fs::write(directory.join("signer.hex"), to_hex(&signer.to_bytes())).unwrap();
                let path = directory.join("service.json");
                fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
                let service = Service::open_configured(
                    Config::load(&path, chain).unwrap(),
                    &ledger_path,
                    params,
                    peers[index].0,
                    Instant::now(),
                )
                .unwrap();
                services.push((index, service, path));
            }
            let step = |services: &mut Vec<(usize, Service, std::path::PathBuf)>, count: usize| {
                for (index, service, _) in services.iter_mut().take(count) {
                    for event in peers[*index].0.drain_events(128) {
                        if let ProductMainlineOverlayEventV1::Inbound(inbound) = event {
                            service.enqueue(inbound);
                        }
                    }
                    service.poll(peers[*index].0, Instant::now()).unwrap();
                }
            };
            let began = Instant::now();
            while began.elapsed() < Duration::from_secs(2) {
                step(&mut services, 2);
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(services
                .iter()
                .all(|(_, s, _)| s.status_json()["decision_confirmed"] == false));
            let began = Instant::now();
            loop {
                step(&mut services, 3);
                if services[..3]
                    .iter()
                    .all(|(_, s, _)| s.status_json()["decision_confirmed"] == true)
                {
                    break;
                }
                assert!(
                    began.elapsed() < Duration::from_secs(120),
                    "successor WSS quorum deadline: {:?}",
                    services
                        .iter()
                        .map(|(_, s, _)| s.status_json())
                        .collect::<Vec<_>>()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            let confirmed = services[0].1.status_json()["decision_certificate_hash"].clone();
            assert!(!confirmed.is_null());
            for (_, service, _) in &services[..3] {
                assert_eq!(
                    service.status_json()["decision_certificate_hash"],
                    confirmed
                );
                assert_eq!(service.status_json()["finalized"], false);
            }
            // A decision identifies the execution target, not one particular
            // valid quorum witness. Preserve each existing node's exact local
            // certificate across restart; the fourth may first observe another
            // valid signer subset / prepare witness for the identical target.
            let preserved = services[..3]
                .iter()
                .map(|(index, _, path)| {
                    let seal = Seal::open(&path.parent().unwrap().join("seal")).unwrap();
                    let certificate = seal
                        .load_decision_certificate_by_height_v3(chain, 1, height)
                        .unwrap()
                        .unwrap();
                    certificate.verify(compiled.validator_set()).unwrap();
                    (*index, certificate)
                })
                .collect::<std::collections::BTreeMap<_, _>>();
            let expected_target = crate::native_block_seal::commit_v3::decision_target_v3(
                &preserved.values().next().unwrap().prepare,
                compiled.validator_set(),
            )
            .unwrap();
            // Reopen all four: the previously unpolled fourth must catch up from
            // the durable full envelopes retransmitted by the other services.
            let mut reopened = Vec::new();
            for (index, service, path) in services {
                drop(service);
                let service = Service::open_configured(
                    Config::load(&path, chain).unwrap(),
                    &ledger_path,
                    params,
                    peers[index].0,
                    Instant::now(),
                )
                .unwrap();
                reopened.push((index, service, path));
            }
            let began = Instant::now();
            loop {
                step(&mut reopened, 4);
                if reopened
                    .iter()
                    .all(|(_, s, _)| s.status_json()["decision_confirmed"] == true)
                {
                    break;
                }
                assert!(
                    began.elapsed() < Duration::from_secs(120),
                    "successor WSS recovery deadline"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            for (index, service, path) in reopened {
                assert_eq!(service.status_json()["finalized"], false);
                // This fixture shares execution state: keep confirmation separate
                // from the publication/recovery fixture below.
                let seal = Seal::open(&path.parent().unwrap().join("seal")).unwrap();
                let certificate = seal
                    .load_decision_certificate_by_height_v3(chain, 1, height)
                    .unwrap()
                    .unwrap();
                certificate.verify(compiled.validator_set()).unwrap();
                assert_eq!(
                    service.status_json()["decision_certificate_hash"],
                    to_hex(&certificate.certificate_hash)
                );
                if let Some(previous) = preserved.get(&index) {
                    assert_eq!(
                        &certificate, previous,
                        "existing node {index} changed its durable decision witness"
                    );
                }
                assert_eq!(
                    crate::native_block_seal::commit_v3::decision_target_v3(
                        &certificate.prepare,
                        compiled.validator_set(),
                    )
                    .unwrap(),
                    expected_target,
                    "node {index} confirmed a different execution target"
                );
                assert_eq!(
                    certificate.prepare.subject.block_hash,
                    artifact.block().header.block_hash
                );
            }
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

fn exercise_automatic_body_delivery(
    peers: &[(
        &crate::product_mainline_overlay::ProductMainlineOverlayRuntimeV1,
        &ed25519_dalek::SigningKey,
    )],
    authority: &crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1,
    height: u64,
    raws: &[Vec<u8>],
    source: &str,
    mut poll: impl FnMut(std::time::Instant) -> anyhow::Result<()>,
) {
    use crate::native_candidate_body::network::CandidateBodyInboxV1;
    use crate::product_mainline_overlay::ProductMainlineOverlayEventV1 as Event;
    use std::time::{Duration, Instant};
    let started = Instant::now();
    let mut receivers = peers
        .iter()
        .filter(|(r, _)| r.startup().local_peer_id != source)
        .map(|(runtime, _)| {
            (
                *runtime,
                CandidateBodyInboxV1::new(authority.clone(), height, started).unwrap(),
                false,
            )
        })
        .collect::<Vec<_>>();
    while receivers.iter().any(|(_, _, complete)| !complete) {
        poll(Instant::now()).unwrap();
        for (runtime, inbox, complete) in &mut receivers {
            for event in runtime.drain_events(128) {
                if let Event::Inbound(inbound) = event {
                    if let Ok(Some(body)) = inbox.accept(&inbound, Instant::now()) {
                        assert_eq!(body.raw_txs, raws);
                        *complete = true;
                    }
                }
            }
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "automatic body delivery deadline"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
