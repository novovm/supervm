//! Same-host real-process correctness gate, not a throughput or overlap claim.
use super::*;
use novovm_node::tx_ingress::fresh_genesis::FreshGenesisConfigV1;
use novovm_protocol::{
    decode_nov_native_tx_wire_v1, encode_nov_native_tx_wire_v1, NovFeePolicyV1, NovNativeTxWireV1,
    NovTransferTxV1, NovTxKindV1,
};
use std::collections::BTreeMap;

const DEADLINE: Duration = Duration::from_secs(120);

pub(super) fn inputs() -> (FreshGenesisConfigV1, NovNativeCandidateExecutionPlanV1) {
    oracle_worker(); // Fail before preparing any real nodes if the explicit worker is absent.
    let (mut genesis, bootstrap) = transfer_throughput::inputs();
    genesis.allocations.truncate(2);
    genesis.total_initial_nov = "2000000".into();
    let plan = NovNativeCandidateExecutionPlanV1::new(
        bootstrap.context,
        genesis.protocol_config_commitment,
        genesis.compile().unwrap().state_root(),
        None,
        bootstrap.tx_hashes,
        bootstrap.raw_txs,
    )
    .unwrap();
    (genesis, plan)
}

fn transactions() -> Vec<([u8; 32], Vec<u8>)> {
    [
        (64, 201, 1, 100),
        (65, 202, 0, 11),
        (64, 201, 2, 50),
        (64, 202, 3, 2_000_000),
        (64, 64, 4, 7),
        (64, 201, 5, 9),
    ]
    .into_iter()
    .map(|(sender, recipient, nonce, amount)| {
        let mut tx = NovNativeTxWireV1 {
            chain_id: CHAIN,
            kind: NovTxKindV1::Transfer(NovTransferTxV1 {
                from: Vec::new(),
                to: novovm_adapter_novovm::address_from_seed_v1([recipient; 32]),
                asset: "NOV".into(),
                amount,
                nonce,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "NOV".into(),
                    max_pay_amount: 1000,
                    slippage_bps: 0,
                },
            }),
            signature: Vec::new(),
        };
        sign_nov_native_tx_with_seed_v1(&mut tx, [sender; 32]).unwrap();
        let raw = encode_nov_native_tx_wire_v1(&tx).unwrap();
        (
            canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap(),
            raw,
        )
    })
    .collect()
}

fn configure(nodes: &[Node], propose: bool) {
    for node in nodes {
        let path = node.0.join("seal.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["follow_finalized_tip"] = true.into();
        config["receive_successors"] = true.into();
        config["propose_successors"] = propose.into();
        config["transaction_ingress_enabled"] = true.into();
        config["proposal_max_transactions"] = 16.into();
        config["proposal_collect_ms"] = 0.into();
        config["transaction_transport"] = serde_json::json!({
            "per_peer_queue":64,"ingress_per_source_per_second":64,
            "ingress_per_poll":64,"gossip_per_peer_per_second":64,
            "gossip_per_poll":192,"bytes_per_poll":1048576,
        });
        fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    }
}

fn check_running(children: &mut [(usize, Child)], began: Instant, evidence: &std::path::Path) {
    for (index, child) in children {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "node {index} exited; {}",
            evidence.display()
        );
    }
    assert!(
        began.elapsed() < DEADLINE,
        "mixed transfer deadline; {}",
        evidence.display()
    );
}

