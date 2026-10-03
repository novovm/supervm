// Included only in the binary test module. Each integration-shaped case owns a
// fresh process and node configuration; no operator database or RPC path override.
fn prepare_funded_pipeline_case(chain_id: u64, count: u64) {
    use novovm_node::tx_ingress::{
        load_nov_native_execution_store_v1, native_host_projection_bootstrap_anchor_commitment_v1,
        save_nov_native_execution_store_v1, NovNativeExecutionStoreV1,
        NOV_NATIVE_AOEM_STATE_BOOTSTRAP_HOST_ANCHOR_ENV,
    };
    use sha2::{Digest, Sha256};

    assert!(std::env::var("NOVOVM_TEST_PIPELINE_CASE").is_ok());
    assert!((1..=5).contains(&count));
    let path = nov_native_execution_store_path_v1();
    assert_eq!(path, std::env::current_dir().unwrap().join("native.json"));
    assert!(!path.exists());
    assert_eq!(
        load_nov_native_execution_store_v1(&path).unwrap(),
        NovNativeExecutionStoreV1::default()
    );
    // One initialization per isolated child, never a transaction-time credit.
    let _marker = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.with_extension("funded-fixture"))
        .unwrap();
    let mut allocation = NovNativeExecutionStoreV1::default();
    for identity in 1..=count {
        let address = novovm_adapter_novovm::address_from_seed_v1(native_fixture_signing_seed_v1(
            chain_id, identity,
        ));
        let account = to_hex_prefixed(&address);
        allocation.module_state.account_asset_balances.insert(
            account,
            std::collections::BTreeMap::from([
                ("USDT".into(), u128::from(identity)),
                ("NOV".into(), 10_000),
            ]),
        );
    }
    let namespace = format!("funded-pipeline-test-{chain_id}");
    let mut hash = Sha256::new();
    hash.update(b"novovm-native-aoem-state-namespace-v1");
    hash.update(namespace.as_bytes());
    let anchor = native_host_projection_bootstrap_anchor_commitment_v1(
        &allocation,
        chain_id,
        &eth_node_hex_lower_v1(&hash.finalize()),
    )
    .unwrap();
    save_nov_native_execution_store_v1(&path, &allocation).unwrap();
    assert_eq!(
        load_nov_native_execution_store_v1(&path).unwrap(),
        allocation
    );
    std::env::set_var("NOVOVM_AOEM_STATE_NAMESPACE", namespace);
    std::env::set_var("NOVOVM_ALLOW_AOEM_STATE_BOOTSTRAP_FROM_HOST", "true");
    std::env::set_var(NOV_NATIVE_AOEM_STATE_BOOTSTRAP_HOST_ANCHOR_ENV, anchor);
}

fn assert_unsealed_pipeline_execution(out: &serde_json::Value, chain_id: u64, count: u64) {
    let projection = &out["batch_result"]["canonical_projection"];
    assert_eq!(projection["tx_count"], count);
    for field in [
        "included_canonical",
        "chain_canonical",
        "proof_sealed",
        "safe",
        "finalized",
    ] {
        assert_eq!(projection[field], false, "{field} must remain false");
    }
    let store = novovm_node::tx_ingress::load_nov_native_execution_store_v1(
        &nov_native_execution_store_path_v1(),
    )
    .unwrap();
    assert!(store.receipts.len() >= count as usize);
    for receipt in store.receipts.values() {
        assert!(
            receipt.status,
            "business receipt rejected: {:?}",
            receipt.failure_reason
        );
        let evidence = receipt
            .aoem_semantic_ingress
            .as_ref()
            .expect("real AOEM ingress evidence");
        assert!(evidence.submitted);
        assert!(evidence.success_ops > 0);
    }
    let executed = store.receipts.len() as u128;
    let funded = store.module_state.account_asset_balances.len() as u128;
    let reserve = store.module_state.treasury_reserves["USDT"];
    assert_eq!(reserve, executed * (executed + 1) / 2);
    let remaining: u128 = store
        .module_state
        .account_asset_balances
        .values()
        .map(|assets| assets.get("USDT").copied().unwrap_or_default())
        .sum();
    assert_eq!(reserve + remaining, funded * (funded + 1) / 2);
    assert_eq!(
        snapshot_network_runtime_native_pending_tx_summary_v1(chain_id).included_canonical_count,
        0
    );
}

