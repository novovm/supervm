use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use novovm_node::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use novovm_node::tx_ingress::native_business_protocol_config_commitment_v1;
use novovm_node::tx_ingress::native_nonce_bundle::{
    checkpoint_bundle_digest_v1, export_nonce_checkpoint_bundle_v1,
    inspect_nonce_checkpoint_source_v1, read_nonce_checkpoint_bundle_v1,
    verify_nonce_checkpoint_bundle_v1, MAX_CHECKPOINT_BUNDLE_BYTES_V1,
};
use novovm_node::tx_ingress::native_nonce_checkpoint::NonceMigrationCheckpointV1;
use novovm_node::tx_ingress::native_nonce_source_qc::{
    verify_nonce_source_qc_json_v1, NonceSourceQcInputsV1, MAX_NONCE_SOURCE_QC_BYTES_V1,
};
use novovm_node::tx_ingress::native_nonce_upgrade_authorization::{
    verify_nonce_upgrade_authorization_json_v1, NonceUpgradeAuthorizationInputsV1,
};
use novovm_node::tx_ingress::native_nonce_upgrade_journal::{
    inspect_nonce_upgrade_v1, stage_nonce_upgrade_v1,
};
use serde_json::{json, Value};

use crate::cli::native_nonce_migration::{
    NativeNonceCheckpointArgs, NativeNonceMigrationArgs, NativeNonceMigrationCommand,
    NativeNonceSourceQcArgs, NativeNonceUpgradeArgs, NativeNonceUpgradeAuthorizationArgs,
    NativeNonceVerifyArgs,
};
use crate::error::CtlError;
use crate::output;

const COMMAND_NAME: &str = "native-nonce-migration";
const MAX_AUTHORITY_JSON_BYTES_V1: usize = 128 * 1024;
const MAX_CERTIFICATE_JSON_BYTES_V1: usize = 256 * 1024;

pub fn run(args: NativeNonceMigrationArgs) -> Result<(), CtlError> {
    let result =
        inner_run(&args).and_then(|report| output::print_success_json(COMMAND_NAME, &report));
    if let Err(error) = &result {
        output::print_error_json(COMMAND_NAME, error);
    }
    result
}

fn checkpoint(args: &NativeNonceCheckpointArgs) -> NonceMigrationCheckpointV1 {
    NonceMigrationCheckpointV1 {
        chain_id: args.chain_id,
        namespace_digest: args.namespace_digest.clone(),
        legacy_protocol_config_commitment: args.legacy_protocol_commitment.clone(),
        tip_block_hash: args.tip_block_hash.clone(),
        snapshot_digest: args.snapshot_digest.clone(),
    }
}

fn inner_run(args: &NativeNonceMigrationArgs) -> Result<Value, CtlError> {
    match &args.command {
        NativeNonceMigrationCommand::Inspect(args) => {
            let observed =
                inspect_nonce_checkpoint_source_v1(&args.snapshot, &args.ledger, args.chain_id)
                    .map_err(|error| {
                        CtlError::IntegrationFailed(format!("offline source inspection: {error:#}"))
                    })?;
            Ok(json!({"action": "inspect", "inspection": observed,
                "observed_only": true, "activation_ready": false, "import_performed": false}))
        }
        NativeNonceMigrationCommand::Export(args) => {
            let output_path = resolve_new_bundle_output_v1(&args.bundle_out, &args.ledger)?;
            let pinned = checkpoint(&args.checkpoint);
            let bytes = export_nonce_checkpoint_bundle_v1(&args.snapshot, &args.ledger, &pinned)
                .map_err(|error| {
                    CtlError::IntegrationFailed(format!(
                        "offline bundle export validation: {error:#}"
                    ))
                })?;
            let report = verify_nonce_checkpoint_bundle_v1(&bytes, &pinned).map_err(|error| {
                CtlError::IntegrationFailed(format!(
                    "offline exported bundle validation: {error:#}"
                ))
            })?;
            let digest = checkpoint_bundle_digest_v1(&bytes);
            // Validation completes before the first output mutation. No live
            // stores, environment-derived paths, overwrite, or import is used.
            write_new_bundle_v1(&output_path, &bytes)?;
            Ok(json!({"action": "export", "bundle_out": output_path,
                "bundle_bytes": bytes.len(), "bundle_digest": digest,
                "report": report, "activation_ready": false, "import_performed": false}))
        }
        NativeNonceMigrationCommand::Verify(args) => {
            let bytes = read_pinned_bundle_v1(args)?;
            let actual_digest = checkpoint_bundle_digest_v1(&bytes);
            let report = verify_nonce_checkpoint_bundle_v1(&bytes, &checkpoint(&args.checkpoint))
                .map_err(|error| {
                CtlError::IntegrationFailed(format!("offline checkpoint verification: {error:#}"))
            })?;
            Ok(json!({"action": "verify", "bundle_digest": actual_digest,
                "bundle_digest_verified": true, "report": report,
                "activation_ready": false, "import_performed": false}))
        }
        NativeNonceMigrationCommand::TargetProtocol => {
            let commitment = current_target_protocol_v1()?;
            Ok(
                json!({"action": "target-protocol", "target_protocol_commitment": commitment,
                "observed_only": true, "authority_state_published": false,
                "activation_ready": false, "import_performed": false}),
            )
        }
        NativeNonceMigrationCommand::PrepareUpgrade(args) => {
            run_upgrade_v1(args, "prepare-upgrade", Some(false))
        }
        NativeNonceMigrationCommand::ResumeUpgrade(args) => {
            run_upgrade_v1(args, "resume-upgrade", Some(true))
        }
        NativeNonceMigrationCommand::InspectUpgrade(args) => {
            run_upgrade_v1(args, "inspect-upgrade", None)
        }
        NativeNonceMigrationCommand::VerifyUpgradeAuthorization(args) => {
            run_upgrade_authorization_v1(args)
        }
        NativeNonceMigrationCommand::VerifySourceQc(args) => run_source_qc_v1(args),
    }
}

