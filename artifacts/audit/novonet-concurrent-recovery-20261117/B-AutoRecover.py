"""One supervised exit of the existing fixed B owner. No start/retry command."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import pwd
import select
import signal
import subprocess
import time

ROOT = Path(__file__).parent
DEPLOY = Path('/home/ecs-user/novovm-common-55cafb07-20261118/Deploy-B.py')
POLICY = '[Unit]\nStartLimitIntervalSec=60s\nStartLimitBurst=3\n[Service]\nRestart=always\nRestartSec=5s\n'
KEYS = ('Id', 'FragmentPath', 'DropInPaths', 'Transient', 'Type', 'User', 'ActiveState',
        'SubState', 'MainPID', 'InvocationID', 'Restart', 'RestartUSec',
        'StartLimitIntervalUSec', 'StartLimitBurst', 'NRestarts',
        'ExecMainStartTimestampMonotonic', 'ExecMainExitTimestampMonotonic')


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def manager(unit):
    raw = subprocess.check_output(['systemctl', 'show', unit,
                                   '-p', ','.join(KEYS)], text=True, timeout=5)
    return dict(line.split('=', 1) for line in raw.splitlines())


def has_policy(state, policy):
    return (state['DropInPaths'].split() == [str(policy)] and
            tuple(state[k] for k in ('Restart', 'RestartUSec', 'StartLimitIntervalUSec',
                                     'StartLimitBurst')) == ('always', '5s', '1min', '3'))


def poll_restart(scenario, unit, before, fd, signalled, old_progress_mtime_ns):
    require(bool(select.select([fd], [], [], 10)[0]), 'Pinned process did not exit; no repeat or escalation')
    exit_observed = time.monotonic_ns() // 1000
    deadline = time.monotonic() + 30
    exit_us = None
    while time.monotonic() < deadline:
        state = manager(unit)
        stamp = int(state['ExecMainExitTimestampMonotonic'])
        if state['InvocationID'] == before['InvocationID'] and stamp >= signalled:
            exit_us = stamp
        new = int(state['MainPID'])
        if new > 1 and new != int(before['MainPID']) and state['ActiveState'] == 'active':
            require(exit_us is not None, 'Missing original manager process-exit timestamp')
            require(state['InvocationID'] != before['InvocationID'] and
                    int(state['NRestarts']) == int(before['NRestarts']) + 1,
                    'Not exactly one automatic restart by the original manager')
            started = int(state['ExecMainStartTimestampMonotonic'])
            require(started - exit_us >= 5_000_000, 'Process gap shorter than configured delay')
            try:
                pid, progress = scenario.owner()
                require(pid == new, 'Unit changed during health check')
                if (scenario.WORK / 'logs/progress.json').stat().st_mtime_ns > old_progress_mtime_ns:
                    return pid, progress, state, exit_us, started, exit_observed
            except (AssertionError, FileNotFoundError):
                pass
        time.sleep(.1)
    raise RuntimeError('No healthy automatic existing restart within bound; no manual retry')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['enable', 'trigger'])
    args = parser.parse_args()
    require(os.getuid() == 0 and __debug__, 'Root and non-optimized owner checks required')
    scenario = load('original_b_scenario', ROOT / 'B-Scenario.py')
    require(scenario.digest(DEPLOY) == 'd3367dc359af72485be03c9a7865881b39604dfca924015388b6eb3f3d6bfb05',
            'Original fixed deployment adapter differs')
    lifecycle = load('original_b_deploy', DEPLOY).owner
    account = pwd.getpwnam('novovm')
    lifecycle.uid, lifecycle.gid = account.pw_uid, account.pw_gid
    state = manager(scenario.UNIT)
    pid, progress = scenario.owner()
    require(state['MainPID'] == str(pid) and state['ActiveState'] == 'active' and
            state['Transient'] == 'yes' and state['Type'] == 'exec' and state['User'] == 'novovm' and
            state['FragmentPath'] == f'/run/systemd/transient/{scenario.UNIT}',
            'Original active transient owner differs')
    policy = lifecycle.no_redirect(Path('/run/systemd/system') / f'{scenario.UNIT}.d/90-novonet-auto-recovery.conf')
    if args.action == 'enable':
        require(state['Restart'] == 'no' and not state['DropInPaths'] and not policy.parent.exists(),
                'Existing recovery policy or drop-in; do not overwrite')
        policy.parent.mkdir(mode=0o755)
        with policy.open('x', encoding='utf-8') as stream:
            stream.write(POLICY)
        try:
            subprocess.run(['systemctl', 'daemon-reload'], check=True, timeout=15)
            after = manager(scenario.UNIT)
            require(has_policy(after, policy) and after['ActiveState'] == 'active' and
                    all(after[k] == state[k] for k in ('MainPID', 'InvocationID',
                                                       'ExecMainStartTimestampMonotonic')),
                    'Policy did not attach to the same live owner')
        except Exception:
            policy.unlink()
            policy.parent.rmdir()
            subprocess.run(['systemctl', 'daemon-reload'], check=True, timeout=15)
            raise
        print(json.dumps({'before': state, 'after': after, 'node_restarted': False,
                          'scope': 'current boot, original B unit only'}))
        return

    require(policy.read_text() == POLICY and has_policy(state, policy), 'Original B auto-restart policy not loaded')
    require(not progress['novonet']['diagnostic_relay']['active'] and
            not progress['novonet']['probe_control']['release_pending'], 'B local diagnostic still busy')
    work = scenario.WORK
    config, network = work / 'run.json', work / 'network.json'
    settings, routing = lifecycle.read(config), lifecycle.read(network)
    require(settings['database'] == str(lifecycle.state / 'chain-state'), 'Original database path differs')
    keys = lifecycle.custody(settings, routing)
    pins = [scenario.digest(config), scenario.digest(network)]
    projection = lifecycle.state_projection(progress)
    services = lifecycle.services()
    epoch = lifecycle.identity(pid, scenario.RELEASE / 'bin/novovm-node')
    evidence = ROOT / 'auto-restart-evidence'
    evidence.mkdir(mode=0o700)  # exclusive one-attempt guard before touching the process
    lifecycle.write(evidence / 'before.json', {'unit': state, 'pid_start_ticks': epoch, 'projection': projection})
    fd = os.pidfd_open(pid)
    try:
        require(lifecycle.identity(pid, scenario.RELEASE / 'bin/novovm-node') == epoch and
                lifecycle.mainpid(scenario.UNIT) == pid, 'Pinned original process changed')
        old_progress_mtime_ns = (work / 'logs/progress.json').stat().st_mtime_ns
        signal_wall_ns = time.time_ns()
        signalled = time.monotonic_ns() // 1000
        signal.pidfd_send_signal(fd, signal.SIGTERM)
        new, progress, after, exit_us, start_us, detected = poll_restart(
            scenario, scenario.UNIT, state, fd, signalled, max(old_progress_mtime_ns, signal_wall_ns))
    finally:
        os.close(fd)
    healthy_us = time.monotonic_ns() // 1000
    require(lifecycle.state_projection(progress) == projection, 'Original state projection differs')
    require([scenario.digest(config), scenario.digest(network)] == pins and
            lifecycle.custody(settings, routing) == keys, 'Original config, network or custody differs')
    require(not progress['novonet']['probe_control']['release_pending'] and
            not progress['novonet']['diagnostic_relay']['active'], 'Stale local diagnostic activated after restart')
    listeners = subprocess.check_output(['ss', '-H', '-lunp', 'sport = :41050 or sport = :42050'], text=True)
    require(all(f':{port} ' in listeners for port in (41050, 42050)) and
            all(f'pid={new},' in line for line in listeners.splitlines()), 'Original listeners not on new PID')
    result = dict(old_pid=pid, pid=new, unit=scenario.UNIT, start='existing', database=settings['database'],
                  stop_signal='SIGTERM via pidfd', manager_before=state, manager_after=after,
                  signalled_monotonic_us=signalled, manager_exit_monotonic_us=exit_us,
                  pidfd_exit_observed_monotonic_us=detected, manager_start_monotonic_us=start_us,
                  healthy_observed_monotonic_us=healthy_us, process_downtime_ms=(start_us-exit_us)/1000,
                  exit_to_health_observed_ms=(healthy_us-exit_us)/1000,
                  projection=projection, listeners=listeners, config_unchanged=True, network_unchanged=True,
                  custody_unchanged=True, other_services_unchanged=lifecycle.services() == services,
                  automatic_restart_count_delta=1, manual_start_issued=False)
    lifecycle.write(evidence / 'restored.json', result)
    print(json.dumps(result))


if __name__ == '__main__':
    main()
