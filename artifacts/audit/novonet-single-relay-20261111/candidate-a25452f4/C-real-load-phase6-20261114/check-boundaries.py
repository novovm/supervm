from pathlib import Path
from collections import defaultdict,Counter
import json,re
root=Path('target/c-real-load-phase6-20261114')
text=(root/'capture2-private.log').read_text(errors='replace');rows=[]
for line_number,line in enumerate(text.splitlines(),1):
 if line.startswith('REC stage='):
  v=dict(re.findall(r'(\w+)=([^ ]+)',line));r={k:(x if k=='stage' else int(x.replace('`',''),16)) for k,x in v.items()};r['log_line']=line_number;rows.append(r)
calls=defaultdict(list)
for r in rows:calls[(r['tid'],r['seq'])].append(r)
m=re.search(r'ModLoad: ([0-9a-f`]+) [0-9a-f`]+\s+c_phase6_status.exe',text);assert m,'image base'
base=int(m[1].replace('`',''),16);mapping=json.loads((root/'c-phase6-status-function-map.json').read_text());writer=base+int(mapping['functions']['string_writer'][0]['rva'],16)
patterns=[['FMT_BEGIN','FMT_CALL','PAD_ENTRY','TAIL_JUMP','WRITER_ENTRY','COPY_CALL'],['FMT_BEGIN','FMT_CALL','PAD_ENTRY','PAD_CALL','WRITER_ENTRY','COPY_CALL']]
failures=[];partial=[];checks=0;counts=Counter()
def check(ok,identity,rule):
 global checks
 checks+=1
 if not ok:failures.append({'capture_sequence':identity,'rule':rule})
for identity,rs in calls.items():
 stages=[r['stage'] for r in rs]
 if stages not in patterns:
  partial.append({'capture_sequence':identity,'ordinals':sorted(set(r['ordinal'] for r in rs)),'stages':stages,'call_target_rvas':sorted(set(hex(r.get('target',r.get('rax',base))-base) for r in rs if r['stage'] in ['TAIL_JUMP','PAD_CALL']))});continue
 counts['tail' if stages==patterns[0] else 'padded']+=1
 f,fc,p,j,w,c=rs
 check(len({r['ordinal'] for r in rs})==1,identity,'same format ordinal')
 check(f['len']<=f['cap'],identity,'String len within cap')
 for r in rs[1:]:
  if 'receiver' in r:check(r['receiver']==f['receiver'],identity,'same receiver '+r['stage'])
  if 'text' in r:check(r['text']==p['text'] and r['len']==p['len'],identity,'same text length '+r['stage'])
 check(j.get('target',j['rax'])==writer,identity,'mapped writer target')
 check(w['rsp']==j['rsp']-(8 if j['stage']=='PAD_CALL' else 0),identity,'call or tail stack relation')
 check(w['rbp']==j['rbp'],identity,'RBP preserved to writer entry')
 check(c['rsp']==w['rsp']-0x48,identity,'writer stack frame')
 check(c['dest']==c['buffer']+c['oldlen'],identity,'copy destination arithmetic')
e=json.loads((root/'capture2-exit.json').read_text());em=re.search(r'Last event: .*Exit process .*code ([0-9a-fA-F]+)',text);target_exit=int(em[1],16) if em else None
end=re.search(r'WINDOW_END tid=\w+ query=\w+ owner=\S+ calls=(\w+)',text);count=int(end[1],16) if end else None
check(target_exit==0 and e['debugger_exit_code']==0 and not e['timeout'],'run','target debugger exits zero')
check('PHASE6_STATUS_PASS' in text and 'PHASE6_DRAIN_PASS' in text,'run','status comparison and clean drain')
check(not re.search(r'^PHASE6_AV$|Syntax error|Range error|Numeric expression missing',text,re.M),'run','no AV or debugger syntax errors')
check(count==131072,'run','observed hash-byte format count matches retained input')
check('novovm_node::native_pipeline::service::rpc::RpcLifecycle::status' in text and 'novovm_node::native_pipeline::profile::snapshot' in text,'run','original status caller stack captured')
result={'boundary_records':len(rows),'capture_sequences':len(calls),'complete_groups':sum(counts.values()),'complete_branches':dict(counts),'partial_groups_not_passed':len(partial),'comparisons':checks,'failures':failures,'observed_format_inner_entries':count,'input_expected_hash_byte_formats':131072,'target_exit_code':target_exit,**e,'first_original_corruption_found':False,'original_fault_reproduced':False,'coverage':'selected records only in one full wrapped delivery snapshot; not all 131072 calls have boundary capture; debugger changes timing; old heap history and exact fault window not recreated'}
for name,data in [('capture-results-public.json',result),('capture-records-public.json',rows),('coverage-gaps-public.json',partial)]:
 (root/name).write_bytes((json.dumps(data,indent=2)+'\n').encode())
print(json.dumps(result,indent=2));raise SystemExit(1 if failures else 0)
