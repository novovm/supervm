"""Offline-only decoder for an existing File-mode qualification.

Never launches a target, starts ETW, changes identity, or accesses node state.
Reads target IDs/QPC from the same run and refuses to overwrite output.
Only selected near-fault raw fragments and executable pages are written locally.
"""
from pathlib import Path
from collections import deque, Counter
import argparse, hashlib, json, re, struct, subprocess, xml.etree.ElementTree as ET

NS='{http://schemas.microsoft.com/win/2004/08/events/event}'
IPT='ff1fd2fd-6008-42bb-9e75-00a20051f3be'
QUAL_EXE_SHA='a1b90b9b100750732cc3e4aac5672ae3319bac1a64ca70dbb7fedb661a11c5de'

def save(path,data):
    path.write_text(json.dumps(data,indent=2),encoding='utf-8')

def extract_modules(dump_path,out):
    b=dump_path.read_bytes()
    assert struct.unpack_from('<I',b)[0]==0x504d444d,'Not a minidump'
    n,drva=struct.unpack_from('<II',b,8)
    streams={}
    for i in range(n):
        st,size,rva=struct.unpack_from('<III',b,drva+12*i);streams[st]=(rva,size)
    assert 9 in streams,'Full-memory Memory64 stream missing'
    rva,_=streams[9];count,data_at=struct.unpack_from('<QQ',b,rva);segments=[]
    for i in range(count):
        start,size=struct.unpack_from('<QQ',b,rva+16+16*i)
        segments.append((start,size,data_at));data_at+=size
    mrva,_=streams[4];mods=[]
    for i in range(struct.unpack_from('<I',b,mrva)[0]):
        p=mrva+4+108*i;base,size=struct.unpack_from('<QI',b,p);nrva=struct.unpack_from('<I',b,p+20)[0]
        nl=struct.unpack_from('<I',b,nrva)[0];name=Path(b[nrva+4:nrva+4+nl].decode('utf-16-le')).name
        mods.append((base,size,name))
    sections=[];rawargs=[]
    for base,size,name in mods:
        image=bytearray(size);covered=bytearray(size)
        for a,s,p in segments:
            lo=max(a,base);hi=min(a+s,base+size)
            if hi>lo:
                image[lo-base:hi-base]=b[p+lo-a:p+hi-a]
                covered[lo-base:hi-base]=b'\x01'*(hi-lo)
        if image[:2]!=b'MZ':continue
        pe=struct.unpack_from('<I',image,0x3c)[0]
        nsec=struct.unpack_from('<H',image,pe+6)[0];opt=struct.unpack_from('<H',image,pe+20)[0]
        for i in range(nsec):
            p=pe+24+opt+40*i;sn=bytes(image[p:p+8]).split(b'\0')[0].decode('ascii')
            vs,va=struct.unpack_from('<II',image,p+8);flags=struct.unpack_from('<I',image,p+36)[0]
            if not flags&0x20000000:continue
            row=dict(module=name,section=sn,base=hex(base+va),bytes=vs)
            if not all(covered[va:va+vs]):
                sections.append(dict(row,mapped=False,missingBytes=covered[va:va+vs].count(0)));continue
            data=bytes(image[va:va+vs]);path=out/(name+'-'+sn.strip('.')+'.bin');path.write_bytes(data)
            rawargs+=['--raw',f'{path}:{base+va:#x}']
            sections.append(dict(row,mapped=True,sha256=hashlib.sha256(data).hexdigest()))
    return mods,sections,rawargs

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--run-root',required=True,type=Path)
    ap.add_argument('--xml',required=True,type=Path)
    ap.add_argument('--output',required=True,type=Path)
    args=ap.parse_args()
    root=Path(__file__).resolve().parents[1]
    run=args.run_root.resolve();out=args.output.resolve()
    assert root in run.parents and root in out.parents,'Must remain under qualification root'
    identity=json.loads((run/'traced-target/result.json').read_text(encoding='utf-8-sig'))
    pid=int(identity['pid']);tid=int(identity['faultTid']);fault=int(identity['faultQpc'])
    start=int(identity['workStartQpc']);freq=int(identity['qpcFrequency'])
    assert pid>0 and tid>0 and fault>start and freq>0
    assert args.xml.is_file(),'Existing export missing; do not launch a capture here'
    out.mkdir(exist_ok=False);private=out/'private';private.mkdir()
    all_count=0;all_headers=[];target=[];selected_before=[];selected_after=[];by_cpu={};target_threads=Counter()
    for _,el in ET.iterparse(args.xml,events=('end',)):
        if el.tag!=NS+'Event':continue
        system=el.find(NS+'System')
        binary=el.find(NS+'BinaryEventData')
        if binary is not None and IPT in ET.tostring(el,encoding='unicode').lower():
            data=bytes.fromhex(binary.text);assert len(data)>=32
            qpc,epid,etid,options,size,pos=struct.unpack_from('<QIIQII',data)
            assert size+32==len(data) and pos<=size,'IPT v2 candidate header mismatch'
            cpu=int(system.find(NS+'Execution').attrib['ProcessorID'])
            all_count+=1
            all_headers.append(dict(qpc=qpc,pid=epid,tid=etid,cpu=cpu,options=hex(options),bufferBytes=size,writePosition=pos,payloadBytes=len(data)))
            c=by_cpu.setdefault(cpu,dict(records=0,minQpc=qpc,maxQpc=qpc))
            c['records']+=1;c['minQpc']=min(c['minQpc'],qpc);c['maxQpc']=max(c['maxQpc'],qpc)
            if epid==pid:
                target_threads[etid]+=1
                if etid==tid:
                    row=dict(index=len(target),qpc=qpc,pid=epid,tid=etid,cpu=cpu,options=hex(options),bufferBytes=size,
                        writePosition=pos,payloadBytes=len(data),secondsFromWorkStart=(qpc-start)/freq,secondsFromFault=(qpc-fault)/freq)
                    target.append(row)
                    candidate=(row,data[32:])
                    if qpc<=fault:
                        selected_before.append(candidate)
                        selected_before.sort(key=lambda v:v[0]['qpc'])
                        selected_before=selected_before[-6:]
                    else:
                        selected_after.append(candidate)
                        selected_after.sort(key=lambda v:v[0]['qpc'])
                        selected_after=selected_after[:2]
        el.clear()
    # Header QPC order is checked rather than assumed to be equivalent to instruction order.
    ordered=all(target[i]['qpc']<=target[i+1]['qpc'] for i in range(len(target)-1))
    target.sort(key=lambda v:v['qpc'])
    selected=list(selected_before)+selected_after
    save(private/'ipt-headers-private.json',all_headers)
    save(out/'target-envelope-public.json',dict(pid=pid,tid=tid,faultQpc=fault,frequency=freq,
        allIptCount=all_count,allCpuEnvelopes=by_cpu,targetThreadCounts=dict(target_threads),xmlHeaderQpcWasMonotonic=ordered,records=target))
    ptdump=root/'libipt/build-packet/bin/ptdump.exe';ptxed=root/'libipt/build-instruction/bin/ptxed.exe'
    code=private/'code';code.mkdir()
    mods,sections,rawargs=extract_modules(run/'traced-target/qualification-full.dmp',code)
    base=next(m[0] for m in mods if m[2].lower()=='ptqualification.exe')
    exe_sha=hashlib.sha256((root/'qualification/PtQualification.exe').read_bytes()).hexdigest()
    symbols_valid=exe_sha==QUAL_EXE_SHA
    decoded=[]
    for row,raw in selected:
        name=f'{row["index"]:06d}-{row["qpc"]}'
        path=private/(name+'.pt.bin');pos=row['writePosition']
        path.write_bytes(raw[pos:]+raw[:pos])
        packet=subprocess.run([str(ptdump),'--raw','--lastip','--exec-mode','--no-pad',str(path)],capture_output=True,text=True,timeout=30)
        (private/(name+'.packets.txt')).write_text(packet.stdout,encoding='utf-8')
        (private/(name+'.packet-stderr.txt')).write_text(packet.stderr,encoding='utf-8')
        ins=subprocess.run([str(ptxed),'--insn-decoder','--raw-insn','--offset','--stat',*rawargs,'--pt',str(path)],capture_output=True,text=True,timeout=120)
        (private/(name+'.instructions.txt')).write_text(ins.stdout,encoding='utf-8')
        (private/(name+'.instruction-stderr.txt')).write_text(ins.stderr,encoding='utf-8')
        entries=Counter();count=0;examples=[];sample_funcs=set()
        for line in ins.stdout.splitlines():
            m=re.match(r'^\s*([0-9a-f]{16})\s+([0-9a-f]{16})\s+(.+)$',line)
            if not m:continue
            count+=1;ip=int(m[2],16)
            if len(examples)<16:examples.append(line)
            if symbols_valid:
                for fn,rva in [('BranchEven',0x1010),('BranchOdd',0x1030),('ExecuteChunk',0x1390)]:
                    if ip==base+rva:entries[fn]+=1
        decoded.append(dict(header=row,ptSha256=hashlib.sha256(path.read_bytes()).hexdigest(),packetExit=packet.returncode,
            packetErrors=[s for s in packet.stdout.splitlines() if 'error' in s.lower()],packetStderr=packet.stderr,
            packetCounters={k:len(re.findall(r'\b'+k+r'\b',packet.stdout)) for k in ['psb','ovf','tsc','cyc']},
            instructionExit=ins.returncode,instructionErrors=[s for s in ins.stdout.splitlines() if 'error' in s.lower()],instructionStderr=ins.stderr,
            instructionOutputLines=count,functionEntryOccurrences=dict(entries),firstInstructions=examples))
    before=[r for r in target if r['qpc']<=fault];after=[r for r in target if r['qpc']>fault]
    summary=dict(kind='actual_file_mode_retained_fragment_decode',pid=pid,tid=tid,base=hex(base),faultQpc=fault,
        sourceExeMatchesPriorExactPdbMapping=symbols_valid,targetPtCount=len(target),selectedFragmentCount=len(decoded),
        firstTargetPtQpc=target[0]['qpc'] if target else None,lastTargetPtQpc=target[-1]['qpc'] if target else None,
        lastTargetBeforeFaultQpc=before[-1]['qpc'] if before else None,firstTargetAfterFaultQpc=after[0]['qpc'] if after else None,
        faultMinusLastBeforeSeconds=(fault-before[-1]['qpc'])/freq if before else None,
        firstAfterMinusFaultSeconds=(after[0]['qpc']-fault)/freq if after else None,
        sections=sections,fragments=decoded,ptdumpSha256=hashlib.sha256(ptdump.read_bytes()).hexdigest(),ptxedSha256=hashlib.sha256(ptxed.read_bytes()).hexdigest(),
        limitations=['PT header QPC associates a record, not every instruction timestamp.','Fragments decode independently; no full-run continuity is implied.',
            'File mode preserves emitted events, not necessarily all hardware instructions between triggers.','PT does not record per-instruction RBP values.',
            'This target is a declared qualification process, not original NOVOVM root-cause evidence.'])
    save(out/'decode-public.json',summary)
    print(json.dumps({k:summary[k] for k in ['pid','tid','targetPtCount','selectedFragmentCount','faultMinusLastBeforeSeconds','firstAfterMinusFaultSeconds']},indent=2))
    print(json.dumps(dict(packetExits=[r['packetExit'] for r in decoded],instructionExits=[r['instructionExit'] for r in decoded],instructionErrors=[r['instructionErrors'] for r in decoded]),indent=2))

if __name__=='__main__':main()
