//! Real main processes and bundled AOEM, with private per-test state roots.
use novovm_node::{
    native_block_ledger::{NovNativeBlockLedgerV1, NovNativeDurableBlockV1},
    native_candidate_plan::NovNativeCandidateExecutionPlanV1,
    tx_ingress::*,
};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CHAIN: u64 = 98_919_601;
struct Node(PathBuf);
impl Node {
    fn new(label: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let dir = root
            .join("artifacts/audit/candidate-node-processes")
            .join(format!(
                "{label}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn command(&self) -> Command {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_novovm-node"));
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy().to_ascii_uppercase();
            if name.starts_with("NOVOVM_") || name.starts_with("AOEM_") {
                cmd.env_remove(key);
            }
        }
        cmd.current_dir(&self.0)
            .env("TEMP", &self.0)
            .env("TMP", &self.0)
            .env("TMPDIR", &self.0)
            .env("NOVOVM_NODE_MODE", "native_candidate_execute")
            .env("NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID", CHAIN.to_string())
            .env("NOVOVM_NATIVE_EXECUTION_STORE", self.0.join("native.json"))
            .env("NOVOVM_NATIVE_EXECUTION_STORE_BACKEND", "dual")
            .env("NOVOVM_UNIFIED_ACCOUNT_DB", self.0.join("accounts.rocksdb"))
            .env("NOVOVM_AOEM_ROOT", root.join("aoem"))
            .env("NOVOVM_AOEM_VARIANT", "core")
            .env("NOVOVM_AOEM_PERSIST_BACKEND", "rocksdb")
            .env("AOEM_PERSISTENCE_PATH", self.0.join("persist"))
            .env(
                "NOVOVM_AOEM_OWNED_STATE_DB_PATH",
                self.0.join("owner.rocksdb"),
            )
            .env("NOVOVM_AOEM_STATE_NAMESPACE", self.0.to_str().unwrap())
            .env("NOVOVM_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED", "true")
            .env("NOVOVM_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED", "true")
            .env(
                NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
                "true",
            )
            .env(NOV_NATIVE_LEGACY_HOST_TRANSITIONAL_FALLBACK_ENV, "false")
            .env(
                NOV_NATIVE_PROTOCOL_CONFIG_EXPECTED_COMMITMENT_ENV,
                native_business_protocol_config_commitment_v1().unwrap(),
            );
        cmd
    }
    fn run(&self, cmd: &mut Command, label: &str) -> (bool, String, String) {
        let stdout = self.0.join(format!("{label}.stdout.log"));
        let stderr = self.0.join(format!("{label}.stderr.log"));
        cmd.stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap());
        let mut child = cmd.spawn().unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > Duration::from_secs(90) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("node timed out: {}", self.0.display());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        (
            status.success(),
            fs::read_to_string(stdout).unwrap(),
            fs::read_to_string(stderr).unwrap(),
        )
    }
    fn ledger(&self) -> NovNativeBlockLedgerV1 {
        NovNativeBlockLedgerV1::open_existing_read_only(
            &self.0.join("native.json.block-ledger.rocksdb"),
        )
        .unwrap()
        .unwrap()
    }
    fn execute(&self, plan: &NovNativeCandidateExecutionPlanV1, label: &str) -> Value {
        fs::write(self.0.join("plan.json"), serde_json::to_vec(plan).unwrap()).unwrap();
        let result = self.run(
            self.command()
                .env("NOVOVM_NATIVE_CANDIDATE_PLAN_PATH", "plan.json")
                .env(
                    "NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT",
                    hex(&plan.plan_commitment),
                ),
            label,
        );
        assert!(result.0, "{}: {}", self.0.display(), result.2);
        serde_json::from_str(&result.1).unwrap()
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn candidate_node_cli_rejects_implicit_or_query_execution_before_persistence() {
    let node = Node::new("negative");
    let cases = [
        (
            "missing",
            "native_candidate_execute",
            None,
            false,
            "explicit plan path",
        ),
        (
            "stray",
            "full",
            Some("plan.json"),
            false,
            "requires native_candidate_execute",
        ),
        (
            "query",
            "native_candidate_execute",
            Some("plan.json"),
            true,
            "query override",
        ),
        (
            "pin",
            "native_candidate_execute",
            Some("plan.json"),
            false,
            "64 lowercase",
        ),
    ];
    for (label, mode, path, query, expected) in cases {
        let mut cmd = node.command();
        cmd.env("NOVOVM_NODE_MODE", mode);
        if let Some(path) = path {
            cmd.env("NOVOVM_NATIVE_CANDIDATE_PLAN_PATH", path)
                .env("NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT", "bad");
        }
        if query {
            cmd.env(
                "NOVOVM_MAINLINE_QUERY_METHOD",
                "nov_getNativeBlockLedgerStatus",
            );
        }
        let result = node.run(&mut cmd, label);
        assert!(!result.0);
        assert!(result.2.contains(expected), "{}", result.2);
        assert!(fs::read_dir(&node.0).unwrap().all(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "log")));
    }
}

