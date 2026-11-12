"""One bounded public snapshot of B's original progress owner; no network writes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from datetime import datetime, timezone

SOURCE = 'a25452f4c49be37cd984e650f2079642e6c273b3'
NODE = '89854a24d4259b912fdbb8c6f8e21ea48af6c74c0c89da8f3cdc648c1845a769'
EXE = '/opt/novovm/releases/novonet-observation-a25452f4-linux-x86_64/bin/novovm-node'
PROGRESS = Path('/var/lib/novovm/selfrun/91129655606029/B/novonet-observation-a25452f4-20261112/logs/progress.json')


def main():
    if len(sys.argv) != 1:
        raise RuntimeError('No arbitrary paths or actions accepted')
    pid = int(subprocess.check_output(['systemctl', 'show', 'novovm-B-a25452f4-network-20261112.service', '--value', '-p', 'MainPID']))
    if pid <= 1 or os.readlink(f'/proc/{pid}/exe') != EXE:
        raise RuntimeError('Original B process not running')
    import pwd
    if Path(f'/proc/{pid}').stat().st_uid != pwd.getpwnam('novovm').pw_uid:
        raise RuntimeError('Original B identity differs')
    with open(EXE, 'rb') as source:
        if hashlib.file_digest(source, 'sha256').hexdigest() != NODE:
            raise RuntimeError('B fixed node differs')
    deadline = time.monotonic() + 15
    while True:
        if PROGRESS.is_symlink() or PROGRESS.stat().st_size > 4*1024*1024 or time.time()-PROGRESS.stat().st_mtime > 15:
            raise RuntimeError('Invalid or stale original progress')
        with PROGRESS.open('rb') as stream:
            raw = stream.read(4*1024*1024+1)
        if len(raw) > 4*1024*1024:
            raise RuntimeError('Progress read exceeded bound')
        try:
            p = json.loads(raw)
            if 'novonet' in p:
                break
        except json.JSONDecodeError:
            pass
        if time.monotonic() >= deadline:
            raise RuntimeError('No complete original progress within observation bound')
        time.sleep(.1)
    if p['chain_id'] != 91129655606029 or p['execution_failures'] != 0 or not p['novonet']['diagnostic_relay']['enabled']:
        raise RuntimeError('Original B progress health differs')
    if int(subprocess.check_output(['systemctl', 'show', 'novovm-B-a25452f4-network-20261112.service', '--value', '-p', 'MainPID'])) != pid:
        raise RuntimeError('B owner changed during snapshot')
    public = dict(host='B', source=SOURCE, node_sha256=NODE, pid=pid,
                  utc=datetime.now(timezone.utc).isoformat(), read_only=True,
                  chain_id=p['chain_id'], current_height=p['current_height'],
                  execution_failures=p['execution_failures'],
                  diagnostic_relay=p['novonet']['diagnostic_relay'],
                  transport=p['native_network_probe']['transport'], business_receipt=False)
    print(json.dumps(public))


if __name__ == '__main__':
    main()