fn run_source_qc_v1(args: &NativeNonceSourceQcArgs) -> Result<Value, CtlError> {
    // Source evidence is interpreted under independently pinned old rules. The
    // current V2 environment is neither required nor authority to approve it.
    let bytes = read_pinned_bundle_v1(&args.evidence)?;
    let authority_bytes = read_bounded_document_v1(
        &args.authority,
        MAX_AUTHORITY_JSON_BYTES_V1,
        "source epoch authority",
    )?;
    let authority = decode_authority_v1(&authority_bytes)?;
    let source_qc = read_bounded_document_v1(
        &args.source_qc,
        MAX_NONCE_SOURCE_QC_BYTES_V1,
        "source prepare-QC evidence",
    )?;
    let pinned = checkpoint(&args.evidence.checkpoint);
    let inputs = NonceSourceQcInputsV1 {
        bundle: &bytes,
        bundle_digest: &args.evidence.bundle_digest,
        checkpoint: &pinned,
        authority: &authority,
        expected_authority_commitment: &args.expected_authority_commitment,
    };
    let report = verify_nonce_source_qc_json_v1(&source_qc, &inputs).map_err(|error| {
        CtlError::IntegrationFailed(format!("offline source prepare-QC verification: {error:#}"))
    })?;
    Ok(json!({"action": "verify-source-qc", "report": report,
        "activation_ready": false, "import_performed": false}))
}

fn current_target_protocol_v1() -> Result<String, CtlError> {
    native_business_protocol_config_commitment_v1().map_err(|error| {
        CtlError::IntegrationFailed(format!("current V2 protocol commitment: {error:#}"))
    })
}

fn run_upgrade_authorization_v1(
    args: &NativeNonceUpgradeAuthorizationArgs,
) -> Result<Value, CtlError> {
    // Environment compatibility is checked before any input file is accessed.
    // A match is not permission to sign, import, activate, or publish anything.
    if args.target_protocol_commitment != current_target_protocol_v1()? {
        return Err(CtlError::InvalidArgument(
            "--target-protocol-commitment does not match this binary's current environment; use target-protocol to observe it, then independently approve the intended configuration".into(),
        ));
    }
    let bytes = read_pinned_bundle_v1(&args.evidence)?;
    let authority_bytes = read_bounded_document_v1(
        &args.authority,
        MAX_AUTHORITY_JSON_BYTES_V1,
        "epoch authority",
    )?;
    let authority = decode_authority_v1(&authority_bytes)?;
    let certificate = read_bounded_document_v1(
        &args.certificate,
        MAX_CERTIFICATE_JSON_BYTES_V1,
        "upgrade certificate",
    )?;
    let pinned = checkpoint(&args.evidence.checkpoint);
    let inputs = NonceUpgradeAuthorizationInputsV1 {
        bundle: &bytes,
        bundle_digest: &args.evidence.bundle_digest,
        checkpoint: &pinned,
        target_protocol: &args.target_protocol_commitment,
        authority: &authority,
        expected_authority_commitment: &args.expected_authority_commitment,
    };
    let report =
        verify_nonce_upgrade_authorization_json_v1(&certificate, &inputs).map_err(|error| {
            CtlError::IntegrationFailed(format!("offline nonce upgrade authorization: {error:#}"))
        })?;
    Ok(
        json!({"action": "verify-upgrade-authorization", "report": report,
        "authority_state_published": false, "activation_ready": false,
        "import_performed": false}),
    )
}

