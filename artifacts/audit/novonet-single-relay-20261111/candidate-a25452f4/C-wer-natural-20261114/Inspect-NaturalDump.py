"""Read PID34452's pinned dump offline; emit metadata/comparisons, never memory bytes."""
import hashlib
import json
import pathlib
import struct
import sys
import uuid

p, output = map(pathlib.Path, sys.argv[1:3])
before = p.stat()
data = p.read_bytes()
sha = hashlib.sha256(data).hexdigest()
assert sha == '7d910fa6713e399c6bb394f2201fec622771dbc8e0665894ce9b8c115c9231aa'
assert data[:4] == b'MDMP'
flags = struct.unpack_from('<Q', data, 24)[0]
n, directory = struct.unpack_from('<II', data, 8)
streams = {}
for i in range(n):
    kind, size, rva = struct.unpack_from('<III', data, directory + i * 12)
    assert rva + size <= len(data)
    streams[kind] = (rva, size)
a, z = streams[9]
count, offset = struct.unpack_from('<QQ', data, a)
assert 16 + count * 16 <= z
regions = []
for i in range(count):
    va, size = struct.unpack_from('<QQ', data, a + 16 + i * 16)
    assert offset + size <= len(data)
    regions.append((va, va + size, offset))
    offset += size

def read(va, size):
    result = bytearray()
    end = va + size
    while va < end:
        r = next((r for r in regions if r[0] <= va < r[1]), None)
        if r is None:
            raise ValueError('Memory64 coverage missing at ' + hex(va))
        take = min(end, r[1]) - va
        result.extend(data[r[2] + va - r[0]:r[2] + va - r[0] + take])
        va += take
    return bytes(result)

def q(va):
    return struct.unpack('<Q', read(va, 8))[0]

threads = []
a, z = streams[3]
for i in range(struct.unpack_from('<I', data, a)[0]):
    pos = a + 4 + i * 48
    tid = struct.unpack_from('<I', data, pos)[0]
    size, rva = struct.unpack_from('<II', data, pos + 40)
    assert size >= 1232 and rva + size <= len(data)
    threads.append(tid)
a, z = streams[6]
tid = struct.unpack_from('<I', data, a)[0]
exception = struct.unpack_from('<I', data, a + 8)[0]
subcode = struct.unpack_from('<Q', data, a + 40)[0]
cs, cr = struct.unpack_from('<II', data, a + 160)
assert cs >= 1232 and cr + cs <= len(data)
rcx, rsp, rbp, rip = [struct.unpack_from('<Q', data, cr + x)[0] for x in (128, 152, 160, 248)]
assert tid in threads
assert (exception, subcode, rip) == (0xc0000409, 2, 0x7ffce31aa114)

# Prologue establishes RBP=RSP+0x100, cookie at RSP+0x130.
# Check bytes from Memory64 itself, not debugger fallback images.
assert read(0x7ffce31a9dfc, 5).hex() == '488d6c24b0'
assert read(0x7ffce31a9e01, 7).hex() == '4881ec50010000'
assert read(0x7ffce31aa108, 7).hex() == '488b4d304833cc'
cookie = q(0x7ffce333f5a8)
correct = q(rsp + 0x130) ^ rsp
actual = q(rbp + 0x30) ^ rsp
coverage = []
for label, va, size in [
    ('ntdll_prologue_and_epilogue', 0x7ffce31a9000, 0x2000),
    ('ntdll_cookie_check', 0x7ffce32ba000, 4096),
    ('node_allocation_caller', 0x7ff7d3484000, 4096),
    ('fault_stack_and_caller_frames', rsp, 0x1800),
    ('old_allocation_candidate', 0x1db44a79200, 0x2d8),
    ('returned_allocation_candidate', 0x1db472d4ce0, 0x2d8),
]:
    try:
        read(va, size)
        ok = True
    except ValueError:
        ok = False
    coverage.append({'label': label, 'address': hex(va), 'bytes': size, 'captured': ok})

modules = []
a, z = streams[4]
for i in range(struct.unpack_from('<I', data, a)[0]):
    pos = a + 4 + i * 108
    base = struct.unpack_from('<Q', data, pos)[0]
    if base not in (0x7ff7d10d0000, 0x7ffb9fe90000):
        continue
    size, rva = struct.unpack_from('<II', data, pos + 76)
    assert size >= 24 and rva + size <= len(data) and data[rva:rva+4] == b'RSDS'
    modules.append({'module': 'node' if base == 0x7ff7d10d0000 else 'aoem',
                    'pdb_guid': str(uuid.UUID(bytes_le=data[rva+4:rva+20])),
                    'pdb_age': struct.unpack_from('<I', data, rva+20)[0]})
after = p.stat()
assert (before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns)
result = {'pid': 34452, 'bytes': len(data), 'sha256': sha, 'flags': hex(flags),
          'memory64_ranges': count, 'thread_count': len(threads), 'all_listed_contexts_present': True,
          'fault_thread_id': tid, 'exception_code': hex(exception), 'fast_fail_subcode': subcode,
          'rip': hex(rip), 'rsp': hex(rsp), 'rbp': hex(rbp),
          'expected_rbp_from_prologue': hex(rsp+0x100), 'rbp_deviation': hex(rbp-rsp-0x100),
          'correct_slot_cookie_matches': correct == cookie,
          'actual_rbp_slot_cookie_matches': actual == cookie,
          'actual_rbp_slot_xor_matches_exception_rcx': actual == rcx,
          'prologue_and_check_bytes_from_dump_verified': True, 'coverage': coverage, 'modules': modules,
          'first_corrupting_instruction_found': False, 'production_fix_applied': False}
output.write_bytes((json.dumps(result, indent=2)+'\n').encode('utf-8'))
print(json.dumps(result))
