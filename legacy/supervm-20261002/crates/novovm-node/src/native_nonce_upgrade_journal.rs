#![forbid(unsafe_code)]

//! Recoverable OFFLINE artifact staging, never a chain-state transaction.
//! Only operator-owned, explicit new directories are created. Resume appends
//! verified deterministic prefixes under a cooperative OS lock; no live store,
//! ledger, AOEM runtime, overwrite, or directory cleanup is involved.

use super::native_nonce_checkpoint::NonceMigrationCheckpointV1;
use super::native_nonce_upgrade::{
    encode_nonce_upgrade_v1, plan_nonce_upgrade_v1, NonceUpgradeTransitionV1,
};
use super::*;
use std::io::{Read, Write};

const LOCK_NAME: &str = "workspace.lock";
const LOCK_BYTES: &[u8] = b"novovm-native-nonce-upgrade-workspace/v1\n";
const ARTIFACT_NAME: &str = "transition.json";
const COMPLETE_NAME: &str = "complete.json";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NonceUpgradeStageReportV1 {
    pub schema: &'static str,
    pub transition_id: String,
    pub source_bundle_digest: String,
    pub proposed_state_root: String,
    pub phase: &'static str,
    pub artifact_complete: bool,
    pub authority_state_published: bool,
    pub activation_ready: bool,
    pub import_performed: bool,
    pub aoem_evidence_verified: bool,
    pub qc_verified: bool,
    pub chain_canonical: bool,
    pub target_runtime_compatibility_verified: bool,
}

struct Expected {
    transition: NonceUpgradeTransitionV1,
    intent_name: String,
    intent: Vec<u8>,
    artifact: Vec<u8>,
    complete: Vec<u8>,
}

impl Expected {
    fn new(
        bundle: &[u8],
        bundle_digest: &str,
        checkpoint: &NonceMigrationCheckpointV1,
        target_protocol: &str,
    ) -> Result<Self> {
        let transition = plan_nonce_upgrade_v1(bundle, bundle_digest, checkpoint, target_protocol)?;
        let artifact = encode_nonce_upgrade_v1(&transition)?;
        let intent = serde_json::to_vec(&serde_json::json!({
            "schema": "novovm-native-nonce-upgrade-intent/v1",
            "transition_id": transition.transition_id,
            "source_bundle_digest": bundle_digest,
            "checkpoint": checkpoint,
            "target_protocol_config_commitment": target_protocol,
        }))?;
        let complete = serde_json::to_vec(&serde_json::json!({
            "schema": "novovm-native-nonce-upgrade-artifact-complete/v1",
            "transition_id": transition.transition_id,
            "artifact_bytes": artifact.len(),
            "artifact_digest": to_hex(&sha256_bytes_v1(&[
                b"novovm-native-nonce-upgrade-artifact-v1\0", &artifact,
            ])),
            "activation_ready": false,
            "authority_state_published": false,
        }))?;
        Ok(Self {
            intent_name: format!("intent-{}.json", transition.transition_id),
            transition,
            intent,
            artifact,
            complete,
        })
    }

    fn report(&self, complete: bool) -> NonceUpgradeStageReportV1 {
        NonceUpgradeStageReportV1 {
            schema: "novovm-native-nonce-upgrade-stage-report/v1",
            transition_id: self.transition.transition_id.clone(),
            source_bundle_digest: self.transition.source_bundle_digest.clone(),
            proposed_state_root: self.transition.proposed_state_root.clone(),
            phase: if complete { "complete" } else { "prepared" },
            artifact_complete: complete,
            authority_state_published: false,
            activation_ready: false,
            import_performed: false,
            aoem_evidence_verified: false,
            qc_verified: false,
            chain_canonical: false,
            target_runtime_compatibility_verified: false,
        }
    }
}

