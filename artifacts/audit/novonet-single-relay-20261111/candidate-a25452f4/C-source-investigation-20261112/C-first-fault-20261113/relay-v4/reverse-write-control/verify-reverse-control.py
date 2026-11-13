"""Offline replay of isolated controls only; no node or recorder is launched."""
from pathlib import Path
import subprocess, re, json, hashlib, struct, datetime
BASE = Path(__file__).resolve().parent
OUT = BASE / 'control-02'
DEBUG = BASE / 'samples/TTD/ReplayApi/TraceDebugger/out/x64-Release/TraceDebugger.exe'
INFO = BASE / 'samples/TTD/ReplayApi/TraceInformation/out/x64-Release/TraceInformation.exe'

def run(exe, trace, commands='', label='replay'):
    p = subprocess.run([str(exe), str(trace)], input=commands.encode(), capture_output=True, timeout=45)
    (OUT / (label + '-local.txt')).write_bytes(p.stdout + p.stderr)
    if p.returncode:
        raise RuntimeError(f'{label} exited {p.returncode}')
    return (p.stdout + p.stderr).decode(errors='replace').replace('\r', '')

def image_bytes(rva, size):
    raw = (BASE / 'ttd-pointer-control-aligned.exe').read_bytes()
    pe = struct.unpack_from('<I', raw, 60)[0]
    count = struct.unpack_from('<H', raw, pe + 6)[0]
    opt = struct.unpack_from('<H', raw, pe + 20)[0]
    for i in range(count):
        off = pe + 24 + opt + 40 * i
        va, n, data = struct.unpack_from('<III', raw, off + 12)
        if va <= rva and rva + size <= va + n:
            return raw[data + rva - va:data + rva - va + size]
    raise ValueError('RVA not in image')

# Fixed fixture instructions checked before interpreting a replay hit.
assert image_bytes(0x2b37, 7).hex() == '48890542260200'
assert image_bytes(0x2bb4, 3).hex() == '488908'
record = json.loads((OUT / 'result-public.json').read_text(encoding='utf-8-sig'))
assert [x['exit_code'] for x in record['results']] == [0, -1073741819]
rows = []
for mode in ['clean', 'bad']:
    trace = OUT / (mode + '.run')
    info = run(INFO, trace, label=mode + '-information')
    match = re.search(r'\]  0x([0-9A-F]+) 0x[0-9A-F]+ .*ttd-pointer-control-aligned\.exe', info)
    assert match, 'Missing fixture module'
    base = int(match.group(1), 16)
    slot, fault = base + 0x25180, base + 0x2bb4
    commands = 'tt 100\n'
    if mode == 'bad':
        commands += f'ba- E {fault:#x} 1\nr\nr rax\n'
    commands += f'ba- W {slot:#x} 8\nr\ndq {slot:#x}\nt-\ndq {slot:#x}\nt\ndq {slot:#x}\nq\n'
    (OUT / (mode + '-verified-commands.txt')).write_text(commands, encoding='utf-8')
    text = run(DEBUG, trace, commands, mode + '-verified-replay')
    values = [int(x, 16) for x in re.findall(rf'0x{slot:016X} 0x([0-9A-F]+)', text)]
    assert len(values) == 3, values
    if mode == 'bad':
        assert values[0] == values[2] == 0xb8 and values[1] not in (0, 0xb8)
        assert f'RIP = 0x{base+0x2b37:X}' in text
        assert f'RIP = 0x{fault:X}' in text and 'rax = 0x00000000000000B8' in text
        assert 'Code: 0xC0000005' in info
    else:
        assert values[0] == values[2] and values[0] not in (0, 0xb8)
        assert re.search(r'Exceptions\s*:\s*0\n', info)
    rows.append(dict(mode=mode,trace_bytes=trace.stat().st_size,
        trace_sha256=hashlib.sha256(trace.read_bytes()).hexdigest(),
        pointer_after=hex(values[0]),pointer_before=hex(values[1]),pointer_after_replay=hex(values[2]),
        passed=True))
result = dict(utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    scope='Isolated intentionally faulty fixture, not the NOVOVM node',
    reverse_write_control_verified=True, node_root_cause_fixed=False, node_started=False,
    bad_writer_rva='0x2b37',fault_consumer_rva='0x2bb4',slot_rva='0x25180',cases=rows,
    limitation='TTD exception Parameters reported read/0xa0, while replayed guest write instruction and RAX identify 0xb8; do not use Parameters alone as guest fault operand proof.')
(OUT / 'replay-public.json').write_bytes((json.dumps(result, indent=2)+'\n').encode())
print(json.dumps(result, indent=2))
