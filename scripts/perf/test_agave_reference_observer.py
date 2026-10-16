import copy
import json
import tempfile
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "observer", Path(__file__).with_name("check-agave-four-validator-reference.py"))
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)


class LoadAuditTests(unittest.TestCase):
    def test_actual_generator_manifest_requires_completion(self):
        row = {"signature": "1" * 64, "boundary": "before_client_queue",
               "queue_attempt_elapsed_ns": 1, "queue_attempt_unix_ns": 2}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "attempts.jsonl"
            path.write_text(json.dumps(row) + "\n")
            with self.assertRaises(ValueError):
                observer.read_manifest(path)
            path.write_text(json.dumps(row) + '\n{"generated_complete":1}\n')
            self.assertEqual(observer.read_manifest(path), {"1" * 64})

    def test_manifest_rejects_ambiguous_denominator(self):
        signature = "1" * 64
        self.assertEqual(observer.submission_signatures({"signatures": [signature]}), {signature})
        for values in [[], [signature, signature], ["0" * 64], ["1" * 65], [None]]:
            with self.subTest(values=values), self.assertRaises(ValueError):
                observer.submission_signatures({"signatures": values})

    def test_missing_submission_is_not_success_or_declared_lost(self):
        result = observer.reconcile_submissions({"ok", "failed", "pending"},
                                                {"ok": [], "failed": []})
        self.assertEqual(result["unconfirmed_signatures"], ["pending"])
        self.assertFalse(result["all_submitted_finalized"])
        self.assertEqual(result["submitted_unique"], 3)
        with self.assertRaises(RuntimeError):
            observer.reconcile_submissions({"ok"}, {"unexpected": []})
        self.assertTrue(observer.reconcile_submissions({"ok"}, {"ok": []})[
            "all_submitted_finalized"])

    def check(self, mode="normal"):
        def rpc(port, method, params):
            if method == "getBlocks":
                return [5]
            rows = []
            for signature, payer, error in [("ok", "payer", None),
                                             ("failed", "payer", {"InstructionError": [0, 1]}),
                                             ("vote", "validator", None)]:
                rows.append({"transaction": {"signatures": [signature],
                             "message": {"accountKeys": [payer]}},
                             "meta": {"err": error, "fee": 5,
                                      "preBalances": [100, 10], "postBalances": [95, 10]}})
            if mode == "mismatch" and port == 4:
                rows[0]["meta"]["err"] = "different"
            if mode == "bad_fee":
                rows[0]["meta"]["postBalances"][0] = 96
            if mode == "duplicate":
                rows.append(copy.deepcopy(rows[0]))
            if mode == "empty":
                rows = []
            return {"blockhash": "same", "transactions": rows}
        with patch.object(observer, "rpc", rpc):
            return observer.audit_load([1, 2, 3, 4], 5, 5, {"payer"})

    def test_success_and_failure_separate_votes_excluded(self):
        result = self.check()
        self.assertEqual(result["unique_finalized_success"], 1)
        self.assertEqual(result["unique_finalized_failure"], 1)
        self.assertEqual(result["fees_lamports"], 10)

    def test_rejects_invalid_evidence(self):
        for mode in ["mismatch", "bad_fee", "duplicate", "empty"]:
            with self.subTest(mode=mode), self.assertRaises(RuntimeError):
                self.check(mode)


if __name__ == "__main__":
    unittest.main()
