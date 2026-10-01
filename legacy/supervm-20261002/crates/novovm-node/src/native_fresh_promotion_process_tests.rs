// Test-only checkpoints; no production environment switch can stop promotion.
const FRESH_PROCESS_MODE: &str = "NOVOVM_TEST_FRESH_PROMOTION_PROCESS_MODE";
const FRESH_PROCESS_PARAMS: &str = "NOVOVM_TEST_FRESH_PROMOTION_PROCESS_PARAMS";
const FRESH_PROCESS_MARKER: &str = "NOVOVM_TEST_FRESH_PROMOTION_PROCESS_MARKER";

#[allow(clippy::too_many_arguments)]
fn interrupt_fresh_promotion_process(
    point: &str,
    chain: u64,
    parent: [u8; 32],
    id: [u8; 32],
    pin: [u8; 32],
    proof: &crate::native_block_ledger::NovNativeFreshFinalityProofV1,
    block: &crate::native_block_ledger::NovNativeDurableBlockV1,
    params: &serde_json::Value,
) -> ! {
    use workspace::PromotionCheckpointV1 as Checkpoint;
    let target = match point {
        "after_authority" => Checkpoint::AfterPublication,
        "after_ledger" => Checkpoint::AfterLedgerCommit,
        "after_finality" => Checkpoint::AfterFinalityCommit,
        _ => panic!("unknown promotion checkpoint"),
    };
    let checkpoint = |actual| -> Result<()> {
        if actual == target {
            use std::io::Write;
            let marker = PathBuf::from(std::env::var_os(FRESH_PROCESS_MARKER).unwrap());
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(marker)?;
            file.write_all(&serde_json::to_vec(&serde_json::json!({
                "checkpoint":point, "pid":std::process::id(), "chain":chain,
                "parent":parent, "id":id, "pin":pin, "proof":proof,
                "block":block, "params":params,
            }))?)?;
            file.sync_all()?;
            // Keep the live coordinator/locks on this stack until the parent
            // forcibly terminates the actual child process. No error unwinding.
            loop {
                std::thread::park_timeout(std::time::Duration::from_secs(1));
            }
        }
        Ok(())
    };
    workspace::prepare_successor_promotion_v1(chain, parent, id, pin, proof, params).unwrap();
    workspace::complete_successor_with_checkpoint_v1(chain, parent, id, pin, params, checkpoint)
        .unwrap();
    workspace::finalize_successor_with_checkpoint_v1(chain, parent, id, pin, params, checkpoint)
        .unwrap();
    panic!("requested checkpoint was not reached")
}

#[test]
#[ignore = "only invoked by fresh promotion process-kill parent"]
fn candidate_workspace_fresh_promotion_process_worker() {
    let mode = std::env::var(FRESH_PROCESS_MODE).unwrap();
    let params: serde_json::Value =
        serde_json::from_str(&std::env::var(FRESH_PROCESS_PARAMS).unwrap()).unwrap();
    let marker = PathBuf::from(std::env::var_os(FRESH_PROCESS_MARKER).unwrap());
    std::thread::Builder::new()
        .stack_size(crate::native_block_seal::service::FRESH_CHAIN_LIFECYCLE_STACK_BYTES_V1)
        .spawn(move || {
            let path = resolve_native_execution_store_path_from_params_v1(&params).unwrap();
            if mode != "recover" {
                independent_fresh_storage_run(&path, &params, Some(&mode));
                panic!("interrupted worker unexpectedly returned");
            }
            let value: serde_json::Value =
                serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
            assert_eq!(value["params"], params);
            let chain = value["chain"].as_u64().unwrap();
            let parent = serde_json::from_value(value["parent"].clone()).unwrap();
            let id = serde_json::from_value(value["id"].clone()).unwrap();
            let pin = serde_json::from_value(value["pin"].clone()).unwrap();
            let proof = serde_json::from_value(value["proof"].clone()).unwrap();
            let block: crate::native_block_ledger::NovNativeDurableBlockV1 =
                serde_json::from_value(value["block"].clone()).unwrap();
            let ledger = nov_native_block_ledger_rocksdb_path_v1(&path);
            let first = workspace::resume_successor_promotion_v1(
                chain, parent, id, pin, &proof, &ledger, &params,
            )
            .unwrap();
            assert!(
                first.finalized
                    && first.aoem_readback_verified
                    && first.ledger_publication_completed
            );
            let replay = workspace::resume_successor_promotion_v1(
                chain, parent, id, pin, &proof, &ledger, &params,
            )
            .unwrap();
            assert_eq!(first, replay);
            let recovered =
                workspace::load_finalized_genesis_parent_v1(chain, id, pin, &params).unwrap();
            assert_eq!(recovered.block(), &block);
            assert_eq!(recovered.finality_proof(), &proof);
            fs::write(
                marker.with_extension("recovered.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "checkpoint":value["checkpoint"], "terminated_pid":value["pid"],
                    "recovery_pid":std::process::id(), "finalized":true,
                    "idempotent_resume":true, "block":block, "proof":proof,
                    "physical_power_loss":false,
                }))
                .unwrap(),
            )
            .unwrap();
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn candidate_workspace_execution_fresh_genesis_promotion_survives_process_kill() {
    let _guard = PLAN_RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/audit/promotion-process-kill")
        .join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&root).unwrap();
    for point in ["after_authority", "after_ledger", "after_finality"] {
        with_plan_runtime(|_, params| {
            let marker = root.join(format!("{point}.json"));
            let spawn = |mode: &str| {
                let log = fs::File::create(root.join(format!("{point}-{mode}.log"))).unwrap();
                Child(
                    std::process::Command::new(std::env::current_exe().unwrap())
                        .args([
                            "candidate_workspace_fresh_promotion_process_worker",
                            "--ignored",
                            "--test-threads=1",
                            "--nocapture",
                        ])
                        .env(FRESH_PROCESS_MODE, mode)
                        .env(FRESH_PROCESS_PARAMS, serde_json::to_string(params).unwrap())
                        .env(FRESH_PROCESS_MARKER, &marker)
                        .stdin(std::process::Stdio::null())
                        .stderr(log.try_clone().unwrap())
                        .stdout(log)
                        .spawn()
                        .unwrap(),
                )
            };
            let mut child = spawn(point);
            let start = std::time::Instant::now();
            loop {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "worker exited before {point}; logs: {}",
                    root.display()
                );
                if let Ok(bytes) = fs::read(&marker) {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        assert_eq!(value["pid"], child.0.id());
                        assert_eq!(value["checkpoint"], point);
                        break;
                    }
                }
                assert!(
                    start.elapsed() < std::time::Duration::from_secs(120),
                    "checkpoint deadline: {}",
                    root.display()
                );
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let mut recovery = spawn("recover");
            let start = std::time::Instant::now();
            loop {
                if let Some(status) = recovery.0.try_wait().unwrap() {
                    assert!(status.success(), "recovery failed: {}", root.display());
                    break;
                }
                assert!(
                    start.elapsed() < std::time::Duration::from_secs(120),
                    "recovery deadline"
                );
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            let recovered: serde_json::Value =
                serde_json::from_slice(&fs::read(marker.with_extension("recovered.json")).unwrap())
                    .unwrap();
            assert_eq!(recovered["recovery_pid"], recovery.0.id());
            assert_eq!(recovered["terminated_pid"], child.0.id());
            assert_eq!(recovered["finalized"], true);
        });
    }
}
