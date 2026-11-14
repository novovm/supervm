"""Offline CSwitch/PT ownership correlation. Never starts tracing or targets.

Only exact QPC+CPU matches are called exact associations. Nearest associations
are separately marked hints; another thread's PID/TID is never published.
"""
import argparse, bisect, collections, csv, hashlib, json, re
from pathlib import Path
ap=argparse.ArgumentParser()
ap.add_argument('--run',required=True,type=Path)
ap.add_argument('--headers',required=True,type=Path)
ap.add_argument('--output',required=True,type=Path)
a=ap.parse_args(); run=a.run.resolve();out=a.output.resolve()
if out.exists():raise SystemExit('Refusing to overwrite evidence')
t=json.loads((run/'traced-target/result.json').read_text(encoding='utf-8-sig'))
tid,pid,qf,freq=t['workTid'],t['pid'],t['faultQpc'],t['qpcFrequency']
headers=json.loads(a.headers.read_text(encoding='utf-8-sig'))
rx=re.compile(r'^#\s*(\d+)\s*#\s*(0x[0-9a-fA-F]+)')
ctx=None;switches=[]
for line in (run/'decode/events-private.csv').open(encoding='latin-1'):
    m=rx.match(line)
    if m:ctx=(int(m[2],16),int(m[1]));continue
    if not ctx or line.startswith('#'):continue
    row=[v.strip() for v in next(csv.reader([line],skipinitialspace=True))]
    if len(row)>16 and row[0]=='CSwitch' and (row[3]==str(tid) or row[9]==str(tid)):
        switches.append(dict(qpc=ctx[0],cpu=int(row[16]),newTid=int(row[3]),oldTid=int(row[9])))
by_cpu=collections.defaultdict(list)
for idx,h in enumerate(headers):by_cpu[h['cpu']].append(dict(h,headerIndex=idx))
for cpu in by_cpu:by_cpu[cpu].sort(key=lambda r:r['qpc'])
qs={cpu:[h['qpc'] for h in rows] for cpu,rows in by_cpu.items()}
def public_pt(h,s=None):
    r={k:h[k] for k in ['qpc','cpu','options','bufferBytes','writePosition','headerIndex']}
    r.update(payloadIsTargetPid=h['pid']==pid,payloadIsTargetTid=h['tid']==tid,
             secondsRelativeToFault=(h['qpc']-qf)/freq)
    if s:
        r.update(deltaTicksFromCSwitch=h['qpc']-s['qpc'],
                 exactQpcAndCpu=h['qpc']==s['qpc'] and h['cpu']==s['cpu'],
                 payloadTidMatchesOutgoing=h['tid']==s['oldTid'],
                 payloadTidMatchesIncoming=h['tid']==s['newTid'])
    return r
def correlate(s):
    cpu=s['cpu'];arr=by_cpu[cpu];times=qs.get(cpu,[])
    lo=bisect.bisect_left(times,s['qpc']);hi=bisect.bisect_right(times,s['qpc'])
    exact=arr[lo:hi]
    candidates=arr[max(0,lo-2):min(len(arr),hi+2)]
    nearest=sorted(candidates,key=lambda h:abs(h['qpc']-s['qpc']))[:2]
    return dict(qpc=s['qpc'],cpu=cpu,secondsRelativeToFault=(s['qpc']-qf)/freq,
                targetIsOutgoing=s['oldTid']==tid,targetIsIncoming=s['newTid']==tid,
                exactPt=[public_pt(h,s) for h in exact],
                nearestPtHints=[public_pt(h,s) for h in nearest])
correlations=[correlate(s) for s in switches]
counts=collections.Counter()
for c in correlations:
    for p in c['exactPt']:
        label=('outgoing' if p['payloadTidMatchesOutgoing'] else '')+('incoming' if p['payloadTidMatchesIncoming'] else '')
        counts[label or 'neither']+=1
target_headers=[h for h in headers if h['pid']==pid and h['tid']==tid]
near=[public_pt(dict(h,headerIndex=i)) for i,h in enumerate(headers)
      if h['pid']==pid and h['tid']==tid and abs(h['qpc']-qf)<=freq*0.005]
window=[c for c in correlations if abs(c['qpc']-qf)<=freq*0.01]
before=[c for c in correlations if c['qpc']<=qf]
after=[c for c in correlations if c['qpc']>qf]
result=dict(scope='Same-run raw PT owner vs raw target CSwitch, offline only.',run=run.name,
    targetPid=pid,targetTid=tid,faultQpc=qf,qpcFrequency=freq,
    targetCSwitchCount=len(switches),exactPtAssociationCounts=dict(counts),
    targetPtCount=len(target_headers),targetPtOptionsCounts=dict(collections.Counter(h['options'] for h in target_headers)),
    nearestSwitchBeforeFault=before[-1] if before else None,
    nearestSwitchAfterFault=after[0] if after else None,
    targetCSwitchWithin10ms=window,targetPtWithin5ms=near,
    associationRule='Exact means equal raw QPC and CPU only. NearestPtHints are temporal candidates, not proof of event identity or outgoing/incoming capture semantics.',
    privacy='Unrelated process and thread IDs are replaced by target/outgoing/incoming equality booleans.',
    limits=['PT payload owner must be checked against code decoded from its own same-run mapping.',
            'An ETW switch crossing exception QPC is not proof the exception instruction is in PT.',
            'Full retained instruction history is not established by envelope timestamps or zero loss counters.'],
    sourceHashes={'targetResult':hashlib.sha256((run/'traced-target/result.json').read_bytes()).hexdigest(),
                  'rawPtHeaders':hashlib.sha256(a.headers.read_bytes()).hexdigest()})
out.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:result[k] for k in ['run','targetPid','targetTid','faultQpc','targetCSwitchCount','exactPtAssociationCounts','targetPtCount','targetPtOptionsCounts']},indent=2))
