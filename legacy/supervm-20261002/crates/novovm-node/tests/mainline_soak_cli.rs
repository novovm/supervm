#![forbid(unsafe_code)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const STALE_SNAPSHOT: &str = include_str!("fixtures/mainline-soak/stale-snapshot.json");
const CHAIN_ID: &str = "9998897";

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let nonce = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "novovm-soak-cli-{label}-{}-{nonce}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos(),
        ));
        fs::create_dir(&path).expect("create isolated soak CLI directory");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn clean_command(binary: &str, dir: &TestDir) -> Command {
    let mut command = Command::new(binary);
    command.current_dir(&dir.0);
    // Clear only the child's environment. Parallel tests must not mutate the
    // parent's process environment or inherit operator soak overrides.
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("NOVOVM_MAINLINE_")
        {
            command.env_remove(key);
        }
    }
    command
}

fn soak_command(dir: &TestDir, snapshot: &Path, report: &Path) -> Command {
    let mut command = clean_command(env!("CARGO_BIN_EXE_supervm-mainline-soak"), dir);
    command
        .env("NOVOVM_MAINLINE_SOAK_PROFILE", "6h")
        .env("NOVOVM_MAINLINE_SOAK_CHAIN_ID", CHAIN_ID)
        .env("NOVOVM_MAINLINE_SOAK_DURATION_SECONDS", "1")
        .env("NOVOVM_MAINLINE_SOAK_INTERVAL_SECONDS", "1")
        .env("NOVOVM_MAINLINE_SOAK_SNAPSHOT_PATH", snapshot)
        .env("NOVOVM_MAINLINE_SOAK_REPORT_PATH", report);
    command
}

fn nightly_command(dir: &TestDir, snapshot: &Path, report: &Path, summary: &Path) -> Command {
    let mut command = clean_command(env!("CARGO_BIN_EXE_supervm-mainline-nightly-gate"), dir);
    command
        .env("NOVOVM_MAINLINE_NIGHTLY_RUN_MAINLINE_GATE", "false")
        .env("NOVOVM_MAINLINE_NIGHTLY_SOAK_PROFILES", "6h")
        .env("NOVOVM_MAINLINE_NIGHTLY_SOAK_CHAIN_ID", CHAIN_ID)
        .env("NOVOVM_MAINLINE_NIGHTLY_SOAK_SNAPSHOT_PATH", snapshot)
        .env("NOVOVM_MAINLINE_NIGHTLY_SOAK_REPORT_PATH", summary)
        .env("NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_DURATION_SECONDS", "1")
        .env("NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_INTERVAL_SECONDS", "1")
        .env("NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_REPORT_PATH", report);
    command
}

