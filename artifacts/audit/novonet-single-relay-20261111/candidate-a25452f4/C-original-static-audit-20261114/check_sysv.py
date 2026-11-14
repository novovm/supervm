"""Static byte-pattern candidate and direct caller audit; does not execute DLLs."""
import bisect
import hashlib
import json
import struct
from audit_original import ROOT, OUT, IMAGES, pefile, capstone, save

md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
md.detail = True
rows, text = [], []
# Exact entry instruction sequence in the historical checked-in Keccak assembly.
pattern = bytes.fromhex('4989e34883e4e04881ec00030000')
for label in ('875-packaged-aoem', 'AOEM966'):
    pe = pefile.PE(str(ROOT / IMAGES[label]))
    ranges = [(f.struct.BeginAddress, f.struct.EndAddress, f.struct.UnwindData) for f in pe.DIRECTORY_ENTRY_EXCEPTION]
    starts = [r[0] for r in ranges]
    row = {'image': label, 'identification': 'source-entry byte-pattern candidate, not a PDB symbol attribution', 'matches': []}
    for sec in pe.sections:
        if not sec.Characteristics & 0x20000000:
            continue
        data, base = sec.get_data(), sec.VirtualAddress
        offset = data.find(pattern)
        while offset >= 0:
            target = base + offset
            insns = []
            for ins in md.disasm(data[offset:offset+16384], target):
                insns.append(ins)
                if ins.mnemonic == 'ret':
                    break
            assert insns and insns[-1].mnemonic == 'ret'
            end = insns[-1].address + insns[-1].size
            match = {'entry_rva': hex(target), 'end_rva': hex(end), 'bytes_sha256': hashlib.sha256(data[offset:offset+end-target]).hexdigest(),
                     'pdata_covers_entry': any(a <= target < b for a,b,_ in ranges), 'direct_callers': []}
            text.append(f'\n=== {label} Keccak candidate {target:x} ===')
            text.extend(f'{i.address:08x} {i.bytes.hex():30} {i.mnemonic} {i.op_str}' for i in insns)
            # E8 candidates must also decode on a caller's .pdata function boundary.
            at = data.find(b'\xe8')
            seen = set()
            while at >= 0 and at + 5 <= len(data):
                call_rva = base + at
                dest = call_rva + 5 + struct.unpack_from('<i', data, at+1)[0]
                if dest == target:
                    ix = bisect.bisect_right(starts, call_rva)-1
                    if ix >= 0:
                        begin, finish, unwind = ranges[ix]
                        if begin <= call_rva < finish and begin not in seen:
                            caller = list(md.disasm(pe.get_data(begin,finish-begin),begin))
                            if any(i.address == call_rva and i.mnemonic == 'call' for i in caller):
                                seen.add(begin)
                                hdr = pe.get_data(unwind,4)
                                slots = pe.get_data(unwind,4+((hdr[2]+1)//2)*4)
                                match['direct_callers'].append({'function_rva':hex(begin),'end_rva':hex(finish),'call_rva':hex(call_rva),
                                     'unwind_header_slots_hex':slots.hex(), 'prologue_bytes':hdr[1]})
                                text.append(f'\n=== {label} caller {begin:x}-{finish:x} ===')
                                text.extend(f'{i.address:08x} {i.bytes.hex():30} {i.mnemonic} {i.op_str}' for i in caller)
                at = data.find(b'\xe8', at+1)
            row['matches'].append(match)
            offset = data.find(pattern,offset+1)
    rows.append(row)
save('sysv-boundary-public.json', rows)
(OUT/'sysv-boundary-assembly.txt').write_bytes(('\n'.join(text)+'\n').encode())
print(json.dumps(rows,indent=2))
