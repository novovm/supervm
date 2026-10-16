import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "observer", Path(__file__).with_name("check-agave-four-validator-reference.py"))
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)


class LoadAuditTests(unittest.TestCase):
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
