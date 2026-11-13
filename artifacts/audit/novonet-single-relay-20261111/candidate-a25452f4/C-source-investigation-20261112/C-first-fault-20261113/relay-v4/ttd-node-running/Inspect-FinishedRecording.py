"""Read a finalized C diagnostic only. Never launch/stop/retry a node or recorder."""
from pathlib import Path
import datetime, hashlib, json, re, subprocess

BASE = Path(__file__).resolve().parent
WORK = BASE / 'run-private-02'
LAUNCH = BASE / 'launch-public-02'
OUT = WORK / 'offline-replay-01'  # inherits the private trace directory ACL
EXPECTED_PID = 39992
INFO = BASE / 'position-reader/TraceInformationWithPosition.exe'
EXPECTED_INFO = '009541ff94177660c071eee229fb78ac393f0e821ded94c2bf76467247bbff16'
DEBUG = Path(r'D:\WEB3_AI\SUPERVM\target\c-first-fault-20261113\ttd-tools\samples\TTD\ReplayApi\TraceDebugger\out\x64-Release\TraceDebugger.exe')
EXPECTED_DEBUG = '0c5051995f59327c78bca0dc2d71032efae29407829089f12b755633a159bfc4'

def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))

def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(8 * 1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()

def run(exe, trace, commands, label):
    try:
        p = subprocess.run([str(exe), str(trace)], input=commands.encode(),
                           capture_output=True, timeout=180)
    except subprocess.TimeoutExpired as exc:
        (OUT / (label + '-private.log')).write_bytes((exc.stdout or b'') + (exc.stderr or b''))
        raise RuntimeError(label + ' exceeded offline replay bound; no retry') from exc
    (OUT / (label + '-private.log')).write_bytes(p.stdout + p.stderr)
    if p.returncode:
        raise RuntimeError(f'{label} exit {p.returncode}; no retry')
    return p.stdout.decode(errors='replace').replace('\r', '')

def main():
    # Require the authoritative owner/recorder end records; do not open a live ring.
    node = read(WORK / 'node-exit-public.json')
    owner = read(WORK / 'owner-terminal-public.json')
    recorder = read(LAUNCH / 'recorder-exit-public.json')
    end = read(LAUNCH / 'end-public.json')
    if node['pid'] != EXPECTED_PID or not node['kernel_handle_exit']:
        raise RuntimeError('Wrong node or missing kernel exit proof')
    if end['nodes'] or not end['identity_network_config_unchanged']:
        raise RuntimeError('End guards failed; preserve without replay')
    if recorder['exit_code'] != 0 or (LAUNCH / 'recorder-forced-exit-public.json').exists():
        raise RuntimeError('Recorder did not finalize normally; preserve incomplete trace')
    if digest(INFO) != EXPECTED_INFO or digest(DEBUG) != EXPECTED_DEBUG:
        raise RuntimeError('Replay reader differs')
    if OUT.exists():
        raise RuntimeError('Evidence directory exists; no overwrite/retry')
    OUT.mkdir()
    trace = WORK / 'node-private.run'
    stat = trace.stat()
    trace_hash = digest(trace)
    info = run(INFO, trace, '', 'information')
    after = trace.stat()
    if (stat.st_size, stat.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
        raise RuntimeError('Trace changed during inspection')
    lifetime = re.search(r'^Lifetime\s*:\s*(.+)$', info, re.M)
    pid = re.search(r'^PID\s*:\s*0x([0-9A-F]+)', info, re.M)
    if not lifetime or not pid or int(pid.group(1), 16) != EXPECTED_PID:
        raise RuntimeError('Trace process/lifetime mismatch')
    bounds = re.fullmatch(r'([0-9A-F]+):([0-9A-F]+)-([0-9A-F]+):([0-9A-F]+)', lifetime[1], re.I)
    if not bounds:
        raise RuntimeError('Unknown retained lifetime format')
    low, high = tuple(int(x, 16) for x in bounds.groups()[:2]), tuple(int(x, 16) for x in bounds.groups()[2:])
    exceptions = []
    for m in re.finditer(r'Position: (\S+) UTID:\s*(\d+)\s+Code: (0x[0-9A-F]+).*?PC: (0x[0-9A-F]+)', info):
        coordinate = tuple(int(x, 16) for x in m[1].split(':'))
        exceptions.append(dict(position=m[1], thread_id=int(m[2]), code=m[3], pc=m[4], in_retained_range=low <= coordinate <= high))
    count = re.search(r'^Exceptions\s*:\s*(\d+)', info, re.M)
    if not count or int(count[1]) != len(exceptions):
        raise RuntimeError('Exception parser/count mismatch')
    faults = [e for e in exceptions if e['in_retained_range'] and e['code'] in ('0xC0000005', '0xC0000374', '0xC0000409', '0xC00000FD')]
    # Preserve exact exception coordinates before any causal interpretation.
    result = dict(utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  node=node, owner=owner, recorder=recorder,
                  trace_sha256=trace_hash, trace_bytes=stat.st_size,
                  retained_lifetime=lifetime[1], ring_tail_only=True,
                  exception_count=len(exceptions), exceptions=exceptions,
                  fault_count=len(faults), first_writer_identified=False,
                  root_cause_fixed=False, relay_ready=False)
    (OUT / 'summary-public.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    if faults:
        event = faults[0]
        registers = run(DEBUG, trace, f"tt {event['position']}\nr\nr *\nq\n", 'first-fault-registers')
        positions = re.findall(r'^Position = (\S+)', registers, re.M)
        threads = re.findall(r'^UTID = (\d+)', registers, re.M)
        if not positions or positions[-1] != event['position'] or not threads or int(threads[-1]) != event['thread_id']:
            raise RuntimeError('Guest position/thread does not match exception')
        rips = re.findall(r'RIP = 0x([0-9A-F]+)', registers)
        if not rips or int(rips[-1], 16) != int(event['pc'], 16):
            raise RuntimeError('Guest PC does not match exception; inspect privately')
        result['first_fault_guest_pc_verified'] = True
        (OUT / 'summary-public.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))

if __name__ == '__main__':
    main()
