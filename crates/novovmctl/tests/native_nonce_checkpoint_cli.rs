//! Real-process CLI checks against deterministic, unsealed legacy test claims.
//! The fixture is not AOEM execution evidence, a trusted chain, or an upgrade.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use novovm_node::tx_ingress::native_nonce_bundle::checkpoint_bundle_digest_v1;
use novovm_node::tx_ingress::native_nonce_checkpoint::NonceMigrationCheckpointV1;
use rocksdb::{IteratorMode, Options, WriteBatch, WriteOptions, DB};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFixture {
    snapshot_json: String,
    checkpoint: NonceMigrationCheckpointV1,
    records: Vec<[String; 2]>,
}

fn fixture_root() -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "novovmctl-native-nonce-cli-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir(&path).expect("create unique CLI fixture under test TEMP");
    path
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert!(value.len().is_multiple_of(2), "fixture hex has odd length");
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = char::from(pair[0])
                .to_digit(16)
                .expect("fixture hex high digit");
            let low = char::from(pair[1])
                .to_digit(16)
                .expect("fixture hex low digit");
            ((high << 4) | low) as u8
        })
        .collect()
}

fn restore_source(root: &Path) -> (SourceFixture, PathBuf, PathBuf) {
    let fixture: SourceFixture =
        serde_json::from_str(include_str!("fixtures/native_nonce_checkpoint_v1.json"))
            .expect("decode frozen source fixture");
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    let snapshot = source.join("legacy-store.json");
    fs::write(&snapshot, fixture.snapshot_json.as_bytes()).unwrap();
    let ledger = source.join("ledger.rocksdb");
    let mut options = Options::default();
    options.create_if_missing(true);
    let db = DB::open(&options, &ledger).expect("create isolated fixture ledger");
    let mut batch = WriteBatch::default();
    for [key, value] in &fixture.records {
        batch.put(decode_hex(key), decode_hex(value));
    }
    let mut writes = WriteOptions::default();
    writes.set_sync(true);
    db.write_opt(batch, &writes).unwrap();
    drop(db);
    (fixture, snapshot, ledger)
}

fn ledger_records(path: &Path) -> Vec<(Vec<u8>, Vec<u8>)> {
    let db = DB::open_for_read_only(&Options::default(), path, false)
        .expect("read copied source ledger records");
    db.iterator(IteratorMode::Start)
        .map(|record| {
            let (key, value) = record.expect("read source ledger entry");
            (key.to_vec(), value.to_vec())
        })
        .collect()
}

fn cli(action: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_novovmctl"));
    command.arg("native-nonce-migration").arg(action);
    command
}

fn add_checkpoint(command: &mut Command, checkpoint: &NonceMigrationCheckpointV1) {
    command
        .arg("--chain-id")
        .arg(checkpoint.chain_id.to_string())
        .arg("--namespace-digest")
        .arg(&checkpoint.namespace_digest)
        .arg("--legacy-protocol-commitment")
        .arg(&checkpoint.legacy_protocol_config_commitment)
        .arg("--tip-block-hash")
        .arg(&checkpoint.tip_block_hash)
        .arg("--snapshot-digest")
        .arg(&checkpoint.snapshot_digest);
}

fn export_command(
    snapshot: &Path,
    ledger: &Path,
    checkpoint: &NonceMigrationCheckpointV1,
    target: &Path,
) -> Command {
    let mut command = cli("export");
    command
        .arg("--snapshot")
        .arg(snapshot)
        .arg("--ledger")
        .arg(ledger)
        .arg("--bundle-out")
        .arg(target);
    add_checkpoint(&mut command, checkpoint);
    command
}

fn verify_command(bundle: &Path, checkpoint: &NonceMigrationCheckpointV1, digest: &str) -> Command {
    let mut command = cli("verify");
    command
        .arg("--bundle")
        .arg(bundle)
        .arg("--bundle-digest")
        .arg(digest);
    add_checkpoint(&mut command, checkpoint);
    command
}

