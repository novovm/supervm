use super::*;
use novovm_node::tx_ingress::fresh_genesis::FreshGenesisConfigV1;
use std::{collections::BTreeMap, path::Path};

const CAPACITY: u64 = 64 * 1024 * 1024;

fn copy_files(source: &Path, destination: &Path) {
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        let output = destination.join(entry.file_name());
        assert!(!output.exists());
        fs::copy(entry.path(), output).unwrap();
    }
}

fn records(path: &Path) -> BTreeMap<String, String> {
    let database =
        rocksdb::DB::open_for_read_only(&rocksdb::Options::default(), path, false).unwrap();
    database
        .iterator(rocksdb::IteratorMode::Start)
        .map(|record| {
            let (key, value) = record.unwrap();
            (hex(&key), hex(&Sha256::digest(&value)))
        })
        .collect()
}

fn aoem_data_files(node: &Node) -> BTreeMap<PathBuf, String> {
    let mut files = BTreeMap::new();
    for directory in ["owner.rocksdb", "persist"] {
        for entry in fs::read_dir(node.0.join(directory)).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            assert!(entry.file_type().unwrap().is_file());
            if path
                .extension()
                .is_some_and(|extension| extension == "sst" || extension == "log")
                && entry.metadata().unwrap().len() > 0
            {
                files.insert(path.clone(), hex(&Sha256::digest(fs::read(&path).unwrap())));
            }
        }
    }
    assert!(!files.is_empty());
    files
}

fn command(node: &Node) -> Command {
    let mut command = node.command();
    command
        .env("NOVOVM_NODE_MODE", "native_execution_tick")
        .env("NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS", "1")
        .env("NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS", "100")
        .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED", "true")
        .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_CONFIG", "overlay.json")
        .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_SIGNOFF_REQUIRED", "false")
        .env("NOVOVM_NATIVE_SEAL_ENABLED", "true")
        .env("NOVOVM_NATIVE_SEAL_CONFIG", "seal.json");
    command
}

fn check_unconfirmed(node: &Node, genesis: &FreshGenesisConfigV1) {
    let store = NovNativeBlockSealStoreV1::open_existing_read_only(&node.0.join("seal-db"))
        .unwrap()
        .unwrap();
    assert!(store.load_qcs_by_height(CHAIN, 1, 1).unwrap().is_empty());
    assert!(store
        .load_decision_certificate_by_height_v3(CHAIN, 1, 1)
        .unwrap()
        .is_none());
    let mut digest = Sha256::new();
    digest.update(b"novovm-native-aoem-state-namespace-v1");
    digest.update(node.0.to_str().unwrap().as_bytes());
    assert!(NovNativeBlockLedgerV1::load_fresh_finality_by_height_v1(
        &nov_native_block_ledger_rocksdb_path_v1(&node.0.join("native.json")),
        genesis.compile().unwrap().config_commitment(),
        digest.finalize().into(),
        1,
    )
    .unwrap()
    .is_none());
}

fn healthy(node: &Node, label: &str) {
    let result = node.run(&mut command(node), label);
    assert!(result.0, "{}: {}", node.0.display(), result.2);
    unconfirmed_output(&result.1);
}

