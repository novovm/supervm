from pathlib import Path
import json,re,hashlib
root=Path(r'D:/WEB3_AI/SUPERVM')
run=root/'target/c-pt-live-qualification-20261115/run-filecs01'
files=[run/'decode/selected-private/private'/s for s in ['020730-3074711919015','020731-3074712146979']]+[run/'decode/fault-adjacent-private/private/340666']
rows=[]
for stem in files:
    raw=Path(str(stem)+'.pt.bin').read_bytes()
    packets=Path(str(stem)+'.packets.txt').read_text()
    covered=set(); seq=[]
    for line in packets.splitlines():
        m=re.match(r'^([0-9a-f]{16})\s+([0-9a-f]+)\s+(\S+)',line)
        if not m:continue
        off=int(m[1],16); data=bytes.fromhex(m[2]); typ=m[3]
        assert raw[off:off+len(data)]==data
        assert not covered.intersection(range(off,off+len(data)))
        covered.update(range(off,off+len(data)));seq.append({'offset':off,'bytes':data.hex(),'type':typ})
    residual=[i for i,b in enumerate(raw) if i not in covered and b!=0]
    assert not residual and {p['type'] for p in seq}<={'psb','cbr','psbend'}
    rows.append({'fragment':stem.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'sequence':seq,'packetBytes':len(covered),'zeroPaddingBytes':len(raw)-len(covered),'unexplainedNonzeroBytes':len(residual)})
c=json.loads((root/'artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-pt-cswitch-20261115/controller-result.json').read_text(encoding='utf-8-sig'))
cmd={r['tag']:r for r in c['commands']}
out={'scope':'Existing raw bytes only; no new capture or node launch','fragments':rows,'allBytesExplainedAsControlPacketsOrZeroPadding':True,'tracedTargetEndUtc':cmd['traced-fault']['endUtc'],'wprStopStartUtc':cmd['wpr-stop']['startUtc'],'wprStopEndUtc':cmd['wpr-stop']['endUtc'],'wprStopExit':cmd['wpr-stop']['exitCode'],'conclusion':'No hidden TIP/TNT bytes left unparsed in these three fragments. Controller requests stop after target exit; cannot attribute missing fault path to a controller stop before the exception. Hardware trace enable/filter/reset state was not captured, so underlying loss mechanism remains unproved.'}
path=root/'artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-pt-tail-bytes-20261115/raw-audit-public.json'
path.write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps({'fragments':len(rows),'allBytesAccounted':True,'sizes':[x['bytes'] for x in rows]}))

