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
use sha2::{Digest, Sha256};
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
#[path = "native_seal_continuous_process.rs"]
mod continuous;
#[path = "native_seal_failover_process.rs"]
mod failover;
#[path = "native_seal_partition_process.rs"]
mod partition;
#[path = "native_seal_storage_process.rs"]
mod storage;
#[path = "native_seal_storage_startup_process.rs"]
mod storage_startup;
#[path = "native_seal_successor_process.rs"]
mod successor;
#[path = "native_transfer_mixed_process.rs"]
mod transfer_mixed;
#[path = "native_transfer_throughput_process.rs"]
mod transfer_throughput;

enum TransferScenario {
    Throughput(transfer_throughput::TransportProfile),
    ContinuousBacklog,
    ContinuousBacklogDurableReceipts,
    MixedParity,
}
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_cluster(
    nodes: &[Node],
    active: &[usize],
    label: &str,
    ticks: u64,
    expect_prepared: bool,
    decision_v3: bool,
    fresh: bool,
) {
    run_cluster_at_height(
        nodes,
        active,
        label,
        ticks,
        expect_prepared,
        decision_v3,
        fresh,
        1,
        None,
        false,
    );
}

#[allow(clippy::too_many_arguments)]
fn run_cluster_at_height(
    nodes: &[Node],
    active: &[usize],
    label: &str,
    ticks: u64,
    expect_prepared: bool,
    decision_v3: bool,
    fresh: bool,
    height: u64,
    inject: Option<&dyn Fn()>,
    kill_after_finalized: bool,
) {
    let mut children = start_cluster(nodes, active, label, ticks);
    finish_cluster(
        nodes,
        &mut children,
        label,
        expect_prepared,
        decision_v3,
        fresh,
        height,
        inject,
        kill_after_finalized,
    );
}

fn start_cluster(nodes: &[Node], active: &[usize], label: &str, ticks: u64) -> Vec<(usize, Child)> {
    let mut children = Vec::new();
    for &index in active {
        let node = &nodes[index];
        let mut cmd = node.command();
        if label.starts_with("continuous") {
            cmd.env("NOVOVM_NATIVE_FRESH_RPC_BIND", "127.0.0.1:0");
        }
        // Preserve environment isolation; forward only this explicit diagnostic
        // switch, and only to the measured workload (not genesis/recovery).
        if (label == transfer_throughput::LABEL
            || label == transfer_throughput::DURABLE_RECEIPTS_LABEL)
            && transfer_throughput::diagnostics_enabled()
        {
            cmd.env("NOVOVM_NATIVE_FRESH_TIMING", "1");
        }
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
        node.assert_execution_policy(&cmd);
        let child = Child(cmd.spawn().unwrap());
        fs::write(
            node.0.join(format!("{label}.process.json")),
            serde_json::to_vec(&serde_json::json!({"node_index":index,"pid":child.0.id()}))
                .unwrap(),
        )
        .unwrap();
        children.push((index, child));
    }
    children
}

