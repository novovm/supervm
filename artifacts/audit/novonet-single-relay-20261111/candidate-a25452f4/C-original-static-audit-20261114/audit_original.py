"""Read historical Git objects, original PE files and archived dump headers only.

No image is loaded/executed; no database, registry or running process is opened.
Outputs are public metadata, source locations and RVA-based disassembly.
This inventory is NOT an exhaustive compiler/ABI or unsafe-soundness proof.
"""
import hashlib
import json
import mmap
import pathlib
import re
import struct
import subprocess
import sys
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[5]
OUT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'target/c-a254-crash-source-20261112T145135Z/python-tools'))
import pefile
import capstone


def save(name, data):
    (OUT / name).write_bytes((json.dumps(data, indent=2, ensure_ascii=False) + '\n').encode())


def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args]).decode('utf-8')


PATTERNS = {
    'unsafe': r'\bunsafe\b',
    'extern': r'\bextern\s+"(?:C(?:-unwind)?|system|win64|sysv64)"',
    'assembly_or_stack_switch': r'\b(?:asm!|global_asm!|naked_asm!|(?:naked|setjmp|longjmp|SwapContext|SwitchToFiber|RtlRestoreContext)\b)',
    'allocator': r'global_allocator|\bGlobalAlloc\b',
    'manual_send_sync': r'unsafe\s+impl.*\b(?:Send|Sync)\b',
}


def source_inventory(repo, ref, label):
    # Entire tracked first-party trees, including inactive/tests/examples, not a link map.
    names = git(repo, 'ls-tree', '-r', '--name-only', ref).splitlines()
    names = [n for n in names if n.startswith(('crates/', 'src/', 'vendor/')) and
             pathlib.PurePosixPath(n).suffix in ('.rs', '.c', '.cc', '.cpp', '.h', '.hpp', '.s', '.S', '.asm')]
    rows = []
    for name in names:
        body = git(repo, 'show', f'{ref}:{name}')
        hits = {k: [i for i, line in enumerate(body.splitlines(), 1) if re.search(p, line)]
                for k, p in PATTERNS.items()}
        if any(hits.values()) or pathlib.PurePosixPath(name).suffix in ('.s', '.S', '.asm'):
            rows.append({'file': name, 'blob': git(repo, 'rev-parse', f'{ref}:{name}').strip(),
                         'locations': {k: v for k, v in hits.items() if v},
                         'assembly_file': pathlib.PurePosixPath(name).suffix in ('.s', '.S', '.asm')})
    return {'label': label, 'commit': git(repo, 'rev-parse', ref).strip(),
            'files_scanned': len(names), 'lexical_only': True,
            'dependency_registry_and_generated_code_included': False, 'files': rows}


IMAGES = {
    '875-node': 'target/c-crash-analysis-20261112T004637Z/pdb-analysis/novovm-node.exe',
    'a254-node': 'target/c-symbolized-original-20261114/novovm-node.exe',
    '875-packaged-aoem': 'target/c-875-existing-update-20261111T103915Z/package/novonet-observation-875d3307-windows-x86_64/aoem/windows/core/bin/aoem_ffi.dll',
    'a254-packaged-aoem': 'target/c-a254-existing-update-20261112T083103Z/package/novonet-observation-a25452f4-windows-x86_64/aoem/windows/core/bin/aoem_ffi.dll',
    'AOEM966': 'target/c-state-context-phase5-20261114/runtime/aoem_ffi.dll',
}
EXPECTED = {
    '875-node': '9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7',
    'a254-node': '1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d',
    'a254-packaged-aoem': '1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50',
    'AOEM966': '5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52',
}


