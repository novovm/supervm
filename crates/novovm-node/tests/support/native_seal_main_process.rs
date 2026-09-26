//! Opt-in real main-process test. Port 443 is part of the signed relay contract.
use super::*;
use ed25519_dalek::SigningKey;
use novovm_network::{
    peer_id_from_ed25519_public_key_v1, sign_bootstrap_manifest_v1, sign_relay_record_v1,
    BootstrapSourceKindV1, RelayEndpointV1, RelayTransportV1, SignedBootstrapManifestV1,
};
use novovm_node::{
    native_block_seal::{
        NovNativeBlockSealStoreV1, NovNativeSealValidatorSetV1, NovNativeSealValidatorV1,
    },
    native_block_seal_overlay::{
        NovNativeSealEpochAuthorityV1, NovNativeSealValidatorTransportBindingV1,
    },
    product_node_overlay::ProductBootstrapSourceV1,
    product_relay_daemon::{run_product_relay_daemon_with_shutdown_v1, ProductRelayDaemonConfigV1},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct Relay {
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<anyhow::Result<()>>>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_cluster(nodes: &[Node], active: &[usize], label: &str, ticks: u64, expect_prepared: bool) {
    let mut children = Vec::new();
    for &index in active {
        let node = &nodes[index];
        let mut cmd = node.command();
        cmd.env("NOVOVM_NODE_MODE", "native_execution_tick")
            .env("NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS", ticks.to_string())
            .env("NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS", "250")
            .env("NOVOVM_NATIVE_EXECUTION_PIPELINE_QUIET_TICKS", "true")
            .env(
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH",
                node.0.join(format!("{label}.progress.json")),
            )
            .env(
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_EXIT_WHEN_SUMMARY_VALID",
                "false",
            )
            .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED", "true")
            .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_CONFIG", "overlay.json")
            .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_SIGNOFF_REQUIRED", "false")
            .env("NOVOVM_NATIVE_SEAL_ENABLED", "true")
            .env("NOVOVM_NATIVE_SEAL_CONFIG", "seal.json")
            .stdout(fs::File::create(node.0.join(format!("{label}.stdout.log"))).unwrap())
            .stderr(fs::File::create(node.0.join(format!("{label}.stderr.log"))).unwrap());
        children.push((index, Child(cmd.spawn().unwrap())));
    }
    let deadline = Instant::now();
    for (index, mut child) in children {
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                deadline.elapsed() < Duration::from_secs(180),
                "main-process deadline"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        let text = fs::read_to_string(nodes[index].0.join(format!("{label}.stdout.log"))).unwrap();
        let error = fs::read_to_string(nodes[index].0.join(format!("{label}.stderr.log"))).unwrap();
        assert_eq!(
            status.success(),
            expect_prepared,
            "node {index}: {error}\n{text}"
        );
        if !expect_prepared {
            assert!(error.contains("without a prepare QC"), "{error}");
        }
        // Startup prints one status line; final summary is the remaining JSON.
        let start = text.find("\n{").expect("final main-node JSON summary") + 1;
        let summary: Value = serde_json::from_str(&text[start..]).unwrap();
        let seal = &summary["product_mainline_overlay"]["native_seal"];
        assert_eq!(seal["prepared"], expect_prepared);
        assert_eq!(seal["halted"], false);
        for field in ["finalized", "safe", "proof_sealed", "chain_canonical"] {
            assert_eq!(seal[field], false);
        }
    }
}

#[test]
#[ignore = "requires exclusive loopback 127.0.0.2:443; run explicitly on a prepared host"]
fn real_aoem_main_nodes_prepare_three_of_four_and_recover() {
    let reserve = std::net::TcpListener::bind("127.0.0.2:443")
        .expect("exclusive loopback 443 required; do not stop other services");
    let (source, block, plan) = source_candidate();
    let mut nodes = vec![source];
    for index in 1..4 {
        let node = Node::new(&format!("seal-validator-{index}"));
        let out = node.execute(&plan, "candidate");
        let actual: NovNativeDurableBlockV1 =
            serde_json::from_value(out["durable_block_candidate_committed"].clone()).unwrap();
        assert_eq!(actual, block);
        nodes.push(node);
    }
    // Test-only disposable identities; signing and transport use different keys.
    let keys: Vec<_> = (0..4)
        .map(|i| SigningKey::from_bytes(&[151 + i; 32]))
        .collect();
    let transport: Vec<_> = (0..4)
        .map(|i| SigningKey::from_bytes(&[161 + i; 32]))
        .collect();
    let validators: Vec<_> = keys
        .iter()
        .map(|k| NovNativeSealValidatorV1::new(*k.verifying_key().as_bytes(), 1).unwrap())
        .collect();
    let peer_ids: Vec<_> = transport
        .iter()
        .map(|k| peer_id_from_ed25519_public_key_v1(k.verifying_key().as_bytes()))
        .collect();
    let set = NovNativeSealValidatorSetV1::new(CHAIN, 1, 1, validators.clone()).unwrap();
    let authority = NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(
        &nodes[0].ledger(),
        set,
        validators
            .iter()
            .enumerate()
            .map(|(i, v)| NovNativeSealValidatorTransportBindingV1 {
                validator_id: v.validator_id,
                transport_peer_id: peer_ids[i].clone(),
            })
            .collect(),
    )
    .unwrap();
    let leader = authority.expected_leader(1, 0).unwrap();
    let leader_index = validators
        .iter()
        .position(|v| v.validator_id == leader)
        .unwrap();
    let active: Vec<_> = (0..4).filter(|i| *i != leader_index).collect();

    let root = Node::new("seal-relay").0;
    let certificate = rcgen::generate_simple_self_signed(vec!["127.0.0.2".into()]).unwrap();
    fs::write(root.join("cert.pem"), certificate.serialize_pem().unwrap()).unwrap();
    fs::write(
        root.join("tls.hex"),
        certificate.serialize_private_key_pem(),
    )
    .unwrap();
    let relay_key = SigningKey::from_bytes(&[171; 32]);
    fs::write(root.join("identity.hex"), hex(&relay_key.to_bytes())).unwrap();
    let config: ProductRelayDaemonConfigV1 = serde_json::from_value(serde_json::json!({
        "bind_addr":"127.0.0.2:443", "tls_cert_path":root.join("cert.pem"), "tls_key_path":root.join("tls.hex"),
        "relay_identity_key_path":root.join("identity.hex"), "report_path":root.join("report.json"),
        "report_interval_ms":20, "session_queue_capacity":128, "offline_queue_per_peer":128,
        "offline_queue_per_source":256, "offline_queue_total":512, "session_ttl_ms":10000,
        "rate_limit_frames":10000, "max_frames_per_window":20000, "rate_limit_window_ms":1000
    })).unwrap();
    drop(reserve);
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let _relay = Relay {
        stop,
        worker: Some(std::thread::spawn(move || {
            run_product_relay_daemon_with_shutdown_v1(config, stopping)
        })),
    };
    let wait = Instant::now();
    while !root.join("report.json").exists() {
        assert!(wait.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let bootstrap_key = SigningKey::from_bytes(&[172; 32]);
    let record = sign_relay_record_v1(
        &relay_key,
        "main-process-test",
        vec![RelayEndpointV1 {
            transport: RelayTransportV1::Wss443,
            uri: "wss://127.0.0.2:443/novovm".into(),
            priority: 1,
            max_sessions: 16,
            max_bytes_per_minute: 64 * 1024 * 1024,
        }],
        now - 1000,
        now + 600_000,
        1,
    )
    .unwrap();
    let mut manifest = SignedBootstrapManifestV1 {
        version: 1,
        manifest_id: "main-process-test".into(),
        issued_at_ms: now - 1000,
        expires_at_ms: now + 600_000,
        candidate_limit: 1,
        full_raw_ip_directory_embedded: false,
        requires_single_official_relay: false,
        requires_single_official_domain: false,
        relay_records: vec![record],
        signatures: vec![],
    };
    sign_bootstrap_manifest_v1(&mut manifest, &bootstrap_key).unwrap();
    let bootstrap = ProductBootstrapSourceV1 {
        source_kind: BootstrapSourceKindV1::EmbeddedInstall,
        priority: 1,
        manifest,
    };
    for (i, node) in nodes.iter().enumerate() {
        fs::write(
            node.0.join("authority.json"),
            serde_json::to_vec(&authority).unwrap(),
        )
        .unwrap();
        fs::write(node.0.join("signer.hex"), hex(&keys[i].to_bytes())).unwrap();
        fs::write(node.0.join("transport.hex"), hex(&transport[i].to_bytes())).unwrap();
        let overlay = serde_json::json!({"chain_id":CHAIN,"role":"duplex", "identity_key_path":"transport.hex",
            "overlay":{"cache_path":"cache.json", "trusted_signer_public_keys":[bootstrap_key.verifying_key().to_bytes()],
                "minimum_bootstrap_signatures":1,"embedded_sources":[bootstrap],"cooldown_base_ms":10,"cooldown_max_ms":100},
            "peers":peer_ids.iter().enumerate().filter(|(j,_)| *j!=i).map(|(j,p)| serde_json::json!({"peer_id":p,"metric_peer_id":8800+j})).collect::<Vec<_>>(),
            "tls_trust":{"explicit_ca":{"certificate_path":root.join("cert.pem")}},
            "metric_peer_id":8800+i,"connect_timeout_ms":1000,"read_timeout_ms":25,
            "channel_capacity":128,"reconnect_base_delay_ms":20,"reconnect_max_delay_ms":200});
        fs::write(
            node.0.join("overlay.json"),
            serde_json::to_vec(&overlay).unwrap(),
        )
        .unwrap();
        fs::write(node.0.join("seal.json"),serde_json::to_vec(&serde_json::json!({
            "schema":"novovm-native-seal-service/v1","enabled":true,"chain_id":CHAIN,"height":1,
            "block_hash":hex(&block.header.block_hash),"authority_path":"authority.json", "signer_key_path":"signer.hex",
            "seal_store_path":"seal-db","round_timeout_ms":15000,"poll_interval_ms":100,
            "ingress_per_source_per_second":8,"ingress_per_poll":16})).unwrap()).unwrap();
    }
    // Same fixed four-member authority throughout. No fake clock or test-driver votes.
    run_cluster(&nodes, &active[..2], "two-of-four", 70, false);
    for &index in &active[..2] {
        let store =
            NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[index].0.join("seal-db"))
                .unwrap()
                .unwrap();
        assert!(store.load_qcs_by_height(CHAIN, 1, 1).unwrap().is_empty());
    }
    run_cluster(&nodes, &active, "three-of-four", 100, true);
    let mut previous_qcs = Vec::new();
    for &index in &active {
        let store =
            NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[index].0.join("seal-db"))
                .unwrap()
                .unwrap();
        let qcs = store.load_qcs_by_height(CHAIN, 1, 1).unwrap();
        assert!(!qcs.is_empty());
        for qc in &qcs {
            qc.verify(&authority.validator_set).unwrap();
            assert_eq!(qc.subject.block_hash, block.header.block_hash);
            assert_eq!(qc.signed_weight, 3);
            assert_eq!(qc.signature_count, 3);
            assert!(qc.threshold_satisfied);
        }
        previous_qcs.push(qcs);
        assert_eq!(
            nodes[index]
                .ledger()
                .load_by_height(CHAIN, 1)
                .unwrap()
                .unwrap(),
            block
        );
    }
    run_cluster(&nodes, &active, "restart", 12, true);
    for (offset, &index) in active.iter().enumerate() {
        let store =
            NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[index].0.join("seal-db"))
                .unwrap()
                .unwrap();
        assert_eq!(
            store.load_qcs_by_height(CHAIN, 1, 1).unwrap(),
            previous_qcs[offset]
        );
        assert_eq!(
            nodes[index]
                .ledger()
                .load_by_height(CHAIN, 1)
                .unwrap()
                .unwrap(),
            block
        );
    }
    fs::write(root.join("acceptance.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "scope":"local_real_aoem_main_process_prepare_qc",
        "chain_id":CHAIN, "height":1, "block_hash":hex(&block.header.block_hash),
        "validator_count":4, "offline_initial_leader_index":leader_index, "active_validator_indices":active,
        "round_timeout_ms":15000, "tick_interval_ms":250,
        "node_evidence_paths":nodes.iter().map(|node| &node.0).collect::<Vec<_>>(),
        "two_of_four_persisted_qc_count":0, "three_of_four_qcs":previous_qcs,
        "restart_preserved_qcs_and_unsealed_ledger":true,
        "proof_sealed":false,"chain_canonical":false,"safe":false,"finalized":false,
        "physical_lan_executed":false,"public_network_executed":false
    })).unwrap()).unwrap();
}