#[allow(clippy::too_many_arguments)]
fn finish_cluster(
    nodes: &[Node],
    children: &mut Vec<(usize, Child)>,
    label: &str,
    expect_prepared: bool,
    decision_v3: bool,
    fresh: bool,
    height: u64,
    inject: Option<&dyn Fn()>,
    kill_after_finalized: bool,
) {
    let deadline = Instant::now();
    if let Some(inject) = inject {
        inject();
    }
    let mut before_kill = std::collections::BTreeMap::new();
    if kill_after_finalized {
        assert!(fresh && decision_v3 && expect_prepared);
        loop {
            for (index, child) in children.iter_mut() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "node exited before forced termination"
                );
                let text = fs::read_to_string(nodes[*index].0.join(format!("{label}.stdout.log")))
                    .unwrap();
                for line in text.lines() {
                    let Some(json) = line.strip_prefix("native_fresh_genesis_decision_confirmed: ")
                    else {
                        continue;
                    };
                    let Ok(value) = serde_json::from_str::<Value>(json) else {
                        continue;
                    };
                    if value["height"] == height
                        && value["finalized"] == true
                        && value["publication"]["ledger_publication_completed"] == true
                    {
                        before_kill.insert(*index, value);
                    }
                }
            }
            if before_kill.len() == children.len() {
                break;
            }
            assert!(
                deadline.elapsed() < Duration::from_secs(180),
                "finalized-before-kill deadline"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        // Only handles spawned above, never a process-name-wide kill. Wait for
        // every voter to publish before removing all three simultaneously.
        for (_, child) in children.iter_mut() {
            assert!(child.0.try_wait().unwrap().is_none());
            child.0.kill().unwrap();
        }
    }
    for (index, mut child) in children.drain(..) {
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
            !kill_after_finalized && (expect_prepared || fresh),
            "node {index}: {error}\n{text}"
        );
        if !expect_prepared && !fresh {
            assert!(error.contains("without a prepare QC"), "{error}");
        }
        // Startup prints one status line; final summary is the remaining JSON.
        let summary: Value = if kill_after_finalized {
            assert!(
                !text.contains("native_fresh_genesis_confirmation_summary: "),
                "forced termination must precede clean exit"
            );
            before_kill.remove(&index).unwrap()
        } else if fresh {
            serde_json::from_str(
                text.lines()
                    .find_map(|line| {
                        line.strip_prefix("native_fresh_genesis_confirmation_summary: ")
                    })
                    .expect("fresh main-process summary"),
            )
            .unwrap()
        } else {
            let start = text.find("\n{").expect("final main-node JSON summary") + 1;
            serde_json::from_str(&text[start..]).unwrap()
        };
        let seal = if kill_after_finalized {
            &summary
        } else if fresh {
            assert_eq!(summary["ordinary_execution_enabled"], false);
            &summary["native_seal_service"]
        } else {
            &summary["product_mainline_overlay"]["native_seal"]
        };
        assert_eq!(seal["prepared"], expect_prepared);
        assert_eq!(seal["decision_v3_enabled"], decision_v3);
        assert_eq!(seal["decision_confirmed"], decision_v3 && expect_prepared);
        assert_eq!(seal["halted"], false);
        assert_eq!(seal["height"], height);
        if fresh && expect_prepared {
            assert_eq!(seal["publication"]["aoem_authority_published"], true);
            assert_eq!(seal["publication"]["ledger_publication_completed"], true);
            assert_eq!(seal["signing_enabled"], false);
        }
        for field in ["finalized", "safe", "proof_sealed", "chain_canonical"] {
            assert_eq!(seal[field], fresh && expect_prepared);
        }
    }
}

#[test]
#[ignore = "requires exclusive loopback 127.0.0.2:443; run explicitly on a prepared host"]
fn real_aoem_main_nodes_prepare_three_of_four_and_recover() {
    run_real_aoem_main_nodes(false, false, false);
}

#[test]
#[ignore = "requires exclusive loopback 127.0.0.2:443; run explicitly on a prepared host"]
fn real_aoem_main_nodes_decision_v3_three_of_four_and_recover() {
    run_real_aoem_main_nodes(true, false, false);
}

#[test]
#[ignore = "requires exclusive loopback 127.0.0.2:443; run explicitly on a prepared host"]
fn fresh_genesis_main_nodes_confirm_three_of_four_and_recover() {
    run_real_aoem_main_nodes(true, true, false);
}

#[test]
#[ignore = "requires exclusive loopback 127.0.0.2:443; run explicitly on a prepared host"]
fn fresh_genesis_main_nodes_continue_three_heights_without_restart() {
    run_real_aoem_main_nodes(true, true, true);
}

#[test]
#[ignore = "real four-process record Transfer measurement; exclusive loopback 127.0.0.2:443"]
fn fresh_record_transfers_measure_rpc_to_finality() {
    run_real_aoem_main_nodes_scenario(
        true,
        true,
        false,
        false,
        LocalFault::None,
        Some(TransferScenario::Throughput(
            transfer_throughput::TransportProfile::LegacyLimits,
        )),
    );
}

#[test]
#[ignore = "real four-process bounded transport measurement; exclusive loopback 127.0.0.2:443"]
fn fresh_record_transfers_bounded_transport_measure_rpc_to_finality() {
    run_real_aoem_main_nodes_scenario(
        true,
        true,
        false,
        false,
        LocalFault::None,
        Some(TransferScenario::Throughput(
            transfer_throughput::TransportProfile::Bounded64,
        )),
    );
}