fn publish_snapshot(path: &Path, fixture: &mut Value) -> std::io::Result<()> {
    fixture["updated_at_unix_ms"] = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
    .into();
    let pending = path.with_extension("pending.json");
    fs::write(&pending, serde_json::to_vec(fixture).unwrap())?;
    // Atomic replacement keeps readers from seeing a truncated JSON object.
    // Windows readers or antivirus can briefly deny a rename; retry that
    // replacement without deleting the last complete published snapshot.
    for attempt in 0..20 {
        match fs::rename(&pending, path) {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < 19
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::PermissionDenied
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::AlreadyExists
                    ) =>
            {
                thread::sleep(Duration::from_millis(5))
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("rename loop always returns")
}

struct SyntheticExporter {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<std::io::Result<()>>>,
}

impl SyntheticExporter {
    fn start(path: &Path, body_updates: u64) -> Self {
        let mut fixture: Value = serde_json::from_str(STALE_SNAPSHOT).unwrap();
        fixture["body_updates"] = body_updates.into();
        publish_snapshot(path, &mut fixture).expect("publish initial synthetic snapshot");
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let path = path.to_path_buf();
        let worker = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                publish_snapshot(&path, &mut fixture)?;
                thread::sleep(Duration::from_millis(25));
            }
            Ok(())
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }

    fn finish(mut self) -> std::io::Result<()> {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("synthetic exporter thread")
    }
}

impl Drop for SyntheticExporter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_with_exporter(mut command: Command, snapshot: &Path, body_updates: u64) -> Output {
    let exporter = SyntheticExporter::start(snapshot, body_updates);
    let result = (|| -> std::io::Result<Output> {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // Each run requests one second; regressions must not hang the suite.
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if child.try_wait()?.is_some() {
                return child.wait_with_output();
            }
            if Instant::now() >= deadline {
                child.kill()?;
                let _ = child.wait_with_output();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "one-second soak child exceeded 15 seconds",
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    })();
    let exporter_result = exporter.finish();
    // Stop and join the writer before any assertion can unwind the test.
    exporter_result.expect("synthetic exporter kept publishing complete snapshots");
    result.expect("run bounded soak CLI")
}

fn output_debug(output: &Output) -> String {
    format!(
        "status={}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}

fn read_failed_soak_report(path: &Path, output: &Output) -> Value {
    assert!(
        !output.status.success(),
        "invalid soak evidence must fail the process\n{}",
        output_debug(output),
    );
    let encoded = fs::read(path).unwrap_or_else(|error| {
        panic!(
            "failed evidence must still persist a report: {error}\n{}",
            output_debug(output),
        )
    });
    let report: Value = serde_json::from_slice(&encoded).expect("decode persisted soak report");
    assert_eq!(report["schema"], "supervm-mainline-soak-report/v2");
    assert_eq!(report["evaluation"]["pass"], false);
    assert_eq!(report["nominal_duration_seconds"], 21_600);
    assert_eq!(report["duration_requirement_met"], false);
    assert!(report["observed_elapsed_ms"].as_u64().unwrap() >= 1_000);
    report
}

fn has_violation(report: &Value, expected: &str) -> bool {
    report["evaluation"]["violations"]
        .as_array()
        .expect("violation array")
        .iter()
        .any(|entry| entry["code"] == expected)
}

#[test]
fn stale_stopped_node_snapshot_fails_cli_and_persists_v2_report() {
    let dir = TestDir::new("stale");
    let snapshot = dir.join("snapshot.json");
    let report_path = dir.join("report.json");
    fs::write(&snapshot, STALE_SNAPSHOT).expect("write stopped-node fixture");
    let output = soak_command(&dir, &snapshot, &report_path)
        .output()
        .expect("run soak CLI");
    let report = read_failed_soak_report(&report_path, &output);
    assert_eq!(report["mode"], "workload");
    assert_eq!(report["validation_scope"], "short_smoke");
    assert_eq!(report["sampling"]["read_error_count"], 0);
    assert!(report["sampling"]["stale_snapshot_count"].as_u64().unwrap() >= 2);
    assert_eq!(report["sampling"]["valid_sample_count"], 0);
    assert!(has_violation(&report, "stale_snapshot"));
    assert!(has_violation(&report, "no_sampled_body_progress"));
}

#[test]
fn missing_snapshot_fails_cli_and_preserves_diagnostics() {
    let dir = TestDir::new("missing");
    let report_path = dir.join("report.json");
    let output = soak_command(&dir, &dir.join("missing.json"), &report_path)
        .output()
        .expect("run soak CLI");
    let report = read_failed_soak_report(&report_path, &output);
    assert!(report["sampling"]["read_error_count"].as_u64().unwrap() >= 2);
    assert_eq!(report["sampling"]["valid_sample_count"], 0);
    assert!(has_violation(&report, "insufficient_valid_samples"));
}

#[test]
fn wrong_schema_snapshot_is_rejected_instead_of_counted_as_evidence() {
    let dir = TestDir::new("schema");
    let snapshot = dir.join("snapshot.json");
    let report_path = dir.join("report.json");
    let mut fixture: Value = serde_json::from_str(STALE_SNAPSHOT).unwrap();
    fixture["schema"] = "unrelated-runtime/v1".into();
    fs::write(&snapshot, serde_json::to_vec(&fixture).unwrap()).unwrap();
    let output = soak_command(&dir, &snapshot, &report_path)
        .output()
        .expect("run soak CLI");
    let report = read_failed_soak_report(&report_path, &output);
    assert!(
        report["sampling"]["wrong_schema_snapshot_count"]
            .as_u64()
            .unwrap()
            >= 2
    );
    assert_eq!(report["sampling"]["valid_sample_count"], 0);
    assert!(has_violation(&report, "wrong_schema_snapshot"));
}

#[test]
fn nightly_persists_failure_and_profile_mode_override_without_claiming_full_soak() {
    for (global_mode, profile_mode, scope) in [
        ("idle_health", "workload", "short_smoke"),
        ("workload", "idle_health", "idle_health"),
    ] {
        let dir = TestDir::new(profile_mode);
        let snapshot = dir.join("snapshot.json");
        let report_path = dir.join("profile.json");
        let summary_path = dir.join("nightly.json");
        fs::write(&snapshot, STALE_SNAPSHOT).unwrap();
        let output = nightly_command(&dir, &snapshot, &report_path, &summary_path)
            .env("NOVOVM_MAINLINE_SOAK_MODE", global_mode)
            .env("NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_MODE", profile_mode)
            .output()
            .expect("run nightly CLI");
        let report = read_failed_soak_report(&report_path, &output);
        assert_eq!(report["mode"], profile_mode);
        assert_eq!(report["validation_scope"], scope);
        let summary: Value = serde_json::from_slice(&fs::read(&summary_path).unwrap()).unwrap();
        assert_eq!(
            summary["schema"],
            "supervm-mainline-nightly-soak-gate-report/v2"
        );
        assert_eq!(summary["overall_pass"], false);
        assert_eq!(summary["run_mainline_gate"], false);
        let profiles = summary["profile_results"].as_array().unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0]["pass"], false);
        assert_eq!(profiles[0]["mode"], profile_mode);
        assert_eq!(profiles[0]["validation_scope"], scope);
        assert_eq!(profiles[0]["nominal_duration_seconds"], 21_600);
        assert_eq!(profiles[0]["duration_requirement_met"], false);
    }
}

fn check_live_short_smoke(mode: &str, body_updates: u64, scope: &str) {
    let dir = TestDir::new(mode);
    let snapshot = dir.join("snapshot.json");
    let standalone_report = dir.join("standalone.json");
    let mut standalone = soak_command(&dir, &snapshot, &standalone_report);
    standalone.env("NOVOVM_MAINLINE_SOAK_MODE", mode);
    let output = run_with_exporter(standalone, &snapshot, body_updates);
    assert!(output.status.success(), "{}", output_debug(&output));
    let report: Value = serde_json::from_slice(&fs::read(&standalone_report).unwrap()).unwrap();
    assert_eq!(report["evaluation"]["pass"], true);
    assert_eq!(report["mode"], mode);
    assert_eq!(report["validation_scope"], scope);
    assert_eq!(report["duration_requirement_met"], false);
    assert!(report["sampling"]["valid_sample_count"].as_u64().unwrap() >= 2);
    assert_eq!(report["sampling"]["read_error_count"], 0);

    let profile_report = dir.join("profile.json");
    let summary_path = dir.join("nightly.json");
    let mut nightly = nightly_command(&dir, &snapshot, &profile_report, &summary_path);
    nightly.env("NOVOVM_MAINLINE_NIGHTLY_SOAK_6H_MODE", mode);
    let output = run_with_exporter(nightly, &snapshot, body_updates);
    assert!(!output.status.success(), "{}", output_debug(&output));
    let report: Value = serde_json::from_slice(&fs::read(&profile_report).unwrap()).unwrap();
    assert_eq!(report["evaluation"]["pass"], true);
    assert_eq!(report["mode"], mode);
    assert_eq!(report["validation_scope"], scope);
    assert_eq!(report["duration_requirement_met"], false);
    let summary: Value = serde_json::from_slice(&fs::read(&summary_path).unwrap()).unwrap();
    assert_eq!(summary["overall_pass"], false);
    assert_eq!(summary["profile_results"][0]["pass"], true);
    assert_eq!(summary["profile_results"][0]["mode"], mode);
    assert_eq!(summary["profile_results"][0]["validation_scope"], scope);
    assert_eq!(
        summary["profile_results"][0]["duration_requirement_met"],
        false
    );
}

#[test]
fn healthy_workload_short_smoke_passes_cli_but_not_nightly_soak() {
    check_live_short_smoke("workload", 1, "short_smoke");
}

#[test]
fn live_idle_heartbeat_passes_cli_but_not_nightly_workload_acceptance() {
    check_live_short_smoke("idle_health", 0, "idle_health");
}

#[test]
fn invalid_nan_threshold_fails_before_sampling() {
    let dir = TestDir::new("nan");
    let report_path = dir.join("report.json");
    let output = soak_command(&dir, &dir.join("unused.json"), &report_path)
        .env("NOVOVM_MAINLINE_SOAK_MIN_BODY_UPDATES_PER_HOUR", "NaN")
        .output()
        .expect("run soak CLI");
    assert!(!output.status.success(), "{}", output_debug(&output));
    assert!(String::from_utf8_lossy(&output.stderr).contains("finite and nonnegative"));
    assert!(!report_path.exists());
}
