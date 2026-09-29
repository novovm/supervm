use super::*;
use novovm_node::tx_ingress::fresh_genesis::{
    FreshGenesisConfigV1, GenesisAllocationV1, GenesisValidatorV1, GENESIS_SCHEMA_V1,
};
use novovm_protocol::*;

pub(super) fn inputs() -> (FreshGenesisConfigV1, NovNativeCandidateExecutionPlanV1) {
    let seed = [0xc3; 32];
    let protocol_hex = native_business_protocol_config_commitment_v1().unwrap();
    let protocol: [u8; 32] =
        std::array::from_fn(|i| u8::from_str_radix(&protocol_hex[i * 2..i * 2 + 2], 16).unwrap());
    let config = FreshGenesisConfigV1 {
        schema: GENESIS_SCHEMA_V1.into(),
        chain_id: CHAIN,
        timestamp_unix_ms: 1_900_000_000_000,
        protocol_config_commitment: protocol,
        allocations: vec![GenesisAllocationV1 {
            account: novovm_adapter_novovm::address_from_seed_v1(seed)
                .try_into()
                .unwrap(),
            nov: "1000".into(),
        }],
        total_initial_nov: "1000".into(),
        validators: (1..=4)
            .map(|i| GenesisValidatorV1 {
                public_key: ed25519_dalek::SigningKey::from_bytes(&[i; 32])
                    .verifying_key()
                    .to_bytes(),
                weight: 1,
            })
            .collect(),
    };
    let mut tx = NovNativeTxWireV1 {
        chain_id: CHAIN,
        kind: NovTxKindV1::Execute(NovExecuteTxV1 {
            caller: Vec::new(),
            account_id: Some("fresh-process".into()),
            fee_owner_account_id: Some("fresh-process".into()),
            nonce_owner_account_id: Some("fresh-process".into()),
            target: NovExecutionTargetV1::NativeModule("treasury".into()),
            method: "deposit_reserve".into(),
            args: serde_json::to_vec(&serde_json::json!({"asset":"NOV", "amount":10})).unwrap(),
            execution_mode: NovExecutionModeV1::Batch,
            execution_policy: NovExecutionPolicyV1::Standard,
            privacy_mode: NovPrivacyModeV1::Public,
            verification_mode: NovVerificationModeV1::Standard,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "NOV".into(),
                max_pay_amount: 1000,
                slippage_bps: 100,
            },
            gas_like_limit: Some(90_000),
            nonce: 0,
        }),
        signature: Vec::new(),
    };
    sign_nov_native_tx_with_seed_v1(&mut tx, seed).unwrap();
    let raw = encode_nov_native_tx_wire_v1(&tx).unwrap();
    let plan = NovNativeCandidateExecutionPlanV1::new(
        NovBlockExecutionContextV1 {
            chain_id: CHAIN,
            block_height: 1,
            parent_block_hash: [0; 32],
            slot: 1,
            timestamp_unix_ms: config.timestamp_unix_ms,
        },
        protocol,
        config.compile().unwrap().state_root(),
        None,
        vec![canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap()],
        vec![raw],
    )
    .unwrap();
    (config, plan)
}

pub(super) fn prepare_command(
    node: &Node,
    config: &FreshGenesisConfigV1,
    plan: &NovNativeCandidateExecutionPlanV1,
) -> Command {
    fs::write(
        node.0.join("genesis.json"),
        serde_json::to_vec(config).unwrap(),
    )
    .unwrap();
    fs::write(node.0.join("plan.json"), serde_json::to_vec(plan).unwrap()).unwrap();
    let mut command = node.command();
    command
        .env("NOVOVM_NODE_MODE", "native_fresh_genesis_prepare")
        .env("NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_PATH", "genesis.json")
        .env(
            "NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_COMMITMENT",
            hex(&config.compile().unwrap().config_commitment()),
        )
        .env("NOVOVM_NATIVE_CANDIDATE_PLAN_PATH", "plan.json")
        .env(
            "NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT",
            hex(&plan.plan_commitment),
        );
    command
}

#[test]
fn fresh_genesis_cli_prepares_and_replays_without_importing_test_ledger() {
    let (config, plan) = inputs();
    let node = Node::new("fresh-genesis-cli");
    let mut command = prepare_command(&node, &config, &plan);
    let first = node.run(&mut command, "prepare");
    assert!(first.0, "{}\n{}", first.1, first.2);
    let first: Value = serde_json::from_str(&first.1).unwrap();
    assert_eq!(first["genesis"]["aoem_genesis_state_persisted"], true);
    assert_eq!(first["execution"]["aoem_called"], true);
    assert_eq!(first["execution"]["transactions_authenticated"], true);
    assert_eq!(
        first["execution"]["batch_result"]["per_tx_receipts"][0]["status_ok"],
        true
    );
    assert_eq!(first["execution"]["candidate_state_persisted"], true);
    assert_eq!(first["execution"]["authority_state_published"], false);
    assert_eq!(first["finalized"], false);
    assert!(!node.0.join("native.json").exists());
    assert!(
        NovNativeBlockLedgerV1::open(&node.0.join("native.json.block-ledger.rocksdb")).is_err()
    );
    let again = node.run(&mut prepare_command(&node, &config, &plan), "replay");
    assert!(again.0, "{}", again.2);
    let again: Value = serde_json::from_str(&again.1).unwrap();
    assert_eq!(first, again);

    let rejected = Node::new("fresh-genesis-wrong-pin");
    let result = rejected.run(
        prepare_command(&rejected, &config, &plan).env(
            "NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_COMMITMENT",
            "99".repeat(32),
        ),
        "reject",
    );
    assert!(
        !result.0 && result.2.contains("operator pin"),
        "{}",
        result.2
    );
    assert!(!rejected.0.join("owner.rocksdb").exists());
    assert!(!rejected.0.join("native.json.block-ledger.rocksdb").exists());

    let occupied = Node::new("fresh-genesis-preserve-existing");
    fs::write(
        occupied.0.join("native.json"),
        b"existing-test-ledger-preserve",
    )
    .unwrap();
    let result = occupied.run(
        &mut prepare_command(&occupied, &config, &plan),
        "reject-import",
    );
    assert!(
        !result.0 && result.2.contains("existing Host projection"),
        "{}",
        result.2
    );
    assert_eq!(
        fs::read(occupied.0.join("native.json")).unwrap(),
        b"existing-test-ledger-preserve"
    );
    assert!(!occupied.0.join("owner.rocksdb").exists());
}