#[test]
#[ignore = "real four-process 256-transfer bounded backlog; exclusive loopback 127.0.0.2:443"]
fn fresh_record_transfers_continuous_backlog_measure_rpc_to_finality() {
    run_real_aoem_main_nodes_scenario(
        true,
        true,
        false,
        false,
        LocalFault::None,
        Some(TransferScenario::ContinuousBacklog),
    );
}

#[test]
#[ignore = "real four-process 96-transfer durable receipt measurement; exclusive loopback 127.0.0.2:443"]
fn fresh_record_transfers_durable_receipts_measure_rpc_to_finality() {
    run_real_aoem_main_nodes_scenario(
        true,
        true,
        false,
        false,
        LocalFault::None,
        Some(TransferScenario::Throughput(
            transfer_throughput::TransportProfile::Bounded64DurableReceipts,
        )),
    );
}

#[test]
#[ignore = "real four-process 256-transfer durable receipt backlog; exclusive loopback 127.0.0.2:443"]
fn fresh_record_transfers_durable_receipts_continuous_backlog_measure_rpc_to_finality() {
    run_real_aoem_main_nodes_scenario(
        true,
        true,
        false,
        false,
        LocalFault::None,
        Some(TransferScenario::ContinuousBacklogDurableReceipts),
    );
}

#[test]
#[ignore = "real four-process proposal collection measurement; exclusive loopback 127.0.0.2:443"]
fn fresh_record_transfers_collect_250_measure_rpc_to_finality() {
    run_real_aoem_main_nodes_scenario(
        true,
        true,
        false,
        false,
        LocalFault::None,
        Some(TransferScenario::Throughput(
            transfer_throughput::TransportProfile::LegacyLimitsCollect250,
        )),
    );
}

#[test]
#[ignore = "real four-process bounded transport and collection measurement; exclusive loopback 127.0.0.2:443"]
fn fresh_record_transfers_bounded_collect_250_measure_rpc_to_finality() {
    run_real_aoem_main_nodes_scenario(
        true,
        true,
        false,
        false,
        LocalFault::None,
        Some(TransferScenario::Throughput(
            transfer_throughput::TransportProfile::Bounded64Collect250,
        )),
    );
}

#[test]
#[ignore = "real four-process mixed Transfer parity; exclusive loopback 127.0.0.2:443 and explicit libtest oracle"]
fn fresh_record_transfers_conflict_failure_serial_parity() {
    run_real_aoem_main_nodes_scenario(
        true,
        true,
        false,
        false,
        LocalFault::None,
        Some(TransferScenario::MixedParity),
    );
}

fn run_real_aoem_main_nodes(decision_v3: bool, fresh: bool, continuous: bool) {
    run_real_aoem_main_nodes_with_failover(decision_v3, fresh, continuous, false);
}

#[test]
#[ignore = "requires isolated loopback WSS and real AOEM; run the local readiness runner"]
fn fresh_genesis_main_nodes_replace_candidate_less_offline_leader() {
    run_real_aoem_main_nodes_with_failover(true, true, false, true);
}

fn run_real_aoem_main_nodes_with_failover(
    decision_v3: bool,
    fresh: bool,
    continuous: bool,
    failover: bool,
) {
    run_real_aoem_main_nodes_with_faults(
        decision_v3,
        fresh,
        continuous,
        failover,
        LocalFault::None,
    );
}

#[test]
#[ignore = "requires isolated WSS, real AOEM and cfg(test) main worker; use readiness runner"]
fn fresh_genesis_main_nodes_heal_prepared_timeout_partition() {
    run_real_aoem_main_nodes_with_faults(true, true, false, false, LocalFault::Partition);
}

#[test]
#[ignore = "requires private mount/network namespaces, bounded tmpfs and real AOEM"]
fn fresh_genesis_main_nodes_recover_transaction_pool_storage_faults() {
    storage::require_private_mount();
    run_real_aoem_main_nodes_with_faults(true, true, false, false, LocalFault::Storage);
}