fn unconfirmed_output(output: &str) {
    let summary: Value = serde_json::from_str(
        output
            .lines()
            .find_map(|line| line.strip_prefix("native_fresh_genesis_confirmation_summary: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(summary["native_seal_service"]["finalized"], false);
    assert_eq!(summary["native_seal_service"]["decision_confirmed"], false);
    assert_eq!(summary["native_seal_service"]["halted"], false);
}

pub(super) fn exercise(
    nodes: &[Node],
    active: &[usize],
    genesis: &FreshGenesisConfigV1,
    plan: &NovNativeCandidateExecutionPlanV1,
    prepared: &Value,
    root: &Path,
) {
    storage::require_private_mount();
    let node = &nodes[active[0]];
    let mut cases = Vec::new();
    for (label, directory, reject_startup) in [
        ("aoem-provider", "owner.rocksdb", true),
        ("aoem-default-store-control", "persist", false),
        ("seal", "seal-db", true),
    ] {
        healthy(node, &format!("{label}-before-fault"));
        check_unconfirmed(node, genesis);
        let seal_before = records(&node.0.join("seal-db"));
        let ledger_before = records(&node.0.join("native.json.block-ledger.rocksdb"));
        let aoem_before = aoem_data_files(node);
        let path = node.0.join(directory);
        let original = node.0.join(format!("{label}-original-evidence"));
        let recovered = node.0.join(format!("{label}-recovered-evidence"));
        assert!(!original.exists() && !recovered.exists());
        fs::rename(&path, &original).unwrap();
        let volume = storage::Volume::with_capacity(path.clone(), CAPACITY);
        copy_files(&original, &path);
        assert_eq!(storage::snapshot(&path), storage::snapshot(&original));
        let before_read_only = storage::snapshot(&path);
        volume.remount(true);
        let error = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join("read-only-probe"))
            .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(30));
        let denied = node.run(&mut command(node), &format!("{label}-read-only"));
        assert_eq!(
            denied.0, !reject_startup,
            "{label}: {}\n{}",
            denied.1, denied.2
        );
        if reject_startup {
            assert!(denied.2.contains("Read-only file system"), "{}", denied.2);
            assert!(!denied.1.contains("native_seal_service_startup: "));
        } else {
            unconfirmed_output(&denied.1);
        }
        assert!(!denied
            .1
            .contains("native_fresh_genesis_decision_confirmed: "));
        assert_eq!(storage::snapshot(&path), before_read_only);
        volume.remount(false);
        let filled_bytes = volume.fill();
        let full = node.run(&mut command(node), &format!("{label}-full"));
        assert_eq!(full.0, !reject_startup, "{label}: {}\n{}", full.1, full.2);
        if reject_startup {
            assert!(full.2.contains("No space left on device"), "{}", full.2);
            assert!(!full.1.contains("native_seal_service_startup: "));
        } else {
            unconfirmed_output(&full.1);
        }
        assert!(!full.1.contains("native_fresh_genesis_decision_confirmed: "));
        volume.release_space();
        assert_eq!(aoem_data_files(node), aoem_before);
        assert_eq!(records(&node.0.join("seal-db")), seal_before);
        assert_eq!(
            records(&node.0.join("native.json.block-ledger.rocksdb")),
            ledger_before
        );
        check_unconfirmed(node, genesis);
        let replay = node.run(
            &mut super::super::native_fresh_genesis_cli::prepare_command(node, genesis, plan),
            &format!("{label}-candidate-replay"),
        );
        assert!(replay.0, "{label} AOEM replay: {}", replay.2);
        assert_eq!(serde_json::from_str::<Value>(&replay.1).unwrap(), *prepared);
        healthy(node, &format!("{label}-recovered"));
        assert_eq!(records(&node.0.join("seal-db")), seal_before);
        assert_eq!(
            records(&node.0.join("native.json.block-ledger.rocksdb")),
            ledger_before
        );
        fs::create_dir(&recovered).unwrap();
        copy_files(&path, &recovered);
        let recovered_files = storage::snapshot(&path);
        assert_eq!(storage::snapshot(&recovered), recovered_files);
        drop(volume);
        fs::remove_dir(&path).unwrap();
        fs::rename(&recovered, &path).unwrap();
        assert_eq!(storage::snapshot(&path), recovered_files);
        cases.push(serde_json::json!({
            "database":label,"path":path,"original_evidence":original,
            "tmpfs_capacity_bytes":CAPACITY,"filled_bytes":filled_bytes,
            "read_only_errno":30,"full_errno":28,"read_only_stderr":denied.2,"full_stderr":full.2,
            "read_only_files_unchanged":true,"startup_rejected":reject_startup,
            "database_required_for_fresh_state":reject_startup,
            "aoem_data_files_unchanged_by_faults":true,
            "seal_records_preserved":true,"ledger_records_preserved":true,
            "candidate_replay_unchanged":true,"restored_original_snapshot":false,
            "recovered_files":recovered_files
        }));
    }
    fs::write(
        root.join("storage-startup-cases.json"),
        serde_json::to_vec_pretty(&cases).unwrap(),
    )
    .unwrap();
    ledger_promotion_fault(nodes, active, genesis, root);
}

fn ledger_promotion_fault(
    nodes: &[Node],
    active: &[usize],
    genesis: &FreshGenesisConfigV1,
    root: &Path,
) {
    let node = &nodes[active[0]];
    check_unconfirmed(node, genesis);
    let path = node.0.join("native.json.block-ledger.rocksdb");
    let original = node.0.join("ledger-original-evidence");
    let recovered = node.0.join("ledger-recovered-evidence");
    assert!(!original.exists() && !recovered.exists());
    let before = records(&path);
    fs::rename(&path, &original).unwrap();
    let volume = storage::Volume::with_capacity(path.clone(), CAPACITY);
    copy_files(&original, &path);
    let before_read_only = storage::snapshot(&path);
    volume.remount(true);
    let error = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.join("read-only-probe"))
        .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(30));
    let mut children: Vec<_> = active
        .iter()
        .map(|&index| {
            let peer = &nodes[index];
            let child = command(peer)
                .env("NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS", "0")
                .stdout(fs::File::create(peer.0.join("ledger-write-read-only.stdout.log")).unwrap())
                .stderr(fs::File::create(peer.0.join("ledger-write-read-only.stderr.log")).unwrap())
                .spawn()
                .unwrap();
            Child(child)
        })
        .collect();
    let started = Instant::now();
    loop {
        if let Some(status) = children[0].0.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "ledger publication fault deadline: {}",
            node.0.display()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    for child in children.iter_mut().skip(1) {
        if child.0.try_wait().unwrap().is_none() {
            child.0.kill().unwrap();
        }
        child.0.wait().unwrap();
    }
    drop(children);
    let error = fs::read_to_string(node.0.join("ledger-write-read-only.stderr.log")).unwrap();
    assert!(error.contains("Read-only file system"), "{error}");
    let output = fs::read_to_string(node.0.join("ledger-write-read-only.stdout.log")).unwrap();
    assert!(output.contains("native_seal_service_startup: "));
    assert!(!output.contains("native_fresh_genesis_decision_confirmed: "));
    assert_eq!(storage::snapshot(&path), before_read_only);
    assert_eq!(records(&path), before);
    let certificate = {
        let store = NovNativeBlockSealStoreV1::open_existing_read_only(&node.0.join("seal-db"))
            .unwrap()
            .unwrap();
        store
            .load_decision_certificate_by_height_v3(CHAIN, 1, 1)
            .unwrap()
            .unwrap()
    };
    certificate
        .verify(genesis.compile().unwrap().validator_set())
        .unwrap();
    assert_eq!(certificate.signed_weight, 3);
    let seal_after_decision = records(&node.0.join("seal-db"));
    let aoem_before = aoem_data_files(node);
    volume.remount(false);
    let filled_bytes = volume.fill();
    let full = node.run(&mut command(node), "ledger-promotion-restart-full");
    assert!(!full.0, "{}", full.1);
    assert!(full.2.contains("No space left on device"), "{}", full.2);
    assert!(!full.1.contains("native_seal_service_startup: "));
    assert!(!full.1.contains("native_fresh_genesis_decision_confirmed: "));
    volume.release_space();
    assert_eq!(records(&path), before);
    assert_eq!(records(&node.0.join("seal-db")), seal_after_decision);
    assert_eq!(aoem_data_files(node), aoem_before);
    fs::create_dir(&recovered).unwrap();
    copy_files(&path, &recovered);
    let recovered_files = storage::snapshot(&path);
    assert_eq!(storage::snapshot(&recovered), recovered_files);
    drop(volume);
    fs::remove_dir(&path).unwrap();
    fs::rename(&recovered, &path).unwrap();
    assert_eq!(storage::snapshot(&path), recovered_files);
    fs::write(root.join("ledger-promotion-fault.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "path":path,"tmpfs_capacity_bytes":CAPACITY,"filled_bytes":filled_bytes,
        "read_only_errno":30,"full_errno":28,"read_only_write_stderr":error,"full_restart_stderr":full.2,
        "ledger_unmodified_under_faults":true,"decision_archived_before_publication_failure":certificate,
        "archived_decision_preserved_on_failed_restart":true,"restored_original_snapshot":false,
        "aoem_data_files_unchanged_by_full_restart":true,"recovered_files":recovered_files
    })).unwrap()).unwrap();
}