fn decode_authority_v1(bytes: &[u8]) -> Result<NovNativeSealEpochAuthorityV1, CtlError> {
    let invalid = |error| CtlError::InvalidArgument(format!("epoch authority JSON: {error}"));
    // Parse the typed document directly so duplicate known fields are rejected.
    // The shared authority type predates deny_unknown_fields; round-trip equality
    // additionally rejects every silently dropped field, including nested ones.
    let authority: NovNativeSealEpochAuthorityV1 =
        serde_json::from_slice(bytes).map_err(invalid)?;
    let source: Value = serde_json::from_slice(bytes).map_err(invalid)?;
    let encoded = serde_json::to_value(&authority).map_err(invalid)?;
    if source != encoded {
        return Err(CtlError::InvalidArgument(
            "epoch authority JSON contains unknown fields or noncanonical field values".into(),
        ));
    }
    authority.validate().map_err(|error| {
        CtlError::InvalidArgument(format!("epoch authority validation: {error:#}"))
    })?;
    Ok(authority)
}

fn read_bounded_document_v1(path: &Path, maximum: usize, label: &str) -> Result<Vec<u8>, CtlError> {
    let failed = |error| CtlError::FileReadFailed(format!("{label} {}: {error}", path.display()));
    let metadata = fs::metadata(path).map_err(failed)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum as u64 {
        return Err(CtlError::FileReadFailed(format!(
            "{label} must be a nonempty regular file of at most {maximum} bytes"
        )));
    }
    let file = File::open(path).map_err(failed)?;
    let opened = file.metadata().map_err(failed)?;
    if !opened.is_file() || opened.len() == 0 || opened.len() > maximum as u64 {
        return Err(CtlError::FileReadFailed(format!(
            "{label} changed or is outside its regular-file size bound"
        )));
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(failed)?;
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(CtlError::FileReadFailed(format!(
            "{label} changed or is outside its nonempty size bound"
        )));
    }
    Ok(bytes)
}

fn read_pinned_bundle_v1(args: &NativeNonceVerifyArgs) -> Result<Vec<u8>, CtlError> {
    require_digest_v1(&args.bundle_digest)?;
    let bytes = read_nonce_checkpoint_bundle_v1(&args.bundle)
        .map_err(|error| CtlError::FileReadFailed(format!("offline evidence bundle: {error:#}")))?;
    if checkpoint_bundle_digest_v1(&bytes) != args.bundle_digest {
        return Err(CtlError::IntegrationFailed(
            "evidence bundle digest does not match the independently supplied digest".into(),
        ));
    }
    Ok(bytes)
}

fn run_upgrade_v1(
    args: &NativeNonceUpgradeArgs,
    action: &'static str,
    resume: Option<bool>,
) -> Result<Value, CtlError> {
    // A supplied target is an explicit compatibility pin, never authority to
    // publish this proposal. Recheck before accessing or creating a workspace.
    if args.target_protocol_commitment != current_target_protocol_v1()? {
        return Err(CtlError::InvalidArgument(
            "--target-protocol-commitment does not match this binary's current environment; use target-protocol to observe it, then independently approve the intended configuration".into(),
        ));
    }
    let bytes = read_pinned_bundle_v1(&args.evidence)?;
    let pinned = checkpoint(&args.evidence.checkpoint);
    let report = match resume {
        Some(resume) => stage_nonce_upgrade_v1(
            &args.workspace,
            &bytes,
            &args.evidence.bundle_digest,
            &pinned,
            &args.target_protocol_commitment,
            resume,
        ),
        None => inspect_nonce_upgrade_v1(
            &args.workspace,
            &bytes,
            &args.evidence.bundle_digest,
            &pinned,
            &args.target_protocol_commitment,
        ),
    }
    .map_err(|error| {
        CtlError::IntegrationFailed(format!("offline nonce upgrade proposal: {error:#}"))
    })?;
    Ok(json!({"action": action, "workspace": args.workspace,
        "target_protocol_commitment": args.target_protocol_commitment,
        "report": report, "authority_state_published": false,
        "activation_ready": false, "import_performed": false}))
}