struct WorkspaceLock(fs::File);
impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn lock_workspace(workspace: &Path, fresh: bool) -> Result<WorkspaceLock> {
    let path = workspace.join(LOCK_NAME);
    if !fresh {
        let meta = fs::symlink_metadata(&path).context("missing upgrade workspace lock")?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            bail!("upgrade workspace lock must be a regular file");
        }
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create_new(fresh);
    let mut file = options.open(&path).context("open upgrade workspace lock")?;
    file.try_lock()
        .context("upgrade workspace busy; another process owns its lock")?;
    if fresh {
        file.write_all(LOCK_BYTES)?;
        file.sync_all()?;
    } else {
        let mut bytes = Vec::new();
        (&mut file)
            .take(LOCK_BYTES.len() as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes != LOCK_BYTES {
            bail!("upgrade workspace lock marker is incomplete or invalid");
        }
    }
    Ok(WorkspaceLock(file))
}

fn validate_existing_directory(workspace: &Path, expected: &Expected) -> Result<()> {
    let meta = fs::symlink_metadata(workspace).context("upgrade workspace must already exist")?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        bail!("upgrade workspace must be a real directory, not an alias");
    }
    let mut intent_seen = false;
    for entry in fs::read_dir(workspace)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            bail!("unknown upgrade workspace entry");
        };
        if ![
            LOCK_NAME,
            expected.intent_name.as_str(),
            ARTIFACT_NAME,
            COMPLETE_NAME,
        ]
        .contains(&name)
        {
            bail!("upgrade workspace contains foreign entries or a different transition identity");
        }
        let file_type = entry.file_type()?;
        if !file_type.is_file() || file_type.is_symlink() {
            bail!("upgrade workspace entries must be regular non-symlink files");
        }
        intent_seen |= name == expected.intent_name;
    }
    if !intent_seen {
        bail!("upgrade workspace has no bound intent; use a new workspace, do not adopt this directory");
    }
    Ok(())
}

// Returns None for an absent next-stage file, otherwise the verified prefix size.
fn prefix_size(path: &Path, expected: &[u8]) -> Result<Option<usize>> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > expected.len() as u64 {
        bail!("upgrade artifact is not a bounded regular file");
    }
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    if !expected.starts_with(&bytes) {
        bail!("upgrade artifact differs from its recomputed deterministic prefix");
    }
    Ok(Some(bytes.len()))
}

fn validate_progress(workspace: &Path, expected: &Expected) -> Result<bool> {
    let intent = prefix_size(&workspace.join(&expected.intent_name), &expected.intent)?
        .context("upgrade intent disappeared")?;
    let artifact = prefix_size(&workspace.join(ARTIFACT_NAME), &expected.artifact)?;
    let complete = prefix_size(&workspace.join(COMPLETE_NAME), &expected.complete)?;
    if (intent != expected.intent.len() && (artifact.is_some() || complete.is_some()))
        || (artifact != Some(expected.artifact.len()) && complete.is_some())
    {
        bail!("upgrade journal completion order is invalid; refusing repair of published claims");
    }
    Ok(complete == Some(expected.complete.len()))
}

fn append_expected(
    path: &Path,
    expected: &[u8],
    label: &str,
    hook: &mut impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let prefix = prefix_size(path, expected)?;
    if prefix == Some(expected.len()) {
        return Ok(());
    }
    let mut file = match prefix {
        Some(_) => fs::OpenOptions::new().append(true).open(path)?,
        None => fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?,
    };
    let start = prefix.unwrap_or(0);
    if file.metadata()?.len() != start as u64 {
        bail!("upgrade file changed while locked");
    }
    let split = start + (expected.len() - start) / 2;
    file.write_all(&expected[start..split])?;
    file.sync_all()?;
    hook(&format!("{label}.partial"))?;
    file.write_all(&expected[split..])?;
    file.sync_all()?;
    hook(&format!("{label}.written"))?;
    Ok(())
}

