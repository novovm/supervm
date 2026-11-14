"""Offline numeric-only projection of old run-01 retention; never starts a target."""
from pathlib import Path
from collections import defaultdict
import hashlib, json, xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'run-01'
OUT = Path(__file__).resolve().parent / 'old-retention-public.json'
if OUT.exists():
    raise SystemExit('Refusing to overwrite retained evidence')

def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))

def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()

target = load(RUN / 'traced-target/result.json')
records = load(RUN / 'decode/ipt-headers-private.json')
ns = {'e': 'http://schemas.microsoft.com/win/2004/08/events/event',
      't': 'http://schemas.microsoft.com/win/2004/08/events/trace'}
drop = []
headers = []
for _, el in ET.iterparse(RUN / 'decode/tracerpt-private.xml', events=['end']):
    if el.tag != '{' + ns['e'] + '}Event':
        continue
    opname = el.findtext('e:RenderingInfo/e:Opcode', namespaces=ns)
    event_guid = el.findtext('t:ExtendedTracingInfo/t:EventGuid', namespaces=ns)
    if event_guid == '{68fdd900-4a3e-11d1-84f4-0000f80464e3}':
        pairs = [(d.attrib['Name'], (d.text or '').strip())
                 for d in el.findall('e:EventData/e:Data', ns)]
        if opname == 'LastDroppedTimes':
            fields = dict(pairs)
            drop.append({'count': int(fields['TimeStampCount']),
                         'timestamps': [int(v) for k, v in pairs if k == 'TimeStamp']})
        elif opname == 'Header':
            d = dict(pairs)
            headers.append({k: d.get(k) for k in
                            ['NumberOfProcessors', 'EventsLost', 'BuffersLost',
                             'PerfFreq', 'LogFileMode', 'BuffersWritten', 'BufferSize']})
    el.clear()
assert drop and all(d == drop[0] for d in drop), 'Drop metadata not uniform'
assert drop[0]['count'] == len(drop[0]['timestamps']) == 32
by_cpu = defaultdict(list)
for r in records:
    by_cpu[r['cpu']].append(r)
q0, qf, freq = target['workStartQpc'], target['faultQpc'], target['qpcFrequency']
mapping = []
for index, timestamp in enumerate(drop[0]['timestamps']):
    rows = by_cpu[index]
    first, last = min(r['qpc'] for r in rows), max(r['qpc'] for r in rows)
    mapping.append({'metadataIndex': index, 'cpu': index, 'lastDroppedTimeQpc': timestamp,
                    'ptCount': len(rows), 'firstRetainedPtQpc': first,
                    'lastRetainedPtQpc': last,
                    'nonzeroMetadataEqualsCpuFirstPt': timestamp == first if timestamp else None,
                    'firstPtSecondsAfterWorkStart': (first - q0) / freq,
                    'nonzeroMetadataAfterFault': bool(timestamp and timestamp > qf)})
target_rows = [r for r in records if r['pid'] == target['pid'] and r['tid'] == target['workTid']]
pid_rows = [r for r in records if r['pid'] == target['pid']]
tid_rows = [r for r in records if r['tid'] == target['workTid']]
output = {
    'scope': 'Offline audit of already captured run-01. No trace or target run.',
    'sourceHashes': {str(p.relative_to(ROOT)): sha(p) for p in
        [RUN / 'qualification.etl', RUN / 'decode/tracerpt-private.xml',
         RUN / 'decode/ipt-headers-private.json', RUN / 'traced-target/result.json']},
    'timestampDerivation': 'All deltas use raw internal PT QPC and target work/fault QPC; divide differences by qpcFrequency. No display-timezone conversion.',
    'qpcFrequency': freq, 'workStartQpc': q0, 'faultQpc': qf,
    'traceHeaders': headers,
    'lastDroppedTimes': {'eventGuid': '{68fdd900-4a3e-11d1-84f4-0000f80464e3}',
        'opcode': 82, 'renderedName': 'LastDroppedTimes',
        'observedIdenticalEventCount': len(drop), 'arrayLength': 32,
        'nonzeroCount': sum(bool(x) for x in drop[0]['timestamps']),
        'allNonzeroIndexesMatchSameCpuFirstPt': all(
            m['nonzeroMetadataEqualsCpuFirstPt'] for m in mapping if m['lastDroppedTimeQpc']),
        'nonzeroBoundaryAfterFaultCount': sum(m['nonzeroMetadataAfterFault'] for m in mapping),
        'mapping': mapping,
        'interpretationBoundary': 'Recorded discarded-time metadata is direct evidence; indexed CPU correspondence is independently supported by 29 exact matches. It is not a complete target scheduling history or proof that every missing packet was caused by ring overwrite.'},
    'allCpuTargetSearch': {'rawRecordCount': len(records), 'cpuCount': len(by_cpu),
        'targetPid': target['pid'], 'targetTid': target['workTid'],
        'pidAndTidMatchCount': len(target_rows), 'pidOnlyMatchCount': len(pid_rows),
        'tidOnlyMatchCount': len(tid_rows),
        'matchingCpus': sorted({r['cpu'] for r in target_rows}),
        'firstPtQpc': min(r['qpc'] for r in target_rows),
        'lastPtQpc': max(r['qpc'] for r in target_rows),
        'unobservedTailSeconds': (qf - max(r['qpc'] for r in target_rows)) / freq,
        'otherCpuTargetPayloadFound': any(r['cpu'] != 1 for r in target_rows)},
    'conclusions': [
        'The target tail gap is not explained by only xperf Unknown/-1 PID association: raw payload identity was checked on all CPUs.',
        'Lost Events/Buffers=0 does not mean complete retention; the same ETL contains nonzero LastDroppedTimes metadata.',
        'Per-CPU retention begins at materially different times; some first-retained boundaries occur after the declared test exception.',
        'Missing final target CPU residency and PT cannot be reconstructed from retained CSwitch/PT alone.',
        'File mode with a bounded disk/time budget is a separately verifiable candidate, not a proven fix; same sampling/PT trigger can still leave gaps.',
        'This is qualification evidence only, not an original NOVOVM failure or first RBP corruption capture.'
    ]}
OUT.write_text(json.dumps(output, indent=2) + '\n', encoding='utf-8')
print(json.dumps({k: output[k] for k in ['allCpuTargetSearch']}, indent=2))
print('Nonzero boundaries:', output['lastDroppedTimes']['nonzeroCount'])
print('After fault:', output['lastDroppedTimes']['nonzeroBoundaryAfterFaultCount'])
print('Identical metadata records:', len(drop))