pub(super) fn finish(nodes: &[Node], root: &Path) {
    let cases: Value =
        serde_json::from_slice(&fs::read(root.join("storage-startup-cases.json")).unwrap())
            .unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 3);
    let ledger_fault: Value =
        serde_json::from_slice(&fs::read(root.join("ledger-promotion-fault.json")).unwrap())
            .unwrap();
    let histories: Vec<_> = nodes
        .iter()
        .map(|node| continuous::history(node, 1))
        .collect();
    assert!(histories.iter().all(|history| history == &histories[0]));
    let report = root.join("storage-startup-acceptance.json");
    fs::write(&report, serde_json::to_vec_pretty(&serde_json::json!({
        "accepted":true,"production_ready":false,"scope":"local_database_startup_and_ledger_publication_storage_faults",
        "normal_binary":true,"runtime_fault_hooks":false,"cases":cases,"ledger_publication_fault":ledger_fault,
        "three_of_four_finalized_after_recovery":true,"restart_preserved_finality":true,
        "late_fourth_caught_up":true,"confirmed_history":histories[0],
        "node_paths":nodes.iter().map(|node| &node.0).collect::<Vec<_>>(),
        "ledger_publication_write_fault_tested":true,"aoem_and_seal_inflight_write_faults_tested":false,
        "physical_power_loss_tested":false,
        "long_duration_soak_tested":false,"multi_machine_tested":false
    })).unwrap()).unwrap();
    println!("storage startup evidence: {}", report.display());
}
