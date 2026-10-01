use super::*;
use novovm_node::native_block_seal::commit_v3::{
    decision_target_v3, NovNativeSealDecisionCertificateV3,
};
use std::collections::BTreeMap;

fn write_json(node: &Node, name: &str, value: &Value) {
    let temporary = node.0.join(format!("{name}.next"));
    fs::write(&temporary, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    fs::rename(temporary, node.0.join(name)).unwrap();
}

fn control(
    nodes: &[Node],
    peers: &[String],
    prepared: &[usize],
    phase: &str,
    offline: Option<usize>,
) {
    for (index, node) in nodes.iter().enumerate() {
        let mut allowed = BTreeMap::new();
        for (source, peer) in peers.iter().enumerate() {
            if source == index || Some(source) == offline {
                continue;
            }
            let kinds: Vec<u8> = match phase {
                "seed" => (1..=10)
                    .filter(|kind| {
                        *kind == 4
                            || (*kind == 5 && index == prepared[0])
                            || (*kind == 6 && prepared.contains(&index))
                    })
                    .collect(),
                "partition" if prepared.contains(&index) == prepared.contains(&source) => {
                    (1..=10).collect()
                }
                "partition" => vec![],
                "heal-prepare" => (1..=6).collect(),
                "heal-decision" => (1..=10).collect(),
                _ => panic!("unknown phase"),
            };
            allowed.insert(peer, kinds);
        }
        write_json(
            node,
            "partition-control.json",
            &serde_json::json!({"phase":phase,"allowed":allowed}),
        );
    }
}

fn spawn(node: &Node, label: &str) -> Child {
    let executable = std::env::var_os("NOVOVM_TEST_MAIN_PARTITION_BINARY")
        .expect("readiness runner must supply freshly built main test binary");
    let base = node.command();
    let mut command = Command::new(executable);
    command.current_dir(base.get_current_dir().unwrap());
    for (key, value) in base.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    command
        .args([
            "--exact",
            "partition_test::main_entry_worker",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("NOVOVM_TEST_PARTITION_WORKER", "1")
        .env("NOVOVM_NODE_MODE", "native_execution_tick")
        .env("NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS", "0")
        .env("NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS", "100")
        .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED", "true")
        .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_CONFIG", "overlay.json")
        .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_SIGNOFF_REQUIRED", "false")
        .env("NOVOVM_NATIVE_SEAL_ENABLED", "true")
        .env("NOVOVM_NATIVE_SEAL_CONFIG", "seal.json")
        .stdout(fs::File::create(node.0.join(format!("{label}.stdout.log"))).unwrap())
        .stderr(fs::File::create(node.0.join(format!("{label}.stderr.log"))).unwrap());
    let child = Child(command.spawn().unwrap());
    write_json(
        node,
        &format!("{label}.process.json"),
        &serde_json::json!({"pid":child.0.id()}),
    );
    child
}

fn wait_for(
    nodes: &[Node],
    children: &mut [Option<Child>],
    phase: &str,
    label: &str,
    condition: impl Fn(usize, &Value) -> bool,
) -> Vec<Value> {
    let start = Instant::now();
    loop {
        let mut reports = vec![Value::Null; nodes.len()];
        let mut complete = true;
        for (index, child) in children.iter_mut().enumerate() {
            let Some(child) = child else { continue };
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "node {index} exited: {}",
                nodes[index].0.display()
            );
            let report: Value = fs::read(nodes[index].0.join("partition-status.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or(Value::Null);
            assert_ne!(report["status"]["halted"], true, "{report}");
            assert_ne!(report["status"]["lifecycle_halted"], true, "{report}");
            complete &= report["pid"] == child.0.id()
                && report["phase"] == phase
                && condition(index, &report);
            reports[index] = report;
        }
        if complete {
            for (index, report) in reports
                .iter()
                .enumerate()
                .filter(|(_, report)| !report.is_null())
            {
                write_json(&nodes[index], &format!("{label}.json"), report);
            }
            return reports;
        }
        assert!(
            start.elapsed() < Duration::from_secs(150),
            "phase {phase} timeout: {reports:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn stop(child: &mut Option<Child>) {
    if let Some(mut child) = child.take() {
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
    }
}

fn facts(node: &Node) -> BTreeMap<String, Vec<u8>> {
    let database = rocksdb::DB::open_for_read_only(
        &rocksdb::Options::default(),
        node.0.join("seal-db"),
        false,
    )
    .unwrap();
    database
        .iterator(rocksdb::IteratorMode::Start)
        .filter_map(|entry| {
            let (key, value) = entry.unwrap();
            let parsed: Value = serde_json::from_slice(&value).ok()?;
            (parsed["schema"] == "novovm-native-seal-decision-lock/v3"
                || (parsed["schema"] == "novovm-native-seal-timeout-observation/v1"
                    && parsed["context"]["round"] == 0))
                .then(|| (String::from_utf8(key.to_vec()).unwrap(), value.to_vec()))
        })
        .collect()
}

fn lock_round(node: &Node, records: &BTreeMap<String, Vec<u8>>) -> Option<u64> {
    let store = NovNativeBlockSealStoreV1::open_existing_read_only(&node.0.join("seal-db"))
        .unwrap()
        .unwrap();
    let locks: Vec<Value> = records
        .values()
        .filter_map(|bytes| serde_json::from_slice::<Value>(bytes).ok())
        .filter(|value| value["schema"] == "novovm-native-seal-decision-lock/v3")
        .collect();
    assert!(locks.len() <= 1);
    locks.first().map(|lock| {
        let hash: [u8; 32] = serde_json::from_value(lock["prepare_qc_hash"].clone()).unwrap();
        store.load_qc(hash).unwrap().unwrap().subject.round
    })
}

pub(super) fn exercise(
    nodes: &[Node],
    authority: &NovNativeSealEpochAuthorityV1,
    validators: &[NovNativeSealValidatorV1],
    peers: &[String],
    block: &NovNativeDurableBlockV1,
    root: &std::path::Path,
) {
    let leader = validators
        .iter()
        .position(|validator| validator.validator_id == authority.expected_leader(1, 0).unwrap())
        .unwrap();
    let prepared = vec![leader, (leader + 1) % 4];
    let advancing: Vec<_> = (0..4).filter(|index| !prepared.contains(index)).collect();
    for node in nodes {
        let mut config: Value =
            serde_json::from_slice(&fs::read(node.0.join("seal.json")).unwrap()).unwrap();
        config["round_timeout_ms"] = 45000.into();
        write_json(node, "seal.json", &config);
    }
    control(nodes, peers, &prepared, "seed", None);
    let mut children: Vec<_> = nodes
        .iter()
        .map(|node| Some(spawn(node, "partition-initial")))
        .collect();
    let seed = wait_for(
        nodes,
        &mut children,
        "seed",
        "partition-seed",
        |index, report| {
            report["status"]["prepared"] == prepared.contains(&index)
                && report["status"]["round"] == 0
                && report["status"]["decision_confirmed"] == false
                && report["counts"]["dropped:9"].as_u64().unwrap_or(0) > 0
        },
    );
    control(nodes, peers, &prepared, "partition", None);
    let partition = wait_for(
        nodes,
        &mut children,
        "partition",
        "partition-held",
        |index, report| {
            report["status"]["prepared"] == prepared.contains(&index)
                && report["status"]["round"] == 0
                && report["status"]["decision_confirmed"] == false
                && report["counts"]["allowed:1"].as_u64().unwrap_or(0) > 0
                && report["counts"]["dropped:1"].as_u64().unwrap_or(0) > 0
                && (!prepared.contains(&index)
                    || report["counts"]["allowed:9"].as_u64().unwrap_or(0) > 0)
        },
    );
    children.iter_mut().for_each(stop);
    let before: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let records = facts(node);
            assert_eq!(
                lock_round(node, &records),
                prepared.contains(&index).then_some(0)
            );
            let timeouts: Vec<_> = records
                .values()
                .filter_map(|bytes| {
                    serde_json::from_slice::<
                        novovm_node::native_block_seal::timeout::NovNativeSealTimeoutVoteV1,
                    >(bytes)
                    .ok()
                })
                .collect();
            assert!(timeouts
                .iter()
                .any(|vote| vote.validator_id == validators[index].validator_id));
            for vote in &timeouts {
                vote.verify(&authority.validator_set).unwrap();
                assert_eq!(vote.context.round, 0);
            }
            let snapshot: BTreeMap<_, Value> = records
                .iter()
                .map(|(key, bytes)| (key, serde_json::from_slice(bytes).unwrap()))
                .collect();
            write_json(
                node,
                "partition-original-signatures.json",
                &serde_json::to_value(snapshot).unwrap(),
            );
            let store = NovNativeBlockSealStoreV1::open_existing_read_only(&node.0.join("seal-db"))
                .unwrap()
                .unwrap();
            assert!(store
                .load_decision_certificate_by_height_v3(CHAIN, authority.epoch, 1)
                .unwrap()
                .is_none());
            records
        })
        .collect();
    children = nodes
        .iter()
        .map(|node| Some(spawn(node, "partition-restarted")))
        .collect();
    let restarted = wait_for(
        nodes,
        &mut children,
        "partition",
        "partition-reopened",
        |index, report| {
            report["status"]["prepared"] == prepared.contains(&index)
                && report["status"]["round"] == 0
                && report["status"]["decision_confirmed"] == false
                && report["counts"]["allowed:1"].as_u64().unwrap_or(0) > 0
        },
    );
    control(nodes, peers, &prepared, "heal-prepare", None);
    let healed = wait_for(
        nodes,
        &mut children,
        "heal-prepare",
        "partition-healed-prepare",
        |_, report| {
            report["status"]["prepared"] == true
                && report["status"]["round"].as_u64().unwrap_or(0) > 0
                && report["status"]["decision_confirmed"] == false
                && report["counts"]["dropped:9"].as_u64().unwrap_or(0) > 0
        },
    );
    children.iter_mut().for_each(stop);
    let healed_facts: Vec<_> = nodes.iter().map(facts).collect();
    for (index, node) in nodes.iter().enumerate() {
        let records = &healed_facts[index];
        for (key, value) in &before[index] {
            assert_eq!(records.get(key), Some(value), "durable signature changed");
        }
        assert_eq!(
            lock_round(node, records).unwrap() == 0,
            prepared.contains(&index)
        );
    }
    let offline = advancing[1];
    let active: Vec<_> = (0..4).filter(|index| *index != offline).collect();
    control(nodes, peers, &prepared, "heal-decision", Some(offline));
    children = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (index != offline).then(|| spawn(node, "partition-confirming")))
        .collect();
    let confirmed = wait_for(
        nodes,
        &mut children,
        "heal-decision",
        "partition-confirmed",
        |_, report| {
            report["status"]["finalized"] == true
                && report["status"]["publication"]["ledger_publication_completed"] == true
                && report["status"]["publication"]["aoem_authority_published"] == true
                && report["status"]["publication"]["aoem_readback_verified"] == true
        },
    );
    children.iter_mut().for_each(stop);
    assert_eq!(facts(&nodes[offline]), healed_facts[offline]);
    {
        let store =
            NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[offline].0.join("seal-db"))
                .unwrap()
                .unwrap();
        assert!(store
            .load_decision_certificate_by_height_v3(CHAIN, authority.epoch, 1)
            .unwrap()
            .is_none());
    }
    let mut decisions: Vec<NovNativeSealDecisionCertificateV3> = Vec::new();
    let mut histories = Vec::new();
    for &index in &active {
        let store =
            NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[index].0.join("seal-db"))
                .unwrap()
                .unwrap();
        let certificate = store
            .load_decision_certificate_by_height_v3(CHAIN, authority.epoch, 1)
            .unwrap()
            .unwrap();
        certificate.verify(&authority.validator_set).unwrap();
        assert_eq!(certificate.votes.len(), 3);
        assert!(certificate
            .votes
            .iter()
            .all(|vote| vote.validator_id != validators[offline].validator_id));
        assert_eq!(
            certificate.prepare.subject.block_hash,
            block.header.block_hash
        );
        if let Some(first) = decisions.first() {
            assert_eq!(
                decision_target_v3(&certificate.prepare, &authority.validator_set).unwrap(),
                decision_target_v3(&first.prepare, &authority.validator_set).unwrap()
            );
        }
        decisions.push(certificate);
        histories.push(continuous::history(&nodes[index], 1));
        assert_eq!(
            histories.last().unwrap()["tip"],
            serde_json::to_value(block).unwrap()
        );
        let records = facts(&nodes[index]);
        for (key, value) in &before[index] {
            assert_eq!(records.get(key), Some(value));
        }
    }
    run_cluster_at_height(
        nodes,
        &active,
        "partition-normal-binary-restart",
        8,
        true,
        true,
        true,
        1,
        None,
        false,
    );
    for (offset, &index) in active.iter().enumerate() {
        assert_eq!(continuous::history(&nodes[index], 1), histories[offset]);
    }
    run_cluster_at_height(
        nodes,
        &[0, 1, 2, 3],
        "partition-late-fourth",
        32,
        true,
        true,
        true,
        1,
        None,
        false,
    );
    for (offset, &index) in active.iter().enumerate() {
        assert_eq!(continuous::history(&nodes[index], 1), histories[offset]);
    }
    let recovered = continuous::history(&nodes[offline], 1);
    assert_eq!(recovered["tip"], serde_json::to_value(block).unwrap());
    let store =
        NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[offline].0.join("seal-db"))
            .unwrap()
            .unwrap();
    let recovered_decision = store
        .load_decision_certificate_by_height_v3(CHAIN, authority.epoch, 1)
        .unwrap()
        .unwrap();
    recovered_decision.verify(&authority.validator_set).unwrap();
    assert_eq!(
        decision_target_v3(&recovered_decision.prepare, &authority.validator_set).unwrap(),
        decision_target_v3(&decisions[0].prepare, &authority.validator_set).unwrap()
    );
    fs::write(root.join("partition-acceptance.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "accepted":true,"production_ready":false,
        "scope":"real_aoem_main_entry_prepared_timeout_partition",
        "instrumentation":"cfg_test_authenticated_ingress_drop_only_no_shipped_fault_switch",
        "normal_binary_candidate_execution_and_restart":true,
        "fake_clock":false,"synthetic_execution":false,"test_injected_votes":false,
        "prepared_round_zero_indices":prepared,"offline_after_healing":offline,
        "seed":seed,"partition":partition,"restarted":restarted,"healed_prepare":healed,
        "confirmed":confirmed,"decisions":decisions,"persistent_history":histories,
        "late_fourth_recovered_with_normal_binary":true,"late_fourth_history":recovered,
        "original_signatures_preserved":true,"physical_power_loss_tested":false,
        "multi_machine_tested":false,"node_paths":nodes.iter().map(|node| &node.0).collect::<Vec<_>>()
    })).unwrap()).unwrap();
    println!(
        "partition evidence: {}",
        root.join("partition-acceptance.json").display()
    );
}
