import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import subprocess
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "local_readiness", Path(__file__).resolve().parents[1] / "novovm-local-readiness.py"
)
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


class LocalReadinessTests(unittest.TestCase):
    def test_compiler_messages_require_one_matching_test_binary(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "build.log"
            item = {"reason": "compiler-artifact", "target": {"name": "novovm_node"},
                    "profile": {"test": True}, "executable": "/verified/test-binary"}
            log.write_text("warning\n" + json.dumps(item) + "\n" + json.dumps(item))
            self.assertEqual(GATE.test_executable(log, "novovm_node"), "/verified/test-binary")
            with self.assertRaises(RuntimeError):
                GATE.test_executable(log, "other")
            log.write_text(json.dumps(item) + "\n" + json.dumps({**item, "executable": "/old/binary"}))
            with self.assertRaises(RuntimeError):
                GATE.test_executable(log, "novovm_node")
            item["profile"]["test"] = False
            log.write_text(json.dumps(item))
            with self.assertRaises(RuntimeError):
                GATE.test_executable(log, "novovm_node")

    def test_failed_command_is_not_acceptance(self):
        with tempfile.TemporaryDirectory() as directory:
            report = {"steps": []}
            with patch.object(GATE.subprocess, "Popen") as spawn:
                spawn.return_value.wait.return_value = 17
                with self.assertRaises(RuntimeError):
                    GATE.run_step(Path(directory), Path(directory), "failed", ["false"], report)
                self.assertEqual(report["steps"][0]["exit_code"], 17)
                self.assertFalse(report["steps"][0]["accepted"])
                self.assertTrue(spawn.call_args.kwargs["start_new_session"])

    def test_namespace_has_no_host_network_fallback(self):
        command = GATE.isolated(["binary", "a space", "; false"])
        self.assertEqual(command[:5], ["unshare", "--user", "--map-root-user", "--net", "--"])
        self.assertEqual(command[-3:], ["binary", "a space", "; false"])
        self.assertIn('ip link set lo up && exec "$@"', command)

    def test_zero_tests_and_missing_summary_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "tests.log"
            for text in ["", "test result: ok. 0 passed; 0 failed; 2 ignored;"]:
                log.write_text(text)
                with self.assertRaises(RuntimeError):
                    GATE.rust_test_result(log)
            log.write_text("test result: ok. 5 passed; 0 failed; 4 ignored; 0 measured;")
            self.assertEqual(GATE.rust_test_result(log), {"passed": 5, "failed": 0, "ignored": 4})

    def test_storage_faults_require_private_mount_and_network_namespaces(self):
        command = GATE.isolated_storage(["binary", "a space", "; false"], "mnt:[12345]")
        self.assertEqual(command[:8], [
            "unshare", "--user", "--map-root-user", "--net", "--mount", "--propagation", "private", "--",
        ])
        self.assertIn("NOVOVM_TEST_STORAGE_PARENT_MOUNT_NS=mnt:[12345]", command)
        self.assertEqual(command[-3:], ["binary", "a space", "; false"])
        for value in ["", "mnt:[]", "net:[12345]", "mnt:[12345]; false"]:
            with self.assertRaises(RuntimeError):
                GATE.isolated_storage(["binary"], value)

    def test_timeout_terminates_only_its_own_process_group(self):
        with tempfile.TemporaryDirectory() as directory:
            report = {"steps": []}
            with patch.object(GATE.subprocess, "Popen") as spawn, patch.object(GATE.os, "killpg") as kill:
                spawn.return_value.pid = 123456
                spawn.return_value.wait.side_effect = [subprocess.TimeoutExpired("test", 1), -9]
                with self.assertRaises(subprocess.TimeoutExpired):
                    GATE.run_step(Path(directory), Path(directory), "timeout", ["test"], report, 1)
                kill.assert_called_once_with(123456, GATE.signal.SIGKILL)
                self.assertFalse(report["steps"][0]["accepted"])

    def test_source_identity_catches_uncommitted_content_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source.rs"
            source.write_text("original")
            answers = [b"source.rs\0", b"test-head\n", b" M source.rs\n"] * 2
            with patch.object(GATE.subprocess, "check_output", side_effect=answers):
                before = GATE.source_identity(root)
                source.write_text("modified")
                after = GATE.source_identity(root)
            self.assertFalse(before["worktree_clean"])
            self.assertEqual(before["head"], after["head"])
            self.assertNotEqual(before["source_sha256"], after["source_sha256"])


if __name__ == "__main__":
    unittest.main()
