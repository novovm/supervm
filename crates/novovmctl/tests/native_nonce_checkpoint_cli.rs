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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationFixture {
    authority: Value,
    certificate: Value,
    expected_authority_commitment: String,
    target_protocol_commitment: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceQcFixture {
    authority: Value,
    expected_authority_commitment: String,
    source_qc: Value,
}

struct SourceQcCli {
    bundle: PathBuf,
    checkpoint: NonceMigrationCheckpointV1,
    digest: String,
    authority: PathBuf,
    expected_authority_commitment: String,
    source_qc: PathBuf,
}

impl SourceQcCli {
    fn command(&self) -> Command {
        let mut command = cli("verify-source-qc");
        command
            .arg("--bundle")
            .arg(&self.bundle)
            .arg("--bundle-digest")
            .arg(&self.digest)
            .arg("--authority")
            .arg(&self.authority)
            .arg("--expected-authority-commitment")
            .arg(&self.expected_authority_commitment)
            .arg("--source-qc")
            .arg(&self.source_qc);
        add_checkpoint(&mut command, &self.checkpoint);
        command
    }
}

fn source_qc_inputs(
    root: &Path,
    source: &SourceFixture,
    snapshot: &Path,
    ledger: &Path,
) -> (SourceQcFixture, SourceQcCli) {
    let fixture: SourceQcFixture =
        serde_json::from_str(include_str!("fixtures/native_nonce_source_qc_v1.json"))
            .expect("decode frozen test-only source prepare-QC evidence");
    let bundle = root.join("checkpoint.bin");
    let exported = run_json(
        export_command(snapshot, ledger, &source.checkpoint, &bundle),
        true,
    );
    let digest = exported["data"]["bundle_digest"]
        .as_str()
        .unwrap()
        .to_string();
    let authority = root.join("authority.json");
    let source_qc = root.join("source-qc.json");
    fs::write(&authority, serde_json::to_vec(&fixture.authority).unwrap()).unwrap();
    fs::write(&source_qc, serde_json::to_vec(&fixture.source_qc).unwrap()).unwrap();
    let inputs = SourceQcCli {
        bundle,
        checkpoint: source.checkpoint.clone(),
        digest,
        authority,
        expected_authority_commitment: fixture.expected_authority_commitment.clone(),
        source_qc,
    };
    (fixture, inputs)
}

struct AuthorizationCli {
    bundle: PathBuf,
    checkpoint: NonceMigrationCheckpointV1,
    digest: String,
    target_protocol: String,
    authority: PathBuf,
    expected_authority_commitment: String,
    certificate: PathBuf,
}

impl AuthorizationCli {
    fn command(&self) -> Command {
        let mut command = cli("verify-upgrade-authorization");
        // The frozen certificate authorizes the compiled default protocol, not
        // a developer shell's overrides. Do not modify the parent environment.
        for (name, _) in std::env::vars_os() {
            if name
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("NOVOVM_NATIVE_")
            {
                command.env_remove(name);
            }
        }
        command
            .arg("--bundle")
            .arg(&self.bundle)
            .arg("--bundle-digest")
            .arg(&self.digest)
            .arg("--target-protocol-commitment")
            .arg(&self.target_protocol)
            .arg("--authority")
            .arg(&self.authority)
            .arg("--expected-authority-commitment")
            .arg(&self.expected_authority_commitment)
            .arg("--certificate")
            .arg(&self.certificate);
        add_checkpoint(&mut command, &self.checkpoint);
        command
    }
}

fn authorization_inputs(
    root: &Path,
    source: &SourceFixture,
    snapshot: &Path,
    ledger: &Path,
) -> (AuthorizationFixture, AuthorizationCli) {
    let fixture: AuthorizationFixture = serde_json::from_str(include_str!(
        "fixtures/native_nonce_upgrade_authorization_v1.json"
    ))
    .expect("decode frozen test-only upgrade certificate");
    let bundle = root.join("checkpoint.bin");
    let exported = run_json(
        export_command(snapshot, ledger, &source.checkpoint, &bundle),
        true,
    );
    let digest = exported["data"]["bundle_digest"]
        .as_str()
        .unwrap()
        .to_string();
    let authority = root.join("authority.json");
    let certificate = root.join("certificate.json");
    fs::write(&authority, serde_json::to_vec(&fixture.authority).unwrap()).unwrap();
    fs::write(
        &certificate,
        serde_json::to_vec(&fixture.certificate).unwrap(),
    )
    .unwrap();
    let inputs = AuthorizationCli {
        bundle,
        checkpoint: source.checkpoint.clone(),
        digest,
        target_protocol: fixture.target_protocol_commitment.clone(),
        authority,
        expected_authority_commitment: fixture.expected_authority_commitment.clone(),
        certificate,
    };
    (fixture, inputs)
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
        "verify-upgrade-authorization",
        "verify-source-qc",
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

#[test]
fn native_nonce_upgrade_authorization_cli_verifies_quorum_without_source_mutation_or_activation() {
    let root = fixture_root();
    let (source, snapshot, ledger) = restore_source(&root);
    let source_records = ledger_records(&ledger);
    let source_snapshot = fs::read(&snapshot).unwrap();
    let (_, inputs) = authorization_inputs(&root, &source, &snapshot, &ledger);
    let bundle_before = fs::read(&inputs.bundle).unwrap();
    let authority_before = fs::read(&inputs.authority).unwrap();
    let certificate_before = fs::read(&inputs.certificate).unwrap();
    let entry_count = fs::read_dir(&root).unwrap().count();
    let verified = run_json(inputs.command(), true);
    assert_eq!(verified["data"]["action"], "verify-upgrade-authorization");
    assert_eq!(verified["data"]["report"]["quorum_verified"], true);
    for field in [
        "authority_state_published",
        "activation_ready",
        "import_performed",
    ] {
        assert_eq!(
            verified["data"][field], false,
            "command must not promote {field}"
        );
        assert_eq!(
            verified["data"]["report"][field], false,
            "certificate must not promote {field}"
        );
    }
    assert_eq!(fs::read_dir(&root).unwrap().count(), entry_count);
    assert_eq!(fs::read(&inputs.bundle).unwrap(), bundle_before);
    assert_eq!(fs::read(&inputs.authority).unwrap(), authority_before);
    assert_eq!(fs::read(&inputs.certificate).unwrap(), certificate_before);
    assert_eq!(fs::read(&snapshot).unwrap(), source_snapshot);
    assert_eq!(ledger_records(&ledger), source_records);

    // The verifier consumes only portable documents, never the source ledger.
    let moved = root.join("source-not-at-original-path");
    fs::rename(root.join("source"), &moved).unwrap();
    let repeated = run_json(inputs.command(), true);
    assert_eq!(verified["data"], repeated["data"]);
    assert_eq!(
        ledger_records(&moved.join("ledger.rocksdb")),
        source_records
    );
}

#[test]
fn native_nonce_upgrade_authorization_cli_rejects_forged_quorum_roots_domains_and_pins() {
    let root = fixture_root();
    let (source, snapshot, ledger) = restore_source(&root);
    let before_records = ledger_records(&ledger);
    let before_snapshot = fs::read(&snapshot).unwrap();
    let (fixture, mut inputs) = authorization_inputs(&root, &source, &snapshot, &ledger);
    let before_bundle = fs::read(&inputs.bundle).unwrap();

    let mut two_votes = fixture.certificate.clone();
    two_votes["votes"]
        .as_array_mut()
        .expect("test certificate votes")
        .truncate(2);
    // Keep or forge claimed quorum counts: verification must count signatures.
    two_votes["signature_count"] = 3.into();
    two_votes["signed_weight"] = 3.into();
    let mut wrong_root = fixture.certificate.clone();
    let root_bytes = wrong_root["subject"]["proposed_state_root"]
        .as_array_mut()
        .expect("test certificate binds proposed state root");
    root_bytes[0] = (root_bytes[0].as_u64().unwrap() ^ 1).into();
    let mut block_qc_domain = fixture.certificate.clone();
    block_qc_domain["schema"] = "novovm-native-block-seal-qc/v1".into();
    let mut unknown_field = fixture.certificate.clone();
    unknown_field["activate"] = true.into();
    for document in [two_votes, wrong_root, block_qc_domain, unknown_field] {
        let bytes = serde_json::to_vec(&document).unwrap();
        fs::write(&inputs.certificate, &bytes).unwrap();
        run_json(inputs.command(), false);
        assert_eq!(fs::read(&inputs.certificate).unwrap(), bytes);
    }
    fs::write(
        &inputs.certificate,
        serde_json::to_vec(&fixture.certificate).unwrap(),
    )
    .unwrap();

    let mut unknown_authority = fixture.authority.clone();
    unknown_authority["activate"] = true.into();
    let mut nested_unknown_authority = fixture.authority.clone();
    nested_unknown_authority["validator_set"]["quorum_override"] = 2.into();
    let mut forged_authority = fixture.authority.clone();
    forged_authority["validator_set"]["quorum_weight"] = 2.into();
    for document in [
        unknown_authority,
        nested_unknown_authority,
        forged_authority,
    ] {
        let bytes = serde_json::to_vec(&document).unwrap();
        fs::write(&inputs.authority, &bytes).unwrap();
        run_json(inputs.command(), false);
        assert_eq!(fs::read(&inputs.authority).unwrap(), bytes);
    }
    let authority_json = serde_json::to_string(&fixture.authority).unwrap();
    let duplicate = format!(
        "{{\"chain_id\":{},{}",
        source.checkpoint.chain_id,
        &authority_json[1..]
    );
    fs::write(&inputs.authority, duplicate.as_bytes()).unwrap();
    run_json(inputs.command(), false);
    assert_eq!(fs::read(&inputs.authority).unwrap(), duplicate.as_bytes());
    fs::write(&inputs.authority, authority_json.as_bytes()).unwrap();

    inputs.expected_authority_commitment = "00".repeat(32);
    run_json(inputs.command(), false);
    inputs.expected_authority_commitment = fixture.expected_authority_commitment;
    inputs.checkpoint.tip_block_hash = "00".repeat(32);
    run_json(inputs.command(), false);
    inputs.checkpoint = source.checkpoint;
    inputs.digest = "00".repeat(32);
    run_json(inputs.command(), false);
    inputs.digest = checkpoint_bundle_digest_v1(&before_bundle);
    inputs.target_protocol = "00".repeat(32);
    inputs.authority = root.join("missing-authority.json");
    inputs.certificate = root.join("missing-certificate.json");
    inputs.bundle = root.join("missing-bundle.bin");
    let rejected = run_json(inputs.command(), false);
    assert!(rejected["error"]["message"]
        .as_str()
        .unwrap()
        .contains("does not match this binary's current environment"));
    assert!(!inputs.authority.exists() && !inputs.certificate.exists() && !inputs.bundle.exists());
    run_json(cli("verify-upgrade-authorization"), false);
    assert_eq!(
        fs::read(root.join("checkpoint.bin")).unwrap(),
        before_bundle
    );
    assert_eq!(fs::read(snapshot).unwrap(), before_snapshot);
    assert_eq!(ledger_records(&ledger), before_records);
}

#[test]
fn native_nonce_source_qc_cli_verifies_prepare_chain_read_only_without_sources_or_finality() {
    let root = fixture_root();
    let (source, snapshot, ledger) = restore_source(&root);
    let source_records = ledger_records(&ledger);
    let source_snapshot = fs::read(&snapshot).unwrap();
    let (_, inputs) = source_qc_inputs(&root, &source, &snapshot, &ledger);
    let bundle_before = fs::read(&inputs.bundle).unwrap();
    let authority_before = fs::read(&inputs.authority).unwrap();
    let source_qc_before = fs::read(&inputs.source_qc).unwrap();
    let entry_count = fs::read_dir(&root).unwrap().count();
    let verified = run_json(inputs.command(), true);
    assert_eq!(verified["data"]["action"], "verify-source-qc");
    assert_eq!(
        verified["data"]["report"]["prepare_qc_chain_verified"],
        true
    );
    for field in [
        "source_finality_verified",
        "aoem_execution_verified",
        "activation_ready",
        "import_performed",
        "chain_canonical",
        "proof_sealed",
        "safe",
        "finalized",
    ] {
        assert_eq!(
            verified["data"]["report"][field], false,
            "source prepare QC must not promote {field}"
        );
    }
    for field in ["activation_ready", "import_performed"] {
        assert_eq!(verified["data"][field], false);
    }
    assert_eq!(fs::read_dir(&root).unwrap().count(), entry_count);
    assert_eq!(fs::read(&inputs.bundle).unwrap(), bundle_before);
    assert_eq!(fs::read(&inputs.authority).unwrap(), authority_before);
    assert_eq!(fs::read(&inputs.source_qc).unwrap(), source_qc_before);
    assert_eq!(fs::read(&snapshot).unwrap(), source_snapshot);
    assert_eq!(ledger_records(&ledger), source_records);

    // Old-source QC verification is portable and independent of current V2
    // runtime configuration. It must not open the old source stores again.
    let moved = root.join("source-moved-away");
    fs::rename(root.join("source"), &moved).unwrap();
    assert!(!snapshot.exists() && !ledger.exists());
    let repeated = run_json(inputs.command(), true);
    assert_eq!(verified["data"], repeated["data"]);
    let mut incompatible_runtime = inputs.command();
    incompatible_runtime
        .env("NOVOVM_NATIVE_CHAIN_ID", "not-a-chain-id")
        .env("NOVOVM_NATIVE_FEE_RATE_PPM", "source-qc-test-only-override");
    let independent = run_json(incompatible_runtime, true);
    assert_eq!(verified["data"], independent["data"]);
    assert_eq!(
        ledger_records(&moved.join("ledger.rocksdb")),
        source_records
    );
    assert_eq!(
        fs::read(moved.join("legacy-store.json")).unwrap(),
        source_snapshot
    );
    assert_eq!(fs::read(&inputs.bundle).unwrap(), bundle_before);
    assert_eq!(fs::read(&inputs.authority).unwrap(), authority_before);
    assert_eq!(fs::read(&inputs.source_qc).unwrap(), source_qc_before);
    assert_eq!(fs::read_dir(&root).unwrap().count(), entry_count);
}

#[test]
fn native_nonce_source_qc_cli_rejects_missing_heights_forged_votes_tampering_and_pins() {
    let root = fixture_root();
    let (source, snapshot, ledger) = restore_source(&root);
    let before_records = ledger_records(&ledger);
    let before_snapshot = fs::read(&snapshot).unwrap();
    let (fixture, mut inputs) = source_qc_inputs(&root, &source, &snapshot, &ledger);
    let before_bundle = fs::read(&inputs.bundle).unwrap();
    let entry_count = fs::read_dir(&root).unwrap().count();

    let mut missing_height = fixture.source_qc.clone();
    assert_eq!(missing_height["entries"].as_array().unwrap().len(), 2);
    missing_height["entries"].as_array_mut().unwrap().remove(0);
    let mut two_votes = fixture.source_qc.clone();
    two_votes["entries"][1]["qc"]["votes"]
        .as_array_mut()
        .unwrap()
        .truncate(2);
    // Claimed totals and threshold flags cannot stand in for valid signatures.
    two_votes["entries"][1]["qc"]["signature_count"] = 3.into();
    two_votes["entries"][1]["qc"]["signed_weight"] = 3.into();
    two_votes["entries"][1]["qc"]["threshold_satisfied"] = true.into();
    let mut duplicate_vote = fixture.source_qc.clone();
    duplicate_vote["entries"][0]["qc"]["votes"][1] =
        duplicate_vote["entries"][0]["qc"]["votes"][0].clone();
    let mut altered_signature = fixture.source_qc.clone();
    let byte = &mut altered_signature["entries"][1]["qc"]["votes"][0]["signature"][0];
    *byte = (byte.as_u64().unwrap() ^ 1).into();
    let mut wrong_root = fixture.source_qc.clone();
    let byte = &mut wrong_root["entries"][1]["proposal"]["subject"]["post_state_root"][0];
    *byte = (byte.as_u64().unwrap() ^ 1).into();
    let mut wrong_domain = fixture.source_qc.clone();
    wrong_domain["schema"] = "novovm-native-nonce-upgrade-authorization/v1".into();
    let mut unknown = fixture.source_qc.clone();
    unknown["finalized"] = true.into();
    let mut nested_unknown = fixture.source_qc.clone();
    nested_unknown["entries"][0]["qc"]["subject"]["execution_verified"] = true.into();
    for document in [
        missing_height,
        two_votes,
        duplicate_vote,
        altered_signature,
        wrong_root,
        wrong_domain,
        unknown,
        nested_unknown,
    ] {
        let bytes = serde_json::to_vec(&document).unwrap();
        fs::write(&inputs.source_qc, &bytes).unwrap();
        run_json(inputs.command(), false);
        assert_eq!(fs::read(&inputs.source_qc).unwrap(), bytes);
    }
    let source_json = serde_json::to_string(&fixture.source_qc).unwrap();
    let duplicated = format!(
        "{{\"schema\":{},{}",
        fixture.source_qc["schema"],
        &source_json[1..]
    );
    fs::write(&inputs.source_qc, duplicated.as_bytes()).unwrap();
    run_json(inputs.command(), false);
    assert_eq!(fs::read(&inputs.source_qc).unwrap(), duplicated.as_bytes());
    fs::write(&inputs.source_qc, source_json.as_bytes()).unwrap();

    let mut nested_authority = fixture.authority.clone();
    nested_authority["validator_set"]["quorum_override"] = 2.into();
    let authority_bytes = serde_json::to_vec(&nested_authority).unwrap();
    fs::write(&inputs.authority, &authority_bytes).unwrap();
    run_json(inputs.command(), false);
    assert_eq!(fs::read(&inputs.authority).unwrap(), authority_bytes);
    let authority_json = serde_json::to_string(&fixture.authority).unwrap();
    let duplicated = format!(
        "{{\"chain_id\":{},{}",
        source.checkpoint.chain_id,
        &authority_json[1..]
    );
    fs::write(&inputs.authority, duplicated.as_bytes()).unwrap();
    run_json(inputs.command(), false);
    assert_eq!(fs::read(&inputs.authority).unwrap(), duplicated.as_bytes());
    fs::write(&inputs.authority, authority_json.as_bytes()).unwrap();

    inputs.expected_authority_commitment = "00".repeat(32);
    run_json(inputs.command(), false);
    inputs.expected_authority_commitment = fixture.expected_authority_commitment;
    inputs.checkpoint.tip_block_hash = "00".repeat(32);
    run_json(inputs.command(), false);
    inputs.checkpoint = source.checkpoint;
    inputs.digest = "00".repeat(32);
    run_json(inputs.command(), false);
    inputs.digest = checkpoint_bundle_digest_v1(&before_bundle);
    inputs.source_qc = root.join("missing-source-qc.json");
    run_json(inputs.command(), false);
    assert!(!inputs.source_qc.exists());
    run_json(cli("verify-source-qc"), false);
    assert_eq!(fs::read_dir(&root).unwrap().count(), entry_count);
    assert_eq!(
        fs::read(root.join("checkpoint.bin")).unwrap(),
        before_bundle
    );
    assert_eq!(
        fs::read(root.join("source-qc.json")).unwrap(),
        source_json.as_bytes()
    );
    assert_eq!(
        fs::read(root.join("authority.json")).unwrap(),
        authority_json.as_bytes()
    );
    assert_eq!(fs::read(snapshot).unwrap(), before_snapshot);
    assert_eq!(ledger_records(&ledger), before_records);
}
