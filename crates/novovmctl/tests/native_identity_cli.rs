//! Actual offline CLI, isolated disposable custody only; no deployed node keys.
#![cfg(any(windows, target_os = "linux"))]
use serde_json::Value;
use std::{
    collections::BTreeMap,
    error::Error,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type TestResult = Result<(), Box<dyn Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn selected_ctl_binary(override_path: Option<std::ffi::OsString>) -> PathBuf {
    match override_path {
        Some(path) => {
            let path = PathBuf::from(path);
            assert!(
                path.is_absolute() && path.is_file(),
                "NOVOVM_CTL_TEST_BINARY must name an existing absolute candidate executable"
            );
            path
        }
        None => PathBuf::from(env!("CARGO_BIN_EXE_novovmctl")),
    }
}

struct Fixture {
    root: PathBuf,
    credentials: PathBuf,
}

// Real executable ownership: regular output files cannot fill a pipe while the
// parent waits, and every failure path reaps only its own temporary child.
struct NatProcess {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
    report: PathBuf,
    label: String,
    started: Instant,
}
impl NatProcess {
    fn spawn(
        binary: &Path,
        fixture: &Fixture,
        config: &Path,
        report: &Path,
        label: &str,
        coordinated_start: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let stdout = fixture.root.join(format!("{label}.stdout"));
        let stderr = fixture.root.join(format!("{label}.stderr"));
        let mut command = Command::new(binary);
        command.arg(config);
        if coordinated_start {
            command.arg("--wait-for-start").stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        command
            .env("CREDENTIALS_DIRECTORY", &fixture.credentials)
            .stdout(std::fs::File::create(&stdout)?)
            .stderr(std::fs::File::create(&stderr)?);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        Ok(Self {
            child: command.spawn()?,
            stdout,
            stderr,
            report: report.to_owned(),
            label: label.to_owned(),
            started: Instant::now(),
        })
    }
    fn wait_prepared(&mut self) -> TestResult {
        let deadline = self.started + Duration::from_secs(25);
        loop {
            if self.child.try_wait()?.is_some() {
                self.finish()?;
                return Err(format!("NAT {} exited before start", self.label).into());
            }
            if std::fs::read_to_string(&self.stdout)?.lines().next() == Some("NAT_PREPARED_V1") {
                println!(
                    "NAT {} prepared elapsed_ms={}",
                    self.label,
                    self.started.elapsed().as_millis()
                );
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(
                    format!("NAT {} preparation exceeded process watchdog", self.label).into(),
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn start(&mut self) -> TestResult {
        use std::io::Write;
        let mut input = self
            .child
            .stdin
            .take()
            .ok_or("NAT start is not available")?;
        input.write_all(&[1])?;
        input.flush()?;
        Ok(())
    }
    fn finish(&mut self) -> TestResult {
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            if let Some(status) = self.child.try_wait()? {
                println!(
                    "NAT {} process elapsed_ms={} status={status}",
                    self.label,
                    self.started.elapsed().as_millis()
                );
                if !status.success() {
                    // The fixture removes its protected keys on drop. Surface
                    // only the original public report before that cleanup,
                    // never the credential/key directory or configuration.
                    let report = match std::fs::read_to_string(&self.report) {
                        Ok(value) => value,
                        Err(error) => format!("public report unavailable: {error}"),
                    };
                    return Err(format!(
                        "NAT {} failed ({status}): {}; public report: {report}",
                        self.label,
                        std::fs::read_to_string(&self.stderr)?
                    )
                    .into());
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("NAT executable exceeded its bounded process watchdog".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for NatProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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
        let mut command = Command::new(selected_ctl_binary(std::env::var_os(
            "NOVOVM_CTL_TEST_BINARY",
        )));
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
    fn runtime_command(&self) -> Command {
        let mut command = Command::new(selected_ctl_binary(std::env::var_os(
            "NOVOVM_CTL_TEST_BINARY",
        )));
        command
            .args(["native-identity", "qualify-runtime"])
            .current_dir(&self.root)
            .env_remove("CREDENTIALS_DIRECTORY");
        command
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
    assert_eq!(data["identity"]["schema_version"], 3);
    assert_eq!(data["identity"]["algorithm"], "ML-DSA-65");
    assert_eq!(
        data["identity"]["public_key"].as_array().unwrap().len(),
        1952
    );
    assert_eq!(
        data["identity"]["qualification"],
        "purpose_key_only_not_membership_launch_or_whole_node_pq"
    );
    assert!(data["identity"].get("secret").is_none());
    assert!(data["identity"].get("seed").is_none());
    Ok(data.clone())
}

#[test]
fn relative_and_parent_traversal_targets_are_not_created() -> TestResult {
    let fixture = Fixture::new()?;
    assert_eq!(
        selected_ctl_binary(None),
        PathBuf::from(env!("CARGO_BIN_EXE_novovmctl"))
    );
    let explicit_selection = fixture.root.join("path-selection-only.bin");
    std::fs::write(
        &explicit_selection,
        b"fixture path selector only; never executed",
    )?;
    assert_eq!(
        selected_ctl_binary(Some(explicit_selection.clone().into_os_string())),
        explicit_selection
    );
    let before = snapshot(&fixture.root)?;
    let mut command = fixture.command("generate", "transport", 31, Path::new("relative.key"));
    command.current_dir(&fixture.root);
    assert!(!command.output()?.status.success());
    let traversal = fixture.credentials.join("..").join("bad.key");
    assert!(!fixture
        .run("generate", "transport", 31, &traversal)?
        .status
        .success());
    assert_eq!(snapshot(&fixture.root)?, before);
    Ok(())
}

#[test]
fn both_purposes_require_explicit_qualified_aoem_and_never_fall_back_to_ed25519() -> TestResult {
    let fixture = Fixture::new()?;
    let path = fixture.root.join("validator.protected");
    let before = snapshot(&fixture.root)?;
    for purpose in ["validator", "transport", "identity", "asset-management"] {
        for action in ["generate", "inspect"] {
            let output = fixture.run(action, purpose, 43, &path)?;
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("--library"));
            assert_eq!(snapshot(&fixture.root)?, before);
        }
    }
    // A regular file with the wrong approved digest is rejected before loading
    // or creating custody. This is not an executable or a fake runtime fallback.
    let library = fixture.root.join("not-a-runtime.bin");
    std::fs::write(&library, b"not an AOEM artifact")?;
    let before = snapshot(&fixture.root)?;
    for purpose in ["validator", "transport", "identity", "asset-management"] {
        let output = fixture
            .command("generate", purpose, 43, &path)
            .arg("--library")
            .arg(&library)
            .arg("--library-sha256")
            .arg("00".repeat(32))
            .output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("SHA-256 mismatch before load"));
        assert_eq!(snapshot(&fixture.root)?, before);
    }
    Ok(())
}

#[test]
fn runtime_qualification_requires_explicit_artifact_and_hash_without_state() -> TestResult {
    let fixture = Fixture::new()?;
    let library = fixture.root.join("not-a-runtime.bin");
    std::fs::write(&library, b"not an AOEM artifact")?;
    let before = snapshot(&fixture.root)?;
    assert!(!fixture.runtime_command().output()?.status.success());
    assert!(!fixture
        .runtime_command()
        .arg("--library")
        .arg(&library)
        .output()?
        .status
        .success());
    assert!(!fixture
        .runtime_command()
        .args(["--library-sha256", &"00".repeat(32)])
        .output()?
        .status
        .success());
    for digest in ["invalid".to_string(), "AA".repeat(32), "00".repeat(32)] {
        let output = fixture
            .runtime_command()
            .arg("--library")
            .arg(&library)
            .arg("--library-sha256")
            .arg(&digest)
            .output()?;
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains(if digest == "00".repeat(32) {
                "SHA-256 mismatch before load"
            } else {
                "lowercase AOEM SHA-256 required"
            }),
            "{error}"
        );
    }
    let output = fixture
        .runtime_command()
        .arg("--library")
        .arg(fixture.root.join("missing-runtime"))
        .arg("--library-sha256")
        .arg("00".repeat(32))
        .output()?;
    assert!(!output.status.success());
    for flag in ["--purpose", "--chain-id", "--key-file"] {
        assert!(!fixture
            .runtime_command()
            .arg("--library")
            .arg(&library)
            .arg("--library-sha256")
            .arg("00".repeat(32))
            .args([flag, "unused"])
            .output()?
            .status
            .success());
    }
    assert_eq!(snapshot(&fixture.root)?, before);
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; actual no-key native verification-owner KAT"]
fn actual_runtime_qualification_uses_original_owner_without_keys_or_state() -> TestResult {
    use sha2::{Digest, Sha256};
    let library = PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .ok_or("set NOVOVM_AOEM_TEST_LIBRARY to the approved platform AOEM artifact")?,
    )
    .canonicalize()?;
    let digest = format!("{:x}", Sha256::digest(std::fs::read(&library)?));
    let fixture = Fixture::new()?;
    let before = snapshot(&fixture.root)?;
    let output = fixture
        .runtime_command()
        .arg("--library")
        .arg(&library)
        .arg("--library-sha256")
        .arg(&digest)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["ok"], true);
    let data = &report["data"];
    assert_eq!(data["action"], "qualify-runtime");
    assert_eq!(data["qualification"], "ml-dsa-65-native-verification-owner");
    assert_eq!(data["algorithm"], "ML-DSA-65");
    assert_eq!(data["signature_suite"], 65);
    assert_eq!(data["library"], library.to_string_lossy().as_ref());
    assert_eq!(data["library_sha256"], digest);
    for field in [
        "verification_qualified",
        "scalar_verification_kat_passed",
        "unified_semantic_verification_kat_passed",
        "mixed_batch_verification_kat_passed",
        "primitive_sign_interface_size_checked",
    ] {
        assert_eq!(data[field], true, "{field}");
    }
    for field in [
        "signing_tested",
        "keygen_tested",
        "protected_key_created",
        "custody_accessed",
        "database_created",
        "service_started",
        "whole_node_post_quantum_qualified",
    ] {
        assert_eq!(data[field], false, "must not claim {field}");
    }
    assert!(data.get("identity").is_none());
    assert_eq!(snapshot(&fixture.root)?, before);
    println!("qualified actual platform runtime SHA256={digest}; no custody/state changed");
    Ok(())
}

#[test]
#[ignore = "requires explicit batch-unavailable library and candidate manifest; countercheck only these recorded artifacts"]
fn actual_runtime_qualification_rejects_selected_library_without_batch_capability() -> TestResult {
    use sha2::{Digest, Sha256};
    let library = PathBuf::from(std::env::var_os("NOVOVM_AOEM_BATCH_UNAVAILABLE_TEST_LIBRARY")
        .ok_or("set NOVOVM_AOEM_BATCH_UNAVAILABLE_TEST_LIBRARY to the exact batch-unavailable artifact under countercheck")?).canonicalize()?;
    let digest = format!("{:x}", Sha256::digest(std::fs::read(&library)?));
    let fixture = Fixture::new()?;
    let before = snapshot(&fixture.root)?;
    let candidate_manifest = PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_MANIFEST")
            .ok_or("set NOVOVM_AOEM_TEST_MANIFEST to the qualified candidate manifest")?,
    )
    .canonicalize()?;
    let mismatched = fixture
        .runtime_command()
        .arg("--library")
        .arg(&library)
        .arg("--library-sha256")
        .arg(&digest)
        .env("AOEM_DLL_MANIFEST", &candidate_manifest)
        .env("AOEM_DLL_MANIFEST_REQUIRED", "1")
        .output()?;
    assert!(!mismatched.status.success());
    assert!(
        String::from_utf8_lossy(&mismatched.stderr).contains("AOEM DLL hash mismatch"),
        "{}",
        String::from_utf8_lossy(&mismatched.stderr)
    );
    assert_eq!(snapshot(&fixture.root)?, before);
    let output = fixture
        .runtime_command()
        .arg("--library")
        .arg(&library)
        .arg("--library-sha256")
        .arg(&digest)
        // This counterexample deliberately selects the formal SDK, not the
        // candidate. Use its original manifest discovery and keep it mandatory.
        .env_remove("AOEM_DLL_MANIFEST")
        .env("AOEM_DLL_MANIFEST_REQUIRED", "1")
        .output()?;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("ML-DSA batch verification capability unavailable"),
        "{error}"
    );
    assert_eq!(snapshot(&fixture.root)?, before);
    println!("rejected exact platform runtime SHA256={digest}: {error}");
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; actual shared AOEM/custody CLI"]
fn actual_mldsa65_both_purposes_generate_inspect_and_rejection_closure() -> TestResult {
    use sha2::{Digest, Sha256};
    let library = PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .ok_or("set NOVOVM_AOEM_TEST_LIBRARY to the approved platform AOEM artifact")?,
    )
    .canonicalize()?;
    let digest = format!("{:x}", Sha256::digest(std::fs::read(&library)?));
    let fixture = Fixture::new()?;
    let validator = fixture.root.join("validator.protected");
    let transport = fixture.root.join("transport.protected");
    let local_identity = fixture.root.join("identity.protected");
    let management = fixture.root.join("management.protected");
    let run = |action, purpose, chain, path: &Path| -> Result<Output, Box<dyn Error>> {
        Ok(fixture
            .command(action, purpose, chain, path)
            .arg("--library")
            .arg(&library)
            .arg("--library-sha256")
            .arg(&digest)
            .output()?)
    };
    let empty = snapshot(&fixture.root)?;
    for (purpose, path) in [("validator", &validator), ("transport", &transport)] {
        assert!(!run("inspect", purpose, 47, path)?.status.success());
        assert_eq!(snapshot(&fixture.root)?, empty);
    }
    let identity = success(run("generate", "validator", 47, &validator)?)?;
    assert_eq!(identity["identity"]["algorithm"], "ML-DSA-65");
    let peer = success(run("generate", "transport", 47, &transport)?)?;
    let local = success(run("generate", "identity", 47, &local_identity)?)?;
    let asset = success(run("generate", "asset-management", 47, &management)?)?;
    assert_eq!(asset["asset_account_hex"].as_str().unwrap().len(), 64);
    assert!(local.get("asset_account_hex").is_none());
    assert!(peer.get("asset_account_hex").is_none());
    assert_ne!(
        local["identity"]["public_key"],
        asset["identity"]["public_key"]
    );
    assert_ne!(
        identity["identity"]["public_key"],
        peer["identity"]["public_key"]
    );
    let before = snapshot(&fixture.root)?;
    for (purpose, path, original, other) in [
        ("validator", &validator, &identity, &transport),
        ("transport", &transport, &peer, &validator),
        ("identity", &local_identity, &local, &management),
        ("asset-management", &management, &asset, &local_identity),
    ] {
        let inspected = success(run("inspect", purpose, 47, path)?)?;
        assert_eq!(inspected["identity"], original["identity"]);
        assert_eq!(
            inspected.get("asset_account_hex"),
            original.get("asset_account_hex")
        );
        assert_eq!(inspected["protected_key_created"], false);
        assert_eq!(snapshot(&fixture.root)?, before);
        for (action, chain, path) in [
            ("generate", 47, path),
            ("inspect", 48, path),
            ("inspect", 0, path),
            ("inspect", 47, other),
        ] {
            assert!(!run(action, purpose, chain, path)?.status.success());
            assert_eq!(snapshot(&fixture.root)?, before);
        }
        for invalid in [
            PathBuf::from("relative.key"),
            fixture.credentials.join("..").join("bad.key"),
        ] {
            assert!(!run("generate", purpose, 47, &invalid)?.status.success());
            assert_eq!(snapshot(&fixture.root)?, before);
        }
    }
    for purpose in ["validator", "transport"] {
        for path in [&local_identity, &management] {
            assert!(!run("inspect", purpose, 47, path)?.status.success());
            assert_eq!(snapshot(&fixture.root)?, before);
        }
    }
    let expected = if cfg!(target_os = "linux") { 9 } else { 4 };
    assert_eq!(
        before.len(),
        expected,
        "only keys, wrapping credential and custody locks"
    );

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let output = fixture
            .command("inspect", "transport", 47, &transport)
            .arg("--library")
            .arg(&library)
            .arg("--library-sha256")
            .arg(&digest)
            .env_remove("CREDENTIALS_DIRECTORY")
            .output()?;
        assert!(!output.status.success());
        assert_eq!(snapshot(&fixture.root)?, before);
        std::fs::set_permissions(&transport, std::fs::Permissions::from_mode(0o644))?;
        assert!(!run("inspect", "transport", 47, &transport)?
            .status
            .success());
        std::fs::set_permissions(&transport, std::fs::Permissions::from_mode(0o600))?;
        let alias = fixture.root.join("alias.protected");
        symlink(&transport, &alias)?;
        assert!(!run("inspect", "transport", 47, &alias)?.status.success());
        assert!(!run("generate", "transport", 47, &alias)?.status.success());
        std::fs::remove_file(alias)?;
    }
    for (purpose, path) in [("validator", &validator), ("transport", &transport)] {
        let mut ciphertext = std::fs::read(path)?;
        let midpoint = ciphertext.len() / 2;
        ciphertext[midpoint] ^= 1;
        std::fs::write(path, ciphertext)?;
        let damaged = snapshot(&fixture.root)?;
        assert!(!run("inspect", purpose, 47, path)?.status.success());
        assert_eq!(snapshot(&fixture.root)?, damaged);
        std::fs::write(path, [29; 32])?;
        let raw = snapshot(&fixture.root)?;
        assert!(!run("inspect", purpose, 47, path)?.status.success());
        assert_eq!(snapshot(&fixture.root)?, raw);
    }
    #[cfg(target_os = "linux")]
    {
        let lock = fixture.root.join("transport.protected.lock");
        std::fs::remove_file(&lock)?;
        let without_lock = snapshot(&fixture.root)?;
        assert!(!run("inspect", "transport", 47, &transport)?
            .status
            .success());
        assert_eq!(snapshot(&fixture.root)?, without_lock);
        assert!(!lock.exists());
    }
    assert!(!fixture.root.join("database").exists());
    assert!(!fixture.root.join("journal").exists());
    Ok(())
}

#[test]
#[ignore = "candidate qualification must include-ignored with explicit NOVOVM_AOEM_TEST_LIBRARY and NOVOVM_PRODUCT_NAT_TEST_BINARY; actual protected CLI and NAT executable main threads"]
fn actual_protected_nat_executables_authenticate_and_reconnect_on_the_same_udp_port() -> TestResult
{
    use sha2::{Digest, Sha256};
    use std::net::UdpSocket;
    let library = PathBuf::from(
        std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
            .ok_or("set NOVOVM_AOEM_TEST_LIBRARY to the approved platform AOEM artifact")?,
    )
    .canonicalize()?;
    let binary = PathBuf::from(std::env::var_os("NOVOVM_PRODUCT_NAT_TEST_BINARY").ok_or(
        "set NOVOVM_PRODUCT_NAT_TEST_BINARY to the actual candidate novovm-product-nat executable",
    )?)
    .canonicalize()?;
    let digest = format!("{:x}", Sha256::digest(std::fs::read(&library)?));
    println!(
        "actual NAT executable={} SHA256={:x}; AOEM SHA256={digest}",
        binary.display(),
        Sha256::digest(std::fs::read(&binary)?)
    );
    let fixture = Fixture::new()?;
    let observer_key = fixture.root.join("observer.protected");
    let probe_key = fixture.root.join("probe.protected");
    let mut public = Vec::new();
    let mut peers = Vec::new();
    for path in [&observer_key, &probe_key] {
        let run = |action| -> Result<Output, Box<dyn Error>> {
            Ok(fixture
                .command(action, "transport", 47, path)
                .arg("--library")
                .arg(&library)
                .arg("--library-sha256")
                .arg(&digest)
                .output()?)
        };
        let generated = success(run("generate")?)?;
        let inspected = success(run("inspect")?)?;
        assert_eq!(generated["identity"], inspected["identity"]);
        let key: Vec<u8> = serde_json::from_value(generated["identity"]["public_key"].clone())?;
        let mut peer_digest = Sha256::new();
        peer_digest.update(b"NOVOVM/transport-identity/ML-DSA-65/v2\0");
        peer_digest.update(&key);
        peers.push(format!("novovm-mldsa65:{:x}", peer_digest.finalize()));
        public.push(key);
    }
    assert_ne!(public[0], public[1]);
    let protected_before = [std::fs::read(&observer_key)?, std::fs::read(&probe_key)?];
    let observer_socket = UdpSocket::bind("127.0.0.1:0")?;
    let probe_socket = UdpSocket::bind("127.0.0.1:0")?;
    let observer_address = observer_socket.local_addr()?.to_string();
    let probe_address = probe_socket.local_addr()?.to_string();
    drop(observer_socket);
    drop(probe_socket);
    let observer_config = fixture.root.join("observer.json");
    let observer_report = fixture.root.join("observer-report.json");
    std::fs::write(
        &observer_config,
        serde_json::to_vec(&serde_json::json!({
            "version":2,"mode":"observed_endpoint_observer","bind_addr":observer_address,
            "identity_key_path":observer_key,"chain_id":47,"library":library,"library_sha256":digest,
            "peer_public_key":public[1],"expected_peer_id":peers[1],"timeout_ms":3000,
            "run_for_ms":15000,"report_path":observer_report
        }))?,
    )?;
    let mut observer = NatProcess::spawn(
        &binary,
        &fixture,
        &observer_config,
        &observer_report,
        "observer",
        true,
    )?;
    let mut probes = Vec::new();
    for attempt in 0..2 {
        let config = fixture.root.join(format!("probe-{attempt}.json"));
        let report_path = fixture.root.join(format!("probe-{attempt}-report.json"));
        std::fs::write(
            &config,
            serde_json::to_vec(&serde_json::json!({
                "version":2,"mode":"observed_endpoint_probe","bind_addr":probe_address,
                "identity_key_path":probe_key,"chain_id":47,"library":library,"library_sha256":digest,
                "peer_public_key":public[0],"expected_peer_id":peers[0],"peer_addr":observer_address,
                "timeout_ms":3000,"run_for_ms":3000,"report_path":report_path
            }))?,
        )?;
        let mut probe = NatProcess::spawn(
            &binary,
            &fixture,
            &config,
            &report_path,
            &format!("probe-{attempt}"),
            true,
        )?;
        // Linux custody takes an exclusive read lock. Prepare the two owners
        // of the same envelope serially; do not weaken or bypass that lock.
        probe.wait_prepared()?;
        probes.push(probe);
    }
    // Both fresh processes qualify their own protected identity on their main
    // thread before any network clock starts. A slow second cold load must not
    // outlive the observer's original 15s observation window. No timeout grows.
    observer.wait_prepared()?;
    // Preparation neither binds the future sockets nor emits a success report.
    drop(UdpSocket::bind(&observer_address)?);
    drop(UdpSocket::bind(&probe_address)?);
    assert!(!observer_report.exists());
    assert!(probes.iter().all(|probe| !probe.report.exists()));
    observer.start()?;
    let mut sessions = Vec::new();
    for probe in &mut probes {
        probe.start()?;
        let outcome = probe.finish();
        if let Err(error) = outcome {
            let observer_status = observer.child.try_wait()?;
            let observer_public = std::fs::read_to_string(&observer_report)
                .unwrap_or_else(|_| "observer report not yet written".to_owned());
            return Err(format!(
                "{error}; observer status={observer_status:?} elapsed_ms={}; observer public report: {observer_public}",
                observer.started.elapsed().as_millis()
            )
            .into());
        }
        let report: Value = serde_json::from_slice(&std::fs::read(&probe.report)?)?;
        assert_eq!(report["accepted"], true);
        assert_eq!(report["scope"], "novovm_native_authenticated_endpoint_v2");
        assert_eq!(report["signature_suite"], 65);
        assert_eq!(report["handshake_version"], 2);
        assert_eq!(report["local_peer_id"], peers[1]);
        assert_eq!(report["peer_id"], peers[0]);
        assert_eq!(report["bind_addr"], probe_address);
        assert_eq!(report["observed_endpoint"], probe_address);
        assert_eq!(report["round_trip_confirmed"], true);
        assert_eq!(report["authenticated_exchanges"], 1);
        assert_eq!(report["network_only"], true);
        assert_eq!(report["aoem_called"], true);
        assert_eq!(report["ledger_semantics"], false);
        let session = report["session_id"]
            .as_array()
            .ok_or("missing authenticated session")?;
        assert_eq!(session.len(), 16);
        assert!(session.iter().any(|v| v.as_u64() != Some(0)));
        sessions.push(report["session_id"].clone());
    }
    assert_ne!(
        sessions[0], sessions[1],
        "reconnect must establish fresh session keys"
    );
    observer.finish()?;
    let report: Value = serde_json::from_slice(&std::fs::read(observer_report)?)?;
    assert_eq!(report["accepted"], true);
    assert_eq!(report["signature_suite"], 65);
    assert_eq!(report["handshake_version"], 2);
    assert_eq!(report["local_peer_id"], peers[0]);
    assert_eq!(report["peer_id"], peers[1]);
    assert_eq!(report["observed_endpoint"], probe_address);
    assert_eq!(report["session_id"], sessions[1]);
    assert!(report["authenticated_requests"].as_u64().unwrap_or(0) >= 2);
    assert!(report["local_replies_submitted"].as_u64().unwrap_or(0) >= 2);
    assert_eq!(
        report["round_trip_confirmed"], false,
        "local reply submission must not impersonate the probe's observed round trip"
    );

    // Exercise the actual failure reporter without an answering peer. The
    // socket only reserves this fixture's loopback port; it never replies.
    let silent_socket = UdpSocket::bind("127.0.0.1:0")?;
    let failure_config = fixture.root.join("unreachable.json");
    let failure_report = fixture.root.join("unreachable-report.json");
    std::fs::write(
        &failure_config,
        serde_json::to_vec(&serde_json::json!({
            "version":2,"mode":"observed_endpoint_probe","bind_addr":probe_address,
            "identity_key_path":probe_key,"chain_id":47,"library":library,"library_sha256":digest,
            "peer_public_key":public[0],"expected_peer_id":peers[0],
            "peer_addr":silent_socket.local_addr()?.to_string(),
            "timeout_ms":100,"run_for_ms":100,"report_path":failure_report
        }))?,
    )?;
    let failure = NatProcess::spawn(
        &binary,
        &fixture,
        &failure_config,
        &failure_report,
        "unreachable",
        false,
    )?
    .finish()
    .expect_err("unanswered native probe must fail");
    let failure = failure.to_string();
    let raw_report = std::fs::read_to_string(&failure_report)?;
    let failed: Value = serde_json::from_str(&raw_report)?;
    assert_eq!(failed["accepted"], false);
    assert_eq!(failed["round_trip_confirmed"], false);
    assert!(failure.contains("NAT unreachable failed"));
    assert!(failure.contains("native authenticated probe did not complete"));
    assert!(failure.contains(&raw_report));
    for private_path in [&observer_key, &probe_key, &fixture.credentials] {
        assert!(!failure.contains(private_path.to_string_lossy().as_ref()));
    }
    assert_eq!(std::fs::read(observer_key)?, protected_before[0]);
    assert_eq!(std::fs::read(probe_key)?, protected_before[1]);
    assert!(!fixture.root.join("database").exists());
    assert!(!fixture.root.join("journal").exists());
    Ok(())
}