fn upgrade_command(
    action: &str,
    bundle: &Path,
    checkpoint: &NonceMigrationCheckpointV1,
    digest: &str,
    target_protocol: &str,
    workspace: &Path,
) -> Command {
    let mut command = cli(action);
    command
        .arg("--bundle")
        .arg(bundle)
        .arg("--bundle-digest")
        .arg(digest)
        .arg("--target-protocol-commitment")
        .arg(target_protocol)
        .arg("--workspace")
        .arg(workspace);
    add_checkpoint(&mut command, checkpoint);
    command
}

fn observed_target_protocol() -> String {
    let report = run_json(cli("target-protocol"), true);
    assert_eq!(report["data"]["action"], "target-protocol");
    assert_eq!(report["data"]["observed_only"], true);
    for field in [
        "authority_state_published",
        "activation_ready",
        "import_performed",
    ] {
        assert_eq!(report["data"][field], false);
    }
    let target = report["data"]["target_protocol_commitment"]
        .as_str()
        .unwrap();
    assert_eq!(target.len(), 64);
    assert!(target
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    target.to_string()
}

fn workspace_files(workspace: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(workspace)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            (
                entry.file_name().into_string().unwrap(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn assert_upgrade_report(report: &Value, action: &str, complete: bool) {
    assert_eq!(report["data"]["action"], action);
    assert_eq!(report["data"]["report"]["artifact_complete"], complete);
    assert_eq!(
        report["data"]["report"]["phase"],
        if complete { "complete" } else { "prepared" }
    );
    assert!(report["data"]["report"]["transition_id"].is_string());
    assert!(report["data"]["report"]["proposed_state_root"].is_string());
    for field in [
        "authority_state_published",
        "activation_ready",
        "import_performed",
    ] {
        assert_eq!(
            report["data"][field], false,
            "command must not promote {field}"
        );
        assert_eq!(
            report["data"]["report"][field], false,
            "journal must not promote {field}"
        );
    }
}

fn run_json(mut command: Command, success: bool) -> Value {
    let output = command.output().expect("launch actual novovmctl binary");
    assert_eq!(
        output.status.success(),
        success,
        "unexpected CLI exit {:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = if success {
        &output.stdout
    } else {
        assert_ne!(output.status.code(), Some(0));
        assert!(
            output.stdout.is_empty(),
            "failed command must not emit a success report"
        );
        &output.stderr
    };
    let report: Value = serde_json::from_slice(bytes).unwrap_or_else(|error| {
        panic!(
            "CLI did not emit one JSON document: {error}: {}",
            String::from_utf8_lossy(bytes)
        )
    });
    assert_eq!(report["ok"], success);
    assert_eq!(report["command"], "native-nonce-migration");
    if !success {
        assert!(report["error"]["kind"].is_string());
        assert!(report["error"]["message"].is_string());
    }
    report
}

fn assert_not_activation(report: &Value) {
    for field in [
        "independent_provenance_verified",
        "execution_replayed",
        "aoem_evidence_verified",
        "qc_verified",
        "chain_canonical",
        "activation_ready",
        "import_performed",
    ] {
        assert_eq!(report[field], false, "must not promote {field}");
    }
    for field in [
        "snapshot_provenance_verified",
        "execution_results_verified",
        "qc_verified",
        "activation_ready",
        "import_performed",
    ] {
        assert_eq!(
            report["migration"][field], false,
            "migration must not promote {field}"
        );
    }
}

#[test]
fn native_nonce_checkpoint_cli_exports_and_verifies_without_sources_or_mutation() {
    let root = fixture_root();
    let (fixture, snapshot, ledger) = restore_source(&root);
    let before = ledger_records(&ledger);
    assert!(!before.is_empty());
    let snapshot_before = fs::read(&snapshot).unwrap();

    let mut inspect = cli("inspect");
    inspect
        .arg("--snapshot")
        .arg(&snapshot)
        .arg("--ledger")
        .arg(&ledger)
        .arg("--chain-id")
        .arg(fixture.checkpoint.chain_id.to_string());
    let inspection = run_json(inspect, true);
    assert_eq!(inspection["data"]["observed_only"], true);
    assert_eq!(
        inspection["data"]["inspection"]["checkpoint"],
        serde_json::to_value(&fixture.checkpoint).unwrap()
    );
    assert_eq!(
        inspection["data"]["inspection"]["independent_provenance_verified"],
        false
    );
    assert_eq!(inspection["data"]["activation_ready"], false);
    assert_eq!(inspection["data"]["import_performed"], false);

    let wrong_target = root.join("must-not-be-created.bin");
    let mut wrong_checkpoint = fixture.checkpoint.clone();
    wrong_checkpoint.tip_block_hash = "00".repeat(32);
    run_json(
        export_command(&snapshot, &ledger, &wrong_checkpoint, &wrong_target),
        false,
    );
    assert!(
        !wrong_target.exists(),
        "invalid pins must fail before creating output"
    );

    let existing = root.join("already-exists.bin");
    fs::write(&existing, b"existing evidence is preserved").unwrap();
    run_json(
        export_command(&snapshot, &ledger, &fixture.checkpoint, &existing),
        false,
    );
    assert_eq!(
        fs::read(existing).unwrap(),
        b"existing evidence is preserved"
    );

    let forbidden = ledger.join("must-not-write-here.bin");
    run_json(
        export_command(&snapshot, &ledger, &fixture.checkpoint, &forbidden),
        false,
    );
    assert!(!forbidden.exists());

    let bundle_path = root.join("checkpoint.bin");
    let exported = run_json(
        export_command(&snapshot, &ledger, &fixture.checkpoint, &bundle_path),
        true,
    );
    let bundle_bytes = fs::read(&bundle_path).unwrap();
    let digest = exported["data"]["bundle_digest"].as_str().unwrap();
    assert_eq!(digest, checkpoint_bundle_digest_v1(&bundle_bytes));
    assert_eq!(exported["data"]["bundle_bytes"], bundle_bytes.len());
    assert_eq!(
        exported["data"]["report"]["checkpoint_anchors_matched"],
        true
    );
    assert_not_activation(&exported["data"]["report"]);
    assert_eq!(
        ledger_records(&ledger),
        before,
        "inspect/export changed source ledger entries"
    );
    assert_eq!(fs::read(&snapshot).unwrap(), snapshot_before);

    // Move only the unique source directory created by this test. The old
    // source paths must be absent when the verifier reads the portable bundle.
    let moved = root.join("source-removed-from-original-location");
    fs::rename(root.join("source"), &moved).unwrap();
    assert!(!snapshot.exists() && !ledger.exists());
    let verified = run_json(
        verify_command(&bundle_path, &fixture.checkpoint, digest),
        true,
    );
    assert_eq!(verified["data"]["bundle_digest_verified"], true);
    assert_eq!(verified["data"]["report"], exported["data"]["report"]);
    assert_not_activation(&verified["data"]["report"]);

    let wrong_digest = if digest == "00".repeat(32) {
        "01".repeat(32)
    } else {
        "00".repeat(32)
    };
    let rejected = run_json(
        verify_command(&bundle_path, &fixture.checkpoint, &wrong_digest),
        false,
    );
    assert!(rejected["error"]["message"]
        .as_str()
        .unwrap()
        .contains("independently supplied digest"));
    run_json(
        verify_command(&bundle_path, &wrong_checkpoint, digest),
        false,
    );
    assert_eq!(fs::read(&bundle_path).unwrap(), bundle_bytes);
    assert_eq!(ledger_records(&moved.join("ledger.rocksdb")), before);
    assert_eq!(
        fs::read(moved.join("legacy-store.json")).unwrap(),
        snapshot_before
    );
}

#[test]
fn native_nonce_checkpoint_cli_rejects_malformed_bundle_and_missing_arguments_as_json() {
    let root = fixture_root();
    let fixture: SourceFixture =
        serde_json::from_str(include_str!("fixtures/native_nonce_checkpoint_v1.json")).unwrap();
    let malformed = root.join("truncated.bin");
    let bytes = b"NVNCPK1\0\xff\xff\xff\xff";
    fs::write(&malformed, bytes).unwrap();
    // Use the actual digest so failure comes from framing, not transport check.
    run_json(
        verify_command(
            &malformed,
            &fixture.checkpoint,
            &checkpoint_bundle_digest_v1(bytes),
        ),
        false,
    );
    assert_eq!(fs::read(&malformed).unwrap(), bytes);
    let missing = root.join("not-created.bin");
    run_json(
        verify_command(&missing, &fixture.checkpoint, &"ab".repeat(32)),
        false,
    );
    assert!(!missing.exists());
    for action in [
        "inspect",
        "export",
        "verify",
        "prepare-upgrade",
        "resume-upgrade",
        "inspect-upgrade",
    ] {
        let report = run_json(cli(action), false);
        assert_eq!(report["error"]["kind"], "InvalidArgument");
    }
}

#[test]
fn native_nonce_upgrade_cli_stages_pinned_proposal_without_source_mutation_or_activation() {
    let root = fixture_root();
    let (fixture, snapshot, ledger) = restore_source(&root);
    let source_records = ledger_records(&ledger);
    let source_snapshot = fs::read(&snapshot).unwrap();
    let bundle = root.join("checkpoint.bin");
    let exported = run_json(
        export_command(&snapshot, &ledger, &fixture.checkpoint, &bundle),
        true,
    );
    let digest = exported["data"]["bundle_digest"].as_str().unwrap();
    let target_protocol = observed_target_protocol();
    let workspace = root.join("upgrade-proposal");
    let wrong_target = if target_protocol == "00".repeat(32) {
        "01".repeat(32)
    } else {
        "00".repeat(32)
    };
    run_json(
        upgrade_command(
            "prepare-upgrade",
            &bundle,
            &fixture.checkpoint,
            digest,
            &wrong_target,
            &workspace,
        ),
        false,
    );
    assert!(
        !workspace.exists(),
        "wrong target must fail before creating output"
    );
    let mut wrong_checkpoint = fixture.checkpoint.clone();
    wrong_checkpoint.snapshot_digest = "00".repeat(32);
    run_json(
        upgrade_command(
            "prepare-upgrade",
            &bundle,
            &wrong_checkpoint,
            digest,
            &target_protocol,
            &workspace,
        ),
        false,
    );
    assert!(
        !workspace.exists(),
        "wrong source pins must fail before creating output"
    );
    run_json(
        upgrade_command(
            "prepare-upgrade",
            &bundle,
            &fixture.checkpoint,
            digest,
            &target_protocol,
            &ledger,
        ),
        false,
    );
    assert_eq!(ledger_records(&ledger), source_records);

    let prepared = run_json(
        upgrade_command(
            "prepare-upgrade",
            &bundle,
            &fixture.checkpoint,
            digest,
            &target_protocol,
            &workspace,
        ),
        true,
    );
    assert_upgrade_report(&prepared, "prepare-upgrade", true);
    assert_eq!(prepared["data"]["report"]["source_bundle_digest"], digest);
    let original_files = workspace_files(&workspace);
    assert!(!original_files.is_empty());
    run_json(
        upgrade_command(
            "prepare-upgrade",
            &bundle,
            &fixture.checkpoint,
            digest,
            &target_protocol,
            &workspace,
        ),
        false,
    );
    assert_eq!(workspace_files(&workspace), original_files);

    // No original snapshot or ledger remains at the exported source paths.
    let moved = root.join("source-moved-away");
    fs::rename(root.join("source"), &moved).unwrap();
    for action in ["inspect-upgrade", "resume-upgrade"] {
        let checked = run_json(
            upgrade_command(
                action,
                &bundle,
                &fixture.checkpoint,
                digest,
                &target_protocol,
                &workspace,
            ),
            true,
        );
        assert_upgrade_report(&checked, action, true);
        assert_eq!(checked["data"]["report"], prepared["data"]["report"]);
        assert_eq!(workspace_files(&workspace), original_files);
        run_json(
            upgrade_command(
                action,
                &bundle,
                &wrong_checkpoint,
                digest,
                &target_protocol,
                &workspace,
            ),
            false,
        );
        assert_eq!(workspace_files(&workspace), original_files);
    }

    let second = root.join("same-proposal-independent-workspace");
    let repeated = run_json(
        upgrade_command(
            "prepare-upgrade",
            &bundle,
            &fixture.checkpoint,
            digest,
            &target_protocol,
            &second,
        ),
        true,
    );
    assert_eq!(repeated["data"]["report"], prepared["data"]["report"]);
    assert_eq!(
        workspace_files(&second),
        original_files,
        "proposal artifacts must be deterministic"
    );
    assert_eq!(
        ledger_records(&moved.join("ledger.rocksdb")),
        source_records
    );
    assert_eq!(
        fs::read(moved.join("legacy-store.json")).unwrap(),
        source_snapshot
    );
}

#[test]
fn native_nonce_upgrade_cli_inspects_partial_artifact_resumes_and_rejects_corruption() {
    let root = fixture_root();
    let (fixture, snapshot, ledger) = restore_source(&root);
    let bundle = root.join("checkpoint.bin");
    let exported = run_json(
        export_command(&snapshot, &ledger, &fixture.checkpoint, &bundle),
        true,
    );
    let digest = exported["data"]["bundle_digest"].as_str().unwrap();
    let target_protocol = observed_target_protocol();
    let workspace = root.join("interrupted-proposal");
    let completed = run_json(
        upgrade_command(
            "prepare-upgrade",
            &bundle,
            &fixture.checkpoint,
            digest,
            &target_protocol,
            &workspace,
        ),
        true,
    );
    let complete_files = workspace_files(&workspace);
    let artifact_path = workspace.join("transition.json");
    let artifact = fs::read(&artifact_path).unwrap();
    assert!(artifact.len() > 2);

    // Fault injection touches only files created by this test. This models a
    // valid interrupted prefix, not a real power-loss or filesystem guarantee.
    fs::remove_file(workspace.join("complete.json")).unwrap();
    fs::write(&artifact_path, &artifact[..artifact.len() / 2]).unwrap();
    let partial_files = workspace_files(&workspace);
    let inspected = run_json(
        upgrade_command(
            "inspect-upgrade",
            &bundle,
            &fixture.checkpoint,
            digest,
            &target_protocol,
            &workspace,
        ),
        true,
    );
    assert_upgrade_report(&inspected, "inspect-upgrade", false);
    assert_eq!(
        workspace_files(&workspace),
        partial_files,
        "inspection must be read-only"
    );
    let resumed = run_json(
        upgrade_command(
            "resume-upgrade",
            &bundle,
            &fixture.checkpoint,
            digest,
            &target_protocol,
            &workspace,
        ),
        true,
    );
    assert_upgrade_report(&resumed, "resume-upgrade", true);
    assert_eq!(resumed["data"]["report"], completed["data"]["report"]);
    assert_eq!(workspace_files(&workspace), complete_files);

    // A completion marker must never authorize a partial or modified artifact.
    fs::write(&artifact_path, &artifact[..artifact.len() / 2]).unwrap();
    let invalid_complete = workspace_files(&workspace);
    for action in ["inspect-upgrade", "resume-upgrade"] {
        run_json(
            upgrade_command(
                action,
                &bundle,
                &fixture.checkpoint,
                digest,
                &target_protocol,
                &workspace,
            ),
            false,
        );
        assert_eq!(workspace_files(&workspace), invalid_complete);
    }
    fs::remove_file(workspace.join("complete.json")).unwrap();
    fs::write(
        &artifact_path,
        b"not a prefix of the verified state transition",
    )
    .unwrap();
    let corrupt_files = workspace_files(&workspace);
    for action in ["inspect-upgrade", "resume-upgrade"] {
        run_json(
            upgrade_command(
                action,
                &bundle,
                &fixture.checkpoint,
                digest,
                &target_protocol,
                &workspace,
            ),
            false,
        );
        assert_eq!(
            workspace_files(&workspace),
            corrupt_files,
            "corruption must not be overwritten"
        );
    }
}
