from pathlib import Path
import struct,json
r=Path('D:/WEB3_AI/SUPERVM/target/c-aoem-node-test-20261113/backtrace-exit-37796-01')
b=(r/'dump-local/novovm-node.exe.37796.dmp').read_bytes()
n,dr=struct.unpack_from('<II',b,8)
streams={struct.unpack_from('<III',b,dr+12*i)[0]:struct.unpack_from('<III',b,dr+12*i)[1:] for i in range(n)}
ranges=[]
if 5 in streams:
 _,m=streams[5]
 for i in range(struct.unpack_from('<I',b,m)[0]):
  va,size,off=struct.unpack_from('<QII',b,m+4+16*i);ranges.append((va,size,off))
if 9 in streams:
 _,m=streams[9];n,off=struct.unpack_from('<QQ',b,m)
 for i in range(n):
  va,size=struct.unpack_from('<QQ',b,m+16+16*i);ranges.append((va,size,off));off+=size
exe=Path('D:/WEB3_AI/SUPERVM/target/c-a254-existing-update-20261112T083103Z/package/novonet-observation-a25452f4-windows-x86_64/bin/novovm-node.exe').read_bytes()
pe=struct.unpack_from('<I',exe,60)[0];count=struct.unpack_from('<H',exe,pe+6)[0];opt=struct.unpack_from('<H',exe,pe+20)[0]
sections=[]
for i in range(count):
 o=pe+24+opt+40*i;vs,va,rs,rp=struct.unpack_from('<IIII',exe,o+8);sections.append((va,rs,rp))
out=[]
for name,rva,length in [('string_write_str',0x1e4cc60,97),('lower_hex',0x1e502f0,112),('format_write',0x1e52459,128),('pad_integral',0x1e52f00,247)]:
 va=0x7ff709a40000+rva
 mem=next((b[off+va-start:off+va-start+length] for start,size,off in ranges if start<=va and va+length<=start+size),None)
 file=next((exe[off+rva-start:off+rva-start+length] for start,size,off in sections if start<=rva and rva+length<=start+size),None)
 out.append({'function':name,'rva':hex(rva),'length':length,'captured_in_dump':mem is not None,'matches_fixed_executable':mem==file if mem is not None else None})
result={'pid':37796,'scope':'Only code bytes actually present in original minidump; absent bytes are not verified','code_checks':out,'root_cause_fixed':False}
(Path(__file__).parent/'captured-code-public.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