fn stage_with_hook(
    workspace: &Path,
    expected: &Expected,
    resume: bool,
    hook: &mut impl FnMut(&str) -> Result<()>,
) -> Result<NonceUpgradeStageReportV1> {
    // Input recomputation always precedes creation. Existing directories are
    // never adopted by prepare, and resume requires the intent-bound namespace.
    if !resume {
        if workspace.file_name().is_none() {
            bail!("upgrade workspace must name a new directory");
        }
        fs::create_dir(workspace).context("create exclusively new upgrade workspace")?;
    } else {
        validate_existing_directory(workspace, expected)?;
    }
    let _lock = lock_workspace(workspace, !resume)?;
    if resume {
        validate_existing_directory(workspace, expected)?;
        if validate_progress(workspace, expected)? {
            return Ok(expected.report(true));
        }
    }
    append_expected(
        &workspace.join(&expected.intent_name),
        &expected.intent,
        "intent",
        hook,
    )?;
    append_expected(
        &workspace.join(ARTIFACT_NAME),
        &expected.artifact,
        "artifact",
        hook,
    )?;
    if prefix_size(&workspace.join(&expected.intent_name), &expected.intent)?
        != Some(expected.intent.len())
        || prefix_size(&workspace.join(ARTIFACT_NAME), &expected.artifact)?
            != Some(expected.artifact.len())
    {
        bail!("upgrade artifact readback is incomplete; refusing completion publication");
    }
    validate_progress(workspace, expected)?;
    append_expected(
        &workspace.join(COMPLETE_NAME),
        &expected.complete,
        "complete",
        hook,
    )?;
    if !validate_progress(workspace, expected)? {
        bail!("upgrade artifact completion did not close");
    }
    Ok(expected.report(true))
}

/// Materialize an explicitly non-authoritative upgrade envelope. Successful
/// staging never authorizes chain activation, even when all artifacts are complete.
pub fn stage_nonce_upgrade_v1(
    workspace: &Path,
    bundle: &[u8],
    bundle_digest: &str,
    checkpoint: &NonceMigrationCheckpointV1,
    target_protocol: &str,
    resume: bool,
) -> Result<NonceUpgradeStageReportV1> {
    if workspace.to_str().is_none() {
        bail!("upgrade workspace must be a Unicode path representable in JSON reports");
    }
    let expected = Expected::new(bundle, bundle_digest, checkpoint, target_protocol)?;
    stage_with_hook(workspace, &expected, resume, &mut |_| Ok(()))
}

/// Observe only after recomputing expected content from independently supplied
/// bundle and pins. Opening/locking the existing lock does not write its bytes.
pub fn inspect_nonce_upgrade_v1(
    workspace: &Path,
    bundle: &[u8],
    bundle_digest: &str,
    checkpoint: &NonceMigrationCheckpointV1,
    target_protocol: &str,
) -> Result<NonceUpgradeStageReportV1> {
    if workspace.to_str().is_none() {
        bail!("upgrade workspace must be a Unicode path representable in JSON reports");
    }
    let expected = Expected::new(bundle, bundle_digest, checkpoint, target_protocol)?;
    validate_existing_directory(workspace, &expected)?;
    let _lock = lock_workspace(workspace, false)?;
    validate_existing_directory(workspace, &expected)?;
    Ok(expected.report(validate_progress(workspace, &expected)?))
}

#[cfg(test)]
mod tests {
    use super::super::native_nonce_bundle::{checkpoint_bundle_digest_v1, encode_bundle};
    use super::super::native_nonce_checkpoint::test_fixture_v1;
    use super::*;

    fn fixture() -> (
        PathBuf,
        Vec<u8>,
        String,
        NonceMigrationCheckpointV1,
        Expected,
    ) {
        let (snapshot, head, blocks, checkpoint, ledger) = test_fixture_v1();
        let root = ledger.with_extension("upgrade-tests");
        fs::create_dir(&root).unwrap();
        let bundle = encode_bundle(&snapshot, &head, &blocks).unwrap();
        let digest = checkpoint_bundle_digest_v1(&bundle);
        let expected = Expected::new(&bundle, &digest, &checkpoint, &"ef".repeat(32)).unwrap();
        (root, bundle, digest, checkpoint, expected)
    }

