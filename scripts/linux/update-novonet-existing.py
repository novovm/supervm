"""B-only 3fa candidate updater, adapted from the original 20261108 Update-B.py.

prepare stages and qualifies a side-by-side package; update is a separate action.
No initializer, key generation, DB copying, LOCK removal or application-unit changes.
The operator must verify the candidate's required gates before invoking update.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import pwd
import signal
import subprocess
import tarfile
import time

SOURCE = '3fa11f940b18e54b46e6b927d3d66294bea37088'
ARCHIVE_PIN = 'd6ed8d869c46530077e9feb61b4ce0ef83dddcddc907f2e6d8a443825ae3e692'
MANIFEST_PIN = '3a837ed6095a826e96df42610972980287f677c59d8ba02d426961a99dae2247'
NODE_PIN = '8ae62f05146112f792b7868e170a1ae03445c783bf7964f2416bfb7ebff2184c'
OLD_NODE_PIN = 'dbdcd2f08bd0ddb83ebd0a6027cf54f8d4dee9f03a7a9ab8b988750d8f79a8b7'
CORE_PIN = 'd73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33'
incoming = Path('/home/ecs-user/novovm-sqlite-update-3fa11f94-20261110')
evidence = incoming / 'evidence'
release = Path('/opt/novovm/releases/novonet-sqlite-3fa11f94-linux-x86_64')
old_release = Path('/opt/novovm/releases/novonet-relay-evidence-2b97fc8e-linux-x86_64')
state = Path('/var/lib/novovm/selfrun/91129655606029/B')
old_work = state / 'novonet-relay-evidence-2b97fc8e-20261109'
old_progress = old_work / 'logs-retry-start-1/progress.json'
work = state / 'novonet-sqlite-3fa11f94-20261110'
old_unit = 'novovm-B-2b97fc8e-network-20261109.service'
new_unit = 'novovm-B-3fa11f94-network-20261110.service'
rollback_unit = 'novovm-B-pre3fa11f94-existing-rollback-20261110.service'
uid = gid = None


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def digest(path):
    with Path(path).open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    with Path(path).open('x', encoding='utf-8') as handle:
        json.dump(value, handle, indent=2)
        handle.flush()
        os.fsync(handle.fileno())


def no_redirect(path):
    path = Path(path).absolute()
    require(all(not p.is_symlink() for p in (path, *path.parents)), 'Redirected deployment path')
    return path


def make_directory(path, owner_uid, mode):
    path = no_redirect(path)
    path.mkdir(mode=mode)
    os.chown(path, owner_uid, gid)
    # mkdir's requested mode is reduced by the updater's private umask.
    path.chmod(mode)


def services():
    return subprocess.check_output([
        'systemctl', 'show', 'novonet-mailbox', 'supervm-relay', 'nginx', 'docker',
        '--property=Id,ActiveState,SubState,MainPID', '--no-pager']).decode()


def mainpid(unit):
    return int(subprocess.check_output(['systemctl', 'show', unit, '--value', '-p', 'MainPID']))


def custody(config, network):
    paths = [config['signing_key_file'], config['transport_key_file'], network['carrier_identity_file']]
    require(paths == [str(state / 'validator.protected'), str(state / 'transport.protected'),
                      str(state / 'novonet-ce1-20261108/carrier.protected')], 'Custody paths differ')
    return {p: digest(no_redirect(p)) for p in paths}


def state_projection(progress):
    require(progress['chain_id'] == 91129655606029 and progress['execution_failures'] == 0,
            'Original progress health differs')
    require(progress['entry'] == 'original_native_execution_tick', 'Wrong progress owner')
    return {k: progress[k] for k in ('chain_id', 'current_height', 'head')}


def adapt_config(config, network, library):
    require(config['profile'] == 'native_execution_tick' and config['key_storage'] == 'protected_v1',
            'Original protected native entry differs')
    require(config['database'] == str(state / 'chain-state') and config['rpc_addr'] is None,
            'Original database or native entry differs')
    require(config['udp_addr'] == '0.0.0.0:41050' and network['bind_address'] == '0.0.0.0:42050',
            'Original listeners differ')
    require(config['novonet_config_file'] == str(old_work / 'network.json'), 'Original network path differs')
    require(network['public_record_file'] == str(state / 'novonet-ce1-20261108/locator-B.json'),
            'Original locator owner differs')
    fresh, routing = copy.deepcopy(config), copy.deepcopy(network)
    fresh['library'] = str(library)
    fresh['novonet_config_file'] = str(work / 'network.json')
    routing['probe_control_file'] = str(work / 'commands/control.nvprobe')
    return fresh, routing


def no_node_or_listener():
    result = subprocess.run(['pgrep', '-x', 'novovm-node'], capture_output=True, text=True)
    require(result.returncode == 1, 'A native node still exists, or process inventory failed')
    sockets = subprocess.check_output(['ss', '-H', '-lunp', 'sport = :41050 or sport = :42050']).decode()
    require(not sockets.strip(), 'Original node port still owned')


def identity(pid, expected):
    require(pid > 1 and Path(f'/proc/{pid}').stat().st_uid == uid, 'Target PID owner differs')
    require(os.readlink(f'/proc/{pid}/exe') == str(expected), 'Target executable differs')
    return Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[19]


def validate_stopped_journal(records, progress_mtime_us, unit=None, root=None):
    # A collected transient unit reports MainPID=0 even without a clean exit.
    # Require its last complete manager start/stop pair and contemporaneous progress.
    unit, root = unit or old_unit, root or old_release
    starts, stops = [], []
    for row in records:
        if row.get('_PID') != '1' or row.get('_UID') != '0' or row.get('_COMM') != 'systemd' or row.get('UNIT') != unit:
            continue
        stamp = int(row['__REALTIME_TIMESTAMP'])
        if row.get('MESSAGE', '').startswith(f'Started {unit} - {root}/bin/novovm-node.'):
            starts.append((stamp, row.get('INVOCATION_ID')))
        if row.get('MESSAGE') == f'{unit}: Deactivated successfully.':
            stops.append((stamp, row.get('INVOCATION_ID')))
    require(bool(starts) and bool(stops), 'No exact manager start/clean-stop evidence')
    (start, start_id), (stop, stop_id) = max(starts), max(stops)
    require(start_id and start_id == stop_id, 'Manager invocation differs')
    require(start < stop and start <= progress_mtime_us <= stop + 2_000_000,
            'Stopped owner/progress times disagree')
    require(stop - progress_mtime_us < 30_000_000, 'Stopped progress is stale')
    return {'start_unix_us': start, 'stop_unix_us': stop, 'progress_mtime_us': progress_mtime_us,
            'invocation_id': start_id}


def owner(resume_stopped):
    pid = mainpid(old_unit)
    if not resume_stopped:
        return {'pid': pid, 'start_ticks': identity(pid, old_release / 'bin/novovm-node')}
    require(pid == 0, 'Old unit is running; stopped recovery is not applicable')
    no_node_or_listener()
    raw = subprocess.check_output(['journalctl', '-u', old_unit, '-n', '30', '--no-pager', '-o', 'json'])
    records = [json.loads(line) for line in raw.splitlines()]
    stopped = validate_stopped_journal(records, old_progress.stat().st_mtime_ns // 1000)
    return {'pid': 0, 'clean_stop': stopped, 'manager_journal': records}


def stop_exact(unit, expected, expected_hash):
    pid = mainpid(unit)
    require(digest(expected) == expected_hash, 'Target binary changed before termination')
    epoch = identity(pid, expected)
    require(mainpid(unit) == pid, 'Unit PID changed')
    # Pin the kernel process identity; a recycled numeric PID must never be signalled.
    fd = os.pidfd_open(pid)
    try:
        require(identity(pid, expected) == epoch, 'Target process changed')
        signal.pidfd_send_signal(fd, signal.SIGTERM)
        method = 'SIGTERM'
        import select
        if not select.select([fd], [], [], 10)[0]:
            signal.pidfd_send_signal(fd, signal.SIGKILL)
            method = 'SIGKILL_after_SIGTERM'
            require(bool(select.select([fd], [], [], 5)[0]), 'Target process did not exit')
    finally:
        os.close(fd)
    no_node_or_listener()
    return {'pid': pid, 'start_ticks': epoch, 'signal': method,
            'scope': 'interrupted original-database recovery; not certified graceful drain'}


def launch(unit, root, config, logs, *, continuous=False):
    no_node_or_listener()
    settings = read(config)
    routing = read(settings['novonet_config_file'])
    for path in (config, settings['novonet_config_file'], settings['signing_key_file'],
                 settings['transport_key_file'], routing['carrier_identity_file'],
                 root / 'aoem/manifest/aoem-manifest.json'):
        subprocess.run(['runuser', '-u', 'novovm', '--', 'test', '-r', str(path)], check=True)
    for path in (logs, Path(settings['database'])):
        for permission in ('-r', '-w', '-x'):
            subprocess.run(['runuser', '-u', 'novovm', '--', 'test', permission, str(path)], check=True)
    for name in ('node.stdout', 'node.stderr'):
        p = logs / name
        p.touch(exist_ok=False)
        os.chown(p, uid, gid)
        p.chmod(0o600)
    manifest = root / 'aoem/manifest/aoem-manifest.json'
    # A discovery bootstrap is an ongoing service, not a 24-hour acceptance job.
    # This controls process lifetime only, never connection/protocol deadlines.
    duration = '' if continuous else 'NOVOVM_NATIVE_EXECUTION_RUN_MS=86400000 '
    env = (f'AOEM_DLL_MANIFEST={manifest} NOVOVM_AOEM_MANIFEST={manifest} AOEM_DLL_MANIFEST_REQUIRED=1 '
           f'AOEM_SEMANTIC_GRAPH_V2_WORKERS=1 NOVOVM_NODE_MODE=native_execution_tick '
           f'NOVOVM_NATIVE_EXECUTION_CONFIG={config} NOVOVM_NATIVE_EXECUTION_START=existing '
           f'{duration}NOVOVM_PROFILE_NATIVE_DELIVERY=1 '
           f'NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH={logs / "progress.json"}')
    subprocess.run(['systemd-run', '--quiet', '--collect', '--unit=' + unit,
                    '-p', 'Type=exec', '-p', 'User=novovm', '-p', 'Group=novovm', '-p', 'UMask=0077', '-p', 'RuntimeMaxSec=infinity' if continuous else 'RuntimeMaxSec=86520',
                    '-p', 'LoadCredentialEncrypted=owner-key:/etc/novovm/credentials/node-owner-key.cred',
                    '-p', 'Environment=' + env, '-p', 'StandardOutput=append:' + str(logs / 'node.stdout'),
                    '-p', 'StandardError=append:' + str(logs / 'node.stderr'), str(root / 'bin/novovm-node')], check=True)
    return mainpid(unit)


def resume_current():
    """Resume a cleanly expired installed candidate, without another upgrade."""
    no_node_or_listener()
    check_package(release)
    config, network = read(work / 'run.json'), read(work / 'network.json')
    require(config['profile'] == 'native_execution_tick' and config['key_storage'] == 'protected_v1', 'Wrong original entry')
    require(config['database'] == str(state / 'chain-state') and config['rpc_addr'] is None, 'Wrong original database')
    require(config['novonet_config_file'] == str(work / 'network.json'), 'Wrong network config')
    require(config['udp_addr'] == '0.0.0.0:41050' and network['bind_address'] == '0.0.0.0:42050', 'Wrong listeners')
    require(config['library'] == str(release / 'aoem/linux/core/bin/libaoem_ffi.so'), 'Wrong runtime')
    keys = custody(config, network)
    original_progress = work / 'logs/progress.json'
    before = state_projection(read(original_progress))
    raw = subprocess.check_output(['journalctl', '-u', new_unit, '-n', '30', '--no-pager', '-o', 'json'])
    records = [json.loads(line) for line in raw.splitlines()]
    stopped = validate_stopped_journal(records, original_progress.stat().st_mtime_ns // 1000, new_unit, release)
    logs = work / 'logs-continuous-resume'
    receipt = incoming / 'continuous-resume'
    require(not logs.exists() and not receipt.exists(), 'Resume already attempted; inspect its original evidence')
    make_directory(receipt, 0, 0o700)
    make_directory(logs, uid, 0o700)
    write(receipt / 'before.json', {'source': SOURCE, 'clean_stop': stopped, 'journal': records,
          'projection': before, 'config_sha256': digest(work / 'run.json'), 'network_sha256': digest(work / 'network.json'),
          'custody_hashes': keys, 'continuous_process': True})
    unit = new_unit.replace('.service', '-continuous.service')
    pid = launch(unit, release, work / 'run.json', logs, continuous=True)
    identity(pid, release / 'bin/novovm-node')
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            progress = read(logs / 'progress.json')
            require(state_projection(progress) == before, 'Resumed state differs')
            require(custody(config, network) == keys, 'Custody changed')
            listeners = subprocess.check_output(['ss', '-H', '-lunp', 'sport = :41050 or sport = :42050']).decode()
            require(f'pid={pid},' in listeners and ':41050' in listeners and ':42050' in listeners, 'Original ports not ready')
            write(receipt / 'result.json', {'source': SOURCE, 'unit': unit, 'pid': pid, 'start': 'existing',
                  'projection': before, 'node_sha256': NODE_PIN, 'aoem_sha256': CORE_PIN,
                  'custody_unchanged': True, 'listeners': listeners, 'continuous_process': True})
            return
        except (FileNotFoundError, json.JSONDecodeError):
            time.sleep(0.2)
    raise RuntimeError('No resumed progress within startup observation bound; preserve the unit and logs')


def check_package(root):
    require(digest(no_redirect(root / 'release-manifest.json')) == MANIFEST_PIN, 'Manifest mismatch')
    manifest = read(root / 'release-manifest.json')
    require(manifest['git_commit'] == SOURCE and len(manifest['artifacts']) == 80, 'Candidate source/count differs')
    for entry in manifest['artifacts']:
        relative = PurePosixPath(entry['path'])
        require(not relative.is_absolute() and '..' not in relative.parts, 'Artifact escaped release')
        p = no_redirect(root / relative)
        require(p.is_file() and p.stat().st_size == entry['bytes'] and digest(p) == entry['sha256'],
                'Artifact mismatch: ' + entry['path'])
    require(digest(root / 'bin/novovm-node') == NODE_PIN, 'Node mismatch')
    library = root / 'aoem/linux/core/bin/libaoem_ffi.so'
    require(manifest['aoem_runtime']['path'] == str(library.relative_to(root)) and digest(library) == CORE_PIN,
            'AOEM mismatch')
    return library


def prepare(resume_stopped):
    config, network = read(old_work / 'run.json'), read(old_work / 'network.json')
    require(digest(old_release / 'bin/novovm-node') == OLD_NODE_PIN, 'Old binary mismatch')
    require(no_redirect(state / 'chain-state').is_dir(), 'Original state missing')
    snapshot = {'owner': owner(resume_stopped), 'services': services(), 'custody': custody(config, network),
                'config': config, 'network': network, 'progress': read(old_progress),
                'config_sha256': digest(old_work / 'run.json'), 'network_sha256': digest(old_work / 'network.json')}
    state_projection(snapshot['progress'])
    archive = incoming / (release.name + '.tar.gz')
    require(digest(no_redirect(archive)) == ARCHIVE_PIN, 'Archive mismatch')
    for p in (release, work, evidence):
        no_redirect(p)
        require(not p.exists(), 'Refuse replacing release/config/evidence')
    # Validate configuration before staging files. Existing genesis fields are copied, never invoked.
    fresh, routing = adapt_config(config, network, release / 'aoem/linux/core/bin/libaoem_ffi.so')
    evidence.mkdir(mode=0o700)
    write(evidence / 'before.json', snapshot)
    with tarfile.open(archive) as bundle:
        for entry in bundle.getmembers():
            path = PurePosixPath(entry.name)
            require(path.parts and path.parts[0] == release.name and not path.is_absolute()
                    and '..' not in path.parts and (entry.isfile() or entry.isdir()), 'Invalid archive member')
        bundle.extractall(release.parent, filter='data')
    for p in (release, *release.rglob('*')):
        no_redirect(p)
        os.chown(p, 0, 0)
        p.chmod(0o755 if p.is_dir() or p.parent == release / 'bin' else 0o644)
    library = check_package(release)
    env = os.environ.copy()
    manifest = str(release / 'aoem/manifest/aoem-manifest.json')
    env.update(AOEM_DLL_MANIFEST=manifest, NOVOVM_AOEM_MANIFEST=manifest, AOEM_DLL_MANIFEST_REQUIRED='1')
    result = subprocess.run([str(release / 'bin/novovmctl'), 'native-identity', 'qualify-runtime',
                             '--library', str(library), '--library-sha256', CORE_PIN],
                            capture_output=True, env=env, timeout=120)
    (evidence / 'runtime.stdout').write_bytes(result.stdout)
    (evidence / 'runtime.stderr').write_bytes(result.stderr)
    require(result.returncode == 0, 'Installed runtime qualification failed')
    runtime = json.loads(result.stdout)
    require(runtime['ok'] and os.path.samefile(runtime['data']['library'], library), 'Actual library differs')
    require(all(runtime['data'][k] is True for k in ('verification_qualified', 'scalar_verification_kat_passed',
                'unified_semantic_verification_kat_passed', 'mixed_batch_verification_kat_passed')), 'Runtime KAT failed')
    make_directory(work, 0, 0o750)
    for name, owner_uid, mode in [('logs', uid, 0o700), ('commands', 0, 0o750)]:
        make_directory(work / name, owner_uid, mode)
    for name, value in [('run.json', fresh), ('network.json', routing)]:
        write(work / name, value)
        os.chown(work / name, 0, gid)
        (work / name).chmod(0o440)
    require(snapshot['custody'] == custody(config, network), 'Custody changed during preparation')
    write(evidence / 'prepared.json', {'source': SOURCE, 'node_sha256': NODE_PIN, 'aoem_sha256': CORE_PIN,
          'artifacts_verified': 80, 'database': fresh['database'], 'start': 'existing',
          'resume_stopped': resume_stopped, 'initialize_executed': False, 'node_started': False,
          'config_sha256': digest(work / 'run.json'), 'network_sha256': digest(work / 'network.json')})
    print('PASS B staged package/runtime; no node started, no database opened')


def recover_old(before, suffix=''):
    require(before['custody'] == custody(before['config'], before['network']), 'Custody changed; no automatic rollback')
    require(digest(old_release / 'bin/novovm-node') == OLD_NODE_PIN, 'Original rollback binary differs')
    require(digest(old_work / 'run.json') == before['config_sha256']
            and digest(old_work / 'network.json') == before['network_sha256'], 'Original rollback config changed')
    logs = no_redirect(work / ('rollback-logs' + suffix))
    require(not logs.exists() and not (evidence / 'rollback-requested.json').exists(),
            'Rollback already attempted; preserve evidence before stopping any node')
    pid = mainpid(new_unit)
    if pid:
        stop_exact(new_unit, release / 'bin/novovm-node', NODE_PIN)
    no_node_or_listener()
    make_directory(logs, uid, 0o700)
    pid = launch(rollback_unit, old_release, old_work / 'run.json', logs)
    write(evidence / 'rollback-requested.json', {'pid': pid, 'start': 'existing', 'initialize_executed': False,
          'scope': 'restart requested; original logs and authenticated Status must still be checked'})


def update(resume_stopped, suffix=''):
    before, prepared = read(evidence / 'before.json'), read(evidence / 'prepared.json')
    require(prepared['source'] == SOURCE and prepared['resume_stopped'] == resume_stopped, 'Preparation mode differs')
    require(not (evidence / 'old-exit.json').exists(), 'Cutover already attempted; preserve evidence')
    check_package(release)
    require(digest(old_release / 'bin/novovm-node') == OLD_NODE_PIN, 'Old binary changed')
    for root, pins in [(old_work, before), (work, prepared)]:
        require(digest(root / 'run.json') == pins['config_sha256']
                and digest(root / 'network.json') == pins['network_sha256'], 'Config changed since preparation')
    require(before['custody'] == custody(before['config'], before['network']),
            'Custody changed before cutover')
    current_owner = owner(resume_stopped)
    require(current_owner == before['owner'], 'Original owner changed since preparation')
    require(not (work / 'commands/control.nvprobe').exists(), 'Unexpected probe command; refuse activation')
    stop = current_owner if resume_stopped else stop_exact(old_unit, old_release / 'bin/novovm-node', OLD_NODE_PIN)
    write(evidence / 'old-exit.json', stop)
    logs = work / ('logs' + suffix)
    try:
        pid = launch(new_unit, release, work / 'run.json', logs)
        identity(pid, release / 'bin/novovm-node')
        for _ in range(120):
            require(mainpid(new_unit) == pid and Path(f'/proc/{pid}').exists(), 'New node exited during startup')
            if (logs / 'progress.json').exists():
                break
            time.sleep(.5)
        progress = read(logs / 'progress.json')
        require(state_projection(progress) == state_projection(before['progress']), 'Recovered head/height differs')
        probe = progress['native_network_probe']
        require(probe['enabled'] and len(probe['transport']['udp_recovery_event_names']) == 22,
                'Original UDP observation unavailable')
        require(all(k in probe['transport'] for k in ('relay_circuits', 'relay_evicted_records', 'relay_omitted_observations')),
                'Original relay observation unavailable')
        require(not probe['events'], 'Probe executed unexpectedly')
        require(before['custody'] == custody(before['config'], before['network']),
                'Custody post-check mismatch')
        # Internal application services may change during development; observe, do not veto cutover.
        services_after = services()
        library = release / 'aoem/linux/core/bin/libaoem_ffi.so'
        require(str(library) in Path(f'/proc/{pid}/maps').read_text(), 'Pinned AOEM not loaded')
        listeners = subprocess.check_output(['ss', '-H', '-lunp', 'sport = :41050 or sport = :42050']).decode()
        require(all(f':{port} ' in listeners for port in (41050, 42050)) and f'pid={pid},' in listeners,
                'Original listeners unavailable')
        write(evidence / 'started.json', {'pid': pid, 'uid': uid, 'source': SOURCE, 'node_sha256': NODE_PIN,
              'aoem_sha256': CORE_PIN, 'database': str(state / 'chain-state'), 'start': 'existing',
              'initialize_executed': False, 'custody_unchanged': True,
              'other_services_unchanged': before['services'] == services_after,
              'other_services_after': services_after, 'other_service_continuity_required': False,
              'progress': progress, 'listeners': listeners, 'external_authenticated_status_required': True})
        print('PASS B existing startup; authenticated Status still required before A update')
    except Exception as error:
        write(evidence / 'startup-failure.json', {'error': str(error)})
        recover_old(before, suffix)
        raise


def retry_failed_start():
    global evidence
    original = evidence
    before, prepared = read(original / 'before.json'), read(original / 'prepared.json')
    require(prepared['source'] == SOURCE and prepared['resume_stopped'], 'Wrong failed candidate')
    require(read(original / 'startup-failure.json')['error'] == 'Target PID owner differs',
            'Not the inspected failed startup')
    rollback = read(original / 'rollback-requested.json')
    require(rollback['start'] == 'existing' and rollback['initialize_executed'] is False,
            'Rollback history differs')
    no_node_or_listener()
    require(mainpid(new_unit) == 0 and mainpid(rollback_unit) == 0, 'A previous launch is still active')
    check_package(release)
    require(before['custody'] == custody(before['config'], before['network']),
            'Custody changed since failed startup')
    for root, pins in ((old_work, before), (work, prepared)):
        require(digest(root / 'run.json') == pins['config_sha256']
                and digest(root / 'network.json') == pins['network_sha256'], 'Prepared config changed')
    permissions = {}
    for path in (work, work / 'commands'):
        info = no_redirect(path).stat()
        require(path.is_dir() and info.st_uid == 0 and info.st_gid == gid
                and info.st_mode & 0o7777 == 0o700, 'Not the inspected umask-reduced directory')
        permissions[str(path)] = {'before': '0700', 'after': '0750', 'uid': 0, 'gid': gid}
    attempt = no_redirect(incoming / 'evidence-retry-start-1')
    require(not attempt.exists() and not (work / 'logs-retry-start-1').exists(), 'Retry already attempted')
    make_directory(attempt, 0, 0o700)
    write(attempt / 'before.json', before)
    write(attempt / 'prepared.json', prepared)
    write(attempt / 'permission-repair.json', {'original_failure_evidence': str(original),
          'directories': permissions, 'database_permissions_changed': False, 'keys_changed': False})
    for path in (work, work / 'commands'):
        path.chmod(0o750)
    make_directory(work / 'logs-retry-start-1', uid, 0o700)
    evidence = attempt
    update(True, '-retry-start-1')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['prepare', 'update', 'rollback', 'retry-failed-start', 'resume-current'])
    parser.add_argument('--resume-stopped', action='store_true')
    args = parser.parse_args()
    require(os.getuid() == 0, 'Root required for this approved single-node updater')
    global uid, gid, evidence
    account = pwd.getpwnam('novovm')
    uid, gid = account.pw_uid, account.pw_gid
    os.umask(0o077)
    for path in (incoming, state, old_release, old_work):
        no_redirect(path)
    if args.mode == 'resume-current':
        resume_current()
    elif args.mode == 'prepare':
        prepare(args.resume_stopped)
    elif args.mode == 'update':
        update(args.resume_stopped)
    elif args.mode == 'retry-failed-start':
        require(args.resume_stopped, 'Failed stopped-owner retry must be explicit')
        retry_failed_start()
    else:
        retry = no_redirect(incoming / 'evidence-retry-start-1')
        if (retry / 'started.json').exists():
            started = read(retry / 'started.json')
            require(started['source'] == SOURCE and started['node_sha256'] == NODE_PIN,
                    'Successful retry source differs')
            evidence = retry
            recover_old(read(evidence / 'before.json'), '-after-retry-start-1')
        else:
            recover_old(read(evidence / 'before.json'))


if __name__ == '__main__':
    main()
