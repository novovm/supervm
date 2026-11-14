from pathlib import Path
from collections import defaultdict, Counter
import re,json
root=Path('target/c-state-context-phase5-20261114')
text=(root/'capture1-private.log').read_text(errors='replace')
rows=[]
for number,line in enumerate(text.splitlines(),1):
 if line.startswith('REC stage='):
  values=dict(re.findall(r'(\w+)=([^ ]+)',line))
  row={k:(v if k=='stage' else int(v.replace('`',''),16)) for k,v in values.items()}
  row['log_line']=number;rows.append(row)
calls=defaultdict(list)
for row in rows:calls[(row['tid'],row['seq'])].append(row)
base=int(re.search(r'ModLoad: ([0-9a-f`]+) [0-9a-f`]+\s+c_phase5_status.exe',text)[1].replace('`',''),16)
writer=base+int(json.loads((root/'c-phase5-status-function-map.json').read_text())['functions']['string_writer'][0]['rva'],16)
tail=['FMT_BEGIN','FMT_CALL','PAD_ENTRY','PREFIX_BEFORE','PREFIX_AFTER','TAIL_READY','TAIL_JUMP','WRITER_ENTRY','COPY_CALL']
padded=['FMT_BEGIN','FMT_CALL','PAD_ENTRY','PAD_PREFIX_BEFORE','PAD_PREFIX_AFTER','PAD_CALL','WRITER_ENTRY','COPY_CALL','PAD_RETURN']
failures=[];coverage=[];checks=0;counts=Counter()
def check(ok,identity,rule):
 global checks
 checks+=1
 if not ok:failures.append({'capture_sequence':identity,'rule':rule})
for identity,records in calls.items():
 stages=[r['stage'] for r in records]
 if stages not in (tail,padded):
  coverage.append({'capture_sequence':identity,'stages':stages,'targets_rva':sorted(set(hex(r[k]-base) for r in records for k in ('rax','target') if k in r and r['stage'] in ('TAIL_READY','TAIL_JUMP','PAD_CALL')))})
  # Partial records never qualify as a complete verified call.
  continue
 counts['tail' if stages==tail else 'padded_call']+=1
 by={r['stage']:r for r in records};f=records[0];p=by['PAD_ENTRY'];w=by['WRITER_ENTRY'];c=by['COPY_CALL']
 check(f['len']<=f['cap'],identity,'String length within capacity')
 for row in records[1:]:
  if 'receiver' in row:check(row['receiver']==f['receiver'],identity,'receiver '+row['stage'])
  if 'text' in row:check(row['text']==p['text'] and row['len']==p['len'],identity,'text and length '+row['stage'])
 pb,pa=records[3:5]
 for reg in ['rbx','rbp','rdi','rsi','r12','r13','r14','r15']:check(pb[reg]==pa[reg],identity,'prefix preserves '+reg)
 if stages==tail:
  t=by['TAIL_READY'];j=by['TAIL_JUMP']
  check(j['rsp']==t['rsp']+0x98 and w['rsp']==j['rsp'],identity,'tail stack restored')
  check(t['rax']==writer and j['rax']==writer,identity,'tail target is captured writer')
  check(w['rbp']==j['rbp'],identity,'tail RBP unchanged into writer')
 else:
  pc=by['PAD_CALL'];pr=by['PAD_RETURN']
  check(w['rsp']==pc['rsp']-8 and pr['rsp']==pc['rsp'],identity,'ordinary call return stack')
  check(pc['target']==writer,identity,'ordinary target is captured writer')
  check(pr['result']==0,identity,'ordinary writer success')
 check(c['rsp']==w['rsp']-0x48,identity,'writer stack frame')
 check(c['dest']==c['buffer']+c['oldlen'],identity,'copy destination')
window=[l for l in text.splitlines() if l.startswith(('WINDOW_','PRODUCER_BOUNDARY','OWNER ','PRODUCER completed=','PHASE5_STATUS_PASS','PHASE5_DRAIN_PASS'))]
exit_data=json.loads((root/'capture1-exit.json').read_text())
em=re.search(r'Last event: .*Exit process .*code ([0-9a-fA-F]+)',text)
target_exit=int(em[1],16) if em else None
check(target_exit==0 and exit_data['debugger_exit_code']==0 and not exit_data['timeout'],'run','actual program and debugger exit zero')
check(not re.search(r'^PHASE5_AV$|Syntax error|Numeric expression missing',text,re.M),'run','no AV or debugger parser failure')
check('PHASE5_STATUS_PASS' in text and 'PHASE5_DRAIN_PASS' in text,'run','real status roundtrip and shutdown')
check('novovm_node::native_pipeline::service::rpc::RpcLifecycle::status' in text and 'novovm_node::native_pipeline::profile::snapshot' in text,'run','actual caller stacks captured')
status=re.search(r'PHASE5_STATUS_PASS producer_before=(\d+) producer_after=(\d+) delivery_contended=(\d+) events=(\d+) encoded_bytes=(\d+)',text)
result={'capture_sequence_count':len(calls),'boundary_records':len(rows),'complete_correlated_calls':sum(counts.values()),'complete_branch_counts':dict(counts),'partial_sequences_not_passed':len(coverage),'checked_comparisons':checks,'failures':failures,'target_exit_code':target_exit,**exit_data,
 'observed_owner_thread_decimal':int(re.search(r'WINDOW_BEGIN tid=(\w+)',text)[1],16),'producer_thread_decimal':int(re.search(r'^PRODUCER_BOUNDARY tid=(\w+)',text,re.M)[1],16),
 'producer_before':int(status[1]),'producer_after':int(status[2]),'delivery_contended_events':int(status[3]),'delivery_snapshot_events':int(status[4]),'serialized_bytes':int(status[5]),
 'original_fault_reproduced':False,'first_original_corruption_found':False,'scope':'one actual RpcLifecycle status with test-only ResidentNode, real AOEM and new local state; not deployed node, production identity or real relay',
 'coverage_limit':'33 partial capture sequences use another write_str body or have repeated/missing breakpoint observations; not 481 proven unique operations; no full-call pass claimed for partial sequences'}
(root/'capture-results-public.json').write_text(json.dumps(result,indent=2)+'\n')
(root/'capture-records-public.json').write_text(json.dumps(rows,indent=2)+'\n')
(root/'coverage-gaps-public.json').write_text(json.dumps(coverage,indent=2)+'\n')
(root/'thread-timeline-public.txt').write_text('\n'.join(window)+'\n')
print(json.dumps(result,indent=2))
raise SystemExit(1 if failures else 0)
