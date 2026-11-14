"""Compare selected ORIGINAL byte ranges. No rebuild and no runtime execution."""
import hashlib
import json
import re
from audit_original import ROOT, OUT, IMAGES, pefile, capstone, save

md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
md.detail = True
pes = {label: pefile.PE(str(ROOT / path)) for label, path in IMAGES.items()}


def normalized(pe, start, end):
    raw = pe.get_data(start, end-start)
    result = bytearray()
    for i in md.disasm(raw,start):
        b = bytearray(i.bytes)
        if i.group(capstone.CS_GRP_CALL) or i.group(capstone.CS_GRP_JUMP):
            if i.imm_size:
                b[i.imm_offset:i.imm_offset+i.imm_size] = bytes(i.imm_size)
        if any(o.type == capstone.x86.X86_OP_MEM and o.mem.base == capstone.x86.X86_REG_RIP for o in i.operands):
            b[i.disp_offset:i.disp_offset+i.disp_size] = bytes(i.disp_size)
        result.extend(b)
    assert len(result) == len(raw)
    return hashlib.sha256(result).hexdigest()


data = {r['image']:r for r in json.loads((OUT/'pe-frontier-public.json').read_text('utf-8'))}
comparisons = []
for left,right in [('875-node','a254-node'),('875-packaged-aoem','AOEM966')]:
    lf,rf = data[left]['selected_functions'],data[right]['selected_functions']
    assert len(lf) == len(rf)
    for a,b in zip(lf,rf):
        x = normalized(pes[left],int(a['rva'],16),int(a['end_rva'],16))
        y = normalized(pes[right],int(b['rva'],16),int(b['end_rva'],16))
        comparisons.append({'left':left,'right':right,'left_function':a['name'],'right_function':b['name'],
                            'left_rva':a['rva'],'right_rva':b['rva'],'left_normalized_sha256':x,'right_normalized_sha256':y,
                            'normalized_bytes_equal':x==y,
                            'unwind_header_slots_equal':a['unwind_header_and_slots_hex']==b['unwind_header_and_slots_hex']})

sysv = json.loads((OUT/'sysv-boundary-public.json').read_text('utf-8'))
checked = []
for row in sysv:
    for match in row['matches']:
        for caller in match['direct_callers']:
            pe=pes[row['image']]
            start,end=int(caller['function_rva'],16),int(caller['end_rva'],16)
            ins=list(md.disasm(pe.get_data(start,end-start),start))
            before=[i for i in ins if i.address<start+caller['prologue_bytes']]
            pushes=[i.op_str for i in before if i.mnemonic=='push']
            pops=[i.op_str for i in ins if i.mnemonic=='pop']
            stores={}
            loads={}
            for i in ins:
                if i.mnemonic!='movaps': continue
                m=re.fullmatch(r'xmmword ptr \[rsp \+ (0x[0-9a-f]+)\], (xmm\d+)',i.op_str)
                if m: stores[m[2]]=m[1]
                m=re.fullmatch(r'(xmm\d+), xmmword ptr \[rsp \+ (0x[0-9a-f]+)\]',i.op_str)
                if m: loads[m[1]]=m[2]
            checked.append({'image':row['image'],'caller_rva':hex(start),'normalized_sha256':normalized(pe,start,end),
                'pushes':pushes,'pops':pops,'push_pop_reverse':pushes==pops[::-1],
                'xmm_stores':stores,'xmm_loads':loads,'xmm6_to_15_matching_slots':all(stores.get(f'xmm{n}')==loads.get(f'xmm{n}') and f'xmm{n}' in stores for n in range(6,16)),
                'scope':'Observed prologue/normal-return instructions; not a proof against intervening corruption or all paths in dependencies'})
save('frontier-comparison-public.json', {'normalization':'Only relative branch/call immediate and RIP displacement; target behavior not proven by equality', 'comparisons':comparisons,'sysv_callers':checked})

leaves=[]
for label in ['875-node','a254-node','875-packaged-aoem','AOEM966']:
    pe=pes[label]
    if label.endswith('node'):
        rva=0x23a9557 if label.startswith('875') else 0x23ae7c7
        name='writer_copy_target'
    else:
        rva=next(x.address for x in pe.DIRECTORY_ENTRY_EXPORT.symbols if x.name==b'aoem_free')
        name='aoem_free'
    body=[]
    for i in md.disasm(pe.get_data(rva,64),rva):
        body.append({'rva':hex(i.address),'bytes':i.bytes.hex(),'mnemonic':i.mnemonic,'operands':i.op_str})
        if i.mnemonic=='jmp': break
    leaves.append({'image':label,'name':name,'rva':hex(rva),'instructions':body})
save('leaf-thunks-public.json',leaves)
print(json.dumps({'comparison_count':len(comparisons),'equal_count':sum(x['normalized_bytes_equal'] for x in comparisons),'sysv_callers':checked},indent=2))
