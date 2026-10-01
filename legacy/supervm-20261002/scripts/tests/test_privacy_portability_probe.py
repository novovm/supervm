import ctypes
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


SPEC = importlib.util.spec_from_file_location(
    "privacy_portability_probe",
    Path(__file__).resolve().parents[1] / "aoem/privacy_portability_probe.py",
)
PROBE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)


def positive():
    return {"ffi_return_code": 0, "response": {
        "version": 1, "accepted": True, "status": "Accepted", "state_materialized": True,
        "error_code": None, "error_reason": None, "tx_results": [{"accepted": True}],
    }}


def negative():
    return {"ffi_return_code": 0, "response": {
        "version": 1, "accepted": False, "status": "Failed", "state_materialized": False,
        "error_code": "ExecutionRejected", "error_reason": "fixture rejection", "tx_results": [],
    }}


def matrix():
    return {role: {"role": role, "pid": 100 + index,
                   "payload_sha256": "a" * 64, "runtime_sha256": "b" * 64,
                   "prove_calls": int(role == "producer"), "positive": positive(),
                   "negatives": {name: negative() for name in PROBE.NEGATIVE_CASES}}
            for index, role in enumerate(("producer", "validator-1", "validator-2"))}


def payload():
    return PROBE.encode({"extra": [1, 2], "fee": 0,
                         "outputs": [{"range_proof": {"proof": [3, 4]}}],
                         "inputs": [{"ring_signature": {"challenge": [5, 6]}}]})


