"""Read-only ETL-export review; all capture and target execution are external.

Example: python analyze_same_run.py --run ../run-file01 --output file01-coverage-public.json
Output is a whitelist, not a copy of full-system events. Existing output is never replaced.
"""
import argparse, collections, csv, datetime, hashlib, json, re
from pathlib import Path
import xml.etree.ElementTree as ET

ap = argparse.ArgumentParser()
ap.add_argument('--run', type=Path, required=True)
ap.add_argument('--output', type=Path, required=True)
ap.add_argument('--xml', type=Path)
ap.add_argument('--ipt-headers', type=Path)
args = ap.parse_args()
run, out = args.run.resolve(), args.output.resolve()
if out.exists(): raise SystemExit('Refusing to overwrite evidence output')
paths = [run/'traced-target/result.json', run/'decode/events-private.csv',
         args.xml.resolve() if args.xml else run/'decode/tracerpt-private.xml',
         args.ipt_headers.resolve() if args.ipt_headers else run/'decode/ipt-headers-private.json']
for path in paths:
    if not path.is_file(): raise SystemExit('Input missing: ' + str(path))
target = json.loads(paths[0].read_text(encoding='utf-8-sig'))
pid, tid = target['pid'], target['workTid']
q0, qf, freq = target['workStartQpc'], target['faultQpc'], target['qpcFrequency']
assert target['kind'] == 'isolated_pt_qualification_not_novovm'

def sha(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''): h.update(block)
    return h.hexdigest()

def event(qpc, cpu, **extra):
    return dict(qpc=qpc, cpu=cpu, secondsAfterWorkStart=(qpc-q0)/freq,
                secondsBeforeFault=(qf-qpc)/freq, **extra)

def numeric(s): return int(s, 16) if s.lower().startswith('0x') else int(s)

def summary(rows):
    rows=sorted(rows,key=lambda r:r['qpc'])
    if not rows:return dict(count=0)
    within=[r for r in rows if q0<=r['qpc']<=qf]
    gaps=[(a['qpc'],b['qpc'],(b['qpc']-a['qpc'])/freq)
          for a,b in zip(within,within[1:])]
    return dict(count=len(rows),first=rows[0],last=rows[-1],
                cpus=sorted({r['cpu'] for r in rows}),countWithinWorkToFault=len(within),
                firstWithin=within[0] if within else None,
                lastWithin=within[-1] if within else None,
                largestBetweenEnvelopeGaps=[dict(beginQpc=a,endQpc=b,seconds=c)
                    for a,b,c in sorted(gaps,key=lambda x:x[2],reverse=True)[:10]],
                gapMeaning='Between retained event timestamps only; not continuous instruction coverage.')

headers={}; in_header=False; context=None; first_raw=None
samples_index=collections.defaultdict(list)
target_lifecycle=[]; process_lifecycle=[]; modules=[]; switches=[]; reliable=[]
kind_counts=collections.Counter()
raw_re=re.compile(r'^#\s*(\d+)\s*#\s*(0x[0-9a-fA-F]+)\s*#\s*(\d+)')
pid_re=re.compile(r'\(\s*'+str(pid)+r'\)\s*$')
with paths[1].open(encoding='latin-1',newline='') as f:
    for line in f:
        line=line.strip()
        if line=='BeginHeader':in_header=True;continue
        if line=='EndHeader':in_header=False;continue
        m=raw_re.match(line)
        if m:
            context=(int(m[2],16),int(m[1]))
            if first_raw is None:first_raw=context[0]
            continue
        if not line or line.startswith('#'):continue
        row=[v.strip() for v in next(csv.reader([line],skipinitialspace=True))]
        kind=row[0]
        if in_header:headers[kind]=row;continue
        if kind.startswith('FirstReliable'):
            reliable.append(dict(kind=kind,value=row[1] if len(row)>1 else None));continue
        if context is None or len(row)<2 or not row[1].isdigit():continue
        qpc,cpu=context;kind_counts[kind]+=1
        if kind in ('SampledProfile','SampledProfileNmi'):
            samples_index[(qpc,numeric(row[4]))].append(dict(cpu=cpu,resolvedTid=row[3]))
        if kind.startswith('T-') and len(row)>3 and row[3]==str(tid):
            target_lifecycle.append(event(qpc,cpu,kind=kind,tid=tid,processPidResolved=bool(pid_re.search(row[2]))))
        if kind.startswith('P-') and len(row)>2 and pid_re.search(row[2]):
            info=dict(kind=kind,pid=pid)
            if kind in ('P-End','P-Zombie') and len(row)>6:info['exitStatus']=row[6]
            process_lifecycle.append(event(qpc,cpu,**info))
        if kind.startswith('I-') and len(row)>8 and pid_re.search(row[2]):
            modules.append(event(qpc,cpu,kind=kind,base=numeric(row[3]),end=numeric(row[4]),
                                 basename=row[8].replace('\\','/').split('/')[-1]))
        if kind=='CSwitch' and len(row)>16 and (row[3]==str(tid) or row[9]==str(tid)):
            switches.append(event(qpc,int(row[16]),direction='in' if row[3]==str(tid) else 'out'))