fn require_digest_v1(value: &str) -> Result<(), CtlError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CtlError::InvalidArgument(
            "--bundle-digest must be canonical lowercase 32-byte hex without a prefix".into(),
        ));
    }
    Ok(())
}

fn path_is_within_v1(path: &Path, parent: &Path) -> bool {
    #[cfg(windows)]
    {
        // Windows canonical paths may retain user-provided component casing.
        PathBuf::from(path.to_string_lossy().to_lowercase())
            .starts_with(PathBuf::from(parent.to_string_lossy().to_lowercase()))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(parent)
    }
}

fn resolve_new_bundle_output_v1(
    output_path: &Path,
    ledger_path: &Path,
) -> Result<PathBuf, CtlError> {
    if output_path.to_str().is_none() {
        return Err(CtlError::InvalidArgument(
            "--bundle-out must be a Unicode path representable in the JSON report".into(),
        ));
    }
    let file_name = output_path.file_name().ok_or_else(|| {
        CtlError::InvalidArgument("--bundle-out must name a new regular file".into())
    })?;
    // A Windows alternate data stream is not an independent evidence file.
    if file_name.to_string_lossy().contains(':') {
        return Err(CtlError::InvalidArgument(
            "--bundle-out must not name an alternate data stream".into(),
        ));
    }
    let parent = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_parent = fs::canonicalize(parent).map_err(|error| {
        CtlError::FileWriteFailed(format!(
            "resolve existing bundle output parent {}: {error}",
            parent.display()
        ))
    })?;
    let canonical_ledger = fs::canonicalize(ledger_path).map_err(|error| {
        CtlError::FileReadFailed(format!(
            "resolve existing offline ledger {}: {error}",
            ledger_path.display()
        ))
    })?;
    let resolved = canonical_parent.join(file_name);
    if resolved.to_str().is_none() {
        return Err(CtlError::InvalidArgument(
            "resolved --bundle-out must be representable in the JSON report".into(),
        ));
    }
    if path_is_within_v1(&resolved, &canonical_ledger) {
        return Err(CtlError::InvalidArgument(
            "--bundle-out must be outside the ledger directory".into(),
        ));
    }
    match fs::symlink_metadata(&resolved) {
        Ok(_) => {
            return Err(CtlError::FileWriteFailed(format!(
                "bundle output already exists; refusing overwrite or input alias: {}",
                resolved.display()
            )))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CtlError::FileWriteFailed(format!(
                "inspect bundle output {}: {error}",
                resolved.display()
            )))
        }
    }
    Ok(resolved)
}

