use super::*;
use novovm_protocol::{decode_nov_native_tx_wire_v1, encode_nov_native_tx_wire_v1, NovTxKindV1};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

pub(super) fn require_private_mount() {
    let parent = std::env::var("NOVOVM_TEST_STORAGE_PARENT_MOUNT_NS")
        .expect("use the private mount namespace readiness runner");
    assert!(parent.starts_with("mnt:[") && parent.ends_with(']'));
    assert_ne!(
        fs::read_link("/proc/self/ns/mnt").expect("Linux private mount namespace required"),
        PathBuf::from(parent)
    );
}

pub(super) struct Volume(pub(super) PathBuf, u64);
impl Volume {
    fn new(path: PathBuf) -> Self {
        Self::with_capacity(path, 8 * 1024 * 1024)
    }

    pub(super) fn with_capacity(path: PathBuf, capacity: u64) -> Self {
        require_private_mount();
        assert!((8 * 1024 * 1024..=64 * 1024 * 1024).contains(&capacity));
        assert!(!path.exists(), "never mount over existing state");
        fs::create_dir(&path).unwrap();
        let status = Command::new("mount")
            .args(["-t", "tmpfs", "-o"])
            .arg(format!(
                "size={capacity},nr_inodes=4096,nodev,nosuid,noexec"
            ))
            .arg("tmpfs")
            .arg(&path)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "private bounded tmpfs required; no host-disk fallback"
        );
        Self(path, capacity)
    }

    pub(super) fn remount(&self, read_only: bool) {
        assert!(Command::new("mount")
            .args([
                "-t",
                "tmpfs",
                "-o",
                if read_only {
                    "remount,ro,nodev,nosuid,noexec"
                } else {
                    "remount,rw,nodev,nosuid,noexec"
                },
                "tmpfs",
            ])
            .arg(&self.0)
            .status()
            .unwrap()
            .success());
    }

    pub(super) fn fill(&self) -> u64 {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.0.join("space-pressure.bin"))
            .unwrap();
        let page = [0x5a; 4096];
        let mut written = 0u64;
        loop {
            match file.write(&page) {
                Ok(count) => {
                    assert!(count > 0);
                    written += count as u64;
                    assert!(
                        written <= self.1,
                        "capacity guard; stop rather than fill host storage"
                    );
                }
                Err(error) => {
                    assert_eq!(error.raw_os_error(), Some(28), "must observe kernel ENOSPC");
                    break;
                }
            }
        }
        file.sync_all().unwrap();
        written
    }

    pub(super) fn release_space(&self) {
        fs::remove_file(self.0.join("space-pressure.bin")).unwrap();
    }
}
impl Drop for Volume {
    fn drop(&mut self) {
        let _ = Command::new("umount").arg(&self.0).status();
    }
}

fn command(node: &Node, label: &str) -> Command {
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
    command
}