ns={'e':'http://schemas.microsoft.com/win/2004/08/events/event',
    't':'http://schemas.microsoft.com/win/2004/08/events/trace'}
def wall100ns(s):
    m=re.fullmatch(r'(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)\.(\d+)(.*)',s)
    if not m:raise ValueError('Unsupported timestamp syntax')
    d=datetime.datetime.fromisoformat(m[1]).replace(tzinfo=datetime.timezone.utc)
    return int(d.timestamp())*10000000+int((m[2]+'0000000')[:7]),m[3]
header_wall=None; trace_headers=[]; drops=[]; raw_samples=[]; mismatches=[]; xml_suffixes=collections.Counter()
for _,el in ET.iterparse(paths[2],events=['end']):
    if el.tag!='{'+ns['e']+'}Event':continue
    pairs=[(d.get('Name'),(d.text or '').strip()) for d in el.findall('e:EventData/e:Data',ns)]
    d=dict(pairs); time=el.find('e:System/e:TimeCreated',ns)
    opname=el.findtext('e:RenderingInfo/e:Opcode',namespaces=ns)
    if 'PerfFreq' in d and 'StartTime' in d:
        if header_wall is None:header_wall=wall100ns(time.get('SystemTime'))
        assert int(d['PerfFreq'])==freq
        trace_headers.append({k:d.get(k) for k in ['NumberOfProcessors','EventsLost','BuffersLost','PerfFreq','LogFileMode','BuffersWritten','BufferSize']})
    if opname=='LastDroppedTimes':
        drops.append(dict(count=int(d['TimeStampCount']),timestamps=[int(v) for k,v in pairs if k=='TimeStamp']))
    if d.get('ThreadId')==str(tid) and 'InstructionPointer' in d:
        wall,suffix=wall100ns(time.get('SystemTime'))
        assert header_wall
        # tracerpt can render +07:59 and +08:00 within one sub-second timeline.
        # Do not use the suffix as an absolute-time conversion. The derived QPC
        # must independently match an xperf raw-QPC + instruction-pointer pair.
        xml_suffixes[suffix]+=1
        qpc=first_raw+(wall-header_wall[0])*freq//10000000
        ip=numeric(d['InstructionPointer'])
        matches=samples_index.get((qpc,ip),[])
        cpu=int(el.find('e:System/e:Execution',ns).get('ProcessorID'))
        if not matches:mismatches.append(dict(qpc=qpc,ip=hex(ip),reason='No exact QPC+IP xperf match'))
        raw_samples.append(event(qpc,cpu,ip=hex(ip),exactXperfMatch=bool(matches),
                                 xperfResolvedTidMatches=any(x['resolvedTid']==str(tid) for x in matches)))
    el.clear()

raw_pt=json.loads(paths[3].read_text(encoding='utf-8-sig'))
pt=[event(r['qpc'],r['cpu'],bufferBytes=r['bufferBytes'],writePosition=r['writePosition'])
    for r in raw_pt if r['pid']==pid and r['tid']==tid]