fn source_candidate() -> (
    Node,
    NovNativeDurableBlockV1,
    NovNativeCandidateExecutionPlanV1,
) {
    // Produce an authentic source candidate through the unchanged normal node
    // pipeline, then extract INPUTS ONLY; never copy its DB/output into peers.
    let source = Node::new("source");
    let result = source.run(
        source
            .command()
            .env("NOVOVM_NODE_MODE", "native_execution_pipeline")
            .env("NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS", "2")
            .env("NOVOVM_NATIVE_EXECUTION_TICK_HARD_BUDGET", "2")
            .env("NOVOVM_NATIVE_EXECUTION_PIPELINE_QUIET_TICKS", "true")
            .env(
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_TX_COUNT",
                "2",
            )
            .env(
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_ASSET",
                "NOV",
            )
            .env(
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_MAX_PAY_AMOUNT",
                "1000",
            ),
        "seed",
    );
    assert!(result.0, "{}", result.2);
    let ledger = source.ledger();
    let block = ledger.load_by_height(CHAIN, 1).unwrap().unwrap();
    let commitment = native_business_protocol_config_commitment_v1().unwrap();
    let protocol =
        std::array::from_fn(|i| u8::from_str_radix(&commitment[i * 2..i * 2 + 2], 16).unwrap());
    let plan = NovNativeCandidateExecutionPlanV1::new(
        block.header.execution_context,
        protocol,
        block.header.pre_state_root,
        block.header.aoem_parent.clone(),
        block.body.tx_hashes.clone(),
        block.body.raw_txs.clone(),
    )
    .unwrap();
    drop(ledger);
    (source, block, plan)
}