fn write_new_bundle_v1(path: &Path, bytes: &[u8]) -> Result<(), CtlError> {
    if bytes.is_empty() || bytes.len() > MAX_CHECKPOINT_BUNDLE_BYTES_V1 {
        return Err(CtlError::InvalidArgument(
            "bundle output exceeds its nonempty size bound".into(),
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            CtlError::FileWriteFailed(format!(
                "create new evidence bundle {}: {error}",
                path.display()
            ))
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            CtlError::FileWriteFailed(format!(
                "write evidence bundle {}; output may be incomplete and must not be used: {error}",
                path.display()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::native_nonce_migration::NativeNonceInspectArgs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "novovmctl-nonce-migration-{}-{}-{}",
            std::process::id(),
            output::now_unix_ms(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    fn pinned_args() -> NativeNonceCheckpointArgs {
        NativeNonceCheckpointArgs {
            chain_id: 7,
            namespace_digest: "ab".repeat(32),
            legacy_protocol_commitment: "cd".repeat(32),
            tip_block_hash: "ef".repeat(32),
            snapshot_digest: "12".repeat(32),
        }
    }

    #[test]
    fn native_nonce_migration_output_never_overwrites_or_enters_ledger() {
        let root = fixture();
        let ledger = root.join("ledger");
        fs::create_dir(&ledger).unwrap();
        let existing = root.join("snapshot.json");
        fs::write(&existing, b"preserved").unwrap();
        assert!(resolve_new_bundle_output_v1(&existing, &ledger).is_err());
        assert!(resolve_new_bundle_output_v1(&ledger.join("bundle.json"), &ledger).is_err());
        assert!(
            resolve_new_bundle_output_v1(&root.join("missing").join("bundle.json"), &ledger)
                .is_err()
        );
        assert!(resolve_new_bundle_output_v1(&root.join("snapshot.json:stream"), &ledger).is_err());
        assert!(write_new_bundle_v1(&existing, b"replacement").is_err());
        assert_eq!(fs::read(existing).unwrap(), b"preserved");
        let target = resolve_new_bundle_output_v1(&root.join("bundle.json"), &ledger).unwrap();
        write_new_bundle_v1(&target, b"verified fixture").unwrap();
        assert_eq!(fs::read(target).unwrap(), b"verified fixture");
        assert_eq!(fs::read_dir(ledger).unwrap().count(), 0);
    }

    #[test]
    fn native_nonce_migration_missing_inputs_fail_nonzero_without_creation() {
        let root = fixture();
        let snapshot = root.join("missing.json");
        let ledger = root.join("missing.rocksdb");
        let args = NativeNonceMigrationArgs {
            command: NativeNonceMigrationCommand::Inspect(NativeNonceInspectArgs {
                snapshot: snapshot.clone(),
                ledger: ledger.clone(),
                chain_id: 7,
            }),
        };
        let error = inner_run(&args).unwrap_err();
        assert_ne!(error.exit_code(), 0);
        assert!(!snapshot.exists() && !ledger.exists());
        let args = NativeNonceMigrationArgs {
            command: NativeNonceMigrationCommand::Verify(NativeNonceVerifyArgs {
                bundle: root.join("missing.bundle"),
                checkpoint: pinned_args(),
                bundle_digest: "34".repeat(32),
            }),
        };
        assert_ne!(inner_run(&args).unwrap_err().exit_code(), 0);
        assert_eq!(fs::read_dir(root).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn native_nonce_migration_output_rejects_symlinked_ledger_parent() {
        let root = fixture();
        let ledger = root.join("ledger");
        fs::create_dir(&ledger).unwrap();
        let alias = root.join("ledger-alias");
        std::os::unix::fs::symlink(&ledger, &alias).unwrap();
        assert!(resolve_new_bundle_output_v1(&alias.join("bundle.json"), &ledger).is_err());
        assert_eq!(fs::read_dir(ledger).unwrap().count(), 0);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn native_nonce_migration_output_rejects_non_unicode_before_creation() {
        let root = fixture();
        let ledger = root.join("ledger");
        fs::create_dir(&ledger).unwrap();
        #[cfg(unix)]
        let name = {
            use std::os::unix::ffi::OsStringExt;
            std::ffi::OsString::from_vec(vec![b'x', 0xff])
        };
        #[cfg(windows)]
        let name = {
            use std::os::windows::ffi::OsStringExt;
            std::ffi::OsString::from_wide(&[u16::from(b'x'), 0xd800])
        };
        let target = root.join(name);
        assert!(resolve_new_bundle_output_v1(&target, &ledger)
            .unwrap_err()
            .to_string()
            .contains("Unicode"));
        assert!(!target.exists());
        assert_eq!(fs::read_dir(root).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn native_nonce_migration_output_checks_windows_case_aliases() {
        assert!(path_is_within_v1(
            Path::new(r"C:\fixture\LEDGER\bundle.json"),
            Path::new(r"c:\FIXTURE\ledger")
        ));
        assert!(!path_is_within_v1(
            Path::new(r"C:\fixture\ledger-other\bundle.json"),
            Path::new(r"c:\fixture\ledger")
        ));
    }

    #[test]
    fn native_nonce_migration_bundle_digest_rejects_before_decoding() {
        let root = fixture();
        let bundle = root.join("invalid.bundle");
        fs::write(&bundle, b"not json").unwrap();
        let args = NativeNonceMigrationArgs {
            command: NativeNonceMigrationCommand::Verify(NativeNonceVerifyArgs {
                bundle: bundle.clone(),
                checkpoint: pinned_args(),
                bundle_digest: "34".repeat(32),
            }),
        };
        assert!(inner_run(&args)
            .unwrap_err()
            .to_string()
            .contains("independently supplied digest"));
        assert_eq!(fs::read(bundle).unwrap(), b"not json");
        for value in [
            "",
            "abcd",
            &"AB".repeat(32),
            &format!("0x{}", "ab".repeat(32)),
        ] {
            assert!(require_digest_v1(value).is_err());
        }
        assert!(require_digest_v1(&"ab".repeat(32)).is_ok());
    }

    #[test]
    fn native_nonce_upgrade_target_mismatch_rejects_before_workspace_access() {
        let root = fixture();
        let workspace = root.join("must-not-create");
        let current = current_target_protocol_v1().unwrap();
        let wrong_target = if current == "00".repeat(32) {
            "01".repeat(32)
        } else {
            "00".repeat(32)
        };
        let args = NativeNonceMigrationArgs {
            command: NativeNonceMigrationCommand::PrepareUpgrade(NativeNonceUpgradeArgs {
                evidence: NativeNonceVerifyArgs {
                    bundle: root.join("missing.bundle"),
                    checkpoint: pinned_args(),
                    bundle_digest: "34".repeat(32),
                },
                target_protocol_commitment: wrong_target,
                workspace: workspace.clone(),
            }),
        };
        assert!(inner_run(&args)
            .unwrap_err()
            .to_string()
            .contains("does not match this binary's current environment"));
        assert!(!workspace.exists());
        assert_eq!(fs::read_dir(root).unwrap().count(), 0);
    }

    #[test]
    fn native_nonce_upgrade_authorization_target_mismatch_rejects_before_file_access() {
        let root = fixture();
        let current = current_target_protocol_v1().unwrap();
        let wrong_target = if current == "00".repeat(32) {
            "01".repeat(32)
        } else {
            "00".repeat(32)
        };
        let args = NativeNonceUpgradeAuthorizationArgs {
            evidence: NativeNonceVerifyArgs {
                bundle: root.join("missing.bundle"),
                checkpoint: pinned_args(),
                bundle_digest: "34".repeat(32),
            },
            target_protocol_commitment: wrong_target,
            authority: root.join("missing-authority.json"),
            expected_authority_commitment: "56".repeat(32),
            certificate: root.join("missing-certificate.json"),
        };
        assert!(run_upgrade_authorization_v1(&args)
            .unwrap_err()
            .to_string()
            .contains("does not match this binary's current environment"));
        assert_eq!(fs::read_dir(root).unwrap().count(), 0);
    }

    #[test]
    fn native_nonce_upgrade_authorization_documents_are_regular_nonempty_and_bounded() {
        let root = fixture();
        for (label, maximum) in [
            ("authority", MAX_AUTHORITY_JSON_BYTES_V1),
            ("certificate", MAX_CERTIFICATE_JSON_BYTES_V1),
        ] {
            assert!(read_bounded_document_v1(&root, maximum, label).is_err());
            let path = root.join(format!("{label}.json"));
            assert!(read_bounded_document_v1(&path, maximum, label).is_err());
            let file = File::create(&path).unwrap();
            assert!(read_bounded_document_v1(&path, maximum, label).is_err());
            file.set_len(maximum as u64 + 1).unwrap();
            assert!(read_bounded_document_v1(&path, maximum, label).is_err());
            drop(file);
            fs::write(&path, b"{}").unwrap();
            assert_eq!(
                read_bounded_document_v1(&path, maximum, label).unwrap(),
                b"{}"
            );
            assert_eq!(fs::read(path).unwrap(), b"{}");
        }
    }

    #[test]
    fn native_nonce_source_qc_document_is_regular_nonempty_and_bounded() {
        let root = fixture();
        let maximum = MAX_NONCE_SOURCE_QC_BYTES_V1;
        assert_eq!(maximum, 64 * 1024 * 1024);
        assert!(read_bounded_document_v1(&root, maximum, "source QC").is_err());
        let path = root.join("source-qc.json");
        assert!(read_bounded_document_v1(&path, maximum, "source QC").is_err());
        let file = File::create(&path).unwrap();
        assert!(read_bounded_document_v1(&path, maximum, "source QC").is_err());
        file.set_len(maximum as u64 + 1).unwrap();
        assert!(read_bounded_document_v1(&path, maximum, "source QC").is_err());
        drop(file);
        fs::write(&path, b"{}").unwrap();
        assert_eq!(
            read_bounded_document_v1(&path, maximum, "source QC").unwrap(),
            b"{}"
        );
        assert_eq!(fs::read(path).unwrap(), b"{}");
    }
}
