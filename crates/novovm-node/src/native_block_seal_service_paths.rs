//! Read-only isolation of seal state and operator input files from node outputs.
//!
//! This checks the current filesystem, including existing symlink/junction
//! ancestors. It is not a defense against an operator changing links after
//! startup; configuration and the containing directories must remain trusted.

use super::service_config::NovNativeSealServiceConfigV1;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Validate before ANY startup recovery, database creation, or report write.
/// `other_write_paths` excludes the ledger, and includes native/AOEM stores,
/// delivery journals, and progress/summary outputs. Those existing outputs are
/// not required to be mutually disjoint here; their own owners enforce that.
/// `other_read_paths` extends the protected config/authority/signer files.
pub fn validate_service_paths_v1(
    config: &NovNativeSealServiceConfigV1,
    ledger_path: &Path,
    other_write_paths: &[PathBuf],
    other_read_paths: &[PathBuf],
) -> Result<()> {
    validate_path_sets(
        &config.seal_store_path,
        ledger_path,
        &config.protected_paths,
        other_write_paths,
        other_read_paths,
    )
}

fn validate_path_sets(
    seal_store_path: &Path,
    ledger_path: &Path,
    protected_paths: &[PathBuf],
    other_write_paths: &[PathBuf],
    other_read_paths: &[PathBuf],
) -> Result<()> {
    let seal = comparable_path(seal_store_path)?;
    let ledger = comparable_path(ledger_path)?;
    ensure_disjoint(&seal, &ledger, "seal store", "native ledger")?;
    let reads = protected_paths
        .iter()
        .chain(other_read_paths)
        .map(|path| comparable_path(path))
        .collect::<Result<Vec<_>>>()?;
    let writes = other_write_paths
        .iter()
        .map(|path| comparable_path(path))
        .collect::<Result<Vec<_>>>()?;
    for read in &reads {
        ensure_disjoint(&seal, read, "seal store", "protected input")?;
        ensure_disjoint(&ledger, read, "native ledger", "protected input")?;
        for write in &writes {
            ensure_disjoint(write, read, "node output", "protected input")?;
        }
    }
    for write in &writes {
        ensure_disjoint(write, &seal, "node output", "seal store")?;
        ensure_disjoint(write, &ledger, "node output", "native ledger")?;
    }
    Ok(())
}

fn ensure_disjoint(left: &str, right: &str, left_label: &str, right_label: &str) -> Result<()> {
    if left == right
        || left
            .strip_prefix(right)
            .is_some_and(|tail| right.ends_with('/') || tail.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|tail| left.ends_with('/') || tail.starts_with('/'))
    {
        bail!("native seal service paths overlap: {left_label} and {right_label}");
    }
    Ok(())
}

