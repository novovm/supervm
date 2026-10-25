//! Actual offline CLI, isolated disposable custody only; no deployed node keys.
#![cfg(any(windows, target_os = "linux"))]
use serde_json::Value;
use std::{
    collections::BTreeMap,
    error::Error,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

type TestResult = Result<(), Box<dyn Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    credentials: PathBuf,
}
impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = std::env::temp_dir().join(format!(
            "novovm-purpose-cli-fixture-{}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        private_directory(&root)?;
        let credentials = root.join("credentials");
        private_directory(&credentials)?;
        #[cfg(target_os = "linux")]
        {
            use std::{io::Write, os::unix::fs::OpenOptionsExt};
            // Deliberately known test-only wrapping material, never deployment.
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(credentials.join("owner-key"))?;
            file.write_all(&[71; 32])?;
            file.sync_all()?;
        }
        Ok(Self { root, credentials })
    }
    fn command(&self, action: &str, purpose: &str, chain: u64, path: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_novovmctl"));
        command
            .arg("native-identity")
            .arg(action)
            .arg("--purpose")
            .arg(purpose)
            .arg("--chain-id")
            .arg(chain.to_string())
            .arg("--key-file")
            .arg(path)
            .env("CREDENTIALS_DIRECTORY", &self.credentials);
        command
    }
    fn run(
        &self,
        action: &str,
        purpose: &str,
        chain: u64,
        path: &Path,
    ) -> Result<Output, Box<dyn Error>> {
        Ok(self.command(action, purpose, chain, path).output()?)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // Only this test's uniquely created immediate temporary child.
        if self.root.parent() == Some(std::env::temp_dir().as_path())
            && self.root.file_name().is_some_and(|s| {
                s.to_string_lossy()
                    .starts_with("novovm-purpose-cli-fixture-")
            })
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn private_directory(path: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(false);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn snapshot(root: &Path) -> std::io::Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn walk(root: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                walk(&entry.path(), out)?;
            } else {
                out.insert(entry.path(), std::fs::read(entry.path())?);
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    walk(root, &mut result)?;
    Ok(result)
}

fn success(output: Output) -> Result<Value, Box<dyn Error>> {
    assert!(
        output.status.success(),
        "CLI rejected: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value["ok"], true);
    let data = &value["data"];
    for field in [
        "private_material_exported",
        "database_created",
        "validator_started",
        "service_started",
        "genesis_activated",
        "consensus_signed",
        "asset_authority_granted",
        "whole_node_post_quantum_qualified",
    ] {
        assert_eq!(data[field], false, "must not claim {field}");
    }
    assert_eq!(data["identity"]["algorithm"], "Ed25519");
    assert_eq!(
        data["identity"]["qualification"],
        "test_node_purpose_only_not_full_node_mldsa"
    );
    assert!(data["identity"].get("secret").is_none());
    assert!(data["identity"].get("seed").is_none());
    Ok(data.clone())
}

#[test]
fn protected_keys_are_independent_and_inspect_is_nonmutating() -> TestResult {
    let fixture = Fixture::new()?;
    let validator = fixture.root.join("validator.protected");
    let transport = fixture.root.join("transport.protected");
    let signer = success(fixture.run("generate", "validator", 19, &validator)?)?;
    let peer = success(fixture.run("generate", "transport", 19, &transport)?)?;
    assert_ne!(
        signer["identity"]["public_key"],
        peer["identity"]["public_key"]
    );
    assert_eq!(signer["protected_key_created"], true);
    let before = snapshot(&fixture.root)?;
    let inspected = success(fixture.run("inspect", "validator", 19, &validator)?)?;
    assert_eq!(inspected["identity"], signer["identity"]);
    assert_eq!(inspected["protected_key_created"], false);
    assert_eq!(snapshot(&fixture.root)?, before);
    assert!(!fixture.root.join("database").exists());
    assert!(!fixture.root.join("journal").exists());
    let expected = if cfg!(target_os = "linux") { 5 } else { 2 };
    assert_eq!(
        before.len(),
        expected,
        "only keys, wrapping credential and custody locks"
    );
    Ok(())
}

#[test]
fn overwrite_wrong_purpose_and_wrong_domain_fail_without_changes() -> TestResult {
    let fixture = Fixture::new()?;
    let path = fixture.root.join("validator.protected");
    success(fixture.run("generate", "validator", 23, &path)?)?;
    let before = snapshot(&fixture.root)?;
    for (action, purpose, chain) in [
        ("generate", "validator", 23),
        ("inspect", "transport", 23),
        ("inspect", "validator", 24),
        ("inspect", "validator", 0),
    ] {
        let output = fixture.run(action, purpose, chain, &path)?;
        assert!(!output.status.success());
        assert_eq!(snapshot(&fixture.root)?, before);
    }
    Ok(())
}

#[test]
fn missing_tampered_and_plain_raw_keys_never_fallback_or_create_files() -> TestResult {
    let fixture = Fixture::new()?;
    let path = fixture.root.join("validator.protected");
    let before = snapshot(&fixture.root)?;
    assert!(!fixture
        .run("inspect", "validator", 29, &path)?
        .status
        .success());
    assert_eq!(snapshot(&fixture.root)?, before);
    success(fixture.run("generate", "validator", 29, &path)?)?;
    let mut bytes = std::fs::read(&path)?;
    let at = bytes.len() / 2;
    bytes[at] ^= 0x80;
    std::fs::write(&path, bytes)?;
    let damaged = snapshot(&fixture.root)?;
    assert!(!fixture
        .run("inspect", "validator", 29, &path)?
        .status
        .success());
    assert_eq!(snapshot(&fixture.root)?, damaged);
    std::fs::write(&path, [37; 32])?;
    assert!(!fixture
        .run("inspect", "validator", 29, &path)?
        .status
        .success());
    Ok(())
}

#[test]
fn relative_and_parent_traversal_targets_are_not_created() -> TestResult {
    let fixture = Fixture::new()?;
    let before = snapshot(&fixture.root)?;
    let mut command = fixture.command("generate", "validator", 31, Path::new("relative.key"));
    command.current_dir(&fixture.root);
    assert!(!command.output()?.status.success());
    let traversal = fixture.credentials.join("..").join("bad.key");
    assert!(!fixture
        .run("generate", "validator", 31, &traversal)?
        .status
        .success());
    assert_eq!(snapshot(&fixture.root)?, before);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn linux_missing_credential_or_custody_lock_is_not_implicitly_repaired() -> TestResult {
    let fixture = Fixture::new()?;
    let path = fixture.root.join("validator.protected");
    let before = snapshot(&fixture.root)?;
    let mut command = fixture.command("generate", "validator", 37, &path);
    command.env_remove("CREDENTIALS_DIRECTORY");
    assert!(!command.output()?.status.success());
    assert_eq!(snapshot(&fixture.root)?, before);
    success(fixture.run("generate", "validator", 37, &path)?)?;
    let lock = fixture.root.join("validator.protected.lock");
    std::fs::remove_file(&lock)?;
    let without_lock = snapshot(&fixture.root)?;
    assert!(!fixture
        .run("inspect", "validator", 37, &path)?
        .status
        .success());
    assert_eq!(snapshot(&fixture.root)?, without_lock);
    assert!(!lock.exists());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn linux_open_permissions_and_symlink_targets_fail_closed() -> TestResult {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fixture = Fixture::new()?;
    let path = fixture.root.join("validator.protected");
    success(fixture.run("generate", "validator", 41, &path)?)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    assert!(!fixture
        .run("inspect", "validator", 41, &path)?
        .status
        .success());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let alias = fixture.root.join("alias.protected");
    symlink(&path, &alias)?;
    assert!(!fixture
        .run("inspect", "validator", 41, &alias)?
        .status
        .success());
    assert!(!fixture
        .run("generate", "validator", 41, &alias)?
        .status
        .success());
    Ok(())
}
