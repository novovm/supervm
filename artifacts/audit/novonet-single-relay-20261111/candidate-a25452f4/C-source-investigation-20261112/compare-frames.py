import sys,pathlib,re,json,hashlib,subprocess
r=pathlib.Path(__file__).parent;sys.path.insert(0,str(r/'python-tools'))
import pefile,capstone
old=pathlib.Path('D:/WEB3_AI/SUPERVM/target/c-crash-analysis-20261112T004637Z/pdb-analysis/novovm-node.exe')
new=pathlib.Path('D:/WEB3_AI/SUPERVM/target/c-a254-existing-update-20261112T083103Z/package/novonet-observation-a25452f4-windows-x86_64/bin/novovm-node.exe')
a,b=pefile.PE(str(old)),pefile.PE(str(new))
base=json.loads((r/'modules-27928-local.json').read_text())[0]['base']
frames=[(int(n),int(v,16)-base) for n,v in re.findall(r'frame #(\d+): (0x[0-9a-f]+) novovm-node.exe',(r/'a254-caller-local.log').read_text())]
cs=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64);cs.detail=True

def norm(pe,start,end):
 code=pe.get_data(start,end-start); out=bytearray(code); covered=0
 for x in cs.disasm(code,start):
  off=x.address-start;covered+=x.size
  # Ignore ONLY PC-relative branch/call targets and RIP-relative addresses.
  if (x.group(capstone.CS_GRP_JUMP) or x.group(capstone.CS_GRP_CALL)) and x.imm_size:
   out[off+x.imm_offset:off+x.imm_offset+x.imm_size]=b'\0'*x.imm_size
  if any(o.type==capstone.x86.X86_OP_MEM and o.mem.base==capstone.x86.X86_REG_RIP for o in x.operands) and x.disp_size:
   out[off+x.disp_offset:off+x.disp_offset+x.disp_size]=b'\0'*x.disp_size
 return bytes(out),covered==len(code)
idx={}
for x in a.DIRECTORY_ENTRY_EXCEPTION:
 s,e=x.struct.BeginAddress,x.struct.EndAddress;idx.setdefault(e-s,[]).append((s,e))
rows=[]
for n,at in frames:
 rf=next((x.struct for x in b.DIRECTORY_ENTRY_EXCEPTION if x.struct.BeginAddress<=at-1<x.struct.EndAddress),None)
 if rf is None:continue
 s,e=rf.BeginAddress,rf.EndAddress; nc,full=norm(b,s,e);matches=[]
 for os,oe in idx.get(e-s,[]):
  oc,ofull=norm(a,os,oe)
  if full and ofull and oc==nc:matches.append(os)
 row={'frame':n,'a254_rva':hex(at),'function_start':hex(s),'function_bytes':e-s,'disassembly_covers_all':full,'normalized_sha256':hashlib.sha256(nc).hexdigest(),'875_matches':[hex(v) for v in matches],'interpretation':'cross-binary code comparison, NOT matching a254 PDB'}
 if len(matches)==1:
  lookup=matches[0]+(at-s)-1
  p=subprocess.run(['C:/Program Files/LLVM/bin/llvm-symbolizer.exe','--obj='+str(old),'--relative-address','--output-style=JSON',hex(lookup)],capture_output=True,text=True)
  row['875_symbolizer_exit']=p.returncode
  try:row['875_symbol']=json.loads(p.stdout)
  except:row['875_symbol_error']=True
 rows.append(row)
(r/'frame-code-comparison-public.json').write_text(json.dumps(rows,indent=2))
for x in rows:print(x['frame'],x['a254_rva'],x['function_bytes'],x['875_matches'],x.get('875_symbol',{}))
