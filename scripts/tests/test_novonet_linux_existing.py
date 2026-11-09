"""Offline guards for the original B-only updater; not node recovery acceptance."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / 'linux/update-novonet-existing.py'
spec = importlib.util.spec_from_file_location('novonet_existing', SOURCE)
update = importlib.util.module_from_spec(spec)
spec.loader.exec_module(update)


class ExistingGuards(unittest.TestCase):
    def records(self):
        base = {'_PID': '1', '_UID': '0', '_COMM': 'systemd', 'UNIT': update.old_unit, 'INVOCATION_ID': 'same'}
        return [dict(base, __REALTIME_TIMESTAMP='100000000',
                     MESSAGE=f'Started {update.old_unit} - {update.old_release}/bin/novovm-node.'),
                dict(base, __REALTIME_TIMESTAMP='200000000',
                     MESSAGE=f'{update.old_unit}: Deactivated successfully.')]

    def test_clean_stop_requires_actual_manager_pair(self):
        value = update.validate_stopped_journal(self.records(), 198000000)
        self.assertEqual(value['stop_unix_us'], 200000000)
        self.assertEqual(value['invocation_id'], 'same')
        for field, value in [('_PID', '99'), ('_UID', '99'), ('_COMM', 'node'),
                             ('UNIT', 'other.service'), ('INVOCATION_ID', 'different')]:
            records = self.records()
            records[1][field] = value
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                update.validate_stopped_journal(records, 198000000)

    def test_incomplete_old_or_out_of_order_stop_rejected(self):
        with self.assertRaises(RuntimeError):
            update.validate_stopped_journal([], 198000000)
        with self.assertRaises(RuntimeError):
            update.validate_stopped_journal(self.records()[:1], 198000000)
        for mtime in (99000000, 150000000, 203000000):
            with self.subTest(mtime=mtime), self.assertRaises(RuntimeError):
                update.validate_stopped_journal(self.records(), mtime)
        records = self.records()
        records.append(dict(records[0], __REALTIME_TIMESTAMP='201000000'))
        with self.assertRaises(RuntimeError):
            update.validate_stopped_journal(records, 198000000)

    def test_config_preserves_all_original_owners_and_limits(self):
        config = {'profile': 'native_execution_tick', 'key_storage': 'protected_v1',
                  'database': str(update.state / 'chain-state'), 'rpc_addr': None,
                  'udp_addr': '0.0.0.0:41050', 'library': 'old',
                  'novonet_config_file': str(update.old_work / 'network.json'),
                  'genesis': {'sentinel': 19}, 'peers': [{'key': [2, 3]}], 'batch_size': 1024}
        routing = {'bind_address': '0.0.0.0:42050', 'carrier_identity_file': 'unchanged',
                   'public_record_file': str(update.state / 'novonet-ce1-20261108/locator-B.json'),
                   'probe_control_file': 'old-command', 'fallback_after_ms': 1000, 'relay_forwarding': True}
        before = copy.deepcopy((config, routing))
        new, network = update.adapt_config(config, routing, Path('/fixed/libaoem_ffi.so'))
        self.assertEqual((config, routing), before)
        self.assertEqual({k for k in new if new[k] != config[k]}, {'library', 'novonet_config_file'})
        self.assertEqual({k for k in network if network[k] != routing[k]}, {'probe_control_file'})
        for name, value in [('database', '/new'), ('rpc_addr', '0.0.0.0:80'), ('udp_addr', '0.0.0.0:1'),
                            ('profile', 'other'), ('key_storage', 'plain'), ('novonet_config_file', '/new')]:
            changed = dict(config, **{name: value})
            with self.subTest(field=name), self.assertRaises(RuntimeError):
                update.adapt_config(changed, routing, Path('/fixed/libaoem_ffi.so'))

    def test_progress_does_not_accept_wrong_owner_chain_or_execution_failure(self):
        valid = {'chain_id': 91129655606029, 'execution_failures': 0,
                 'entry': 'original_native_execution_tick', 'head': None, 'current_height': 1}
        self.assertEqual(update.state_projection(valid)['current_height'], 1)
        for k, v in [('chain_id', 47), ('execution_failures', 1), ('entry', 'fixture')]:
            with self.subTest(field=k), self.assertRaises(RuntimeError):
                update.state_projection(dict(valid, **{k: v}))

    def test_no_existing_process_or_socket_may_be_overlooked(self):
        from subprocess import CompletedProcess
        for code in (0, 2):
            with patch.object(update.subprocess, 'run', return_value=CompletedProcess([], code)), \
                    self.assertRaises(RuntimeError):
                update.no_node_or_listener()
        with patch.object(update.subprocess, 'run', return_value=CompletedProcess([], 1)), \
                patch.object(update.subprocess, 'check_output', return_value=b'UNCONN :41050 pid=123'), \
                self.assertRaises(RuntimeError):
            update.no_node_or_listener()

    def test_redirected_paths_and_evidence_overwrite_rejected(self):
        with tempfile.TemporaryDirectory(prefix='novonet-update-guard-') as directory:
            root = Path(directory)
            (root / 'real').mkdir()
            (root / 'link').symlink_to(root / 'real', target_is_directory=True)
            with self.assertRaises(RuntimeError):
                update.no_redirect(root / 'link' / 'future-file')
            update.write(root / 'report.json', {'original': True})
            with self.assertRaises(FileExistsError):
                update.write(root / 'report.json', {'original': False})
            self.assertEqual(json.loads((root / 'report.json').read_text()), {'original': True})


if __name__ == '__main__':
    unittest.main()
