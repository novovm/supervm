from pathlib import Path
import sys, json, hashlib
sys.path.insert(0, str(Path('target/c-a254-crash-source-20261112T145135Z/python-tools').resolve()))
import pefile
from capstone import Cs, CS_ARCH_X86, CS_MODE_64, CS_GRP_CALL, CS_GRP_JUMP
from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_REG_RIP

root = Path('target/c-state-context-phase5-20261114')
reference = json.loads(Path('target/c-call-boundary-phase3-20261113/code-comparison-v2-public.json').read_text())['functions']
md = Cs(CS_ARCH_X86, CS_MODE_64)
md.detail = True

def normalized(code, rva):
    buf = bytearray(code)
    for i in md.disasm(code, rva):
        at = i.address-rva
        if (i.group(CS_GRP_CALL) or i.group(CS_GRP_JUMP)) and any(o.type == X86_OP_IMM for o in i.operands):
            buf[at+i.imm_offset:at+i.imm_offset+i.imm_size] = bytes(i.imm_size)
        if any(o.type == X86_OP_MEM and o.mem.base == X86_REG_RIP for o in i.operands):
            buf[at+i.disp_offset:at+i.disp_offset+4] = bytes(4)
    return hashlib.sha256(buf).hexdigest()

original = pefile.PE('target/c-symbolized-original-20261114/novovm-node.exe')
for name, versions in reference.items():
    ref = versions['a254']
    start = int(ref['start_rva'],16)
    assert normalized(original.get_data(start,ref['bytes']),start) == ref['normalized_sha256'], name

image = Path(sys.argv[1])
pe = pefile.PE(str(image))
result = {'image':image.name,'sha256':hashlib.sha256(image.read_bytes()).hexdigest(),'functions':{}}
for name, versions in reference.items():
    ref=versions['a254'];matches=[]
    for e in pe.DIRECTORY_ENTRY_EXCEPTION:
        s=e.struct.BeginAddress; length=e.struct.EndAddress-s
        if length == ref['bytes'] and normalized(pe.get_data(s,length),s)==ref['normalized_sha256']:
            unwind=pe.get_data(e.struct.UnwindData,4)
            size=4+unwind[2]*2
            codes=pe.get_data(e.struct.UnwindData,size).hex()
            matches.append({'rva':hex(s),'bytes':length,'normalized_sha256':ref['normalized_sha256'],
                            'unwind_codes_hex':codes,'unwind_matches_original':codes==ref['unwind_codes_hex']})
    result['functions'][name]=matches
out=root/(image.stem+'-function-map.json')
out.write_bytes((json.dumps(result,indent=2)+'\n').encode())
print(json.dumps(result,indent=2))