pt.sort(key=lambda r:r['qpc'])
percpu=collections.defaultdict(list)
for r in raw_pt:percpu[r['cpu']].append(r['qpc'])
drop_mappings=[]
for record in drops:
    rows=[]
    for i,q in enumerate(record['timestamps']):
        first=min(percpu[i]) if percpu[i] else None
        rows.append(dict(index=i,dropQpc=q,cpuFirstPtQpc=first,
                         matchesFirstPt=bool(q and q==first),afterFault=bool(q and q>qf)))
    drop_mappings.append(dict(nonzeroCount=sum(bool(r['dropQpc']) for r in rows),
                              exactMatchCount=sum(r['matchesFirstPt'] for r in rows),rows=rows))
cpu_sequence=[]
for r in sorted(switches,key=lambda r:r['qpc']):
    if r['direction']=='in' and (not cpu_sequence or cpu_sequence[-1]['cpu']!=r['cpu']):cpu_sequence.append(r)
before_fault=[r for r in pt if r['qpc']<=qf]
after_fault=[r for r in pt if r['qpc']>=qf]
fault_ip=numeric(target['rip'])
fault_modules=[dict(basename=r['basename'],base=r['base'],end=r['end'],kind=r['kind'],qpc=r['qpc'])
               for r in modules if r['base']<=fault_ip<r['end']]
output=dict(scope='Read-only same-run export analysis; qualification program, not NOVOVM.',
    run=run.name,targetPid=pid,targetTid=tid,qpcFrequency=freq,workStartQpc=q0,faultQpc=qf,
    timestampDerivation='Raw xperf QPC anchor + same-XML relative wall-clock 100ns fields, ignoring inconsistent rendered offset suffix; independently checked exact raw QPC+IP. PT timestamps are raw internal QPC; deltas / qpcFrequency. No absolute UTC inference from XML.',
    xmlRenderedOffsetSuffixCounts=dict(xml_suffixes),
    firstXperfRawQpc=first_raw,traceHeaders=trace_headers,lastDroppedTimes=drop_mappings,
    reliableMarkers=reliable,sourceHashes={str(p.relative_to(run)):sha(p) for p in paths},
    processLifecycle=process_lifecycle,targetThreadLifecycle=target_lifecycle,
    targetModuleRecords=modules,faultIpMapping=fault_modules,
    targetPt=summary(pt),targetRawSamples=summary(raw_samples),
    rawSampleExactMatchFailures=mismatches,targetCSwitch=summary(switches),
    targetSwitchInCpuSequence=cpu_sequence,
    preFaultLastPt=before_fault[-1] if before_fault else None,
    postFaultFirstPt=after_fault[0] if after_fault else None,
    faultTailSeconds=(qf-before_fault[-1]['qpc'])/freq if before_fault else None,
    allCpuIdentitySearch=dict(totalRawPt=len(raw_pt),cpuCount=len(percpu),
      exactTargetCount=len(pt),pidOnlyCount=sum(r['pid']==pid for r in raw_pt),
      tidOnlyCount=sum(r['tid']==tid for r in raw_pt)),
    limits=['PT event envelopes and sample gaps do not measure complete instruction history.',
            'A sample absent between two timestamps is not proof the whole interval was unobserved; packet decode is required.',
            'Target CSwitch transitions show only retained scheduling boundaries; missing boundaries cannot be invented.',
            'No original node, original database, or identity was used. No claim of original RBP violation.',
            'Zero lost counters alone are insufficient; inspect retention metadata, sideband, decoder truncation and fault vicinity.'])
out.parent.mkdir(parents=True,exist_ok=True)
out.write_text(json.dumps(output,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:output[k] for k in ['run','targetPid','targetTid','faultTailSeconds','allCpuIdentitySearch']},indent=2))
print('Process lifecycle',len(process_lifecycle),'Thread lifecycle',len(target_lifecycle),'Modules',len(modules),'Sample mismatches',len(mismatches))