class PrivacyPortabilityTests(unittest.TestCase):
    def test_positive_requires_all_transaction_results_and_materialization(self):
        self.assertTrue(PROBE.accepted(positive()))
        for invalid in (None, [], {}, {"response": None}, {"response": "accepted"}):
            self.assertFalse(PROBE.accepted(invalid))
        for key, value in (("version", True), ("accepted", 1), ("status", "Failed"),
                           ("state_materialized", False), ("error_code", "Failure"),
                           ("error_reason", "unexpected"), ("tx_results", []),
                           ("tx_results", [{"accepted": True}, {"accepted": False}]),
                           ("tx_results", [{"accepted": False}]),
                           ("tx_results", [{"accepted": True, "error_code": "Failure"}])):
            result = positive()
            result["response"][key] = value
            self.assertFalse(PROBE.accepted(result), (key, value))
        for return_code in (-5, False, "0"):
            result = positive()
            result["ffi_return_code"] = return_code
            self.assertFalse(PROBE.accepted(result))

    def test_unavailable_backend_is_not_a_successful_negative(self):
        self.assertTrue(PROBE.rejected(negative()))
        for invalid in (None, [], {}, {"response": None}, {"response": "rejected"}):
            self.assertFalse(PROBE.rejected(invalid))
        for key, value in (("version", True), ("accepted", 0), ("status", "Accepted"),
                           ("error_code", "BackendUnavailable"), ("error_code", "LicenseRequired"),
                           ("state_materialized", True), ("tx_results", [{"accepted": True}])):
            result = negative()
            result["response"][key] = value
            self.assertFalse(PROBE.rejected(result), (key, value))

    def test_same_process_success_does_not_hide_portability_failure(self):
        results = matrix()
        self.assertTrue(PROBE.assess(results)["accepted"])
        results["validator-1"]["positive"] = negative()
        report = PROBE.assess(results)
        self.assertTrue(report["same_process_accepted"])
        self.assertFalse(report["independent_processes_accepted"])
        self.assertTrue(report["negative_admission_rejected"])
        self.assertFalse(report["negative_crypto_verification_proven"])
        self.assertFalse(report["accepted"])

    def test_missing_case_or_accepted_negative_cannot_pass(self):
        results = matrix()
        del results["validator-2"]
        self.assertFalse(PROBE.assess(results)["accepted"])
        for role in matrix():
            results = matrix()
            del results[role]["negatives"]["range_proof"]
            self.assertFalse(PROBE.assess(results)["accepted"])
            results = matrix()
            results[role]["negatives"]["range_proof"] = positive()
            self.assertFalse(PROBE.assess(results)["accepted"])

    def test_fixture_runtime_and_process_identity_are_required(self):
        for key, value in (("runtime_sha256", "c" * 64), ("payload_sha256", "d" * 64),
                           ("payload_sha256", ""), ("role", "producer"),
                           ("pid", 100), ("pid", True), ("prove_calls", 1)):
            results = matrix()
            results["validator-1"][key] = value
            self.assertFalse(PROBE.assess(results)["accepted"], (key, value))

    def test_mutations_are_distinct_and_do_not_modify_original(self):
        original = payload()
        mutated = PROBE.mutations(original)
        self.assertEqual(set(mutated), set(PROBE.NEGATIVE_CASES))
        self.assertEqual(original, payload())
        self.assertEqual(len(set(mutated.values())), len(PROBE.NEGATIVE_CASES))
        self.assertTrue(all(value != original for value in mutated.values()))
        broken = {"extra": [], "fee": 0}
        with self.assertRaises((IndexError, KeyError)):
            PROBE.mutations(PROBE.encode(broken))

    def test_stale_report_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "acceptance.json"
            PROBE.write_json(path, {"accepted": False})
            with self.assertRaises(FileExistsError):
                PROBE.write_json(path, {"accepted": True})
            self.assertIn("false", path.read_text())

    def test_control_environment_is_not_inherited_by_workers(self):
        with patch.dict(os.environ, {"AOEM_TEST_ONLY": "secret", "NOVOVM_TEST_ONLY": "override",
                                     "PATH": "kept"}, clear=True):
            self.assertEqual(PROBE.clean_environment(), {"PATH": "kept"})

    def test_failed_worker_never_counts_as_runtime_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            args = SimpleNamespace(out=Path(directory), runtime=Path(directory) / "library",
                                   timeout=1, backend="Auto")
            with patch.object(PROBE.subprocess, "run", return_value=SimpleNamespace(returncode=5)):
                with self.assertRaisesRegex(RuntimeError, "process exit 5"):
                    PROBE.run_worker(args, "validator-1", args.out / "payload", "b" * 64)
            with patch.object(PROBE.subprocess, "run", side_effect=subprocess.TimeoutExpired("fixture", 1)):
                with self.assertRaises(subprocess.TimeoutExpired):
                    PROBE.run_worker(args, "validator-2", args.out / "payload", "b" * 64)

    def test_verifier_worker_never_regenerates_the_received_proof(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            library = path / "library"
            library.write_bytes(b"test fixture; never loaded")
            fixture = path / "payload"
            fixture.write_bytes(payload())
            args = SimpleNamespace(runtime=library, payload=fixture, out=path,
                                   worker="validator-1", backend="Auto")
            with patch.object(PROBE, "PrivacyProbeAbi") as backend:
                backend.return_value.execute.return_value = negative()
                PROBE.worker(args)
                backend.return_value.prove.assert_not_called()
                self.assertEqual(backend.return_value.execute.call_count, 6)
                self.assertEqual(backend.return_value.execute.call_args_list[0].args[0], payload())

    def test_native_output_is_freed_even_when_contract_is_invalid(self):
        api = PROBE.PrivacyProbeAbi.__new__(PROBE.PrivacyProbeAbi)
        api.runtime = SimpleNamespace(aoem_free=Mock())
        storage = (ctypes.c_uint8 * 2)(1, 2)

        def operation(input_buffer, input_length, output_pointer, output_length):
            self.assertEqual(input_length, 2)
            ctypes.cast(output_pointer, ctypes.POINTER(ctypes.POINTER(ctypes.c_uint8)))[0] = storage
            ctypes.cast(output_length, ctypes.POINTER(ctypes.c_size_t))[0] = 0
            return -4

        with self.assertRaisesRegex(RuntimeError, "invalid AOEM output"):
            api.allocated(operation, b"hi")
        api.runtime.aoem_free.assert_called_once()


if __name__ == "__main__":
    unittest.main()
