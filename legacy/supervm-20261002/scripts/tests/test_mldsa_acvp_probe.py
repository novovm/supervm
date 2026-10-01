import copy
import ctypes
import importlib.util
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest


SPEC = importlib.util.spec_from_file_location(
    "mldsa_acvp_probe", Path(__file__).resolve().parents[1] / "aoem/mldsa_acvp_probe.py",
)
PROBE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)


def documents():
    prompt = {"algorithm": "ML-DSA", "mode": "sigVer", "revision": "FIPS204", "vsId": 42, "testGroups": []}
    expected = copy.deepcopy(prompt)
    for group_id, (level, interface) in PROBE.GROUPS.items():
        group = {"tgId": group_id, "testType": "AFT", "parameterSet": f"ML-DSA-{level}",
                 "signatureInterface": interface, "tests": []}
        if interface == "external":
            group["preHash"] = "pure"
        else:
            group["externalMu"] = False
        answers = {"tgId": group_id, "tests": []}
        key_bytes, signature_bytes = PROBE.SIZES[level]
        for case_id in range(1, 16):
            case = {"tcId": case_id, "pk": "01" * key_bytes, "signature": "02" * signature_bytes,
                    "message": "0304"}
            if interface == "external":
                case["context"] = "0506"
            group["tests"].append(case)
            answers["tests"].append({"tcId": case_id, "testPassed": case_id <= 3})
        prompt["testGroups"].append(group)
        expected["testGroups"].append(answers)
    return prompt, expected


class MldsaAcvpTests(unittest.TestCase):
    def test_pure_and_internal_messages_are_not_mixed(self):
        cases = PROBE.vectors(*documents())
        self.assertEqual(len(cases), 90)
        self.assertEqual(sum(case["expected_valid"] for case in cases), 18)
        for case in cases:
            expected_message = bytes([0, 2, 5, 6, 3, 4]) if case["interface"] == "external" else bytes([3, 4])
            self.assertEqual(case["message"], expected_message)

    def test_wrong_dataset_hash_fails_before_json_decode(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input.json"
            path.write_bytes(b"{}")
            self.assertEqual(PROBE.load_pinned(path, PROBE.digest(b"{}")), {})
            with self.assertRaisesRegex(ValueError, "SHA256 mismatch"):
                PROBE.load_pinned(path, PROBE.PROMPT_SHA256)

    def test_duplicate_missing_or_mismatched_case_is_rejected(self):
        for change in ("duplicate", "missing", "mismatch", "expected_boolean"):
            prompt, expected = documents()
            if change == "duplicate":
                prompt["testGroups"][0]["tests"][1]["tcId"] = 1
            elif change == "missing":
                prompt["testGroups"][0]["tests"].pop()
            elif change == "mismatch":
                expected["testGroups"][0]["tests"][0]["tcId"] = 90
            else:
                expected["testGroups"][0]["tests"][0]["testPassed"] = "true"
            with self.assertRaises(ValueError, msg=change):
                PROBE.vectors(prompt, expected)

    def test_profile_size_and_context_errors_fail_closed(self):
        for change in ("parameterSet", "preHash", "externalMu", "key_size", "context", "hex"):
            prompt, expected = documents()
            first = prompt["testGroups"][0]
            if change == "parameterSet":
                first[change] = "ML-DSA-65"
            elif change == "preHash":
                first[change] = "preHash"
            elif change == "externalMu":
                prompt["testGroups"][3][change] = True
            elif change == "key_size":
                first["tests"][0]["pk"] = "00"
            elif change == "context":
                first["tests"][0]["context"] = "00" * 256
            else:
                first["tests"][0]["message"] = "01 02"
            with self.assertRaises(ValueError, msg=change):
                PROBE.vectors(prompt, expected)

    def test_all_false_or_all_true_is_not_qualification(self):
        cases = PROBE.vectors(*documents())
        for value, passed in ((False, 72), (True, 18)):
            report = PROBE.run_matrix(cases, lambda case: value)
            self.assertEqual(report["passed"], passed)
            self.assertFalse(report["accepted"])
        report = PROBE.run_matrix(cases, lambda case: case["expected_valid"])
        self.assertEqual(report["passed"], 90)
        self.assertTrue(report["accepted"])

    def test_runtime_errors_and_missing_boolean_cannot_pass_negative(self):
        def failed(case):
            if not case["expected_valid"]:
                raise RuntimeError("backend unavailable")
            return True

        cases = PROBE.vectors(*documents())
        report = PROBE.run_matrix(cases, failed)
        self.assertEqual(report["passed"], 18)
        self.assertFalse(report["accepted"])
        self.assertFalse(PROBE.run_matrix(cases, lambda case: 0)["accepted"])

    def test_reduced_profile_or_duplicate_evidence_cannot_pass(self):
        cases = PROBE.vectors(*documents())
        with self.assertRaises(ValueError):
            PROBE.run_matrix(cases[:15], lambda case: case["expected_valid"])
        cases[-1] = cases[-2]
        self.assertFalse(PROBE.run_matrix(cases, lambda case: case["expected_valid"])["accepted"])

    def test_one_bad_positive_rejects_whole_matrix(self):
        cases = PROBE.vectors(*documents())
        report = PROBE.run_matrix(cases, lambda case: False if case["group_id"] == 3 and case["case_id"] == 1 else case["expected_valid"])
        self.assertEqual(report["passed"], 89)
        self.assertFalse(report["accepted"])
        self.assertEqual(report["groups"][1]["failed_case_ids"], [1])

    def test_raw_abi_requires_zero_return_and_written_boolean(self):
        case = PROBE.vectors(*documents())[0]
        verifier = PROBE.VerifyAbi.__new__(PROBE.VerifyAbi)
        for return_code, valid, expected in ((0, 1, True), (0, 0, False), (-5, 0, None), (0, 2, None), (0, None, None)):
            def verify(level, public_key, key_length, message, message_length, signature, signature_length, output):
                self.assertEqual(level, 44)
                self.assertEqual(key_length, PROBE.SIZES[44][0])
                self.assertEqual(signature_length, PROBE.SIZES[44][1])
                self.assertEqual(bytes(message), case["message"])
                if valid is not None:
                    ctypes.cast(output, ctypes.POINTER(ctypes.c_uint32))[0] = valid
                return return_code

            verifier.runtime = SimpleNamespace(aoem_mldsa_verify=verify)
            if expected is None:
                with self.assertRaises(ValueError):
                    verifier.verify(case)
            else:
                self.assertIs(verifier.verify(case), expected)


if __name__ == "__main__":
    unittest.main()
