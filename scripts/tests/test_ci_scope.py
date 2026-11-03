"""No Python packages; exercise routing, Git ranges and the real PowerShell selector."""
import importlib.util
import copy
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import hashlib
import shutil
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("ci_scope", Path(__file__).parents[1] / "ci_scope.py")
ci = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ci)


class ScopeTests(unittest.TestCase):
    def test_only_reviewed_prose_is_light(self):
        for name in ci.PROSE_ONLY:
            self.assertEqual(ci.select_scope("push", {}, [name]), "docs")
        self.assertEqual(ci.select_scope("pull_request", {}, sorted(ci.PROSE_ONLY)), "docs")

    def test_code_dependencies_and_unknown_docs_are_full(self):
        for name in ("docs/NOVORUDP_BOUNDARY.md", "docs/new.md", "docs/config.json",
                     "docs/example.rs", "Cargo.lock", "runtime/Cargo.toml", "aoem/sdk.json",
                     "scripts/ci_scope.py", ".github/workflows/ci.yml", "new/file", "crates/a/lib.rs"):
            with self.subTest(name=name):
                self.assertEqual(ci.select_scope("push", {}, ["AGENTS.md", name]), "full")

    def test_empty_unknown_and_missing_diff_fail_closed(self):
        for event in ("push", "pull_request", "unexpected"):
            for paths in (None, []):
                self.assertEqual(ci.select_scope(event, {}, paths), "full")

    def test_dispatch_and_schedules(self):
        self.assertEqual(ci.select_scope("workflow_dispatch", {}), "full")
        self.assertEqual(ci.select_scope("workflow_dispatch", {"inputs": {"profile": "unknown"}}), "full")
        self.assertEqual(ci.select_scope("workflow_dispatch", {"inputs": {"profile": "security"}}), "security")
        self.assertEqual(ci.select_scope("schedule", {"schedule": ci.DAILY_SECURITY}), "security")
        self.assertEqual(ci.select_scope("schedule", {"schedule": "47 3 * * 0"}), "full")
        self.assertEqual(ci.select_scope("schedule", {}), "full")

    def test_outputs_never_claim_unrun_product_tests(self):
        tracked = ["Cargo.toml", "runtime/Cargo.toml", "scripts/a.py", "package.json"]
        for scope in ("docs", "security", "full"):
            out = ci.outputs_for(scope, tracked)
            for name in ("has_rust", "has_python", "has_node", "full"):
                self.assertEqual(out[name], str(scope == "full").lower())
            self.assertEqual(out["security"], str(scope != "docs").lower())

    def test_zero_or_invalid_base_rejected(self):
        for base in (None, "0" * 40, "--help"):
            with self.assertRaises(ValueError):
                ci.changed_paths("push", {"before": base, "after": "a" * 40})

    def test_missing_manifest_cannot_silently_skip_rust(self):
        with self.assertRaises(ValueError):
            ci.outputs_for("full", ["runtime/Cargo.toml"])

    def test_summary_rejects_skipped_cancelled_failed_selected_jobs(self):
        for scope in ("docs", "security", "full"):
            out = ci.outputs_for(scope, ["Cargo.toml", "scripts/a.py", "package.json"])
            needs = {"detect": {"result": "success", "outputs": out}}
            for job, key in {"lan_script_launch": "full", "runtime_modules": "full",
                             "runtime_modules_release": "full",
                             "native_transport_qualification": "full",
                             "rust": "has_rust", "rust_security": "security",
                             "python": "has_python", "node": "has_node"}.items():
                needs[job] = {"result": "success" if out[key] == "true" else "skipped"}
            self.assertEqual(ci.validate_results(needs), scope)
            for job in needs:
                original = needs[job]["result"]
                for wrong in ("cancelled", "failure", "skipped", "success"):
                    if wrong != original:
                        changed = copy.deepcopy(needs)
                        changed[job]["result"] = wrong
                        with self.assertRaises(ValueError):
                            ci.validate_results(changed)
                missing = copy.deepcopy(needs)
                del missing[job]
                if job != "detect":
                    with self.assertRaisesRegex(ValueError, job):
                        ci.validate_results(missing)

    def test_runtime_split_retains_both_platforms_and_independent_budgets(self):
        workflow = (Path(__file__).parents[2] / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        jobs = dict(re.findall(r"^  (\w+):\n(.*?)(?=^  \w+:\n|\Z)", workflow, re.M | re.S))
        for job, group in (("runtime_modules", "ci-runtime-"),
                           ("runtime_modules_release", "ci-runtime-release-")):
            with self.subTest(job=job):
                block = jobs[job]
                self.assertIn("needs: detect\n", block)
                self.assertIn("if: needs.detect.outputs.full == 'true'\n", block)
                self.assertIn("os: [ubuntu-latest, windows-latest]\n", block)
                self.assertIn("fail-fast: false\n", block)
                self.assertIn("timeout-minutes: 15\n", block)
                self.assertIn("group: " + group + "${{ matrix.os }}-${{ github.ref }}-", block)
                self.assertIn("&& github.run_id || 'auto'", block)
                self.assertIn("cancel-in-progress: true\n", block)
                self.assertIn("rustup toolchain install 1.94.0", block)
        required = re.search(r"^    needs: \[([^\]]+)\]$", jobs["result"], re.M).group(1).split(", ")
        self.assertIn("runtime_modules", required)
        self.assertIn("runtime_modules_release", required)
        self.assertIn("if: always()\n", jobs["result"])

    def test_runtime_split_retains_complete_debug_and_release_commands(self):
        workflow = (Path(__file__).parents[2] / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        jobs = dict(re.findall(r"^  (\w+):\n(.*?)(?=^  \w+:\n|\Z)", workflow, re.M | re.S))
        debug, release = jobs["runtime_modules"], jobs["runtime_modules_release"]
        for command in (
            "./scripts/tests/workspace-isolation.tests.ps1",
            "./runtime/check-isolation.ps1",
            "cargo fmt --manifest-path runtime/Cargo.toml --all -- --check",
            "cargo clippy --manifest-path runtime/Cargo.toml --workspace --all-targets --locked -- -D warnings",
            "cargo clippy --manifest-path runtime/Cargo.toml -p novovm-host --lib --no-default-features --locked -- -D warnings",
            "cargo test --manifest-path runtime/Cargo.toml --workspace --locked",
        ):
            with self.subTest(command=command):
                self.assertEqual(debug.count(command), 1)
                self.assertNotIn(command, release)
        command = "cargo test --manifest-path runtime/Cargo.toml --workspace --release --locked -- --include-ignored --test-threads=1"
        self.assertEqual(release.count(command), 1)
        self.assertNotIn(command, debug)
        self.assertNotRegex(release, r"\s--ignored(?:\s|$)")
        for value in ("aoem/linux/core/bin/libaoem_ffi.so", "aoem/windows/core/bin/aoem_ffi.dll",
                      'git lfs pull --include="$env:AOEM_TEST_BINARY" --exclude=""',
                      "$env:NOVOVM_AOEM_TEST_LIBRARY = (Resolve-Path $env:AOEM_TEST_BINARY).Path",
                      "exit $LASTEXITCODE"):
            self.assertIn(value, release)

    def test_native_signature_input_is_separate_and_all_existing_gates_remain(self):
        workflow = (Path(__file__).parents[2] / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        jobs = dict(re.findall(r"^  (\w+):\n(.*?)(?=^  \w+:\n|\Z)", workflow, re.M | re.S))
        native = jobs["native_transport_qualification"]
        self.assertIn("needs: detect\n", native)
        self.assertIn("if: needs.detect.outputs.full == 'true'\n", native)
        self.assertIn("os: [ubuntu-latest, windows-latest]\n", native)
        self.assertIn("fail-fast: false\n", native)
        self.assertIn("--test native_identity_cli --locked -- --include-ignored", native)
        for name in ("actual_runtime_qualification_uses_original_owner_without_keys_or_state",
                     "actual_runtime_qualification_rejects_selected_library_without_batch_capability",
                     "actual_protected_nat_executables_authenticate_and_reconnect_on_the_same_udp_port",
                     "--test rendezvous_process", "--test rendezvous_owner_process",
                     "--test bootstrap_publisher_process", "--test bootstrap_mirror_process",
                     "linux_custody_and_listener_restart", "Formal SDK counterexample identity changed"):
            self.assertIn(name, native)
        required = re.search(r"^    needs: \[([^\]]+)\]$", jobs["result"], re.M).group(1).split(", ")
        self.assertIn("native_transport_qualification", required)
        rust = jobs["rust"]
        formal = rust.index('export NOVOVM_AOEM_TEST_LIBRARY="$PWD/aoem/linux/core/bin/libaoem_ffi.so"')
        resident = rust.index("cargo test -p aoem-bindings --lib resident")
        pin = rust.index("ci-native-runtime-candidate.ps1 -Platform linux-x86_64")
        current = rust.index("cargo test -p novovm-node --lib native_pipeline")
        self.assertLess(formal, resident)
        self.assertLess(resident, pin)
        self.assertLess(pin, current)
        self.assertNotIn('export NOVOVM_AOEM_TEST_LIBRARY="$PWD/aoem/linux/core/bin/libaoem_ffi.so"', rust[pin:])
        for command in ("private-profile-ffi", "native_successor_", "native_only_",
                        "actual_mldsa_uca_initialized_spend_respend_journal_recovery",
                        "native_uca_process_"):
            self.assertIn(command, rust)
        self.assertNotIn("ci-native-runtime-candidate", jobs["runtime_modules_release"])

    def test_original_script_matrix_covers_windows_package_and_adapter_contracts(self):
        workflow = (Path(__file__).parents[2] / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        jobs = dict(re.findall(r"^  (\w+):\n(.*?)(?=^  \w+:\n|\Z)", workflow, re.M | re.S))
        block = jobs["lan_script_launch"]
        self.assertIn("os: [ubuntu-latest, windows-latest]", block)
        for command in ("./scripts/tests/linux-package-safety.tests.ps1",
                        "./scripts/tests/linux-package-build-failure.tests.ps1",
                        "./scripts/tests/linux-package-assembly.tests.ps1\n"):
            self.assertIn(command, block)
        windows = re.search(r"if \(\$IsWindows\) \{(.*?)\n          \}", block, re.S).group(1)
        for command in ("./scripts/tests/linux-package-assembly.tests.ps1 -Target x86_64-pc-windows-msvc",
                        "./scripts/tests/windows-relay-install.tests.ps1",
                        "./scripts/tests/windows-node-network.tests.ps1"):
            self.assertEqual(windows.count(command), 1)

    def test_missing_history_and_malformed_event_select_full(self):
        for raw in ("{broken", "null", "[]", json.dumps({"before": [], "after": "b" * 40}),
                    json.dumps({"before": "a" * 40, "after": "b" * 40})):
            with tempfile.TemporaryDirectory() as temp:
                event = Path(temp) / "event.json"
                output = Path(temp) / "output"
                summary = Path(temp) / "summary"
                event.write_text(raw, encoding="utf-8")
                def unavailable(*args):
                    if args[0] == "ls-files":
                        return b"Cargo.toml\0"
                    raise subprocess.CalledProcessError(128, "git")
                with patch.dict(os.environ, {"GITHUB_EVENT_NAME": "push",
                                             "GITHUB_EVENT_PATH": str(event),
                                             "GITHUB_OUTPUT": str(output),
                                             "GITHUB_STEP_SUMMARY": str(summary)}), patch.object(ci, "git", side_effect=unavailable):
                    ci.main()
                self.assertIn("scope=full\n", output.read_text())

    def test_summary_rejects_inconsistent_full_claim(self):
        out = ci.outputs_for("docs", ["Cargo.toml"])
        out["scope"] = "full"
        with self.assertRaisesRegex(ValueError, "contradicts"):
            ci.validate_results({"detect": {"result": "success", "outputs": out}})

    def test_git_push_range_and_rename_cannot_hide_code(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            def git(*args):
                return subprocess.check_output(["git", "-C", temp, *args], stderr=subprocess.PIPE)
            git("init")
            git("config", "user.name", "CI fixture")
            git("config", "user.email", "fixture@example.invalid")
            (root / "logic.rs").write_text("original", encoding="utf-8")
            git("add", ".")
            git("commit", "-m", "base")
            base = git("rev-parse", "HEAD").decode().strip()
            (root / "logic.rs").rename(root / "AGENTS.md")
            git("add", "-A")
            git("commit", "-m", "rename code to reviewed prose")
            head = git("rev-parse", "HEAD").decode().strip()
            with patch.object(ci, "git", side_effect=git):
                paths = ci.changed_paths("push", {"before": base, "after": head})
                self.assertCountEqual(paths, ["logic.rs", "AGENTS.md"])
                self.assertEqual(ci.select_scope("push", {}, paths), "full")
                pr = {"pull_request": {"base": {"sha": base}, "head": {"sha": head}}}
                self.assertCountEqual(ci.changed_paths("pull_request", pr), paths)
                # A later docs-only commit must not hide earlier code changes
                # in the same push. Also exercise a genuine prose-only range.
                (root / "AGENTS.md").write_text("updated prose", encoding="utf-8")
                git("add", ".")
                git("commit", "-m", "prose")
                final = git("rev-parse", "HEAD").decode().strip()
                whole = ci.changed_paths("push", {"before": base, "after": final})
                self.assertEqual(ci.select_scope("push", {}, whole), "full")
                prose = ci.changed_paths("push", {"before": head, "after": final})
                self.assertEqual(ci.select_scope("push", {}, prose), "docs")


class RuntimeCandidateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.helper = Path(__file__).parents[1] / "ci-native-runtime-candidate.ps1"
        self.pwsh = shutil.which("pwsh")
        if not self.pwsh:
            self.fail("PowerShell is required to test the actual cross-platform CI selector")
        self.manifest = {"schema_version": 1,
                         "scope": "isolated_native_transport_mldsa65_core_candidates_not_shipped_sdk",
                         "platforms": {}}
        for platform, library in (("windows-x86_64", "windows/aoem_ffi.dll"),
                                  ("linux-x86_64", "linux/libaoem_ffi.so")):
            header = library.rsplit("/", 1)[0] + "/aoem.h"
            for name, data in ((library, b"fixture-not-an-executable"), (header, b"fixture header")):
                path = self.root / name
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(data)
            self.manifest["platforms"][platform] = {
                "library": library, "sha256": hashlib.sha256((self.root / library).read_bytes()).hexdigest(),
                "header": header, "header_sha256": hashlib.sha256((self.root / header).read_bytes()).hexdigest(),
                "source": {"base_commit": "a" * 40, "overlay_sha256": "b" * 64, "lock_sha256": "c" * 64},
                "features": ["mldsa"]}

    def run_helper(self, platform="linux-x86_64", *extra, env=None):
        (self.root / "manifest.json").write_text(json.dumps(self.manifest), encoding="utf-8")
        return subprocess.run([self.pwsh, "-NoProfile", "-File", str(self.helper),
                               "-CandidateRoot", str(self.root), "-Platform", platform, *extra],
                              capture_output=True, text=True, encoding="utf-8", env=env, timeout=20)

    def test_pins_both_platforms_without_claiming_runtime_qualification(self):
        for platform in self.manifest["platforms"]:
            with self.subTest(platform=platform):
                output = self.run_helper(platform)
                self.assertEqual(output.returncode, 0, output.stderr)
                report = json.loads(output.stdout)
                self.assertEqual(report["sha256"], self.manifest["platforms"][platform]["sha256"])
                self.assertFalse(report["runtime_kat_executed"])
                self.assertEqual(Path(report["library"]), self.root / self.manifest["platforms"][platform]["library"])

    def test_corruption_identity_drift_and_missing_provenance_are_rejected(self):
        original = copy.deepcopy(self.manifest)
        for field, value in (("library", "../unreviewed.so"), ("sha256", "A" * 64),
                             ("header_sha256", "0" * 64), ("features", []), ("source", {})):
            with self.subTest(field=field):
                self.manifest = copy.deepcopy(original)
                self.manifest["platforms"]["linux-x86_64"][field] = value
                self.assertNotEqual(self.run_helper().returncode, 0)
        self.manifest = original
        (self.root / "linux/libaoem_ffi.so").write_bytes(b"changed artifact")
        self.assertNotEqual(self.run_helper().returncode, 0)

    def test_ci_selects_same_pinned_library_for_default_and_explicit_consumers(self):
        output_file = self.root / "github-env"
        env = dict(os.environ, GITHUB_ENV=str(output_file))
        output = self.run_helper("linux-x86_64", "-GitHubEnvironment", env=env)
        self.assertEqual(output.returncode, 0, output.stderr)
        lines = dict(line.split("=", 1) for line in output_file.read_text(encoding="utf-8-sig").splitlines())
        self.assertEqual(lines["NOVOVM_AOEM_TEST_LIBRARY"], lines["NOVOVM_AOEM_DLL"])
        self.assertEqual(lines["NOVOVM_NATIVE_RUNTIME_SHA256"], self.manifest["platforms"]["linux-x86_64"]["sha256"])


if __name__ == "__main__":
    unittest.main()
