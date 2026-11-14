from pathlib import Path
import json,statistics,collections
from decode import decode
HERE=Path(__file__).resolve().parent
ROOT=Path('target/c-partitioned-writer-20261114/qualification')
old=HERE.parent/'C-lowimpact-capture-20261114/analyze.py'
# Reuse statistics/resource/real-probe readers, not the obsolete ring decoder/gate.
code=old.read_text(encoding='utf-8');code=code[:code.index('def main():')]
scope={'__file__':str(old)};exec(compile(code,str(old),'exec'),scope)
stats=scope['stats']
rows=[]
for run in sorted(ROOT.glob('qualification-*')):
 resources=json.loads((run/'resource-samples-public.json').read_text());ex=json.loads((run/'exit-public.json').read_text())
 state=run/'state-private';queries=[];polls=[];intervals=[];metrics=[];rings=[];events=collections.Counter();net=[]
 for i in range(4):
  m=json.loads((state/f'metrics-{i}.json').read_text());metrics.append(m)
  queries+=m['query_ns'];polls+=m['sampled_poll_ns'];intervals += [b-a for a,b in zip(m['query_start_ns'],m['query_start_ns'][1:])]
  p=state/f'events-{i}-private.bin'
  if p.exists():rings.append(decode(p)[0])
  s=json.loads((state/f'status-{i}-private.json').read_text());n=s['node'];v=s['novonet']
  events.update(e['event'] for e in n.get('native_network_probe',{}).get('events',[]))
  net.append({'installed_bound_peer_routes':v.get('installed_bound_peer_routes'),'peer_discovery_rpc':v.get('peer_discovery_rpc'),'delivery_counters':{k:v for k,v in n.get('stage_profile',{}).get('native_delivery',{}).items() if k in ['capacity','evicted','evicted_events','dropped','count']}})
 cpu=[];rss=[];switch=[]
 for i in range(4):
  pts=[(r['elapsed'],p) for r in resources for p in r['nodes'] if p['index']==i and 10<=r['elapsed']<=45]
  if pts:rss.append(max(p['rss'] for _,p in pts))
  if len(pts)>1:
   cpu.append((pts[-1][1]['cpu_seconds']-pts[0][1]['cpu_seconds'])/(pts[-1][0]-pts[0][0]));switch.append(pts[-1][1]['context_switches']-pts[0][1]['context_switches'])
 rows.append(dict(run=run.name,variant='instrumented' if 'instrumented' in run.name else 'baseline',exit=ex,query_ns=stats(queries),poll_ns=stats(polls),query_interval_ns=stats(intervals),loops_per_second=statistics.mean(m['ticks']/(m['elapsed_ns']/1e9) for m in metrics),mean_cpu_cores=statistics.mean(cpu) if cpu else None,peak_rss_bytes=rss,context_switch_deltas=switch,mean_system_cpu_percent=statistics.mean(r['system_cpu_percent'] for r in resources),probe_events=dict(events),network=net,rings=rings))
ratios={}
for field,quantiles in [('query_ns',['p50','p95','p99']),('poll_ns',['p95'])]:
 for q in quantiles:
  b=[r[field][q] for r in rows if r['variant']=='baseline'];v=[r[field][q] for r in rows if r['variant']=='instrumented']
  ratios[field+'_'+q]=statistics.median(v)/statistics.median(b)
ratios['loops_per_second']=statistics.median(r['loops_per_second'] for r in rows if r['variant']=='instrumented')/statistics.median(r['loops_per_second'] for r in rows if r['variant']=='baseline')
reasons=[]
for k,v in ratios.items():
 if k!='loops_per_second' and v>=1.1:reasons.append(k+' >=10% regression')
if ratios['loops_per_second']<.9:reasons.append('loop throughput down >=10%')
if len(rows)!=4 or not (ROOT/'comparison-complete.json').exists():reasons.append('comparison incomplete')
traces=[t for r in rows for t in r['rings']]
if not traces or any(not t['complete_sampled_source_groups'] for t in traces):reasons.append('source Writer group absent')
if any(t['online_receiver_mismatches'] or t['receiver_mismatches_in_complete_groups'] or t['text_mismatches_at_pad_writer_in_complete_groups'] for t in traces):reasons.append('source boundary discrepancy')
if any(any(c['overwritten'] or c['reentrant_failed'] or c['unregistered_failed_process_total'] for c in t['channels'][:2]) for t in traces):reasons.append('A/B retained history incomplete')
if any(r['exit']['debugger_exits']!=[0]*4 or r['exit']['reason'] for r in rows):reasons.append('target/debugger failure')
if any(not r['probe_events'].get('probe_object_done') or not r['probe_events'].get('probe_received_integrity_verified') for r in rows):reasons.append('real probe delivery evidence absent')
out=dict(runs=rows,median_run_ratios=ratios,gate_rejection_reasons=reasons,short_timing_gate_passed=not reasons,long_failure_capture_qualified=False,original_fault_reproduced=False,first_corrupting_instruction_found=False,production_fix=False,
 evidence_gaps=['source-level entry hooks are not original machine-entry registers or pre-tailcall RSP','standard-library rebuild and injected calls change original code layout and scheduling','whole-call sampling 1/256 omits history; online comparison is not historical machine execution','only prewarmed owner thread records; not full refcount/free/reuse coverage','normal-stop ring decoder, no validated atomic exception-dump extraction','original packet order/multi-hour schedule/progress-file publication/active Carrier not reproduced','no lock-wait or scheduler trace; context-switch counts only'])
p=HERE/'result-public.json';assert not p.exists();p.write_bytes((json.dumps(out,indent=2)+'\n').encode());print(json.dumps(dict(ratios=ratios,rejected=reasons)))