fn start(node: &Node, label: &str) -> (Child, String) {
    let mut child = Child(command(node, label).spawn().unwrap());
    let started = Instant::now();
    loop {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "early exit: {} {label}",
            node.0.display()
        );
        let log = fs::read_to_string(node.0.join(format!("{label}.stdout.log"))).unwrap();
        if log.contains("native_seal_service_startup: ") {
            let address = log
                .lines()
                .find_map(|line| line.strip_prefix("native_fresh_rpc_listening: "))
                .unwrap()
                .to_owned();
            return (child, address);
        }
        assert!(started.elapsed() < Duration::from_secs(45));
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn rpc(address: &str, method: &str, argument: Option<&[u8]>) -> Result<Value, String> {
    let params = argument
        .map(|bytes| serde_json::json!([hex(bytes)]))
        .unwrap_or_else(|| serde_json::json!([]));
    let response = ureq::post(&format!("http://{address}/"))
        .timeout(Duration::from_secs(5))
        .set("Content-Type", "application/json")
        .send_string(
            &serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
                .to_string(),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&response.into_string().map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn stop(child: &mut Child) {
    child.0.kill().unwrap();
    assert!(!child.0.wait().unwrap().success());
}

fn read_rpc(address: &str, method: &str, argument: Option<&[u8]>) -> Value {
    assert!(matches!(
        method,
        "nov_getTransactionStatus" | "nov_chainStatus"
    ));
    let started = Instant::now();
    loop {
        match rpc(address, method, argument) {
            Ok(response) => return response,
            Err(error) => assert!(
                started.elapsed() < Duration::from_secs(45),
                "read-only RPC deadline: {error}"
            ),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn failed_exit(child: &mut Child) {
    let started = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(!status.success());
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "storage fault must fail closed"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub(super) fn snapshot(path: &Path) -> BTreeMap<String, String> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            (
                entry.file_name().to_str().unwrap().to_owned(),
                hex(&Sha256::digest(fs::read(entry.path()).unwrap())),
            )
        })
        .collect()
}

fn blocks(node: &Node, height: u64) -> Vec<NovNativeDurableBlockV1> {
    let genesis: novovm_node::tx_ingress::fresh_genesis::FreshGenesisConfigV1 =
        serde_json::from_slice(&fs::read(node.0.join("genesis.json")).unwrap()).unwrap();
    let pin = genesis.compile().unwrap().config_commitment();
    let mut digest = Sha256::new();
    digest.update(b"novovm-native-aoem-state-namespace-v1");
    digest.update(node.0.to_str().unwrap().as_bytes());
    let namespace: [u8; 32] = digest.finalize().into();
    let ledger = nov_native_block_ledger_rocksdb_path_v1(&node.0.join("native.json"));
    (2..=height)
        .map(|current| {
            NovNativeBlockLedgerV1::load_fresh_finalized_block_by_height_v1(
                &ledger, pin, namespace, current,
            )
            .unwrap()
            .unwrap()
            .0
        })
        .collect()
}

pub(super) fn exercise(
    nodes: &[Node],
    authority: &NovNativeSealEpochAuthorityV1,
    validators: &[NovNativeSealValidatorV1],
    plan: &NovNativeCandidateExecutionPlanV1,
    root: &Path,
) {
    let leader = authority.expected_leader(2, 0).unwrap();
    let ingress = validators
        .iter()
        .position(|validator| validator.validator_id != leader)
        .unwrap();
    let node = &nodes[ingress];
    let original = continuous::history(node, 1);
    for peer in nodes {
        let file = peer.0.join("seal.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        config["follow_finalized_tip"] = true.into();
        config["receive_successors"] = true.into();
        config["propose_successors"] = true.into();
        fs::write(file, serde_json::to_vec(&config).unwrap()).unwrap();
    }
    let volume = Volume::new(node.0.join("seal-db.txpool"));
    let mut transactions = Vec::new();
    for nonce in 1..=12 {
        let mut transaction = decode_nov_native_tx_wire_v1(&plan.raw_txs[0]).unwrap();
        let NovTxKindV1::Execute(execution) = &mut transaction.kind else {
            panic!("execute fixture")
        };
        execution.nonce = nonce;
        sign_nov_native_tx_with_seed_v1(&mut transaction, [0xc3; 32]).unwrap();
        let raw = encode_nov_native_tx_wire_v1(&transaction).unwrap();
        let hash = canonical_nov_native_tx_hash_from_payload_v1(&raw).unwrap();
        transactions.push((raw, hash));
    }
    let (mut child, address) = start(node, "storage-before-full");
    let first = rpc(&address, "nov_sendRawTransaction", Some(&transactions[0].0)).unwrap();
    assert_eq!(first["result"]["status"], "queued", "{first}");
    let filled_bytes = volume.fill();
    let mut attempts = vec![serde_json::json!({"nonce":1,"response":first,"acknowledged":true})];
    let mut failed_index = None;
    for (index, (raw, _)) in transactions.iter().enumerate().skip(1) {
        let result = rpc(&address, "nov_sendRawTransaction", Some(raw));
        match result {
            Ok(response) if response["result"]["status"] == "queued" => {
                attempts.push(
                    serde_json::json!({"nonce":index+1,"response":response,"acknowledged":true}),
                );
            }
            other => {
                if let Ok(response) = &other {
                    assert!(
                        response["error"]["message"]
                            .as_str()
                            .unwrap()
                            .contains("persistence failed"),
                        "{response}"
                    );
                }
                attempts.push(
                    serde_json::json!({"nonce":index+1,"response":other.ok(),"acknowledged":false}),
                );
                failed_index = Some(index);
                break;
            }
        }
    }
    let failed_index =
        failed_index.expect("must reach storage failure, not merely test a full filesystem");
    failed_exit(&mut child);
    assert!(
        fs::read_to_string(node.0.join("storage-before-full.stderr.log"))
            .unwrap()
            .contains("fresh lifecycle halted")
    );
    assert_eq!(continuous::history(node, 1), original);
    volume.release_space();
    let (mut child, address) = start(node, "storage-recovered");
    for (_, hash) in &transactions[..failed_index] {
        assert_eq!(
            rpc(&address, "nov_getTransactionStatus", Some(hash)).unwrap()["result"]["status"],
            "queued"
        );
    }
    let ambiguous_before_retry = rpc(
        &address,
        "nov_getTransactionStatus",
        Some(&transactions[failed_index].1),
    )
    .unwrap();
    assert!(["unknown", "queued"]
        .contains(&ambiguous_before_retry["result"]["status"].as_str().unwrap()));
    for (raw, _) in &transactions[..=failed_index] {
        assert_eq!(
            rpc(&address, "nov_sendRawTransaction", Some(raw)).unwrap()["result"]["status"],
            "queued"
        );
    }
    assert_eq!(
        rpc(&address, "nov_chainStatus", None).unwrap()["result"]["durable_pending_transactions"],
        failed_index + 1
    );
    stop(&mut child);
    let before_read_only = snapshot(&volume.0);
    volume.remount(true);
    let mut denied = Child(command(node, "storage-read-only").spawn().unwrap());
    failed_exit(&mut denied);
    let error = fs::read_to_string(node.0.join("storage-read-only.stderr.log")).unwrap();
    assert!(error.contains("Read-only file system"), "{error}");
    assert!(
        !fs::read_to_string(node.0.join("storage-read-only.stdout.log"))
            .unwrap()
            .contains("native_seal_service_startup: ")
    );
    assert_eq!(snapshot(&volume.0), before_read_only);
    assert_eq!(continuous::history(node, 1), original);
    volume.remount(false);
    let mut live: Vec<_> = nodes
        .iter()
        .map(|peer| start(peer, "storage-resume"))
        .collect();
    let started = Instant::now();
    let final_height = loop {
        let mut heights = Vec::new();
        let mut complete = true;
        for (child, address) in &mut live {
            assert!(child.0.try_wait().unwrap().is_none());
            for (_, hash) in &transactions[..=failed_index] {
                let status = read_rpc(address, "nov_getTransactionStatus", Some(hash));
                assert!(status.get("error").is_none(), "{status}");
                complete &= status["result"]["status"] == "finalized";
            }
            let status = read_rpc(address, "nov_chainStatus", None);
            assert_eq!(status["result"]["lifecycle_halted"], false, "{status}");
            complete &= status["result"]["durable_pending_transactions"] == 0
                && status["result"]["finalized"] == true;
            heights.push(status["result"]["height"].as_u64().unwrap());
        }
        if complete && heights.iter().all(|height| *height == heights[0]) {
            break heights[0];
        }
        assert!(
            started.elapsed() < Duration::from_secs(180),
            "post-storage recovery finality deadline"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    for (child, _) in &mut live {
        stop(child);
    }
    assert!(final_height >= 2);
    let history = continuous::history(node, final_height);
    let finalized_blocks = blocks(node, final_height);
    assert_eq!(
        finalized_blocks
            .iter()
            .flat_map(|block| &block.body.raw_txs)
            .collect::<Vec<_>>(),
        transactions[..=failed_index]
            .iter()
            .map(|(raw, _)| raw)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        finalized_blocks
            .iter()
            .flat_map(|block| &block.body.tx_hashes)
            .collect::<Vec<_>>(),
        transactions[..=failed_index]
            .iter()
            .map(|(_, hash)| hash)
            .collect::<Vec<_>>()
    );
    for peer in nodes {
        assert_eq!(blocks(peer, final_height), finalized_blocks);
    }
    let (mut child, address) = start(node, "storage-final-replay");
    for (raw, hash) in &transactions[..=failed_index] {
        let status = rpc(&address, "nov_sendRawTransaction", Some(raw)).unwrap();
        assert_eq!(status["result"]["status"], "finalized");
        assert_eq!(
            rpc(&address, "nov_getTransactionStatus", Some(hash)).unwrap()["result"],
            status["result"]
        );
    }
    assert_eq!(
        rpc(&address, "nov_chainStatus", None).unwrap()["result"]["durable_pending_transactions"],
        0
    );
    stop(&mut child);
    assert_eq!(continuous::history(node, final_height), history);
    let copy = node.0.join("storage-recovered-pool-evidence");
    fs::create_dir(&copy).unwrap();
    for file in fs::read_dir(&volume.0).unwrap() {
        let file = file.unwrap();
        fs::copy(file.path(), copy.join(file.file_name())).unwrap();
    }
    fs::write(root.join("storage-fault-acceptance.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "accepted":true,"production_ready":false,"scope":"local_main_process_transaction_pool_kernel_storage_faults",
        "normal_binary":true,"runtime_fault_hooks":false,"kernel_enospc_errno":28,
        "tmpfs_capacity_bytes":8388608,"filler_bytes":filled_bytes,"ingress_validator_index":ingress,
        "attempts":attempts,"ambiguous_before_retry":ambiguous_before_retry,
        "acknowledged_transactions_survived_restart":true,"read_only_startup_rejected":true,
        "read_only_pool_files_unchanged":true,"confirmed_history":history,
        "finalized_blocks":finalized_blocks,"final_height":final_height,
        "replay_finalized_without_reexecution":true,"recovered_pool_files":copy,
        "node_paths":nodes.iter().map(|node| &node.0).collect::<Vec<_>>(),
        "physical_power_loss_tested":false,"multi_machine_tested":false,
        "aoem_or_seal_disk_full_tested":false,"long_duration_soak_tested":false
    })).unwrap()).unwrap();
    println!(
        "storage fault evidence: {}",
        root.join("storage-fault-acceptance.json").display()
    );
}