#[test]
fn candidate_node_cli_real_aoem_common_plan_and_restart_match_across_processes() {
    let (_source, block, plan) = source_candidate();
    for (label, pin, chain) in [
        ("wrong-pin", "00".repeat(32), CHAIN),
        ("wrong-chain", hex(&plan.plan_commitment), CHAIN + 1),
    ] {
        let rejected = Node::new(label);
        fs::write(
            rejected.0.join("plan.json"),
            serde_json::to_vec(&plan).unwrap(),
        )
        .unwrap();
        let result = rejected.run(
            rejected
                .command()
                .env("NOVOVM_NATIVE_CANDIDATE_PLAN_PATH", "plan.json")
                .env("NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT", pin)
                .env("NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID", chain.to_string()),
            "reject",
        );
        assert!(!result.0);
        assert!(
            result.2.contains(if label == "wrong-pin" {
                "operator pin"
            } else {
                "plan chain"
            }),
            "{}",
            result.2
        );
        assert!(
            fs::read_dir(&rejected.0).unwrap().all(|entry| {
                let path = entry.unwrap().path();
                path.file_name().is_some_and(|name| name == "plan.json")
                    || path.extension().is_some_and(|ext| ext == "log")
            }),
            "invalid input must not initialize persistence"
        );
    }
    // A valid operator pin is not transaction authentication: tamper the signed
    // business input, recompute the outer plan, and require the Host to reject it.
    let mut tx = novovm_protocol::decode_nov_native_tx_wire_v1(&plan.raw_txs[0]).unwrap();
    let novovm_protocol::NovTxKindV1::Execute(execute) = &mut tx.kind else {
        panic!("execute fixture");
    };
    execute.args = serde_json::to_vec(&serde_json::json!({"asset":"NOV","amount":999})).unwrap();
    let raw = novovm_protocol::encode_nov_native_tx_wire_v1(&tx).unwrap();
    let bad = NovNativeCandidateExecutionPlanV1::new(
        plan.context,
        plan.protocol_config_commitment,
        plan.pre_state_root,
        plan.aoem_parent.clone(),
        vec![canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap()],
        vec![raw],
    )
    .unwrap();
    let rejected = Node::new("bad-signature");
    fs::write(
        rejected.0.join("plan.json"),
        serde_json::to_vec(&bad).unwrap(),
    )
    .unwrap();
    let result = rejected.run(
        rejected
            .command()
            .env("NOVOVM_NATIVE_CANDIDATE_PLAN_PATH", "plan.json")
            .env(
                "NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT",
                hex(&bad.plan_commitment),
            ),
        "reject",
    );
    assert!(
        !result.0,
        "an operator pin must not bypass signature checks"
    );
    assert!(
        result.2.to_lowercase().contains("signature"),
        "{}",
        result.2
    );
    if let Some(ledger) = NovNativeBlockLedgerV1::open_existing_read_only(
        &rejected.0.join("native.json.block-ledger.rocksdb"),
    )
    .unwrap()
    {
        assert!(ledger.load_head(CHAIN).unwrap().is_none());
        assert!(ledger.load_prepared(CHAIN).unwrap().is_none());
    }
    for name in ["replica-a", "replica-b"] {
        let node = Node::new(name);
        let out = node.execute(&plan, "execute");
        let actual: NovNativeDurableBlockV1 =
            serde_json::from_value(out["durable_block_candidate_committed"].clone()).unwrap();
        assert_eq!(
            actual, block,
            "all state/receipt/block/evidence commitments must match"
        );
        assert!(actual.header.aoem_readback_verified);
        assert_eq!(actual.body.tx_hashes.len(), 2);
        assert_eq!(
            out["tx_ingress_selected_path"],
            "aoem_runtime_owned_state_persistence"
        );
        assert_eq!(out["legacy_host_transitional_fallback_used"], false);
        for result in out["results"].as_array().unwrap() {
            assert_eq!(result["native_receipt"]["status"], true);
        }
        let persisted = node.ledger();
        assert_eq!(persisted.load_by_height(CHAIN, 1).unwrap().unwrap(), actual);
        for hash in &plan.tx_hashes {
            assert_eq!(
                persisted
                    .load_tx_location(CHAIN, *hash)
                    .unwrap()
                    .unwrap()
                    .block_hash,
                actual.header.block_hash
            );
            assert_eq!(
                persisted
                    .load_receipt_location(CHAIN, *hash)
                    .unwrap()
                    .unwrap()
                    .block_hash,
                actual.header.block_hash
            );
        }
        drop(persisted);
        assert!(!actual.header.finalized && !actual.header.proof_sealed && !actual.header.safe);
        let first = node.ledger().load_head(CHAIN).unwrap();
        let replay = node.execute(&plan, "restart-replay");
        assert_eq!(replay["batch_replay"], true);
        assert_eq!(replay["aoem_reexecution"], false);
        assert_eq!(node.ledger().load_head(CHAIN).unwrap(), first);
    }
}

#[path = "support/native_seal_main_process.rs"]
mod native_seal_main_process;

#[test]
fn candidate_node_cli_rejects_malformed_and_oversized_files_before_persistence() {
    for (label, body, expected) in [
        ("malformed", b"{}".as_slice(), "decode candidate plan"),
        ("oversized", b"".as_slice(), "16 MiB"),
    ] {
        let node = Node::new(label);
        fs::write(node.0.join("plan.json"), body).unwrap();
        if label == "oversized" {
            fs::OpenOptions::new()
                .write(true)
                .open(node.0.join("plan.json"))
                .unwrap()
                .set_len(16 * 1024 * 1024 + 1)
                .unwrap();
        }
        let result = node.run(
            node.command()
                .env("NOVOVM_NATIVE_CANDIDATE_PLAN_PATH", "plan.json")
                .env("NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT", "00".repeat(32)),
            "reject",
        );
        assert!(!result.0);
        assert!(result.2.contains(expected), "{}", result.2);
        assert_eq!(fs::read_dir(&node.0).unwrap().count(), 3);
    }
}