fn isolated_pipeline_case(name: &str) -> bool {
    const CHILD: &str = "NOVOVM_TEST_PIPELINE_CASE";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        assert_eq!(
            nov_native_execution_store_path_v1(),
            std::env::current_dir().unwrap().join("native.json")
        );
        return false;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let dir = root
        .join("artifacts/audit/pipeline-process-tests")
        .join(format!("{}-{}-{}", std::process::id(), now_unix_ms(), name));
    fs::create_dir_all(dir.parent().unwrap()).unwrap();
    fs::create_dir(&dir).unwrap();
    fs::create_dir(dir.join("tmp")).unwrap();
    let stdout = fs::File::create(dir.join("stdout.log")).unwrap();
    let stderr = fs::File::create(dir.join("stderr.log")).unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    for (key, _) in std::env::vars_os() {
        let upper = key.to_string_lossy().to_ascii_uppercase();
        if upper.starts_with("NOVOVM_") || upper.starts_with("AOEM_") {
            command.env_remove(key);
        }
    }
    // This binary-test helper exercises historical Host comparison paths.
    // Preserve only the caller's explicit permission, never supply a default.
    if let Some(permit) = std::env::var_os("NOVOVM_ALLOW_LEGACY_HOST_EXECUTION") {
        command.env("NOVOVM_ALLOW_LEGACY_HOST_EXECUTION", permit);
    }
    let mut child = command
        .arg(format!("native_execution_pipeline_tests::{name}"))
        .args(["--exact", "--test-threads=1", "--nocapture"])
        .current_dir(&dir)
        .env(CHILD, name)
        .env("TEMP", dir.join("tmp"))
        .env("TMP", dir.join("tmp"))
        .env("TMPDIR", dir.join("tmp"))
        .env("NOVOVM_NATIVE_EXECUTION_STORE", dir.join("native.json"))
        .env("NOVOVM_UNIFIED_ACCOUNT_DB", dir.join("accounts.rocksdb"))
        .env("NOVOVM_AOEM_ROOT", root.join("aoem"))
        .env("NOVOVM_AOEM_VARIANT", "core")
        .env("NOVOVM_AOEM_PERSIST_BACKEND", "none")
        .env(
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_ASSET",
            "USDT",
        )
        .env(
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_MAX_PAY_AMOUNT",
            "1000",
        )
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(90) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "isolated pipeline case timed out; evidence: {}",
                dir.display()
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stdout = fs::read_to_string(dir.join("stdout.log")).unwrap();
    let stderr = fs::read_to_string(dir.join("stderr.log")).unwrap();
    assert!(
        status.success() && stdout.contains("1 passed; 0 failed"),
        "{name}: {stdout}\n{stderr}\nevidence: {}",
        dir.display()
    );
    true
}

fn assert_pipeline_transport_message(
    message: &novovm_protocol::ProtocolMessage,
    chain_id: u64,
    source: NodeId,
    tx_hash: [u8; 32],
    raw: &[u8],
) {
    let novovm_protocol::ProtocolMessage::EvmNative(
        novovm_protocol::EvmNativeMessage::Transactions {
            from,
            chain_id: actual_chain,
            tx_hash: actual_hash,
            tx_count,
            payload,
            ..
        },
    ) = message
    else {
        panic!("expected transaction message")
    };
    assert_eq!(*from, source);
    assert_eq!(*actual_chain, chain_id);
    assert_eq!(*actual_hash, tx_hash);
    assert_eq!(*tx_count, 1);
    assert_eq!(payload.as_slice(), raw);
}

#[test]
fn native_execution_pipeline_funding_refuses_reinitialization() {
    if isolated_pipeline_case("native_execution_pipeline_funding_refuses_reinitialization") {
        return;
    }
    let chain_id = 9_998_894;
    assert!(std::panic::catch_unwind(|| prepare_funded_pipeline_case(chain_id, 0)).is_err());
    prepare_funded_pipeline_case(chain_id, 2);
    let before = novovm_node::tx_ingress::load_nov_native_execution_store_v1(
        &nov_native_execution_store_path_v1(),
    )
    .unwrap();
    assert!(std::panic::catch_unwind(|| prepare_funded_pipeline_case(chain_id, 2)).is_err());
    assert_eq!(
        novovm_node::tx_ingress::load_nov_native_execution_store_v1(
            &nov_native_execution_store_path_v1(),
        )
        .unwrap(),
        before
    );
}

#[test]
fn native_execution_pipeline_session_scope_releases_on_unwind_and_reopens() {
    if isolated_pipeline_case(
        "native_execution_pipeline_session_scope_releases_on_unwind_and_reopens",
    ) {
        return;
    }
    let chain_id = 9_998_893;
    prepare_funded_pipeline_case(chain_id, 2);
    let payloads = build_native_execution_pipeline_fixture_payloads_v1(chain_id, 2).unwrap();
    let execute = |raw: &[u8]| {
        ingest_local_nov_raw_tx_payload_v1(&serde_json::json!({"chain_id":chain_id}), raw).unwrap();
        let out = run_nov_native_execution_tick_from_params_v1(
            &serde_json::json!({"chain_id":chain_id,"hard_budget_per_tick":1}),
        )
        .unwrap();
        assert_eq!(out["executed_count"], 1);
        assert_unsealed_pipeline_execution(&out, chain_id, 1);
    };
    let caught = std::panic::catch_unwind(|| {
        let _scope = novovm_node::tx_ingress::NativeAoemSemanticSessionScopeV1::default();
        execute(&payloads[0]);
        panic!("intentional fixture unwind after execution");
    });
    assert!(caught.is_err());
    let before = novovm_node::tx_ingress::native_aoem_semantic_ingress_runtime_reuse_counters_v1()
        ["aoem_session_created_count"]
        .as_u64()
        .unwrap();
    assert_eq!(before, 1);
    {
        let _scope = novovm_node::tx_ingress::NativeAoemSemanticSessionScopeV1::default();
        execute(&payloads[1]);
    }
    let after = novovm_node::tx_ingress::native_aoem_semantic_ingress_runtime_reuse_counters_v1()
        ["aoem_session_created_count"]
        .as_u64()
        .unwrap();
    assert_eq!(
        after,
        before + 1,
        "scope exit must release the cached session"
    );
    let store = novovm_node::tx_ingress::load_nov_native_execution_store_v1(
        &nov_native_execution_store_path_v1(),
    )
    .unwrap();
    assert_eq!(store.receipts.len(), 2);
}
