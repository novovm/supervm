import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "timing", Path(__file__).with_name("measure-agave-fixed-load.py"))
timing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(timing)


class TimingTests(unittest.TestCase):
    def test_slowest_validator_and_success_only(self):
        attempts = {"ok": 1_000_000_000, "fail": 2_000_000_000}
        observations = {
            "ok": {p: (p * 1_000_000_000, 10, None) for p in range(2, 6)},
            "fail": {p: (6_000_000_000, 11, "failure") for p in range(2, 6)}}
        result = timing.summary(attempts, observations)
        self.assertEqual(result["successful_tps"], .2)
        self.assertEqual(result["success_latency_p99_seconds"], 4)
        self.assertEqual(result["business_failed"], 1)
        del observations["ok"][5]
        self.assertIsNone(timing.summary(attempts, observations)["successful_tps"])

    def test_rejects_disagreement_and_reversed_time(self):
        observations = {"x": {p: (3, 1, None) for p in range(4)}}
        with self.assertRaises(ValueError):
            timing.summary({"x": 4}, observations)
        observations["x"][3] = (3, 2, None)
        with self.assertRaises(ValueError):
            timing.summary({"x": 1}, observations)

    def test_empty_is_not_a_measurement(self):
        self.assertIsNone(timing.summary({}, {})["successful_tps"])


if __name__ == "__main__":
    unittest.main()
