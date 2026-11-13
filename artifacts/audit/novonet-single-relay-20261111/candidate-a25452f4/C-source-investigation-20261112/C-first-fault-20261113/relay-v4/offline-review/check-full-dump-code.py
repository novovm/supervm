from pathlib import Path
import json,struct,mmap
b=Path(__file__).parent
class Pe:
 def __init__(self,path):
  self.raw=Path(path).read_bytes();p=struct.unpack_from('<I',self.raw,60)[0]
  count=struct.unpack_from('<H',self.raw,p+6)[0];opt=struct.unpack_from('<H',self.raw,p+20)[0]
  self.sections=[]
  for i in range(count):
   pos=p+24+opt+40*i;vs,va,size,off=struct.unpack_from('<IIII',self.raw,pos+8);self.sections.append((va,size,off))
  self.pdata,self.pdata_size=struct.unpack_from('<II',self.raw,p+24+112+3*8)
 def read(self,rva,n):
  for va,size,off in self.sections:
   if va<=rva and rva+n<=va+size:return self.raw[off+rva-va:off+rva-va+n]
  raise ValueError(f'PE RVA missing {rva:x}')
 def unwind(self,pc):
  for begin,end,xdata in struct.iter_unpack('<III',self.read(self.pdata,self.pdata_size)):
   if begin<=pc<end:return {'begin':hex(begin),'end':hex(end),'unwind_rva':hex(xdata),'unwind_info':self.read(xdata,16).hex()}
  return None
f=(b/'run-private-03/node-private.dmp').open('rb');m=mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ)
n,dr=struct.unpack_from('<II',m,8)
streams={}
for i in range(n):
 typ,size,rva=struct.unpack_from('<III',m,dr+12*i);streams[typ]=(rva,size)
pos,_=streams[9];cnt,off=struct.unpack_from('<QQ',m,pos);ranges=[]
for i in range(cnt):
 va,size=struct.unpack_from('<QQ',m,pos+16+16*i);ranges.append((va,size,off));off+=size
def mem(va,n):
 pieces=[]
 while n:
  row=next((x for x in ranges if x[0]<=va<x[0]+x[1]),None)
  if row is None:return None
  start,size,off=row;k=min(n,start+size-va);pieces.append(m[off+va-start:off+va-start+k]);va+=k;n-=k
 return b''.join(pieces)
mods={};pos,_=streams[4]
for i in range(struct.unpack_from('<I',m,pos)[0]):
 o=pos+4+108*i;base,size,_,_,name=struct.unpack_from('<QIIII',m,o)
 length=struct.unpack_from('<I',m,name)[0];path=m[name+4:name+4+length].decode('utf-16le');name=path.replace('\\','/').split('/')[-1].lower()
 mods[name]=(base,size,path)
node=Pe('D:/WEB3_AI/SUPERVM/target/c-a254-existing-update-20261112T083103Z/package/novonet-observation-a25452f4-windows-x86_64/bin/novovm-node.exe')
nt=Pe('C:/Windows/System32/ntdll.dll')
checks=[]
for mod,pe,rva,n in [('ntdll.dll',nt,0x1ff1b,60),('novovm-node.exe',node,0x2432780,128),('novovm-node.exe',node,0x2c9b4fc,16)]:
 captured=mem(mods[mod][0]+rva,n)
 checks.append({'module':mod,'rva':hex(rva),'bytes':n,'captured':captured is not None,'matches_disk':captured==pe.read(rva,n) if captured else None})
field=mem(0x6975aec790,8)
result={'scope':'Read-only comparison of selected captured code/unwind metadata and fault-operand field; no arbitrary heap contents exported.', 'checks':checks,'node_allocator_frame':node.unwind(0x2432795),'ntdll_fault_frame':nt.unwind(0x1ff3f),'node_unwind_metadata_at_fault':'0x2c9b4fc','fault_operand_field':{'address':'0x6975aec790','value':hex(int.from_bytes(field,'little')) if field else None},'root_cause_fixed':False}
(b/'full-dump-code-public.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
