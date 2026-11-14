"""Offline only: extract target IPT payload candidates from existing tracerpt XML.
The v2 header interpretation is cross-checked against lengths, target IDs/QPC,
and Intel libipt packet decoding; it is not a claim of complete history.
"""
from pathlib import Path
import csv, hashlib, json, re, struct, subprocess, xml.etree.ElementTree as ET

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
PID,TID=28712,42560
NS='{http://schemas.microsoft.com/win/2004/08/events/event}'
PTDUMP=ROOT/'libipt/build-packet/bin/ptdump.exe'
psb=bytes.fromhex('0282'*8)
rows=[]
selected=[]
out=HERE/'target-pt-private'
out.mkdir(exist_ok=False)
for _,el in ET.iterparse(HERE/'tracerpt-private.xml',events=('end',)):
    if el.tag!=NS+'Event': continue
    serialized=ET.tostring(el,encoding='unicode')
    if 'ff1fd2fd-6008-42bb-9e75-00a20051f3be' in serialized.lower():
        binary=el.find(NS+'BinaryEventData')
        if binary is None: raise ValueError('IPT candidate without raw payload')
        data=bytes.fromhex(binary.text)
        if len(data)<32: raise ValueError('short header')
        qpc,pid,tid,options,size,pos=struct.unpack_from('<QIIQII',data)
        if size+32!=len(data) or pos>size: raise ValueError('candidate header mismatch')
        system=el.find(NS+'System')
        cpu=int(system.find(NS+'Execution').attrib['ProcessorID'])
        row=dict(qpc=qpc,pid=pid,tid=tid,cpu=cpu,options=hex(options),bufferBytes=size,
                 writePosition=pos,payloadBytes=len(data),psbCount=data[32:].count(psb))
        rows.append(row)
        if pid==PID and tid==TID:
            index=len(selected)
            raw=data[32:]
            ordered=raw[pos:]+raw[:pos]
            prefix=out/f'{index:02d}-{qpc}'
            prefix.with_suffix('.raw.bin').write_bytes(raw)
            ordered_path=prefix.with_suffix('.ordered.bin')
            ordered_path.write_bytes(ordered)
            command=[str(PTDUMP),'--raw','--lastip','--exec-mode','--no-pad',str(ordered_path)]
            proc=subprocess.run(command,capture_output=True,text=True,timeout=30)
            prefix.with_suffix('.packets.txt').write_text(proc.stdout,encoding='utf-8')
            prefix.with_suffix('.stderr.txt').write_text(proc.stderr,encoding='utf-8')
            row=dict(row,decodeExit=proc.returncode,decodeStdoutLines=len(proc.stdout.splitlines()),
                     decodeStderr=proc.stderr,rawSha256=hashlib.sha256(raw).hexdigest(),
                     orderedSha256=hashlib.sha256(ordered).hexdigest(),
                     decodedErrorLines=[s for s in proc.stdout.splitlines() if 'error' in s.lower()])
            selected.append(row)
    el.clear()
(HERE/'ipt-headers-private.json').write_text(json.dumps(rows,indent=2),encoding='utf-8')
(HERE/'target-pt-public.json').write_text(json.dumps(dict(pid=PID,tid=TID,headerCandidate='<QIIQII',
    allIptCandidateCount=len(rows),allQpcMin=min(r['qpc'] for r in rows),allQpcMax=max(r['qpc'] for r in rows),
    target=selected,fullHistory=False),indent=2),encoding='utf-8')

headers={}
events=[]
rawqpc=None
cpu=None
for line in (HERE/'events-private.csv').read_text(encoding='utf-8-sig',errors='replace').splitlines():
    if line.startswith('#'):
        m=re.search(r'^#\s*(\d+)\s*#\s*(0x[0-9a-fA-F]+)',line)
        if m: cpu,rawqpc=int(m[1]),int(m[2],16)
        continue
    values=next(csv.reader([line],skipinitialspace=True),[])
    if not values: continue
    kind=values[0].strip()
    if len(values)>1 and values[1].strip()=='TimeStamp':
        headers[kind]=[v.strip() for v in values]
        continue
    if kind not in ('SampledProfile','CSwitch','T-Start','T-End','P-Start','P-End','I-Start','I-End'): continue
    if not any(v.strip()==str(TID) for v in values) and not any(re.search(r'\(\s*'+str(PID)+r'\s*\)',v) for v in values): continue
    events.append(dict(kind=kind,qpc=rawqpc,cpu=cpu,values=values))
(HERE/'target-sideband-private.json').write_text(json.dumps(dict(headers=headers,events=events),indent=2),encoding='utf-8')
print(json.dumps(dict(targetPtCount=len(selected),decodeExits=[r['decodeExit'] for r in selected],
    decodeErrors=sum(len(r['decodedErrorLines']) for r in selected),
    targetSidebandCounts={kind:sum(e['kind']==kind for e in events) for kind in set(e['kind'] for e in events)}),indent=2))
