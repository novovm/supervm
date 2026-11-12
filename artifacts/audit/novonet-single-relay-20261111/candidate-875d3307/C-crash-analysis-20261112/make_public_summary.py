import json,pathlib,re
r=pathlib.Path(__file__).parent
summary=[]
for row in json.loads((r/'exceptions-public.json').read_text()):
    pid=row['pid']
    modules=json.loads((r/f'modules-{pid}-local.json').read_text())
    text=(r/f'lldb-{pid}-final-local.log').read_text(encoding='utf-8-sig')
    frames=[]
    for match in re.finditer(r'frame #(\d+): (0x[0-9a-f]+)',text):
        index,address=int(match[1]),int(match[2],16)
        module=next((m for m in modules if m['base']<=address<m['base']+m['size']),None)
        frames.append({'index':index,'module':module['module'] if module else 'unmapped','rva':hex(address-module['base']) if module else None})
    clean={k:v for k,v in row.items() if k!='fault_address'}
    clean['frames']=frames
    clean['unwind_source']='LLDB21.1.7 original minidump, matching875 executable; PDB absent'
    clean['absolute_addresses_and_memory_omitted']=True
    if pid==31296:
        rbp=int(re.search(r'rbp = (0x[0-9a-f]+)',text)[1],16)
        clean.update({'fault_instruction':'movaps xmm6, [rbp+0x20]','observed_operand_alignment_mod16':(rbp+0x20)%16,'interpretation':'Observed operand is misaligned for movaps. Consistent with an alignment fault; cause of misalignment not established.','unwind_limit':'Only fault frame recovered; no complete caller chain claimed.'})
    else:
        clean.update({'fault_instruction':'mov word ptr [rax], cx','access_bytes':2,'interpretation':'Exception records a write violation at the destination used by the instruction. Faulting runtime module does not prove a bad runtime installation.','export_symbol_caveat':'Nearest exported symbol shown by LLDB is not used as the actual routine name.'})
    summary.append(clean)
(r/'crash-summary-public.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8')
print(json.dumps(summary,indent=2))
