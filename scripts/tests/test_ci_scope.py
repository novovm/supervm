"""No external dependencies; exercise routing and real Git range/rename cases."""
import importlib.util
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
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


if __name__ == "__main__":
    unittest.main()
