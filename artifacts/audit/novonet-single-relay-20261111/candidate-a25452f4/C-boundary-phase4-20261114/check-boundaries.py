from pathlib import Path
import re, json, hashlib
root=Path('target/c-boundary-phase4-20261114')
text=(root/'capture3-private.log').read_text(errors='replace')
rows=[]
for line in text.splitlines():
    if line.startswith('REC stage='):
        values=dict(re.findall(r'(\w+)=([^ ]+)',line))
        rows.append({k:(v if k=='stage' else int(v.replace('`',''),16)) for k,v in values.items()})
assert rows, 'no actual boundary samples'
tail=['FMT_BEGIN','FMT_CALL','PAD_ENTRY','PREFIX_BEFORE','PREFIX_AFTER','TAIL_READY','TAIL_JUMP','WRITER_ENTRY','COPY_CALL']
padded=['FMT_BEGIN','FMT_CALL','PAD_ENTRY','PAD_PREFIX_BEFORE','PAD_PREFIX_AFTER','PAD_CALL','WRITER_ENTRY','COPY_CALL','PAD_RETURN']
calls={}
for row in rows:calls.setdefault((row['tid'],row['seq']),[]).append(row)
failures=[]
checks=0
counts={'tail':0,'padded_call':0}
base=int(re.search(r'ModLoad: ([0-9a-f`]+) [0-9a-f`]+\s+c_profile_boundary.exe',text)[1].replace('`',''),16)
writer=base+int(json.loads((root/'c-profile-boundary-function-map.json').read_text())['functions']['string_writer'][0]['rva'],16)
def check(ok,identity,rule):
    global checks
    checks+=1
    if not ok:failures.append({'call':identity,'rule':rule})
for identity,records in calls.items():
    stages=[r['stage'] for r in records]
    is_tail=stages==tail;is_padded=stages==padded
    check(is_tail or is_padded,identity,'complete ordered boundary records')
    if not(is_tail or is_padded):continue
    counts['tail' if is_tail else 'padded_call']+=1
    f,fc,p,pb,pa=records[:5]
    bystage={x['stage']:x for x in records};w=bystage['WRITER_ENTRY'];c=bystage['COPY_CALL']
    check(f['cap']==0 and f['buffer']==1 and f['len']==0,identity,'original empty String at formatting entry')
    for row in records[1:]:
        if 'receiver' in row:check(row['receiver']==f['receiver'],identity,'receiver '+row['stage'])
        if 'text' in row:check(row['text']==p['text'] and row['len']==p['len'],identity,'text/length '+row['stage'])
    for reg in ['rbx','rbp','rdi','rsi','r12','r13','r14','r15']:
        check(pb[reg]==pa[reg],identity,'prefix preserves '+reg)
    if is_tail:
        t=bystage['TAIL_READY'];j=bystage['TAIL_JUMP']
        check(j['rsp']==t['rsp']+0x98 and w['rsp']==j['rsp'],identity,'tail stack restoration')
        check(t['rax']==writer and j['rax']==writer,identity,'tail target matches actual writer')
        check(w['rbp']==j['rbp'],identity,'tail preserves restored frame register into writer')
    else:
        pc=bystage['PAD_CALL'];pr=bystage['PAD_RETURN']
        check(w['rsp']==pc['rsp']-8 and pr['rsp']==pc['rsp'],identity,'ordinary call pushes return address and restores')
        check(pc['target']==writer,identity,'ordinary indirect target matches actual writer')
        check(pr['result']==0,identity,'padded writer returned success')
    check(c['rsp']==w['rsp']-0x48,identity,'writer stack allocation')
    check(c['dest']==c['buffer']+c['oldlen'],identity,'destination buffer plus length')
check(len(calls)==64,'run','64 byte formatting calls from one original snapshot')
check(len({r['tid'] for r in rows})==1,'run','one correlated thread')
check('PHASE4_SNAPSHOT_PASS events=1 hash_bytes=64 original_failure_reproduced=false' in text,'run','snapshot content assertions')
check(not any(l.strip() in ('PHASE4_MISMATCH','PHASE4_AV') for l in text.splitlines()),'run','no breakpoint mismatch or AV')
debug_exit=json.loads((root/'capture3-exit.json').read_text())['debugger_exit_code']
exit_match=re.search(r'Last event: .*Exit process .*code ([0-9a-fA-F]+)',text)
target_exit=int(exit_match[1],16) if exit_match else None
check(debug_exit==0 and target_exit==0,'run','debugger and target both exit zero')
check(not re.search(r'Numeric expression missing|Syntax error',text),'run','no debugger parser error')
result={'debugger_exit_code':debug_exit,'target_exit_code':target_exit,'branch_counts':counts,'capture_build_sha256':json.loads((root/'c-profile-boundary-function-map.json').read_text())['sha256'],
        'calls':len(calls),'records':len(rows),'checks':checks,'failures':failures,
        'original_failure_reproduced':False,'first_original_corruption_found':False,
        'scope':'one original a254 profile snapshot with test-only records, under debugger; not deployed node execution'}
(root/'capture-results-public.json').write_bytes((json.dumps(result,indent=2)+'\n').encode())
(root/'capture-records-public.json').write_bytes((json.dumps(rows,indent=2)+'\n').encode())
print(json.dumps(result,indent=2))
raise SystemExit(1 if failures else 0)
