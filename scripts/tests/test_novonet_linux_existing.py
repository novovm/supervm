"""Offline guards for the original B-only updater; not node recovery acceptance."""
import copy
import importlib.util
import json
import os
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

    def test_actual_directory_modes_survive_private_umask(self):
        with tempfile.TemporaryDirectory(prefix='novonet-umask-') as directory:
            root = Path(directory)
            previous = os.umask(0o077)
            try:
                # This is the former implementation's actual filesystem failure.
                (root / 'old-work').mkdir(mode=0o750)
                self.assertEqual((root / 'old-work').stat().st_mode & 0o777, 0o700)
                with patch.object(update, 'gid', os.getgid()):
                    for name, mode in (('work', 0o750), ('commands', 0o750), ('logs', 0o700)):
                        path = root / name
                        update.make_directory(path, os.getuid(), mode)
                        self.assertEqual(path.stat().st_mode & 0o777, mode)
                        self.assertEqual(path.stat().st_uid, os.getuid())
                        with self.assertRaises(FileExistsError):
                            update.make_directory(path, os.getuid(), 0o777)
                        self.assertEqual(path.stat().st_mode & 0o777, mode)
            finally:
                os.umask(previous)

    def test_launch_checks_service_account_access_before_exec_ready_systemd_start(self):
        from subprocess import CalledProcessError, CompletedProcess
        with tempfile.TemporaryDirectory(prefix='novonet-launch-') as directory:
            root = Path(directory)
            config, network, logs = root / 'run.json', root / 'network.json', root / 'logs'
            logs.mkdir()
            config.write_text(json.dumps({'novonet_config_file': str(network), 'database': str(root / 'db'),
                                         'signing_key_file': 'validator.protected',
                                         'transport_key_file': 'transport.protected'}))
            network.write_text(json.dumps({'carrier_identity_file': 'carrier.protected'}))
            with patch.object(update, 'no_node_or_listener'), patch.object(update, 'uid', os.getuid()), \
                    patch.object(update, 'gid', os.getgid()), patch.object(update, 'mainpid', return_value=123), \
                    patch.object(update.subprocess, 'run', return_value=CompletedProcess([], 0)) as run:
                self.assertEqual(update.launch('target.service', root, config, logs), 123)
                commands = [call.args[0] for call in run.call_args_list]
                self.assertTrue(all(command[:4] == ['runuser', '-u', 'novovm', '--']
                                    for command in commands[:-1]))
                self.assertEqual(len(commands[:-1]), 12)
                self.assertEqual(commands[-1][0], 'systemd-run')
                self.assertIn('Type=exec', commands[-1])
                self.assertIn('User=novovm', commands[-1])
                self.assertIn('NOVOVM_NATIVE_EXECUTION_START=existing', ' '.join(commands[-1]))
                self.assertNotIn('initialize', ' '.join(commands[-1]))
            with patch.object(update, 'no_node_or_listener'), \
                    patch.object(update.subprocess, 'run', side_effect=CalledProcessError(1, 'runuser')) as run:
                with self.assertRaises(CalledProcessError):
                    update.launch('must-not-start.service', root, config, logs)
                self.assertEqual(run.call_count, 1)

    def test_retry_requires_inspected_failure_and_same_candidate(self):
        for prepared, failure in (({'source': 'different', 'resume_stopped': True}, {}),
                                  ({'source': update.SOURCE, 'resume_stopped': False}, {}),
                                  ({'source': update.SOURCE, 'resume_stopped': True}, {'error': 'other failure'})):
            with self.subTest(prepared=prepared, failure=failure), \
                    patch.object(update, 'read', side_effect=[{}, prepared, failure]), \
                    patch.object(update, 'no_node_or_listener') as inventory, \
                    patch.object(update, 'make_directory') as create:
                with self.assertRaises(RuntimeError):
                    update.retry_failed_start()
                inventory.assert_not_called()
                create.assert_not_called()

    def test_retry_never_bypasses_existing_process_or_port(self):
        prepared = {'source': update.SOURCE, 'resume_stopped': True}
        records = [{}, prepared, {'error': 'Target PID owner differs'},
                   {'start': 'existing', 'initialize_executed': False}]
        with patch.object(update, 'read', side_effect=records), \
                patch.object(update, 'no_node_or_listener', side_effect=RuntimeError('live node')), \
                patch.object(update, 'make_directory') as create:
            with self.assertRaisesRegex(RuntimeError, 'live node'):
                update.retry_failed_start()
            create.assert_not_called()

    def test_repeated_rollback_is_rejected_before_stopping_a_live_node(self):
        with tempfile.TemporaryDirectory(prefix='novonet-rollback-') as directory:
            root = Path(directory)
            (root / 'rollback-logs').mkdir()
            before = {'custody': {}, 'config': {}, 'network': {}, 'config_sha256': 'fixed',
                      'network_sha256': 'fixed'}
            with patch.object(update, 'work', root), patch.object(update, 'evidence', root), \
                    patch.object(update, 'custody', return_value={}), \
                    patch.object(update, 'digest', side_effect=[update.OLD_NODE_PIN, 'fixed', 'fixed']), \
                    patch.object(update, 'mainpid') as pid, patch.object(update, 'stop_exact') as stop:
                with self.assertRaisesRegex(RuntimeError, 'Rollback already attempted'):
                    update.recover_old(before)
                pid.assert_not_called()
                stop.assert_not_called()


if __name__ == '__main__':
    unittest.main()
