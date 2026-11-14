"""Offline PT decode, adding captured executable pages of same-dump modules."""
from pathlib import Path
import hashlib,json,re,struct,subprocess,collections
HERE=Path(__file__).resolve().parent; ROOT=HERE.parents[1]
out=HERE/'instructions-modules-private'; out.mkdir(exist_ok=False)
b=(HERE.parent/'traced-target/qualification-full.dmp').read_bytes()
n,drva=struct.unpack_from('<II',b,8)
streams={}
for i in range(n):
    st,size,rva=struct.unpack_from('<III',b,drva+12*i);streams[st]=(rva,size)
rva,_=streams[9]; count,data_at=struct.unpack_from('<QQ',b,rva);segments=[]
for i in range(count):
    start,size=struct.unpack_from('<QQ',b,rva+16+16*i);segments.append((start,size,data_at));data_at+=size
mrva,_=streams[4];mods=[]
for i in range(struct.unpack_from('<I',b,mrva)[0]):
    p=mrva+4+108*i;base,size=struct.unpack_from('<QI',b,p);name_rva=struct.unpack_from('<I',b,p+20)[0]
    nl=struct.unpack_from('<I',b,name_rva)[0];name=b[name_rva+4:name_rva+4+nl].decode('utf-16-le')
    mods.append((base,size,Path(name).name))
sections=[];rawargs=[]
for base,size,name in mods:
    image=bytearray(size);covered=bytearray(size)
    for a,s,p in segments:
        lo=max(a,base);hi=min(a+s,base+size)
        if hi>lo:
            image[lo-base:hi-base]=b[p+lo-a:p+hi-a];covered[lo-base:hi-base]=b'\x01'*(hi-lo)
    if image[:2]!=b'MZ': continue
    pe=struct.unpack_from('<I',image,0x3c)[0]
    nsec=struct.unpack_from('<H',image,pe+6)[0];opt=struct.unpack_from('<H',image,pe+20)[0]
    for i in range(nsec):
        p=pe+24+opt+40*i;sn=bytes(image[p:p+8]).split(b'\0')[0].decode('ascii')
        vs,va=struct.unpack_from('<II',image,p+8);flags=struct.unpack_from('<I',image,p+36)[0]
        if not flags&0x20000000:continue
        if not all(covered[va:va+vs]):
            sections.append(dict(module=name,section=sn,base=hex(base+va),bytes=vs,mapped=False,missingBytes=covered[va:va+vs].count(0)));continue
        data=bytes(image[va:va+vs]);path=out/(name+'-'+sn.strip('.')+'.bin');path.write_bytes(data)
        rawargs+=['--raw',f'{path}:{base+va:#x}']
        sections.append(dict(module=name,section=sn,base=hex(base+va),bytes=vs,mapped=True,sha256=hashlib.sha256(data).hexdigest()))
ptxed=ROOT/'libipt/build-instruction/bin/ptxed.exe';rows=[]
funcs=[('BranchEven',0x1010,0x1029),('BranchOdd',0x1030,0x103f),('ExecuteChunk',0x1390,0x13dd)]
base=0x7ff616bb0000
for pt in sorted((HERE/'target-pt-private').glob('*.ordered.bin')):
    cmd=[str(ptxed),'--insn-decoder','--raw-insn','--offset','--stat',*rawargs,'--pt',str(pt)]
    r=subprocess.run(cmd,capture_output=True,text=True,timeout=120);prefix=out/pt.stem
    prefix.with_suffix('.instructions.txt').write_text(r.stdout,encoding='utf-8');prefix.with_suffix('.stderr.txt').write_text(r.stderr,encoding='utf-8')
    lines=r.stdout.splitlines();insn=[]
    for line in lines:
        m=re.match(r'^\s*([0-9a-f]{16})\s+([0-9a-f]{16})\s+(.+)$',line)
        if m:insn.append((int(m[2],16),line))
    counts={fn:sum(base+lo<=ip<base+hi for ip,_ in insn) for fn,lo,hi in funcs}
    entries={fn:sum(ip==base+lo for ip,_ in insn) for fn,lo,hi in funcs}
    ips=[ip for ip,_ in insn];by_module=collections.Counter()
    for ip in ips:
        for mb,ms,mn in mods:
            if mb<=ip<mb+ms:by_module[mn]+=1;break
        else:by_module['unmapped']+=1
    rows.append(dict(fragment=pt.name,exitCode=r.returncode,stderr=r.stderr,decodedInstructionLines=len(insn),functionInstructionCounts=counts,functionEntryOccurrences=entries,moduleInstructionCounts=dict(by_module),errors=[s for s in lines if 'error' in s.lower()],firstInstructions=[l for _,l in insn[:30]],tail=lines[-8:]))
public=dict(kind='offline_instruction_decode_of_retained_real_ipt',pid=28712,tid=42560,moduleBase=hex(base),codeSource='Same-run FullMemory dump executable pages only',sections=sections,
    ptxedSha256=hashlib.sha256(ptxed.read_bytes()).hexdigest(),fragments=rows,historyComplete=False,registerValueHistory=False,
    notes=['The debugger was used offline only to resolve the exact qualification PDB function names.','Libipt may return exit 0 while reporting packet decode errors; errors are listed independently.'])
(HERE/'instruction-modules-public.json').write_text(json.dumps(public,indent=2),encoding='utf-8')
print(json.dumps(dict(exits=[r['exitCode'] for r in rows],totalInstructions=sum(r['decodedInstructionLines'] for r in rows),functionEntries={fn:sum(r['functionEntryOccurrences'][fn] for r in rows) for fn,_,_ in funcs},errors=[r['errors'] for r in rows],sectionsMapped=sum(s['mapped'] for s in sections),sectionsMissing=[s for s in sections if not s['mapped']]),indent=2))
