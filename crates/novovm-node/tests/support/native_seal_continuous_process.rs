use super::*;
use novovm_protocol::{decode_nov_native_tx_wire_v1, encode_nov_native_tx_wire_v1, NovTxKindV1};

const FIRST: u64 = 3;
const LAST: u64 = 5;
const LABEL: &str = "continuous-three-heights";

fn rpc(node: &Node, label: &str, method: &str, params: Value) -> Value {
    let deadline = Instant::now();
    loop {
        if let Ok(text) = fs::read_to_string(node.0.join(format!("{label}.stdout.log"))) {
            if let Some(address) = text
                .lines()
                .find_map(|line| line.strip_prefix("native_fresh_rpc_listening: "))
            {
                let result = ureq::post(&format!("http://{address}/"))
                    .timeout(Duration::from_secs(5))
                    .set("Content-Type", "application/json")
                    .send_string(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string());
                if let Ok(response) = result {
                    return serde_json::from_str(&response.into_string().unwrap()).unwrap();
                }
            }
        }
        assert!(
            deadline.elapsed() < Duration::from_secs(45),
            "RPC not ready: {}",
            node.0.display()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn queued_crash(node: &Node, raw: &[u8]) -> u32 {
    let label = "continuous-queue-crash";
    let mut command = node.command();
    command
        .env("NOVOVM_NODE_MODE", "native_execution_tick")
        .env("NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS", "0")
        .env("NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS", "100")
        .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED", "true")
        .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_CONFIG", "overlay.json")
        .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_SIGNOFF_REQUIRED", "false")
        .env("NOVOVM_NATIVE_SEAL_ENABLED", "true")
        .env("NOVOVM_NATIVE_SEAL_CONFIG", "seal.json")
        .env("NOVOVM_NATIVE_FRESH_RPC_BIND", "127.0.0.1:0")
        .stdout(fs::File::create(node.0.join(format!("{label}.stdout.log"))).unwrap())
        .stderr(fs::File::create(node.0.join(format!("{label}.stderr.log"))).unwrap());
    let mut child = Child(command.spawn().unwrap());
    let response = rpc(
        node,
        label,
        "nov_sendRawTransaction",
        serde_json::json!([hex(raw)]),
    );
    assert_eq!(response["result"]["status"], "queued", "{response}");
    assert_eq!(
        rpc(
            node,
            label,
            "nov_sendRawTransaction",
            serde_json::json!([hex(raw)])
        ),
        response
    );
    let mut corrupt = raw.to_vec();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(rpc(
        node,
        label,
        "nov_sendRawTransaction",
        serde_json::json!([hex(&corrupt)])
    )
    .get("error")
    .is_some());
    let pid = child.0.id();
    child.0.kill().unwrap();
    assert!(!child.0.wait().unwrap().success());
    pid
}

fn confirmed(node: &Node, height: u64) -> Option<Value> {
    let text = fs::read_to_string(node.0.join(format!("{LABEL}.stdout.log"))).unwrap();
    text.lines().find_map(|line| {
        let json = line.strip_prefix("native_fresh_genesis_decision_confirmed: ")?;
        let status: Value = serde_json::from_str(json).ok()?;
        (status["height"] == height
            && status["finalized"] == true
            && status["publication"]["ledger_publication_completed"] == true
            && status["publication"]["aoem_readback_verified"] == true
            && status["lifecycle_halted"] == false)
            .then_some(status)
    })
}

fn history(node: &Node, height: u64) -> Value {
    let genesis: novovm_node::tx_ingress::fresh_genesis::FreshGenesisConfigV1 =
        serde_json::from_slice(&fs::read(node.0.join("genesis.json")).unwrap()).unwrap();
    let pin = genesis.compile().unwrap().config_commitment();
    let mut digest = Sha256::new();
    digest.update(b"novovm-native-aoem-state-namespace-v1");
    digest.update(node.0.to_str().unwrap().as_bytes());
    let namespace: [u8; 32] = digest.finalize().into();
    let ledger = nov_native_block_ledger_rocksdb_path_v1(&node.0.join("native.json"));
    let proofs: Vec<_> = (1..=height)
        .map(|current| {
            NovNativeBlockLedgerV1::load_fresh_finality_by_height_v1(
                &ledger, pin, namespace, current,
            )
            .unwrap()
            .unwrap()
        })
        .collect();
    let block =
        NovNativeBlockLedgerV1::load_fresh_successor_published_block_v1(&ledger, pin, namespace)
            .unwrap()
            .unwrap();
    assert_eq!(block.header.height, height);
    serde_json::json!({"proofs":proofs,"tip":block})
}

pub(super) fn exercise(
    nodes: &[Node],
    authority: &NovNativeSealEpochAuthorityV1,
    validators: &[NovNativeSealValidatorV1],
    _peer_ids: &[String],
    first_plan: &NovNativeCandidateExecutionPlanV1,
    evidence_root: &std::path::Path,
) {
    let leaders: Vec<_> = (FIRST..=LAST)
        .map(|height| {
            validators
                .iter()
                .position(|validator| {
                    validator.validator_id == authority.expected_leader(height, 0).unwrap()
                })
                .unwrap()
        })
        .collect();
    let sender_index = (0..nodes.len())
        .find(|index| !leaders.contains(index))
        .unwrap();
    let active: Vec<_> = (0..nodes.len())
        .filter(|index| *index != sender_index)
        .collect();
    let original_configs: Vec<_> = nodes
        .iter()
        .map(|node| fs::read(node.0.join("seal.json")).unwrap())
        .collect();
    let previous = history(&nodes[active[0]], FIRST - 1);
    for &index in &active {
        assert_eq!(history(&nodes[index], FIRST - 1), previous);
    }
    let transactions: Vec<_> = (FIRST..=LAST)
        .map(|height| {
            let mut transaction = decode_nov_native_tx_wire_v1(&first_plan.raw_txs[0]).unwrap();
            let NovTxKindV1::Execute(execution) = &mut transaction.kind else {
                panic!("execute fixture")
            };
            execution.nonce = height - 1;
            sign_nov_native_tx_with_seed_v1(&mut transaction, [0xc3; 32]).unwrap();
            let raw = encode_nov_native_tx_wire_v1(&transaction).unwrap();
            let hash = canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap();
            (hash, raw)
        })
        .collect();
    let mut live = Vec::new();
    let ingress = *active.iter().find(|&&index| index != leaders[0]).unwrap();
    let queue_crash_pid = queued_crash(&nodes[ingress], &transactions[0].1);
    {
        let inject = || {
            for (offset, height) in (FIRST..=LAST).enumerate() {
                let deadline = Instant::now();
                if offset > 0 {
                    let target = *active
                        .iter()
                        .find(|&&index| index != leaders[offset])
                        .unwrap();
                    let result = rpc(
                        &nodes[target],
                        LABEL,
                        "nov_sendRawTransaction",
                        serde_json::json!([hex(&transactions[offset].1)]),
                    );
                    assert_eq!(result["result"]["status"], "queued", "{result}");
                }
                loop {
                    if active
                        .iter()
                        .all(|&index| confirmed(&nodes[index], height).is_some())
                    {
                        break;
                    }
                    assert!(
                        deadline.elapsed() < Duration::from_secs(120),
                        "continuous height {height} deadline; evidence: {}",
                        evidence_root.display()
                    );
                    std::thread::sleep(Duration::from_millis(25));
                }
                for &index in &active {
                    let status = rpc(
                        &nodes[index],
                        LABEL,
                        "nov_getTransactionStatus",
                        serde_json::json!([hex(&transactions[offset].0)]),
                    );
                    assert_eq!(status["result"]["status"], "finalized", "{status}");
                }
                let replay = rpc(
                    &nodes[ingress],
                    LABEL,
                    "nov_sendRawTransaction",
                    serde_json::json!([hex(&transactions[offset].1)]),
                );
                assert_eq!(replay["result"]["status"], "finalized", "{replay}");
            }
        };
        run_cluster_at_height(
            nodes,
            &active,
            LABEL,
            0,
            true,
            true,
            true,
            LAST,
            Some(&inject),
            true,
        );
        for height in FIRST..=LAST {
            let statuses: Vec<_> = active
                .iter()
                .map(|&index| confirmed(&nodes[index], height).unwrap())
                .collect();
            let expected = statuses[0]["decision_certificate_hash"].as_str().unwrap();
            assert!(statuses
                .iter()
                .all(|status| status["decision_certificate_hash"] == expected));
            live.push(serde_json::json!({"height":height,"statuses":statuses}));
        }
    }
    let expected = history(&nodes[active[0]], LAST);
    assert_eq!(
        &expected["proofs"].as_array().unwrap()[..2],
        previous["proofs"].as_array().unwrap()
    );
    assert_eq!(
        expected["tip"]["body"]["tx_hashes"],
        serde_json::json!([transactions.last().unwrap().0])
    );
    assert_eq!(
        expected["tip"]["body"]["raw_txs"],
        serde_json::json!([transactions.last().unwrap().1])
    );
    for &index in &active {
        assert_eq!(history(&nodes[index], LAST), expected);
    }
    run_cluster_at_height(
        nodes,
        &active,
        "continuous-restart",
        8,
        true,
        true,
        true,
        LAST,
        None,
        false,
    );
    for &index in &active {
        assert_eq!(history(&nodes[index], LAST), expected);
    }
    assert_eq!(
        history(&nodes[sender_index], FIRST - 1)["tip"]["header"]["height"],
        FIRST - 1
    );
    let catchup_nodes: Vec<_> = (0..nodes.len())
        .filter(|index| *index != leaders[0])
        .collect();
    run_cluster_at_height(
        nodes,
        &catchup_nodes,
        "continuous-offline-history",
        0,
        true,
        true,
        true,
        LAST,
        None,
        true,
    );
    for &index in &catchup_nodes {
        assert_eq!(history(&nodes[index], LAST), expected);
    }
    run_cluster_at_height(
        nodes,
        &catchup_nodes,
        "continuous-history-restart",
        8,
        true,
        true,
        true,
        LAST,
        None,
        false,
    );
    for &index in &catchup_nodes {
        assert_eq!(history(&nodes[index], LAST), expected);
    }
    for (node, original) in nodes.iter().zip(original_configs) {
        assert_eq!(fs::read(node.0.join("seal.json")).unwrap(), original);
    }
    let processes: Vec<Value> = active
        .iter()
        .map(|&index| {
            serde_json::from_slice(
                &fs::read(nodes[index].0.join(format!("{LABEL}.process.json"))).unwrap(),
            )
            .unwrap()
        })
        .collect();
    fs::write(
        evidence_root.join("continuous-acceptance.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"local_rpc_durable_ingress_and_history_recovery_v1",
            "accepted":true,"first_height":FIRST,"last_height":LAST,
            "active_validator_indices":active,"initially_offline_validator_index":sender_index,
            "rpc_admission_validator_index":ingress,
            "leader_indices":leaders,"live_confirmations":live,
            "processes":processes,
            "persistent_readback":expected,
            "same_processes_for_all_three_heights":true,
            "restart_preserved_full_finality_history":true,
            "forced_termination_after_finalization":true,
            "operator_configs_unchanged":true,
            "durable_rpc_admission_before_kill":true,"queue_crash_pid":queue_crash_pid,
            "rpc_nonleader_gossip_finalized":true,
            "offline_node_multiple_height_catchup_executed":true,
            "historical_proposer_offline_index":leaders[0],
            "catchup_restarted_and_verified":true,
            "physical_lan_executed":false,"power_loss_executed":false,
            "signed_user_loopback_rpc_executed":true,
            "public_internet_rpc_executed":false
        }))
        .unwrap(),
    )
    .unwrap();
}
