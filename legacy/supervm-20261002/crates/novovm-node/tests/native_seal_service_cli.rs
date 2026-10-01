//! Real main-binary opt-in checks. These must fail before network, AOEM or DB
//! initialization; positive consensus coverage lives in the real-WSS lib tests.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nov-seal-cli-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_novovm-node"));
        cmd.current_dir(&self.0);
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy().to_ascii_uppercase();
            if name.starts_with("NOVOVM_") || name.starts_with("AOEM_") {
                cmd.env_remove(key);
            }
        }
        cmd
    }
    fn assert_failure(&self, cmd: &mut Command, expected: &str) {
        let out = cmd.output().unwrap();
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(expected),
            "expected {expected}, got {stderr}"
        );
        assert!(!String::from_utf8_lossy(&out.stdout).contains("native_seal_service_startup"));
        assert_eq!(
            fs::read_dir(&self.0).unwrap().count(),
            0,
            "preflight created persistence"
        );
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn native_seal_service_cli_disabled_preserves_non_seal_entry() {
    let temp = Sandbox::new();
    temp.assert_failure(
        temp.command().env("NOVOVM_NODE_MODE", "invalid-test-mode"),
        "non-full node_mode is disabled",
    );
}

#[test]
fn native_seal_service_cli_requires_explicit_enablement_and_native_overlay_mode() {
    let temp = Sandbox::new();
    temp.assert_failure(
        temp.command()
            .env("NOVOVM_NATIVE_SEAL_CONFIG", "missing.json"),
        "without explicit enablement",
    );
    temp.assert_failure(
        temp.command().env("NOVOVM_NATIVE_SEAL_ENABLED", "yes"),
        "must be explicitly",
    );
    temp.assert_failure(
        temp.command()
            .env("NOVOVM_NATIVE_SEAL_ENABLED", "1")
            .env("NOVOVM_NATIVE_SEAL_CONFIG", "missing.json"),
        "requires native execution tick mode",
    );
    temp.assert_failure(
        temp.command()
            .env("NOVOVM_NATIVE_SEAL_ENABLED", "1")
            .env("NOVOVM_NODE_MODE", "native_execution_pipeline")
            .env("NOVOVM_NATIVE_SEAL_CONFIG", "missing.json"),
        "requires native execution tick mode",
    );
}

#[test]
fn native_seal_service_cli_bad_config_fails_before_startup_recovery() {
    let temp = Sandbox::new();
    temp.assert_failure(
        temp.command()
            .env("NOVOVM_NATIVE_SEAL_ENABLED", "1")
            .env("NOVOVM_NODE_MODE", "native_execution_pipeline")
            .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED", "1")
            .env("NOVOVM_NATIVE_SEAL_CONFIG", "missing.json"),
        "config",
    );
}

#[test]
fn native_seal_service_cli_query_override_cannot_enable_signing() {
    let temp = Sandbox::new();
    temp.assert_failure(
        temp.command()
            .env("NOVOVM_NATIVE_SEAL_ENABLED", "1")
            .env("NOVOVM_NODE_MODE", "native_execution_pipeline")
            .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED", "1")
            .env(
                "NOVOVM_MAINLINE_QUERY_METHOD",
                "nov_getNativeBlockLedgerStatus",
            )
            .env("NOVOVM_NATIVE_SEAL_CONFIG", "missing.json"),
        "requires native execution tick mode",
    );
}