fn comparable_path(path: &Path) -> Result<String> {
    if path.as_os_str().is_empty() {
        bail!("native seal service path must not be empty");
    }
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            bail!("native seal service paths must not contain '..' components");
        }
        if cfg!(windows) {
            if let Component::Normal(name) = component {
                let name = name.to_str().context("native seal path must be Unicode")?;
                if name.ends_with(['.', ' ']) || name.contains(':') {
                    bail!("native seal path contains an ambiguous Windows component");
                }
            }
        }
    }
    let mut existing = std::path::absolute(path).context("resolve native seal service path")?;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    existing
                        .file_name()
                        .context("native seal path has no existing ancestor")?
                        .to_os_string(),
                );
                if !existing.pop() {
                    bail!("native seal path has no resolvable ancestor");
                }
            }
            Err(error) => return Err(error).context("inspect native seal path ancestor"),
        }
    }
    let mut resolved = fs::canonicalize(&existing)
        .context("canonicalize native seal path ancestor (broken links are forbidden)")?;
    if !missing.is_empty() && !resolved.is_dir() {
        bail!("native seal path has a non-directory ancestor");
    }
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    let mut comparable = resolved
        .to_str()
        .context("native seal path must be Unicode")?
        .replace('\\', "/");
    while comparable.len() > 1 && comparable.ends_with('/') {
        comparable.pop();
    }
    if cfg!(windows) {
        comparable = comparable.to_lowercase();
    }
    Ok(comparable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap();
            let parent = root.join("artifacts/audit/seal-service-path-tests");
            fs::create_dir_all(&parent).unwrap();
            let parent = fs::canonicalize(parent).unwrap();
            assert!(parent.starts_with(fs::canonicalize(root).unwrap()));
            let path = parent.join(format!(
                "{}-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        fn validate(&self, writes: &[PathBuf], reads: &[PathBuf]) -> Result<()> {
            validate_path_sets(
                &self.path("seal"),
                &self.path("ledger"),
                &[self.path("config.json"), self.path("signer.key")],
                writes,
                reads,
            )
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            // Only this unique, test-owned directory is removed; never a parent.
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn native_seal_service_paths_read_only_and_other_writes_may_overlap() {
        let dir = TestDir::new();
        let writes = [dir.path("outputs"), dir.path("outputs/report.json")];
        dir.validate(&writes, &[dir.path("authority.json")])
            .unwrap();
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
        dir.validate(&[dir.path("seal-old"), dir.path("ledger-old")], &[])
            .unwrap();
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
    }

    #[test]
    fn native_seal_service_paths_protect_inputs_and_state_both_directions() {
        let dir = TestDir::new();
        fs::write(dir.path("signer.key"), b"test-secret-marker").unwrap();
        for output in [
            dir.path("signer.key"),
            dir.path("config.json"),
            dir.path("seal"),
            dir.path("seal/report.json"),
            dir.path("ledger"),
            dir.path("ledger/report.json"),
            dir.0.clone(),
        ] {
            assert!(dir.validate(&[output], &[]).is_err());
        }
        assert!(dir
            .validate(&[dir.path("authority.json")], &[dir.path("authority.json")])
            .is_err());
        assert!(dir.validate(&[], &[dir.path("seal/key")]).is_err());
        assert!(dir.validate(&[], &[dir.path("ledger/key")]).is_err());
        for ledger in [dir.path("seal"), dir.path("seal/child"), dir.0.clone()] {
            assert!(validate_path_sets(&dir.path("seal"), &ledger, &[], &[], &[]).is_err());
        }
        assert_eq!(
            fs::read(dir.path("signer.key")).unwrap(),
            b"test-secret-marker"
        );
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn native_seal_service_paths_reject_native_lock_and_backup_as_protected_inputs() {
        let dir = TestDir::new();
        let params = serde_json::json!({ "native_execution_store_path": dir.path("native.json") });
        let inventory = crate::tx_ingress::native_persistence_write_paths_v1(&params);
        let writes = inventory
            .iter()
            .filter(|(label, _)| *label != "native block ledger")
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        let sidecars = inventory
            .iter()
            .filter(|(label, _)| matches!(*label, "Host coordination lock" | "Host JSON backup"))
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        assert_eq!(sidecars.len(), 2);
        for sidecar in sidecars {
            assert!(validate_path_sets(
                &dir.path("seal"),
                &dir.path("ledger"),
                &[sidecar],
                &writes,
                &[],
            )
            .is_err());
        }
        // The inventory and guard never create either a lock or a backup.
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
    }

    #[test]
    fn native_seal_service_paths_reject_cache_staging_and_ca_overwrites() {
        let dir = TestDir::new();
        let cache = dir.path("bootstrap-cache.json");
        let staging = cache.with_extension("json.tmp");
        let certificate = dir.path("relay-ca.pem");
        fs::write(&staging, b"operator-key-fixture").unwrap();
        fs::write(&certificate, b"operator-ca-fixture").unwrap();
        assert!(validate_path_sets(
            &dir.path("seal"),
            &dir.path("ledger"),
            std::slice::from_ref(&staging),
            &[cache, staging.clone()],
            std::slice::from_ref(&certificate),
        )
        .is_err());
        assert!(dir
            .validate(
                std::slice::from_ref(&certificate),
                std::slice::from_ref(&certificate),
            )
            .is_err());
        assert!(validate_path_sets(
            &dir.path("seal"),
            &dir.path("ledger"),
            &[],
            &[],
            &[dir.path("seal/relay-ca.pem")],
        )
        .is_err());
        assert_eq!(fs::read(staging).unwrap(), b"operator-key-fixture");
        assert_eq!(fs::read(certificate).unwrap(), b"operator-ca-fixture");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
    }

    #[test]
    fn native_seal_service_paths_reject_parent_empty_and_file_ancestors() {
        let dir = TestDir::new();
        assert!(comparable_path(Path::new("")).is_err());
        // PathBuf::join on a Windows verbatim base already normalizes '..'.
        // Supply raw components to exercise the guard itself, before joining.
        assert!(comparable_path(Path::new("missing/../seal")).is_err());
        assert!(comparable_path(Path::new("../seal")).is_err());
        fs::write(dir.path("file"), b"fixture").unwrap();
        assert!(comparable_path(&dir.path("file/child")).is_err());
        assert_eq!(
            comparable_path(&dir.path("./seal")).unwrap(),
            comparable_path(&dir.path("seal")).unwrap()
        );
    }

    #[cfg(windows)]
    #[test]
    fn native_seal_service_paths_windows_case_and_ambiguous_names() {
        let dir = TestDir::new();
        assert!(dir.validate(&[dir.path("SIGNER.KEY")], &[]).is_err());
        assert!(dir.validate(&[dir.path("SEAL/sub")], &[]).is_err());
        for name in ["seal.", "seal ", "signer.key:stream"] {
            assert!(comparable_path(&dir.path(name)).is_err());
        }
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn native_seal_service_paths_resolve_directory_aliases_before_missing_tail() {
        let dir = TestDir::new();
        let actual = dir.path("actual");
        let alias = dir.path("alias");
        fs::create_dir(&actual).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let output = std::process::Command::new("cmd")
                .args(["/D", "/C", "mklink", "/J"])
                .arg(&alias)
                .arg(&actual)
                .creation_flags(0x08000000)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "create test-owned directory junction failed"
            );
        }
        assert_eq!(
            comparable_path(&actual.join("new/seal")).unwrap(),
            comparable_path(&alias.join("new/seal")).unwrap()
        );
        assert!(validate_path_sets(
            &actual.join("seal"),
            &dir.path("ledger"),
            &[],
            &[alias.join("seal/report.json")],
            &[],
        )
        .is_err());
        assert!(!actual.join("new").exists());
    }
}
