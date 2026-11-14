"""Classify only the prior audit's missing-pdata ranges, using original bytes."""
import hashlib
import json
import pathlib
import struct
import sys

OUT=pathlib.Path(__file__).resolve().parent
ROOT=OUT.parents[4]
sys.path.insert(0,str(ROOT/'target/c-a254-crash-source-20261112T145135Z/python-tools'))
import pefile
import capstone

prior=OUT.parent/'C-original-static-audit-20261114'
image_rows={x['image']:x for x in json.loads((prior/'pe-frontier-public.json').read_text())}
paths={
 '875-node':ROOT/'target/c-crash-analysis-20261112T004637Z/pdb-analysis/novovm-node.exe',
 'a254-node':ROOT/'target/c-symbolized-original-20261114/novovm-node.exe',
 '875-packaged-aoem':ROOT/'target/c-875-existing-update-20261111T103915Z/package/novonet-observation-875d3307-windows-x86_64/aoem/windows/core/bin/aoem_ffi.dll',
 'AOEM966':ROOT/'target/c-state-context-phase5-20261114/runtime/aoem_ffi.dll',
}
targets=[]
for leaf in json.loads((prior/'leaf-thunks-public.json').read_text()):
    start=int(leaf['rva'],16);last=leaf['instructions'][-1]
    targets.append((leaf['image'],leaf['name'],start,int(last['rva'],16)+len(bytes.fromhex(last['bytes']))))
for row in json.loads((prior/'sysv-boundary-public.json').read_text()):
    for m in row['matches']:targets.append((row['image'],'source-correlated-SysV-Keccak-candidate',int(m['entry_rva'],16),int(m['end_rva'],16)))
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64);md.detail=True
images={};rows=[]
for label,path in paths.items():
    raw=path.read_bytes();assert hashlib.sha256(raw).hexdigest()==image_rows[label]['sha256']
    pe=pefile.PE(data=raw,fast_load=True)
    ex=pe.OPTIONAL_HEADER.DATA_DIRECTORY[3]
    pdata=list(struct.iter_unpack('<III',pe.get_data(ex.VirtualAddress,ex.Size)))
    images[label]=(pe,pdata)
for label,name,start,end in targets:
    pe,pdata=images[label];raw=pe.get_data(start,end-start)
    ins=list(md.disasm(raw,start));assert sum(x.size for x in ins)==len(raw)
    writes=set();rsp=[];calls=[]
    for i in ins:
        _,w=i.regs_access();names={i.reg_name(x) for x in w};writes.update(names)
        if 'rsp' in names:rsp.append({'rva':hex(i.address),'instruction':i.mnemonic+' '+i.op_str})
        if i.mnemonic=='call': calls.append(hex(i.address))
    nonvolatile=sorted(writes & {'rbx','rbp','rdi','rsi','r12','r13','r14','r15'})
    xmm=sorted(r for r in writes if r.startswith(('xmm','ymm','zmm')) and r[3:].isdigit() and 6<=int(r[3:])<=15)
    # RET's architectural RSP increment is normal and not a stack frame allocation.
    body_rsp=[r for r in rsp if not r['instruction'].startswith('ret')]
    leaf=not body_rsp and not calls and not nonvolatile and not xmm
    rows.append({'image':label,'name':name,'start_rva':hex(start),'end_rva':hex(end),
        'body_sha256':hashlib.sha256(raw).hexdigest(),
        'static_pdata_overlaps':[{'start':hex(a),'end':hex(b),'unwind':hex(u)} for a,b,u in pdata if a<end and b>start],
        'rsp_writes':rsp,'direct_calls':calls,'nonvolatile_gpr_writes':nonvolatile,'xmm6_to15_alias_writes':xmm,
        'classification':'leaf_or_tail_thunk_no_frame_in_this_range' if leaf else 'stack_changing_SysV_body_not_Windows_leaf',
        'normal_return_abi_bug_proven':False,
        'unwind_requirement':'No local frame unwind needed for this range; tail target judged separately' if leaf else 'Needs supported unwind coverage for Windows exception traversal; static pdata absent, runtime coverage not demonstrated',
        'original_crash_caused_by_this_range':None})
(OUT/'unwind-classification-public.json').write_bytes((json.dumps(rows,indent=2)+'\n').encode())
print(json.dumps([{k:r[k] for k in ['image','name','classification','rsp_writes','nonvolatile_gpr_writes']} for r in rows],indent=2))
