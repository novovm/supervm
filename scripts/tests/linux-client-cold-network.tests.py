"""Offline contract checks for the cold network witness. No unit or network access."""
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("linux-client-cold-network.py")
CHECK = runpy.run_path(str(SCRIPT))


class ColdNetwork(unittest.TestCase):
    def setUp(self):
        self.identity = "1" * 64
        self.args = SimpleNamespace(publisher="2" * 64, state_root="3" * 64, height=1, nodes=1, leaves=1)
        self.cfg = {"genesis": [4] * 32, "protocol": [5] * 32, "chain_id": 123,
                    "seeds": [{"contact": {"a": 1}, "transport_key": [i]} for i in range(2)],
                    "verification": {"trusted_parent": {"height": 0}}}
        self.start = dict(event="starting", reopened_height=0, transport_identity_sha256=self.identity)
        peer = dict(peer_transport_sha256=self.args.publisher, transport_identity_sha256=self.identity,
                    locator_sequence=7)
        self.locator = dict(peer, event="verified_locator_accepted")
        self.state = dict(peer, event="verified_state", height=1, transport="iroh",
                          verified_nodes=1, downloaded_nodes=1, verified_leaves=1,
                          state_root="0x" + self.args.state_root)

    def test_complete_witness_and_partial_progress(self):
        verify = CHECK["verified_event"]
        self.assertIsNone(verify([self.start, self.locator], self.args))
        self.assertEqual(verify([self.start, self.locator, self.state], self.args), self.state)

    def test_dual_source_requires_the_locator_used_for_verified_state(self):
        verify = CHECK["verified_event"]
        sources = ["a" * 64, "b" * 64]
        matching = dict(self.locator, locator_returned_by=sources[::-1])
        self.assertEqual(verify([self.start, matching, self.state], self.args, sources), self.state)
        for attributed in (sources[:1], sources[:1] * 2, [sources[0], "c" * 64]):
            with self.subTest(attributed=attributed):
                locator = dict(self.locator, locator_returned_by=attributed)
                other_sequence = dict(matching, locator_sequence=8)
                trace = [self.start, locator, other_sequence, self.state]
                self.assertIsNone(verify(trace, self.args, sources))

    def test_single_source_requires_the_exact_attribution_used_for_sync(self):
        verify = CHECK["verified_event"]
        sources = ["a" * 64, "b" * 64]
        for source, other in (sources, sources[::-1]):
            matching = dict(self.locator, locator_returned_by=[source])
            self.assertEqual(verify([self.start, matching, self.state], self.args, [source], sources),
                             self.state)
            for attributed in ([other], sources, [source, source], []):
                with self.subTest(source=source, attributed=attributed):
                    locator = dict(self.locator, locator_returned_by=attributed)
                    other_sequence = dict(matching, locator_sequence=8)
                    trace = [self.start, locator, other_sequence, self.state]
                    self.assertIsNone(verify(trace, self.args, [source], sources))
        infrastructure_start = dict(self.start, transport_identity_sha256=sources[1])
        with self.assertRaisesRegex(ValueError, "reused a bootstrap identity"):
            verify([infrastructure_start], self.args, sources[:1], sources)

    def test_reject_changed_identity_reused_state_or_unbound_route(self):
        for changes in ({"transport_identity_sha256": "4" * 64}, {"downloaded_nodes": 0},
                        {"state_root": "0x" + "0" * 64}, {"locator_sequence": 8},
                        {"transport": "native_novorudp"}):
            with self.subTest(fields=changes), self.assertRaises(ValueError):
                CHECK["verified_event"]([self.start, self.locator, dict(self.state, **changes)], self.args)
        with self.assertRaises(ValueError):
            CHECK["verified_event"]([self.start, self.state, self.locator], self.args)

    def test_reject_provider_pins_and_duplicate_bootstraps(self):
        check = CHECK["check_profile"]
        sources = check(self.cfg, self.args.publisher)
        for invalid in (dict(self.cfg, seeds=self.cfg["seeds"][:1] * 2),
                        dict(self.cfg, lookup_transport_keys=[[2]]),
                        dict(self.cfg, native_query_peers=["localhost"])):
            with self.assertRaises(ValueError):
                check(invalid, self.args.publisher)
        with self.assertRaises(ValueError):
            check(self.cfg, sources[0])

    def test_disk_requires_all_heights_identity_and_root(self):
        with tempfile.TemporaryDirectory() as folder:
            state = Path(folder)
            name = "123-0x" + "04" * 32 + "-0x" + "05" * 32
            domain = state / "NOVOVM/native-discovery" / name
            root = domain / "headers-0-fixture"
            nodes = root / "state-nodes"
            nodes.mkdir(parents=True)
            (domain / "transport.sha256").write_text(self.identity)
            (nodes / ("0x" + self.args.state_root + ".node")).write_bytes(b"offline file")
            (root / "00000000000000000001.proof").write_bytes(b"offline file")
            evidence = CHECK["disk_evidence"](state, self.cfg, self.args, self.identity)
            self.assertEqual((evidence["headers"], evidence["nodes"]), (1, 1))
            with self.assertRaises(ValueError):
                CHECK["disk_evidence"](state, self.cfg, self.args, self.args.publisher)
            (root / "00000000000000000001.proof").rename(root / "00000000000000000002.proof")
            with self.assertRaises(ValueError):
                CHECK["disk_evidence"](state, self.cfg, self.args, self.identity)

    @unittest.skipUnless(sys.platform == "linux", "Linux UID semantics")
    def test_cleanup_refuses_substituted_unit_without_touching_it(self):
        absent = dict(LoadState="not-found", ActiveState="inactive", FragmentPath="", DropInPaths="")
        with tempfile.TemporaryDirectory() as folder:
            unit = Path(folder) / CHECK["UNIT"]
            unit.write_bytes(b"foreign")
            cleanup = CHECK["cleanup"]
            with patch.dict(cleanup.__globals__, properties=lambda: absent), self.assertRaises(ValueError):
                cleanup(unit, unit.with_suffix(".link"), b"expected", None)
            self.assertEqual(unit.read_bytes(), b"foreign")

    def test_incomplete_arguments_fail_before_systemd_access(self):
        cmd = subprocess.run([sys.executable, SCRIPT, "--height", "0"], capture_output=True, text=True)
        self.assertEqual(cmd.returncode, 2)
        self.assertIn("positive integer", cmd.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
