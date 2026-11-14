"""Static code mapping only; never launch or attach to a process."""
from pathlib import Path
import sys,re,json,hashlib
sys.path.insert(0,str(Path('target/c-a254-crash-source-20261112T145135Z/python-tools').resolve()))
import pefile
from capstone import Cs,CS_ARCH_X86,CS_MODE_64
ROOT=Path('target/c-partitioned-writer-20261114/instrumented');HERE=Path(__file__).parent
pe=pefile.PE(str(ROOT/'c-lowimpact-capture.exe'));md=Cs(CS_ARCH_X86,CS_MODE_64)
text=(ROOT/'public-symbols-private.txt').read_text(encoding='utf-8-sig')
found={}
for name,segment,offset in re.findall(r'`([^`]+)`\s+flags = function, addr = (\d+):(\d+)',text):
 if not (name=='novovm_diag_boundary' or 'format12format_inner' in name or 'Formatter12pad_integral' in name or ('6String' in name and 'Write9write_str' in name)):continue
 rva=pe.sections[int(segment)-1].VirtualAddress+int(offset)
 found.setdefault(rva,[]).append(name)
rows=[]
for rva,names in sorted(found.items()):
 entry=next((e.struct for e in pe.DIRECTORY_ENTRY_EXCEPTION if e.struct.BeginAddress==rva),None)
 if entry is None:continue
 code=pe.get_data(rva,entry.EndAddress-rva);asm=[dict(rva=hex(i.address),instruction=i.mnemonic+' '+i.op_str) for i in md.disasm(code,rva)]
 u=pe.get_data(entry.UnwindData,4);uw=pe.get_data(entry.UnwindData,4+2*u[2]).hex()
 rows.append(dict(rva=hex(rva),aliases=names,bytes=len(code),sha256=hashlib.sha256(code).hexdigest(),unwind=uw,instructions=asm))
assert any(any('format12format_inner' in n for n in r['aliases']) for r in rows)
assert any(any('Write9write_str' in n for n in r['aliases']) for r in rows)
assert any(any('Formatter12pad_integral' in n for n in r['aliases']) for r in rows)
(HERE/'writer-code-public.json').write_bytes((json.dumps({'binary_sha256':hashlib.sha256((ROOT/'c-lowimpact-capture.exe').read_bytes()).hexdigest(),'static_only':True,'not_original_entry_registers':True,'functions':rows},indent=2)+'\n').encode())
print('Mapped',len(rows),'unique diagnostic functions; no process launched')