fn start(
    nodes: &[Node],
    label: &str,
    evidence: &std::path::Path,
) -> (Vec<(usize, Child)>, Vec<String>) {
    let mut children = start_cluster(nodes, &(0..nodes.len()).collect::<Vec<_>>(), label, 0);
    let began = Instant::now();
    loop {
        check_running(&mut children, began, evidence);
        if let Some(addresses) = nodes
            .iter()
            .map(|node| transfer_throughput::address_for(node, label))
            .collect::<Option<Vec<_>>>()
        {
            return (children, addresses);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn stop(mut children: Vec<(usize, Child)>) {
    for (_, child) in &mut children {
        child.0.kill().unwrap();
        child.0.wait().unwrap();
    }
}

fn chain_statuses(addresses: &[String]) -> Vec<Value> {
    addresses
        .iter()
        .map(|address| {
            transfer_throughput::rpc_once(address, "nov_chainStatus", serde_json::json!([]))
                .unwrap()
        })
        .collect()
}

fn transaction_statuses(
    addresses: &[String],
    transactions: &[([u8; 32], Vec<u8>)],
) -> Vec<Vec<Value>> {
    addresses
        .iter()
        .map(|address| {
            transactions
                .iter()
                .map(|(hash, _)| {
                    transfer_throughput::rpc_once(
                        address,
                        "nov_getTransactionStatus",
                        serde_json::json!([hex(hash)]),
                    )
                    .unwrap()
                })
                .collect()
        })
        .collect()
}

fn wait_transactions(
    children: &mut [(usize, Child)],
    addresses: &[String],
    transactions: &[([u8; 32], Vec<u8>)],
    expected: &str,
    evidence: &std::path::Path,
) -> Vec<Vec<Value>> {
    let began = Instant::now();
    loop {
        check_running(children, began, evidence);
        let statuses = transaction_statuses(addresses, transactions);
        for values in &statuses {
            for (value, (hash, _)) in values.iter().zip(transactions) {
                assert_eq!(value["tx_hash"], hex(hash));
                if expected == "queued" {
                    assert_ne!(
                        value["status"], "finalized",
                        "proposal-disabled admission changed authority"
                    );
                }
            }
        }
        if statuses
            .iter()
            .flatten()
            .all(|status| status["status"] == expected)
        {
            return statuses;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn check_receipts(statuses: &[Vec<Value>], transactions: &[([u8; 32], Vec<u8>)]) {
    for node in statuses {
        for (index, (status, (hash, _))) in node.iter().zip(transactions).enumerate() {
            assert_eq!(status["status"], "finalized");
            assert_eq!(status["tx_hash"], hex(hash));
            let receipt = &status["receipt"];
            assert_eq!(receipt["status"], index != 3);
            assert_eq!(receipt["tx_hash"], hex(hash));
            assert!(receipt["settled_fee_nov"].as_u64().unwrap() > 0);
            assert_eq!(receipt["paid_amount"], receipt["settled_fee_nov"]);
            assert_eq!(receipt["paid_asset"], "NOV");
            assert!(receipt["logs"].as_array().unwrap().iter().any(|log| {
                log["event"] == "aoem.native_transfer.computed"
                    && log["data"]["scheduler"] == "aoem_generic_compute_v2"
                    && log["data"]["tx_hash"] == hex(hash)
            }));
            if index == 3 {
                assert!(receipt["failure_reason"]
                    .as_str()
                    .unwrap()
                    .starts_with("native.transfer.insufficient NOV"));
            } else {
                assert!(receipt["failure_reason"].is_null());
            }
        }
    }
}

fn nonce_conflict(raw: &[u8], amount: u128) -> Vec<u8> {
    let mut tx = decode_nov_native_tx_wire_v1(raw).unwrap();
    let NovTxKindV1::Transfer(transfer) = &mut tx.kind else {
        panic!("Transfer fixture");
    };
    transfer.amount = amount;
    sign_nov_native_tx_with_seed_v1(&mut tx, [64; 32]).unwrap();
    encode_nov_native_tx_wire_v1(&tx).unwrap()
}

fn assert_rpc_rejection(address: &str, raw: &[u8], reason: &str) {
    let error = transfer_throughput::rpc_once(
        address,
        "nov_sendRawTransaction",
        serde_json::json!([hex(raw)]),
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.starts_with("RPC nov_sendRawTransaction:") && error.contains(reason),
        "expected explicit RPC rejection {reason}, not a transport failure: {error}"
    );
}

fn check_blocks(blocks: &[NovNativeDurableBlockV1], transactions: &[([u8; 32], Vec<u8>)]) {
    let expected: BTreeMap<_, _> = transactions.iter().cloned().collect();
    let a = novovm_adapter_novovm::address_from_seed_v1([64; 32]);
    let c = novovm_adapter_novovm::address_from_seed_v1([65; 32]);
    let mut seen = BTreeMap::new();
    let mut a_nonces = Vec::new();
    let mut conflicts_in_one_block = false;
    let mut independent_in_one_block = false;
    for block in blocks.iter().skip(1) {
        let mut a_count = 0;
        let mut c_count = 0;
        assert_eq!(block.body.tx_hashes.len(), block.body.raw_txs.len());
        for (hash, raw) in block.body.tx_hashes.iter().zip(&block.body.raw_txs) {
            assert_eq!(
                canonical_nov_native_tx_hash_from_payload_v1(raw).unwrap(),
                *hash
            );
            assert!(
                seen.insert(*hash, raw.clone()).is_none(),
                "duplicate durable transaction"
            );
            let NovTxKindV1::Transfer(transfer) = decode_nov_native_tx_wire_v1(raw).unwrap().kind
            else {
                panic!("mixed fixture must preserve signed Transfer wires");
            };
            if transfer.from == a {
                a_count += 1;
                a_nonces.push(transfer.nonce);
            } else {
                assert_eq!(transfer.from, c);
                assert_eq!(transfer.nonce, 0);
                c_count += 1;
            }
        }
        conflicts_in_one_block |= a_count > 1;
        independent_in_one_block |= a_count > 0 && c_count > 0;
    }
    assert_eq!(seen, expected);
    assert_eq!(
        a_nonces,
        [1, 2, 3, 4, 5],
        "business failure must consume nonce and permit later success"
    );
    assert!(conflicts_in_one_block && independent_in_one_block);
    assert_eq!(
        blocks.last().unwrap().header.state_version - blocks[0].header.state_version,
        6
    );
}

pub(super) fn exercise(nodes: &[Node], evidence: &std::path::Path) {
    oracle_worker();
    assert_eq!(nodes.len(), 4);
    let bootstrap = transfer_throughput::finalized_blocks(&nodes[0], 1);
    let transactions = transactions();
    configure(nodes, false);
    let (mut children, addresses) = start(nodes, "continuous-mixed-admission", evidence);
    let before = chain_statuses(&addresses);
    let admissions: Vec<_> = transactions
        .iter()
        .enumerate()
        .map(|(index, (hash, raw))| {
            let result = transfer_throughput::rpc_once(
                &addresses[0],
                "nov_sendRawTransaction",
                serde_json::json!([hex(raw)]),
            )
            .unwrap();
            assert_eq!(result["status"], "queued");
            assert_eq!(result["tx_hash"], hex(hash));
            if index == 0 {
                assert_rpc_rejection(
                    &addresses[0],
                    &nonce_conflict(raw, 101),
                    "signer nonce conflict",
                );
            }
            result
        })
        .collect();
    let queued = wait_transactions(&mut children, &addresses, &transactions, "queued", evidence);
    let staged = chain_statuses(&addresses);
    for (status, previous) in staged.iter().zip(&before) {
        assert_eq!(status["height"], 1);
        assert_eq!(status["finalized"], true);
        assert_eq!(status["lifecycle_halted"], false);
        assert_eq!(status["automatic_proposal_enabled"], false);
        assert_eq!(status["proposed_successors"], 0);
        assert_eq!(status["transaction_ingress_enabled"], true);
        assert_eq!(status["durable_pending_transactions"], 6);
        assert_eq!(status["publication"], previous["publication"]);
    }
    fs::write(evidence.join("mixed-transfer-admission.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "single_ingress_validator_index":0,"admissions":admissions,"queued_by_node":queued,"before":before,"after":staged,
    })).unwrap()).unwrap();
    stop(children);
    for node in nodes {
        assert_eq!(transfer_throughput::finalized_blocks(node, 1), bootstrap);
        // A historical prefix alone cannot prove that no child was committed
        // between the last live status sample and stopping the process.
        let genesis: FreshGenesisConfigV1 =
            serde_json::from_slice(&fs::read(node.0.join("genesis.json")).unwrap()).unwrap();
        let mut digest = Sha256::new();
        digest.update(b"novovm-native-aoem-state-namespace-v1");
        digest.update(node.0.to_str().unwrap().as_bytes());
        assert!(NovNativeBlockLedgerV1::load_fresh_finality_by_height_v1(
            &nov_native_block_ledger_rocksdb_path_v1(&node.0.join("native.json")),
            genesis.compile().unwrap().config_commitment(),
            digest.finalize().into(),
            2,
        )
        .unwrap()
        .is_none());
    }

    configure(nodes, true);
    let (mut children, addresses) = start(nodes, "continuous-mixed-finality", evidence);
    wait_transactions(
        &mut children,
        &addresses,
        &transactions,
        "finalized",
        evidence,
    );
    let statuses = chain_statuses(&addresses);
    let tip = statuses[0]["height"].as_u64().unwrap();
    for status in &statuses {
        assert_eq!(status["height"], tip);
        assert_eq!(status["finalized"], true);
        assert_eq!(status["lifecycle_halted"], false);
        assert_eq!(status["durable_pending_transactions"], 0);
    }
    let receipts = transaction_statuses(&addresses, &transactions);
    check_receipts(&receipts, &transactions);
    let replay = transfer_throughput::rpc_once(
        &addresses[0],
        "nov_sendRawTransaction",
        serde_json::json!([hex(&transactions[0].1)]),
    )
    .unwrap();
    assert_eq!(replay["status"], "finalized");
    assert_eq!(replay["receipt"], receipts[0][0]["receipt"]);
    assert_rpc_rejection(
        &addresses[0],
        &nonce_conflict(&transactions[0].1, 102),
        "nonce already consumed",
    );
    fs::write(
        evidence.join("mixed-transfer-receipts.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
    stop(children);
    let blocks = transfer_throughput::finalized_blocks(&nodes[0], tip);
    let histories: Vec<_> = nodes
        .iter()
        .map(|node| continuous::history(node, tip))
        .collect();
    check_blocks(&blocks, &transactions);
    for node in nodes {
        assert_eq!(transfer_throughput::finalized_blocks(node, tip), blocks);
    }
    let before_oracle = verify_oracle(nodes, &blocks, &receipts, "before-restart");

    let (mut children, addresses) = start(nodes, "continuous-mixed-restart", evidence);
    let recovered = wait_transactions(
        &mut children,
        &addresses,
        &transactions,
        "finalized",
        evidence,
    );
    assert_eq!(recovered, receipts);
    for status in chain_statuses(&addresses) {
        assert_eq!(status["height"], tip);
        assert_eq!(status["finalized"], true);
        assert_eq!(status["lifecycle_halted"], false);
        assert_eq!(status["durable_pending_transactions"], 0);
    }
    stop(children);
    for (node, history) in nodes.iter().zip(&histories) {
        assert_eq!(transfer_throughput::finalized_blocks(node, tip), blocks);
        // Each node's durable proof must recover exactly. Different valid
        // quorum subsets across nodes need not have identical witness bytes.
        assert_eq!(&continuous::history(node, tip), history);
    }
    assert_eq!(
        verify_oracle(nodes, &blocks, &recovered, "after-restart"),
        before_oracle
    );
    fs::write(evidence.join("mixed-transfer-acceptance.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "scope":"same_host_four_process_mixed_transfer_finality_v1","accepted":true,
        "transaction_count":6,"business_successes":5,"business_failures":1,
        "single_ingress_validator_index":0,"all_nodes_queued_before_proposal":true,
        "conflicting_nonces_ordered":true,"failure_nonce_consumed":true,"serial_oracle_verified":true,
        "pending_nonce_conflict_rejected":true,"consumed_nonce_conflict_rejected":true,"finalized_replay_idempotent":true,
        "full_blocks_equal":true,"all_durable_BFT_proofs_verified":true,
        "restart_proofs_equal_per_node":true,"restart_receipts_equal":true,"durable_pools_empty":true,
        "block_tx_counts":blocks.iter().map(|block|block.header.tx_count).collect::<Vec<_>>(),
        "parallel_overlap_measured":false,"physical_lan_executed":false,"production_signoff":false,
    })).unwrap()).unwrap();
}

fn oracle_worker() -> PathBuf {
    let path = PathBuf::from(std::env::var_os("NOVOVM_TRANSFER_PARITY_WORKER").expect(
        "set NOVOVM_TRANSFER_PARITY_WORKER to the explicitly built novovm-node libtest executable",
    ));
    assert!(
        path.is_file(),
        "serial oracle worker is not a file: {}",
        path.display()
    );
    fs::canonicalize(path).unwrap()
}

fn verify_oracle(
    nodes: &[Node],
    blocks: &[NovNativeDurableBlockV1],
    receipts: &[Vec<Value>],
    label: &str,
) -> Vec<Value> {
    let worker = oracle_worker();
    let tip = blocks.last().unwrap();
    nodes
        .iter()
        .zip(receipts)
        .map(|(node, node_receipts)| {
            let input = node.0.join(format!("mixed-oracle-{label}.input.json"));
            let output = node.0.join(format!("mixed-oracle-{label}.output.json"));
            let stdout = node.0.join(format!("mixed-oracle-{label}.stdout.log"));
            let stderr = node.0.join(format!("mixed-oracle-{label}.stderr.log"));
            assert!(!output.exists(), "oracle must produce new evidence");
            fs::write(
                &input,
                serde_json::to_vec(&serde_json::json!({"tip_height":tip.header.height})).unwrap(),
            )
            .unwrap();
            let template = node.command();
            let mut command = Command::new(&worker);
            for (name, value) in template.get_envs() {
                match value {
                    Some(value) => {
                        command.env(name, value);
                    }
                    None => {
                        command.env_remove(name);
                    }
                }
            }
            command
                .current_dir(template.get_current_dir().unwrap())
                .args([
                    "native_transfer_process_serial_parity_worker_v1",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("NOVOVM_TRANSFER_PARITY_INPUT", &input)
                .env("NOVOVM_TRANSFER_PARITY_OUTPUT", &output)
                .stdout(fs::File::create(&stdout).unwrap())
                .stderr(fs::File::create(&stderr).unwrap());
            let mut child = Child(command.spawn().unwrap());
            let started = Instant::now();
            let status = loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    break status;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(90),
                    "serial oracle timed out: {}",
                    node.0.display()
                );
                std::thread::sleep(Duration::from_millis(25));
            };
            assert!(
                status.success(),
                "serial oracle failed; {} / {}",
                stdout.display(),
                stderr.display()
            );
            let log = fs::read_to_string(&stdout).unwrap();
            assert!(
                log.contains("running 1 test")
                    && log.contains("test result: ok. 1 passed; 0 failed;"),
                "oracle filter must run exactly one test: {log}"
            );
            let report: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
            assert_eq!(report["accepted"], true);
            assert_eq!(
                report["scope"],
                "same_host_four_process_mixed_transfer_serial_reference_v1"
            );
            assert_eq!(report["block_count"], blocks.len());
            assert_eq!(report["tx_count"], 7);
            assert_eq!(report["state_root"], hex(&tip.header.post_state_root));
            assert_eq!(
                report["receipt_root"],
                hex(&tip.header.cumulative_receipt_root)
            );
            assert_eq!(report["receipts"].as_object().unwrap().len(), 7);
            for rpc in node_receipts {
                assert_eq!(
                    report["receipts"][rpc["tx_hash"].as_str().unwrap()],
                    rpc["receipt"]
                );
            }
            report
        })
        .collect()
}
