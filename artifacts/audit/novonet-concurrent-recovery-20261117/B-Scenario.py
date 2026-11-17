"""Fixed common95 management actions; only original CLI/unit, no network socket."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shlex
import shutil
import subprocess
import time

if not __debug__:
    raise SystemExit('Safety assertions require non-optimized Python')

UNIT = 'novovm-B-common95-network-20261117.service'
RELEASE = Path('/opt/novovm/releases/novovm-common-95ec9c2e-aoem570-linux-x86_64')
WORK = Path('/var/lib/novovm/selfrun/91129655606029/B/novovm-common-95ec9c2e-20261117')
OUT = Path('/home/ecs-user/novonet-concurrent-recovery-20261117/evidence')
IDS = ['b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43',
       '4cecd4ba4092c821333501687e7a363d829d544c1a203ea2390d76e8b2a5627f',
       '1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a',
       'd90f5145f324790d6cef7b592d25747d0fb21958327eaeeb2367c800d74a8f90']


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def record(name, value):
    OUT.mkdir(parents=True, exist_ok=True)
    with (OUT / name).open('x') as stream:
        json.dump(value, stream, indent=2)


def progress():
    path = WORK / 'logs/progress.json'
    for _ in range(10):
        try:
            assert path.is_file() and not path.is_symlink() and path.stat().st_size <= 4 * 1024 * 1024
            p = json.loads(path.read_text())
            assert 0 <= time.time() - path.stat().st_mtime <= 15, 'stale B progress'
            assert p['chain_id'] == 91129655606029 and p['execution_failures'] == 0
            return p
        except json.JSONDecodeError:
            time.sleep(.05)
    raise RuntimeError('incomplete B progress')


def owner():
    pid = int(subprocess.check_output(['systemctl', 'show', UNIT, '-p', 'MainPID', '--value']))
    assert pid > 1 and Path(f'/proc/{pid}').stat().st_uid == 996, 'wrong B account'
    assert Path(os.readlink(f'/proc/{pid}/exe')) == RELEASE / 'bin/novovm-node'
    assert digest(RELEASE / 'bin/novovm-node') == 'e709c0d78776492b0e04f28cb817ae83f73e3f4642ccda67b27838feb448b0a4'
    assert digest(RELEASE / 'bin/novovmctl') == 'ee05a11e15cd6e723c0a3a2405fe67718a7caf0af26fa447f03ba57742e58f4c'
    fields = dict(v.split('=', 1) for v in shlex.split(subprocess.check_output(
        ['systemctl', 'show', UNIT, '-p', 'Environment', '--value'], text=True)) if '=' in v)
    assert fields['NOVOVM_NATIVE_EXECUTION_START'] == 'existing'
    assert Path(fields['NOVOVM_NATIVE_EXECUTION_CONFIG']) == WORK / 'run.json'
    config = json.loads((WORK / 'run.json').read_text())
    net = json.loads((WORK / 'network.json').read_text())
    assert Path(config['novonet_config_file']) == WORK / 'network.json'
    assert net['relay_forwarding'] and Path(net['probe_control_file']) == WORK / 'commands/control.nvprobe'
    ids = [hashlib.sha256(b'novovm-round-bft/v2/mldsa65/validator-id\0' + bytes(v['public_key'])).hexdigest()
           for v in config['genesis']['validators']]
    assert ids == IDS, 'changed fixed validator identities'
    assert digest(Path(config['library'])) == '21cf190ede73c2dc3040cf5ca51502a7bf56d1aeffbf7baeb31afdd5f71d0231'
    assert str(Path(config['library'])) in Path(f'/proc/{pid}/maps').read_text(), 'new AOEM not mapped'
    p = progress()
    assert not p['recovery_in_progress'] and p['current_height'] == 1 and p['head'] is None
    return pid, p


def snapshot(p):
    return dict(wall_ms=time.time_ns() // 1000000, monotonic_ns=time.monotonic_ns(),
                diagnostic=p['novonet']['diagnostic_relay'], transport=p['native_network_probe']['transport'])


def cli(action, *args):
    record(action + '-started.json', dict(wall_ms=time.time_ns() // 1000000))
    command = [str(RELEASE / 'bin/novovmctl'), 'native-network-probe', action, *args,
               '--output', str(WORK / 'commands/control.nvprobe')]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    record(action + '-cli.json', dict(exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr))
    result.check_returncode()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['preflight', 'clock-stream', 'arm', 'send-at', 'snapshot', 'interrupt'])
    parser.add_argument('--wall-ms', type=int)
    args = parser.parse_args()
    if args.action == 'clock-stream':
        import sys
        print(json.dumps(dict(ready=True)), flush=True)
        for _ in range(3):
            challenge = sys.stdin.readline().strip()
            assert len(challenge) == 32 and all(c in '0123456789abcdef' for c in challenge)
            print(json.dumps(dict(challenge=challenge, wall_ms=time.time_ns() // 1000000,
                                  monotonic_ns=time.monotonic_ns())), flush=True)
        return
    pid, p = owner()
    if args.action in ('preflight', 'snapshot'):
        print(json.dumps(dict(pid=pid, **snapshot(p))))
        return
    if args.action == 'arm':
        assert not p['novonet']['diagnostic_relay']['active'], 'B diagnostic busy'
        OUT.mkdir(parents=True, exist_ok=True)
        slot = WORK / 'commands/control.nvprobe'
        nonce = secrets.token_hex(32)
        record('before-arm.json', dict(pid=pid, nonce=nonce, **snapshot(p)))
        if slot.exists():
            assert slot.is_file() and not slot.is_symlink() and slot.stat().st_size in (409, 441, 377), 'unexpected B command slot'
            shutil.copyfile(slot, OUT / 'prior-control.nvprobe')
        cli('prepare', '--sender', IDS[1], '--recipient', IDS[3], '--via', IDS[0], '--via', IDS[2], '--nonce', nonce)
        shutil.copyfile(WORK / 'commands/control.nvprobe', OUT / 'arm.nvprobe')
        deadline = time.monotonic() + 35
        while time.monotonic() < deadline:
            p = progress()
            r = p['novonet']['diagnostic_relay']['reservation']
            if r and bytes(r['nonce']).hex() == nonce and r['ready']:
                result = dict(pid=pid, nonce=nonce, **snapshot(p))
                record('ready.json', result)
                print(json.dumps(result))
                return
            time.sleep(.1)
        raise RuntimeError('B original reservation not ready; no object sent')
    if args.action == 'send-at':
        assert args.wall_ms is not None and 0 < args.wall_ms - time.time_ns() // 1000000 < 15000
        ready = json.loads((OUT / 'ready.json').read_text())
        r = p['novonet']['diagnostic_relay']['reservation']
        assert r and bytes(r['nonce']).hex() == ready['nonce'] and r['ready']
        deadline = time.monotonic() + (args.wall_ms - time.time_ns() // 1000000) / 1000
        while time.monotonic() < deadline:
            time.sleep(min(.002, max(0, deadline - time.monotonic())))
        cli('send-relay', '--from', str(OUT / 'arm.nvprobe'))
        record('published.json', dict(pid=pid, **snapshot(progress())))
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            p = progress()
            events = [e for e in p['novonet']['diagnostic_relay']['events']
                      if e.get('probe') and bytes(e['probe']['nonce']).hex() == ready['nonce']]
            if any(e['event'] == 'relay_probe_object_done' for e in events):
                result = dict(pid=pid, sender_done=True, events=events, **snapshot(p))
                record('sender-result.json', result)
                print(json.dumps(result))
                return
            if any(e['event'] in ('relay_diagnostic_failed', 'relay_probe_terminal_failure') for e in events):
                record('first-send-failure.json', snapshot(p))
                raise RuntimeError('B original object terminal failure; no retry')
            time.sleep(.1)
        record('send-no-done.json', snapshot(progress()))
        raise RuntimeError('B no original DONE; no retry')
    if args.action == 'interrupt':
        assert not p['novonet']['diagnostic_relay']['active'], 'B original diagnostic still busy'
        assert json.loads((OUT / 'sender-result.json').read_text())['sender_done'], 'concurrent B result not complete'
        before = dict(pid=pid, config_sha256=digest(WORK / 'run.json'), network_sha256=digest(WORK / 'network.json'),
                      start_ticks=Path(f'/proc/{pid}/stat').read_text().split(') ', 1)[1].split()[19], **snapshot(p))
        record('before-interrupt.json', before)
        stopped = False
        try:
            subprocess.run(['systemctl', 'stop', UNIT], check=True, timeout=35)
            stopped = True
            assert not Path(f'/proc/{pid}').exists(), 'old B PID still alive'
            slot = WORK / 'commands/control.nvprobe'
            if slot.exists():
                # Only this completed workload's stale local command, never keys/cache/state.
                assert (OUT / 'sender-result.json').exists()
                shutil.copyfile(slot, OUT / 'completed-control-before-stop.nvprobe')
                slot.unlink()
            record('stopped.json', dict(old_pid=pid, wall_ms=time.time_ns() // 1000000))
            time.sleep(5)
        finally:
            # Same existing-mode unit is always restored, even if fault observation fails.
            resume_wall = time.time()
            subprocess.run(['systemctl', 'start', UNIT], check=True, timeout=35)
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            try:
                new_pid, p = owner()
                assert new_pid != pid and stopped
                assert (WORK / 'logs/progress.json').stat().st_mtime >= resume_wall, 'new B progress not yet written'
                assert digest(WORK / 'run.json') == before['config_sha256']
                assert digest(WORK / 'network.json') == before['network_sha256']
                result = dict(old_pid=pid, pid=new_pid, start='existing', config_unchanged=True,
                              network_unchanged=True, **snapshot(p))
                record('restored.json', result)
                print(json.dumps(result))
                return
            except (AssertionError, FileNotFoundError, json.JSONDecodeError):
                time.sleep(.2)
        raise RuntimeError('B existing restoration health not established; preserve evidence')


if __name__ == '__main__':
    main()
