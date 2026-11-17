"""Management lifecycle regression only; not network or existing-DB acceptance."""
import importlib.util
import json
from pathlib import Path
import tempfile
import types
import unittest

path = Path(__file__).resolve().parents[2] / 'artifacts/audit/novonet-concurrent-recovery-20261117/B-Scenario.py'
spec = importlib.util.spec_from_file_location('scenario', path)
scenario = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scenario)


class RestoreTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='transient-owner-contract-')
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        scenario.WORK = self.work
        self.logs = self.work / 'logs'
        self.logs.mkdir()
        for name, value in [('run.json', {}), ('network.json', {}), ('logs/progress.json', {'height': 1})]:
            (self.work / name).write_text(json.dumps(value))
        self.before = dict(pid=71, config_sha256=scenario.digest(self.work / 'run.json'),
                           network_sha256=scenario.digest(self.work / 'network.json'))
        self.calls = []
        self.lifecycle = types.SimpleNamespace(
            uid=996, no_node_or_listener=lambda: self.calls.append('no-owner'),
            check_package=lambda p: self.calls.append('package'), read=lambda p: json.loads(p.read_text()),
            custody=lambda c, n: {'unchanged': True}, no_redirect=lambda p: p,
            state_projection=lambda p: p,
            make_directory=lambda p, uid, mode: p.mkdir(mode=mode),
            launch=self.launch, identity=lambda pid, exe: self.calls.append(('identity', pid, exe)))

    def launch(self, unit, release, config, logs, *, continuous):
        self.calls.append(('launch', unit, release, config, logs, continuous))
        return 72

    def fail(self):
        raise RuntimeError('injected manager boundary failure')

    def test_recreates_original_unit_and_preserves_old_logs(self):
        r = scenario.restore_original_owner(self.lifecycle, self.before)
        self.assertEqual(r['pid'], 72)
        self.assertEqual(json.loads((self.work / 'logs-before-interruption-71/progress.json').read_text()), {'height': 1})
        self.assertFalse((self.logs / 'progress.json').exists())
        launch = next(c for c in self.calls if isinstance(c, tuple) and c[0] == 'launch')
        self.assertEqual(launch[1:], (scenario.UNIT, scenario.RELEASE, self.work / 'run.json', self.logs, True))

    def test_changed_config_is_rejected_before_log_move(self):
        (self.work / 'run.json').write_text('{"changed":true}')
        with self.assertRaises(AssertionError):
            scenario.restore_original_owner(self.lifecycle, self.before)
        self.assertTrue((self.logs / 'progress.json').exists())

    def test_existing_archive_is_never_overwritten(self):
        (self.work / 'logs-before-interruption-71').mkdir()
        with self.assertRaises(AssertionError):
            scenario.restore_original_owner(self.lifecycle, self.before)
        self.assertTrue((self.logs / 'progress.json').exists())

    def test_live_owner_prevents_mutation(self):
        self.lifecycle.no_node_or_listener = self.fail
        with self.assertRaises(RuntimeError):
            scenario.restore_original_owner(self.lifecycle, self.before)
        self.assertTrue((self.logs / 'progress.json').exists())

    def test_failed_package_prevents_mutation(self):
        self.lifecycle.check_package = lambda p: self.fail()
        with self.assertRaises(RuntimeError):
            scenario.restore_original_owner(self.lifecycle, self.before)
        self.assertTrue((self.logs / 'progress.json').exists())

    def test_failed_launch_retains_evidence_without_retry(self):
        def fail_launch(*args, **kwargs):
            self.calls.append('failed-launch')
            self.fail()
        self.lifecycle.launch = fail_launch
        with self.assertRaises(RuntimeError):
            scenario.restore_original_owner(self.lifecycle, self.before)
        self.assertEqual(self.calls.count('failed-launch'), 1)
        self.assertTrue((self.work / 'logs-before-interruption-71/progress.json').exists())


if __name__ == '__main__':
    unittest.main()
