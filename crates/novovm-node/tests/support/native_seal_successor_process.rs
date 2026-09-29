//! A second height is executed independently by live main processes, not
//! precomputed and copied into their candidate stores.
use super::*;
use novovm_node::product_mainline_overlay::{
    load_product_mainline_overlay_config_v1, ProductMainlineOverlayPayloadClassV1,
    ProductMainlineOverlayRuntimeV1,
};
use novovm_protocol::{decode_nov_native_tx_wire_v1, encode_nov_native_tx_wire_v1, NovTxKindV1};

pub(super) fn exercise(
    nodes: &[Node],
    authority: &NovNativeSealEpochAuthorityV1,
    validators: &[NovNativeSealValidatorV1],
    peer_ids: &[String],
    first_plan: &NovNativeCandidateExecutionPlanV1,
    evidence_root: &std::path::Path,
) {
    for node in nodes {
        let path = node.0.join("seal.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["follow_finalized_tip"] = true.into();
        config["receive_successors"] = true.into();
        config["propose_successors"] = true.into();
        fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    }
    let leader = validators
        .iter()
        .position(|v| v.validator_id == authority.expected_leader(2, 0).unwrap())
        .unwrap();
    let sender_index = (0..nodes.len()).find(|i| *i != leader).unwrap();
    let active: Vec<_> = (0..nodes.len()).filter(|i| *i != sender_index).collect();
    let mut tx = decode_nov_native_tx_wire_v1(&first_plan.raw_txs[0]).unwrap();
    let NovTxKindV1::Execute(execution) = &mut tx.kind else {
        panic!("execute fixture")
    };
    execution.nonce = 1;
    sign_nov_native_tx_with_seed_v1(&mut tx, [0xc3; 32]).unwrap();
    let raw = encode_nov_native_tx_wire_v1(&tx).unwrap();
    let hash = canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap();
    {
        // The fourth test identity submits transactions only, not votes. Its
        // validator process is stopped; no simultaneous owners share its key.
        let config =
            load_product_mainline_overlay_config_v1(nodes[sender_index].0.join("overlay.json"))
                .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let sender = ProductMainlineOverlayRuntimeV1::start(config, now).unwrap();
        let inject = || {
            for _ in 0..8 {
                assert!(sender
                    .try_submit_to_peer(
                        &peer_ids[leader],
                        ProductMainlineOverlayPayloadClassV1::NativeTransaction,
                        hash,
                        raw.clone()
                    )
                    .unwrap());
                sender.drain_events(128);
                std::thread::sleep(Duration::from_secs(1));
            }
        };
        run_cluster_at_height(
            nodes,
            &active,
            "successor-two",
            120,
            true,
            true,
            true,
            2,
            Some(&inject),
            true,
        );
    }
    let certificate = |index: usize| {
        let store =
            NovNativeBlockSealStoreV1::open_existing_read_only(&nodes[index].0.join("seal-db"))
                .unwrap()
                .unwrap();
        let qc = store
            .load_decision_certificate_by_height_v3(CHAIN, 1, 2)
            .unwrap()
            .unwrap();
        qc.verify(&authority.validator_set).unwrap();
        assert_eq!(qc.votes.len(), 3);
        qc
    };
    let expected = certificate(active[0]);
    for &index in &active {
        assert_eq!(certificate(index), expected);
    }
    // Original height-one operator files stay unchanged. The three recovered
    // publishers relay body/QC to the fourth, which must execute height two.
    run_cluster_at_height(
        nodes,
        &[0, 1, 2, 3],
        "successor-restart-fourth",
        120,
        true,
        true,
        true,
        2,
        None,
        false,
    );
    for index in 0..4 {
        assert_eq!(certificate(index), expected);
    }
    let mut blocks = Vec::new();
    let mut proofs = Vec::new();
    for node in nodes {
        let genesis: novovm_node::tx_ingress::fresh_genesis::FreshGenesisConfigV1 =
            serde_json::from_slice(&fs::read(node.0.join("genesis.json")).unwrap()).unwrap();
        let pin = genesis.compile().unwrap().config_commitment();
        let mut digest = Sha256::new();
        digest.update(b"novovm-native-aoem-state-namespace-v1");
        digest.update(node.0.to_str().unwrap().as_bytes());
        let namespace: [u8; 32] = digest.finalize().into();
        let ledger = nov_native_block_ledger_rocksdb_path_v1(&node.0.join("native.json"));
        let block = NovNativeBlockLedgerV1::load_fresh_successor_published_block_v1(
            &ledger, pin, namespace,
        )
        .unwrap()
        .unwrap();
        assert_eq!(block.header.height, 2);
        assert_eq!(block.header.block_hash, expected.prepare.subject.block_hash);
        assert_eq!(block.body.tx_hashes, vec![hash]);
        assert_eq!(block.body.raw_txs, vec![raw.clone()]);
        let proof =
            NovNativeBlockLedgerV1::load_fresh_finality_by_height_v1(&ledger, pin, namespace, 2)
                .unwrap()
                .unwrap();
        blocks.push(block);
        proofs.push(proof);
    }
    assert!(blocks.iter().all(|block| block == &blocks[0]));
    assert!(proofs.iter().all(|proof| proof == &proofs[0]));
    fs::write(
        evidence_root.join("successor-acceptance.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"local_independent_main_process_successor_v3",
            "height":2, "transaction_hash":hex(&hash),
            "decision":expected, "active_validator_indices":active,
            "transaction_sender_only_index":sender_index,
            "restart_and_fourth_node_catchup":true,
        "independent_full_block_and_finality_readback_equal":true,
            "forced_process_termination_after_finalization":true,
            "physical_lan_executed":false, "power_loss_executed":false,
            "interrupted_promotion_commit_executed":false
        }))
        .unwrap(),
    )
    .unwrap();
}