    fn files(path: &Path) -> BTreeMap<String, Vec<u8>> {
        fs::read_dir(path)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn native_nonce_upgrade_staging_is_idempotent_and_non_authoritative() {
        let (root, bundle, digest, checkpoint, expected) = fixture();
        let path = root.join("workspace");
        let report = stage_nonce_upgrade_v1(
            &path,
            &bundle,
            &digest,
            &checkpoint,
            &"ef".repeat(32),
            false,
        )
        .unwrap();
        assert!(report.artifact_complete);
        assert!(
            !report.activation_ready
                && !report.authority_state_published
                && !report.import_performed
        );
        assert!(!report.qc_verified && !report.aoem_evidence_verified && !report.chain_canonical);
        assert!(!report.target_runtime_compatibility_verified);
        let before = files(&path);
        assert_eq!(
            report,
            stage_nonce_upgrade_v1(&path, &bundle, &digest, &checkpoint, &"ef".repeat(32), true)
                .unwrap()
        );
        assert_eq!(
            report,
            inspect_nonce_upgrade_v1(&path, &bundle, &digest, &checkpoint, &"ef".repeat(32))
                .unwrap()
        );
        assert!(stage_with_hook(&path, &expected, false, &mut |_| Ok(())).is_err());
        assert_eq!(files(&path), before);
    }

    #[test]
    fn native_nonce_upgrade_valid_prefixes_resume_without_replacing_bytes() {
        let (root, _, _, _, expected) = fixture();
        for label in [
            "intent.partial",
            "intent.written",
            "artifact.partial",
            "artifact.written",
            "complete.partial",
        ] {
            let path = root.join(label);
            let mut stop = |event: &str| {
                if event == label {
                    bail!("injected interruption");
                }
                Ok(())
            };
            assert!(stage_with_hook(&path, &expected, false, &mut stop).is_err());
            validate_existing_directory(&path, &expected).unwrap();
            assert!(!validate_progress(&path, &expected).unwrap());
            let prefixes = files(&path);
            assert!(
                stage_with_hook(&path, &expected, true, &mut |_| Ok(()))
                    .unwrap()
                    .artifact_complete
            );
            for (name, bytes) in prefixes {
                assert!(fs::read(path.join(name)).unwrap().starts_with(&bytes));
            }
        }
    }

    #[test]
    fn native_nonce_upgrade_wrong_input_corruption_and_early_completion_never_repair() {
        let (root, bundle, digest, checkpoint, expected) = fixture();
        let path = root.join("workspace");
        stage_with_hook(&path, &expected, false, &mut |_| Ok(())).unwrap();
        let original = files(&path);
        assert!(stage_nonce_upgrade_v1(
            &path,
            &bundle,
            &digest,
            &checkpoint,
            &"ab".repeat(32),
            true
        )
        .is_err());
        assert_eq!(files(&path), original);
        for name in [&expected.intent_name, ARTIFACT_NAME, COMPLETE_NAME] {
            let mut corrupt = original[name].clone();
            corrupt[0] ^= 1;
            fs::write(path.join(name), &corrupt).unwrap();
            let before = files(&path);
            assert!(stage_with_hook(&path, &expected, true, &mut |_| Ok(())).is_err());
            assert_eq!(files(&path), before);
            fs::write(path.join(name), &original[name]).unwrap();
        }
        fs::write(path.join(ARTIFACT_NAME), &expected.artifact[..5]).unwrap();
        let before = files(&path);
        assert!(stage_with_hook(&path, &expected, true, &mut |_| Ok(())).is_err());
        assert_eq!(files(&path), before);
        let missing = root.join("must-not-create");
        assert!(stage_nonce_upgrade_v1(
            &missing,
            &bundle,
            &"00".repeat(32),
            &checkpoint,
            &"ef".repeat(32),
            false
        )
        .is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn native_nonce_upgrade_artifact_readback_precedes_completion_publication() {
        let (root, _, _, _, expected) = fixture();
        for corrupt in [false, true] {
            let path = root.join(if corrupt { "corrupt" } else { "truncated" });
            let mut hook = |event: &str| {
                if event == "artifact.written" {
                    let bytes = if corrupt {
                        b"bad".as_slice()
                    } else {
                        &expected.artifact[..5]
                    };
                    fs::write(path.join(ARTIFACT_NAME), bytes)?;
                }
                Ok(())
            };
            assert!(stage_with_hook(&path, &expected, false, &mut hook).is_err());
            assert!(!path.join(COMPLETE_NAME).exists());
        }
    }

    #[test]
    fn native_nonce_upgrade_foreign_unbound_or_locked_workspace_is_rejected() {
        let (root, _, _, _, expected) = fixture();
        let path = root.join("workspace");
        fs::create_dir(&path).unwrap();
        assert!(stage_with_hook(&path, &expected, true, &mut |_| Ok(())).is_err());
        fs::write(path.join("CURRENT"), b"foreign database").unwrap();
        assert!(stage_with_hook(&path, &expected, true, &mut |_| Ok(())).is_err());
        assert_eq!(files(&path).len(), 1);
        let path = root.join("bound");
        stage_with_hook(&path, &expected, false, &mut |_| Ok(())).unwrap();
        let before = files(&path);
        let lock = lock_workspace(&path, false).unwrap();
        assert!(stage_with_hook(&path, &expected, true, &mut |_| Ok(())).is_err());
        drop(lock);
        assert_eq!(files(&path), before);
    }

    #[test]
    #[ignore = "invoked only by the upgrade interruption parent test"]
    fn native_nonce_upgrade_process_worker() {
        let root = PathBuf::from(std::env::var_os("NOV_UPGRADE_TEST_ROOT").unwrap());
        let bundle = fs::read(root.join("bundle.bin")).unwrap();
        let checkpoint: NonceMigrationCheckpointV1 =
            serde_json::from_slice(&fs::read(root.join("checkpoint.json")).unwrap()).unwrap();
        let digest = checkpoint_bundle_digest_v1(&bundle);
        let expected = Expected::new(&bundle, &digest, &checkpoint, &"ef".repeat(32)).unwrap();
        let stop = std::env::var("NOV_UPGRADE_TEST_STOP").unwrap();
        let mut hook = |event: &str| {
            if event == stop {
                std::process::exit(73);
            }
            Ok(())
        };
        stage_with_hook(
            &root.join("workspace"),
            &expected,
            stop == "resume",
            &mut hook,
        )
        .unwrap();
        fs::write(
            root.join("worker-completed"),
            expected.transition.transition_id,
        )
        .unwrap();
    }

    #[test]
    fn native_nonce_upgrade_process_exit_releases_lock_and_resumes_exact_artifact() {
        let (root, bundle, digest, checkpoint, expected) = fixture();
        for stop in ["intent.partial", "artifact.partial", "complete.partial"] {
            let case = root.join(stop);
            fs::create_dir(&case).unwrap();
            fs::write(case.join("bundle.bin"), &bundle).unwrap();
            fs::write(
                case.join("checkpoint.json"),
                serde_json::to_vec(&checkpoint).unwrap(),
            )
            .unwrap();
            let child = |phase: &str| {
                let mut process = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["tx_ingress::native_nonce_upgrade_journal::tests::native_nonce_upgrade_process_worker", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
                    .env("NOV_UPGRADE_TEST_ROOT", &case).env("NOV_UPGRADE_TEST_STOP", phase)
                    .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
                    .spawn().unwrap();
                let deadline = std::time::Instant::now() + Duration::from_secs(30);
                while process.try_wait().unwrap().is_none() {
                    if std::time::Instant::now() >= deadline {
                        let _ = process.kill();
                        let _ = process.wait();
                        panic!("upgrade worker exceeded 30 second budget");
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                process.wait_with_output().unwrap()
            };
            let interrupted = child(stop);
            assert_eq!(
                interrupted.status.code(),
                Some(73),
                "{}",
                String::from_utf8_lossy(&interrupted.stderr)
            );
            assert!(!case.join("worker-completed").exists());
            let report = inspect_nonce_upgrade_v1(
                &case.join("workspace"),
                &bundle,
                &digest,
                &checkpoint,
                &"ef".repeat(32),
            )
            .unwrap();
            assert_eq!(report.phase, "prepared");
            let resumed = child("resume");
            assert!(
                resumed.status.success(),
                "{}",
                String::from_utf8_lossy(&resumed.stderr)
            );
            assert_eq!(
                fs::read_to_string(case.join("worker-completed")).unwrap(),
                expected.transition.transition_id
            );
            assert_eq!(
                fs::read(case.join("workspace").join(ARTIFACT_NAME)).unwrap(),
                expected.artifact
            );
            assert!(
                inspect_nonce_upgrade_v1(
                    &case.join("workspace"),
                    &bundle,
                    &digest,
                    &checkpoint,
                    &"ef".repeat(32)
                )
                .unwrap()
                .artifact_complete
            );
        }
    }
}