enum LocalFault {
    None,
    Partition,
    Storage,
    StorageStartup,
}

#[test]
#[ignore = "requires private mount/network namespaces, bounded storage and real AOEM"]
fn fresh_genesis_main_nodes_recover_storage_startup_faults() {
    storage::require_private_mount();
    run_real_aoem_main_nodes_with_faults(true, true, false, false, LocalFault::StorageStartup);
}

fn run_real_aoem_main_nodes_with_faults(
    decision_v3: bool,
    fresh: bool,
    continuous: bool,
    failover: bool,
    fault: LocalFault,
) {
    run_real_aoem_main_nodes_scenario(decision_v3, fresh, continuous, failover, fault, None);
}

fn run_real_aoem_main_nodes_scenario(
    decision_v3: bool,
    fresh: bool,
    continuous: bool,
    failover: bool,
    fault: LocalFault,
    transfer_scenario: Option<TransferScenario>,
) {
    let reserve = std::net::TcpListener::bind("127.0.0.2:443")
        .expect("exclusive loopback 443 required; do not stop other services");
    let (genesis, fresh_plan) = match &transfer_scenario {
        Some(
            TransferScenario::Throughput(_)
            | TransferScenario::ContinuousBacklog
            | TransferScenario::ContinuousBacklogDurableReceipts,
        ) => transfer_throughput::inputs(),
        Some(TransferScenario::MixedParity) => transfer_mixed::inputs(),
        None => super::native_fresh_genesis_cli::inputs(),
    };
    let mut fresh_outputs = Vec::new();
    let (nodes, block) = if fresh {
        let mut nodes = Vec::new();
        for index in 0..4 {
            let node = Node::new(&format!("fresh-validator-{index}"));
            let node = if matches!(transfer_scenario, Some(TransferScenario::MixedParity)) {
                node.without_legacy_host_execution()
            } else {
                node
            };
            let result = node.run(
                &mut super::native_fresh_genesis_cli::prepare_command(&node, &genesis, &fresh_plan),
                "prepare",
            );
            assert!(result.0, "{}", result.2);
            let output: Value = serde_json::from_str(&result.1).unwrap();
            fresh_outputs.push(output);
            nodes.push(node);
        }
        let block: NovNativeDurableBlockV1 =
            serde_json::from_value(fresh_outputs[0]["durable_block_candidate"].clone()).unwrap();
        assert!(fresh_outputs.iter().all(
            |out| out["durable_block_candidate"] == fresh_outputs[0]["durable_block_candidate"]
        ));
        (nodes, block)
    } else {
        let (source, block, plan) = source_candidate();
        let mut nodes = vec![source];
        for index in 1..4 {
            let node = Node::funded(&format!("seal-validator-{index}"));
            let out = node.execute(&plan, "candidate");
            let actual: NovNativeDurableBlockV1 =
                serde_json::from_value(out["durable_block_candidate_committed"].clone()).unwrap();
            assert_eq!(actual, block);
            nodes.push(node);
        }
        (nodes, block)
    };
    // Test-only disposable identities; signing and transport use different keys.
    let keys: Vec<_> = (0..4)
        .map(|i| SigningKey::from_bytes(&[if fresh { 1 + i } else { 151 + i }; 32]))
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
    let bindings = validators
        .iter()
        .enumerate()
        .map(|(i, v)| NovNativeSealValidatorTransportBindingV1 {
            validator_id: v.validator_id,
            transport_peer_id: peer_ids[i].clone(),
        })
        .collect();
    let authority = if fresh {
        NovNativeSealEpochAuthorityV1::derive_operator_pinned_fresh_genesis_epoch(
            &genesis,
            genesis.compile().unwrap().config_commitment(),
            bindings,
        )
    } else {
        NovNativeSealEpochAuthorityV1::derive_operator_pinned_genesis_epoch(
            &nodes[0].ledger(),
            set,
            bindings,
        )
    }
    .unwrap();
    let leader = authority.expected_leader(1, 0).unwrap();
    let leader_index = validators
        .iter()
        .position(|v| v.validator_id == leader)
        .unwrap();
    let active: Vec<_> = if fresh {
        std::iter::once(leader_index)
            .chain((0..4).filter(|i| *i != leader_index))
            .take(3)
            .collect()
    } else {
        (0..4).filter(|i| *i != leader_index).collect()
    };

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
            "decision_v3_enabled":decision_v3,
            "block_hash":hex(&block.header.block_hash),"authority_path":"authority.json", "signer_key_path":"signer.hex",
            "seal_store_path":"seal-db","round_timeout_ms":15000,"poll_interval_ms":100,
            "ingress_per_source_per_second":8,"ingress_per_poll":16})).unwrap()).unwrap();
        if fresh {
            let path = node.0.join("seal.json");
            let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            config["fresh_genesis_config_commitment"] =
                serde_json::json!(hex(&genesis.compile().unwrap().config_commitment()));
            let id: [u8; 32] =
                serde_json::from_value(fresh_outputs[i]["workspace_id"].clone()).unwrap();
            config["isolated_workspace_id"] = serde_json::json!(hex(&id));
            config["round_timeout_ms"] = serde_json::json!(300000);
            fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
        }
    }
    if matches!(fault, LocalFault::Partition) {
        partition::exercise(&nodes, &authority, &validators, &peer_ids, &block, &root);
        return;
    }
    // Same fixed four-member authority throughout. No fake clock or test-driver votes.
    run_cluster(
        &nodes,
        &active[..2],
        "two-of-four",
        if fresh { 4 } else { 70 },
        false,
        decision_v3,
        fresh,
    );
    for &index in &active[..2] {
        let store =
            NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[index].0.join("seal-db"))
                .unwrap()
                .unwrap();
        assert!(store.load_qcs_by_height(CHAIN, 1, 1).unwrap().is_empty());
        assert!(store
            .load_decision_certificate_by_height_v3(CHAIN, 1, 1)
            .unwrap()
            .is_none());
    }
    if matches!(fault, LocalFault::StorageStartup) {
        storage_startup::exercise(
            &nodes,
            &active,
            &genesis,
            &fresh_plan,
            &fresh_outputs[active[0]],
            &root,
        );
    }
    run_cluster(
        &nodes,
        &active,
        "three-of-four",
        if fresh { 32 } else { 100 },
        true,
        decision_v3,
        fresh,
    );
    let mut previous_qcs = Vec::new();
    let mut previous_decisions = Vec::new();
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
        let decision = store
            .load_decision_certificate_by_height_v3(CHAIN, 1, 1)
            .unwrap();
        assert_eq!(decision.is_some(), decision_v3);
        if let Some(certificate) = &decision {
            certificate.verify(&authority.validator_set).unwrap();
            assert_eq!(
                certificate.prepare.subject.block_hash,
                block.header.block_hash
            );
            assert_eq!(certificate.signed_weight, 3);
            assert_eq!(certificate.votes.len(), 3);
        }
        previous_decisions.push(decision);
        if !fresh {
            assert_eq!(
                nodes[index]
                    .ledger()
                    .load_by_height(CHAIN, 1)
                    .unwrap()
                    .unwrap(),
                block
            );
        }
    }
    if fresh {
        let expected = previous_decisions[0].as_ref().unwrap().certificate_hash;
        assert!(previous_decisions.iter().all(|decision| decision
            .as_ref()
            .unwrap()
            .certificate_hash
            == expected));
    }
    run_cluster(
        &nodes,
        &active,
        "restart",
        if fresh { 3 } else { 12 },
        true,
        decision_v3,
        fresh,
    );
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
            store
                .load_decision_certificate_by_height_v3(CHAIN, 1, 1)
                .unwrap(),
            previous_decisions[offset]
        );
        if !fresh {
            assert_eq!(
                nodes[index]
                    .ledger()
                    .load_by_height(CHAIN, 1)
                    .unwrap()
                    .unwrap(),
                block
            );
        } else {
            assert!(!nodes[index].0.join("native.json").exists());
            let mut hash = Sha256::new();
            hash.update(b"novovm-native-aoem-state-namespace-v1");
            hash.update(nodes[index].0.to_str().unwrap().as_bytes());
            let namespace: [u8; 32] = hash.finalize().into();
            let published = || {
                NovNativeBlockLedgerV1::load_fresh_genesis_published_block_v1(
                    &novovm_node::tx_ingress::nov_native_block_ledger_rocksdb_path_v1(
                        &nodes[index].0.join("native.json"),
                    ),
                    genesis.compile().unwrap().config_commitment(),
                    namespace,
                )
                .unwrap()
                .unwrap()
            };
            assert_eq!(published(), block);
            let result = nodes[index].run(
                &mut super::native_fresh_genesis_cli::prepare_command(
                    &nodes[index],
                    &genesis,
                    &fresh_plan,
                ),
                "verify-authority",
            );
            assert!(
                !result.0,
                "genesis preparation must not reset published authority"
            );
            assert_eq!(published(), block);
        }
    }
    if fresh {
        // Three nodes now relay immutable archives only; the fourth was offline
        // during voting and must still catch up without those peers re-signing.
        run_cluster(&nodes, &[0, 1, 2, 3], "late-fourth", 32, true, true, true);
        for node in &nodes {
            let store = NovNativeBlockSealStoreV1::open_existing_read_only(&node.0.join("seal-db"))
                .unwrap()
                .unwrap();
            assert_eq!(
                store
                    .load_decision_certificate_by_height_v3(CHAIN, 1, 1)
                    .unwrap()
                    .unwrap()
                    .certificate_hash,
                previous_decisions[0].as_ref().unwrap().certificate_hash
            );
        }
    }
    fs::write(root.join("acceptance.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "scope":if fresh { "local_fresh_genesis_main_process_decision_v3" } else if decision_v3 { "local_real_aoem_main_process_decision_v3" } else { "local_real_aoem_main_process_prepare_qc" },
        "chain_id":CHAIN, "height":1, "block_hash":hex(&block.header.block_hash),
        "validator_count":4, "offline_initial_leader_index":if fresh { None } else { Some(leader_index) }, "active_validator_indices":active,
        "round_timeout_ms":if fresh { 300000 } else { 15000 }, "tick_interval_ms":250,
        "node_evidence_paths":nodes.iter().map(|node| &node.0).collect::<Vec<_>>(),
        "two_of_four_persisted_qc_count":0, "three_of_four_qcs":previous_qcs,
        "decision_v3_enabled":decision_v3, "three_of_four_decisions":previous_decisions,
        "restart_preserved_qcs_and_unsealed_ledger":true,
        "fresh_authority_and_ledger_published":fresh,
        "late_fourth_node_caught_up_from_read_only_relays":fresh,
        "proof_sealed":fresh,"chain_canonical":fresh,"safe":fresh,"finalized":fresh,
        "finality_scope":"first_height_bft_decision_v3", "zero_knowledge_execution_proof":false,
        "physical_lan_executed":false,"public_network_executed":false
    })).unwrap()).unwrap();
    if fresh {
        if let Some(scenario) = transfer_scenario {
            match scenario {
                TransferScenario::Throughput(profile) => {
                    transfer_throughput::exercise(&nodes, &root, profile)
                }
                TransferScenario::ContinuousBacklog => {
                    transfer_throughput::exercise_continuous(&nodes, &root)
                }
                TransferScenario::ContinuousBacklogDurableReceipts => {
                    transfer_throughput::exercise_continuous_durable_receipts(&nodes, &root)
                }
                TransferScenario::MixedParity => transfer_mixed::exercise(&nodes, &root),
            }
            return;
        }
        if matches!(fault, LocalFault::StorageStartup) {
            storage_startup::finish(&nodes, &root);
            return;
        }
        if matches!(fault, LocalFault::Storage) {
            storage::exercise(&nodes, &authority, &validators, &fresh_plan, &root);
            return;
        }
        successor::exercise(
            &nodes,
            &authority,
            &validators,
            &peer_ids,
            &fresh_plan,
            &root,
        );
        if failover {
            failover::exercise(&nodes, &authority, &validators, &fresh_plan, &root);
        }
        if continuous {
            continuous::exercise(
                &nodes,
                &authority,
                &validators,
                &peer_ids,
                &fresh_plan,
                &root,
            );
        }
    }
}