def inspect_images():
    rows, text, calls = [], [], []
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.detail = True
    for label, name in IMAGES.items():
        data = (ROOT / name).read_bytes()
        sha = hashlib.sha256(data).hexdigest()
        if label in EXPECTED:
            assert sha == EXPECTED[label], label
        pe = pefile.PE(data=data)
        imports = {int(x.address - pe.OPTIONAL_HEADER.ImageBase):
                   e.dll.decode() + '!' + (x.name.decode() if x.name else str(x.ordinal))
                   for e in getattr(pe, 'DIRECTORY_ENTRY_IMPORT', []) for x in e.imports}
        exports = {x.name.decode(): x.address for x in getattr(getattr(pe, 'DIRECTORY_ENTRY_EXPORT', None), 'symbols', []) if x.name}
        functions = list(getattr(pe, 'DIRECTORY_ENTRY_EXCEPTION', []))
        row = {'image': label, 'bytes': len(data), 'sha256': sha,
               'machine': hex(pe.FILE_HEADER.Machine), 'imports': imports,
               'runtime_function_entries': len(functions), 'selected_functions': []}
        selected = {n: exports[n] for n in ('aoem_execute_ops_wire_v1', 'aoem_compute_result_take_v1', 'aoem_free', 'aoem_destroy') if n in exports}
        if label.endswith('node'):
            selected = {'writer': 0x1e47a00 if label.startswith('875') else 0x1e4cc60}
        # New frontier: original writer's direct external callees, not re-running its tests.
        queue = [(n, rva, 0) for n, rva in selected.items()]
        visited = set()
        while queue:
            symbol, rva, depth = queue.pop(0)
            entry = next((f.struct for f in functions if f.struct.BeginAddress <= rva < f.struct.EndAddress), None)
            if entry is None or entry.BeginAddress in visited:
                continue
            visited.add(entry.BeginAddress)
            start, end = entry.BeginAddress, entry.EndAddress
            raw = pe.get_data(start, end - start)
            insns = list(md.disasm(raw, start))
            complete = sum(x.size for x in insns) == len(raw)
            assert complete, (label, symbol, hex(start))
            hdr = pe.get_data(entry.UnwindData, 4)
            count = hdr[2]
            unwind = pe.get_data(entry.UnwindData, 4 + ((count + 1) // 2) * 4)
            info = {'name': symbol, 'rva': hex(start), 'end_rva': hex(end),
                    'sha256': hashlib.sha256(raw).hexdigest(), 'decoded_all_bytes': complete,
                    'unwind_header_and_slots_hex': unwind.hex(),
                    'unwind_version': hdr[0] & 7, 'unwind_flags': hdr[0] >> 3,
                    'prologue_bytes': hdr[1], 'frame_byte': hdr[3],
                    'depth': depth, 'calls': []}
            text.append(f'\n=== {label} {symbol} RVA {start:x}-{end:x} ===')
            for ins in insns:
                text.append(f'{ins.address:08x} {ins.bytes.hex():30} {ins.mnemonic} {ins.op_str}')
                if ins.mnemonic not in ('call', 'jmp'):
                    continue
                op = ins.operands[0] if ins.operands else None
                call = {'instruction_rva': hex(ins.address), 'op': ins.mnemonic, 'operand': ins.op_str}
                if op and op.type == capstone.x86.X86_OP_IMM:
                    target = op.imm
                    call['target_rva'] = hex(target)
                    if label.endswith('node') and not start <= target < end and depth < 2:
                        queue.append((f'direct-depth{depth+1}-{target:x}', target, depth + 1))
                elif op and op.type == capstone.x86.X86_OP_MEM and op.mem.base == capstone.x86.X86_REG_RIP:
                    slot = ins.address + ins.size + op.mem.disp
                    call['slot_rva'] = hex(slot)
                    call['import'] = imports.get(slot)
                info['calls'].append(call)
            row['selected_functions'].append(info)
        rows.append(row)
    save('pe-frontier-public.json', rows)
    (OUT / 'pe-frontier-assembly.txt').write_bytes(('\n'.join(text) + '\n').encode())


DUMPS = {
    '31296-original': 'target/c-crash-analysis-20261112T004637Z/dumps-local/novovm-node.exe.31296.dmp',
    '3228-original': 'target/c-crash-analysis-20261112T004637Z/dumps-local/novovm-node.exe.3228.dmp',
    '13020-original': 'target/c-a254-crash-source-20261112T145135Z/dumps-local/novovm-node.exe.13020.dmp',
    '27928-original': 'target/c-a254-crash-source-20261112T145135Z/dumps-local/novovm-node.exe.27928.dmp',
    '37796-original': 'target/c-aoem-node-test-20261113/backtrace-exit-37796-01/dump-local/novovm-node.exe.37796.dmp',
    'PageHeap-separate-run': 'target/c-first-fault-20261113/run-private-03/node-private.dmp',
    'TTD-recorder-separate-run': 'target/c-ttd-node-20261113/run-private-02/crash-39992-private-v2/novovm-node.exe.39992.dmp',
    'controlled-capture-gate': 'target/c-joint-network-capture-20261114/gate-private.dmp',
}


def inspect_dumps():
    rows = []
    for label, rel in DUMPS.items():
        path = ROOT / rel
        with path.open('rb') as file, mmap.mmap(file.fileno(), 0, access=mmap.ACCESS_READ) as data:
            assert data[:4] == b'MDMP'
            n, directory = struct.unpack_from('<II', data, 8)
            flags = struct.unpack_from('<Q', data, 24)[0]
            streams = {}
            for i in range(n):
                kind, size, rva = struct.unpack_from('<III', data, directory + i*12)
                assert rva + size <= len(data)
                streams[kind] = (rva, size)
            count, memory_bytes = 0, 0
            if 9 in streams:
                at, _ = streams[9]
                count, base = struct.unpack_from('<QQ', data, at)
                for i in range(count):
                    _, size = struct.unpack_from('<QQ', data, at + 16 + 16*i)
                    memory_bytes += size
                assert base + memory_bytes <= len(data)
            threads = struct.unpack_from('<I', data, streams[3][0])[0] if 3 in streams else None
            row = {'label': label, 'bytes': len(data), 'flags': hex(flags),
                   'full_memory_flag': bool(flags & 2), 'memory64_ranges': count,
                   'memory64_bytes': memory_bytes, 'threads': threads, 'exception_stream': 6 in streams,
                   'stream_types': sorted(streams), 'original_non_instrumented_run': label.endswith('-original')}
            if label.endswith('-original'):
                row['sha256'] = hashlib.sha256(data).hexdigest()
            rows.append(row)
    save('dump-coverage-public.json', {'scope': 'Named private archives only, not a whole-disk absence proof', 'dumps': rows})


if __name__ == '__main__':
    assert (ROOT / '.git').exists(), ROOT
    save('source-boundaries-public.json', [
        source_inventory(ROOT, '875d3307', 'SUPERVM875'),
        source_inventory(ROOT, 'a25452f4', 'SUPERVMa254'),
        source_inventory(ROOT.parent / 'AOEM', '9662470d', 'AOEM966-source'),
    ])
    inspect_images()
    inspect_dumps()
    print('Offline inventory completed; no runtime or workload executed.')
