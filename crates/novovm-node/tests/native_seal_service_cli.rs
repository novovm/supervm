//! Real main-binary preflight checks. The old Seal loader is exercised only
//! under its explicit historical-fixture opt-in. The current product must reject
//! coexistence with that owner, before network, AOEM or persistence startup.
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
    fn inputs(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files = fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                assert!(
                    entry.file_type().unwrap().is_file(),
                    "unexpected persistence directory"
                );
                (entry.path(), fs::read(entry.path()).unwrap())
            })
            .collect::<Vec<_>>();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    }
    fn assert_failure(&self, cmd: &mut Command, expected: &str) {
        let before = self.inputs();
        let out = cmd.output().unwrap();
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(expected),
            "expected {expected}, got {stderr}"
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        for marker in [
            "native_seal_service_startup:",
            "native_execution_startup_recovery:",
            "native_block_ledger_startup_recovery:",
            "native_tick_start:",
        ] {
            assert!(!stdout.contains(marker), "startup ran: {stdout}");
            assert!(!stderr.contains(marker), "startup ran: {stderr}");
        }
        assert_eq!(
            self.inputs(),
            before,
            "preflight created persistence or changed fixture inputs"
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
    // Without this opt-in, the retired raw/recovery path must not be entered.
    // A production ResidentConfig is deliberately NOT a route to the old Seal
    // owner: the independent coexistence test below verifies that prohibition.
    for (path, expected) in [
        ("missing.json", "resolve native seal service config"),
        ("invalid.json", "invalid native seal service config JSON"),
    ] {
        if path == "invalid.json" {
            fs::write(temp.0.join(path), b"{not-valid-json").unwrap();
        }
        temp.assert_failure(
            temp.command()
                .env("NOVOVM_JULY_HISTORICAL_FIXTURE", "1")
                .env("NOVOVM_NATIVE_SEAL_ENABLED", "1")
                .env("NOVOVM_NODE_MODE", "native_execution_pipeline")
                .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED", "1")
                .env("NOVOVM_NODE_VERBOSE", "1")
                .env("NOVOVM_NATIVE_SEAL_CONFIG", path),
            expected,
        );
    }
}

#[test]
fn native_seal_service_cli_historical_loader_requires_fixture_opt_in() {
    let temp = Sandbox::new();
    temp.assert_failure(
        temp.command()
            .env("NOVOVM_NATIVE_SEAL_ENABLED", "1")
            .env("NOVOVM_NODE_MODE", "native_execution_pipeline")
            .env("NOVOVM_PRODUCT_MAINLINE_OVERLAY_ENABLED", "1")
            .env("NOVOVM_NATIVE_SEAL_CONFIG", "missing.json"),
        "native tick requires NOVOVM_NATIVE_EXECUTION_CONFIG; old raw/recovery wiring is historical-fixture only",
    );
}

#[test]
fn native_seal_service_cli_product_owner_rejects_seal_before_config_loading() {
    let temp = Sandbox::new();
    for (name, value) in [
        ("NOVOVM_NATIVE_SEAL_ENABLED", "1"),
        ("NOVOVM_NATIVE_SEAL_CONFIG", "missing-seal.json"),
    ] {
        for mode in ["native_execution_tick", "native_execution_pipeline"] {
            temp.assert_failure(
                temp.command()
                    .env("NOVOVM_NODE_MODE", mode)
                    // Intentionally nonexistent: mutual exclusion precedes
                    // even reading either configuration, not just DB open.
                    .env("NOVOVM_NATIVE_EXECUTION_CONFIG", "unread-native.json")
                    .env("NOVOVM_NATIVE_EXECUTION_START", "create")
                    .env(name, value),
                &format!("{name} cannot coexist with the original native product owner"),
            );
        }
    }
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
