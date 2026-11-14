"""Public summaries only: never export test keys, payloads, locators or pointers."""
from pathlib import Path
import collections,datetime,hashlib,json,math,statistics,struct,sys
ROOT=Path('target/c-lowimpact-capture-20261114')
NO_DEBUG_HEAP=sys.argv[1:]==['--without-debug-heap']
if NO_DEBUG_HEAP:ROOT=ROOT/'heap-disabled'
OUT=Path(__file__).resolve().parent
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def quantile(xs,p):return sorted(xs)[max(0,math.ceil(len(xs)*p)-1)] if xs else None
def stats(xs):return {'n':len(xs),'p50':quantile(xs,.5),'p95':quantile(xs,.95),'p99':quantile(xs,.99)}
def ring(p):
 counts=collections.Counter();bad=[];generations=set();dropped=set();mismatch=0
 with p.open('rb') as f:
  assert f.read(8)==b'NCLTLS01';tid,total,retained,overwritten,reentrant,unregistered,capacity,stride=struct.unpack('<8Q',f.read(64))
  assert retained<=capacity and stride==128 and p.stat().st_size==72+retained*stride
  last_ns=0
  for i in range(retained):
   e=struct.unpack('<16Q',f.read(stride));seq,ns,kind,site,thread,call,obj,gen,length,cap,*_=e
   assert seq==total-retained+1+i and ns>=last_ns and thread==tid;last_ns=ns
   counts[str(kind)]+=1
   if kind==2:generations.add(gen)
   if kind==3:dropped.add(gen)
   if kind==11 and (length>cap or length==0 or e[14]==0):mismatch+=1
 return {'bytes':p.stat().st_size,'sha256':sha(p),'os_thread_id':tid,'total':total,'retained':retained,'overwritten':overwritten,'reentrant_lost':reentrant,'unregistered_lost_process_total':unregistered,'retained_event_counts':dict(counts),'retained_string_descriptor_anomalies':mismatch,'retained_creation_and_drop_generations':len(generations&dropped),'drop_without_retained_creation':len(dropped-generations),'capture_sequence_valid':True}
def main():
 rows=[]
 for run in sorted(ROOT.glob('qualification-*')):
  if not run.is_dir():continue
  exit=json.loads((run/'exit-public.json').read_text()) if (run/'exit-public.json').exists() else {'incomplete':True}
  resources=json.loads((run/'resource-samples-public.json').read_text()) if (run/'resource-samples-public.json').exists() else []
  metrics=[];queries=[];polls=[];intervals=[];events=collections.Counter();network=[];traces=[]
  for i in range(4):
   state=run/'state-private';mp=state/f'metrics-{i}.json'
   if not mp.exists():continue
   m=json.loads(mp.read_text());metrics.append({k:v for k,v in m.items() if k not in ['query_ns','query_start_ns','sampled_poll_ns']})
   queries+=m['query_ns'];polls+=m['sampled_poll_ns'];intervals += [b-a for a,b in zip(m['query_start_ns'],m['query_start_ns'][1:])]
   status=json.loads((state/f'status-{i}-private.json').read_text());ns=status['node'];net=status['novonet']
   for event in ns.get('native_network_probe',{}).get('events',[]):events[event['event']]+=1
   network.append({'installed_bound_peer_routes':net.get('installed_bound_peer_routes'),'connection_failures':net.get('connection_failures'),'peer_discovery_rpc':net.get('peer_discovery_rpc'),'native_delivery':{k:v for k,v in ns.get('stage_profile',{}).get('native_delivery',{}).items() if k in ['capacity','evicted','evicted_events','dropped','count']}})
   rp=state/f'events-{i}-private.bin'
   if rp.exists():traces.append(ring(rp))
  cpu=[];rss=[];switches=[];threadcounts=[]
  for i in range(4):
   points=[(r['elapsed'],n) for r in resources for n in r['nodes'] if n['index']==i and 10<=r['elapsed']<=45]
   if points:
    rss.append(max(n['rss'] for _,n in points));threadcounts.append(max(n['threads'] for _,n in points))
   if len(points)>1:
    dt=points[-1][0]-points[0][0];cpu.append((points[-1][1]['cpu_seconds']-points[0][1]['cpu_seconds'])/dt)
    switches.append(points[-1][1]['context_switches']-points[0][1]['context_switches'])
  elapsed=[m['elapsed_ns']/1e9 for m in metrics]
  rows.append({'run':run.name,'variant':'instrumented' if 'instrumented' in run.name else 'baseline','exit':exit,'nodes':metrics,'query_ns':stats(queries),'sampled_poll_ns':stats(polls),'query_interval_ns':stats(intervals),'mean_loops_per_second':statistics.mean(m['ticks']/(m['elapsed_ns']/1e9) for m in metrics) if metrics else None,'probe_event_counts':dict(events),'object_done_per_second':events['probe_object_done']/max(elapsed) if elapsed else None,'observations':network,'rings':traces,'resource_window_seconds':[10,45],'mean_cpu_cores':statistics.mean(cpu) if cpu else None,'peak_rss_bytes_by_node':rss,'context_switch_deltas_by_node':switches,'peak_thread_counts_by_node':threadcounts})
 ratios={}
 for name in ['query_ns','sampled_poll_ns']:
  b=[r[name]['p95'] for r in rows if r['variant']=='baseline' and r[name]['p95'] is not None];t=[r[name]['p95'] for r in rows if r['variant']=='instrumented' and r[name]['p95'] is not None]
  if b and t:ratios[name+'_p95']=statistics.median(t)/statistics.median(b)
 for metric in ['p50','p99']:
  b=[r['query_ns'][metric] for r in rows if r['variant']=='baseline' and r['query_ns'][metric] is not None];t=[r['query_ns'][metric] for r in rows if r['variant']=='instrumented' and r['query_ns'][metric] is not None]
  if b and t:ratios['query_ns_'+metric]=statistics.median(t)/statistics.median(b)
 b=[r['mean_loops_per_second'] for r in rows if r['variant']=='baseline' and r['mean_loops_per_second']];t=[r['mean_loops_per_second'] for r in rows if r['variant']=='instrumented' and r['mean_loops_per_second']]
 if b and t:ratios['loop_throughput']=statistics.median(t)/statistics.median(b)
 reasons=[]
 if len(rows)!=4 or not (ROOT/'comparison-complete.json').exists():reasons.append('qualification incomplete')
 if any(r.get('debugger_exits')!=[0]*4 or r.get('reason') for r in [v['exit'] for v in rows]):reasons.append('execution failure')
 for key in ['query_ns_p95','sampled_poll_ns_p95']:
  if ratios.get(key,0)>=1.2:reasons.append(key+' regression >=20%')
 if ratios.get('loop_throughput',1)<.8:reasons.append('loop throughput regression >=20%')
 if any(r['probe_event_counts'].get('probe_object_done',0)==0 or r['probe_event_counts'].get('probe_received_integrity_verified',0)==0 for r in rows):reasons.append('missing probe DONE or receive evidence')
 if any(t['reentrant_lost'] or t['unregistered_lost_process_total'] or t['retained_string_descriptor_anomalies'] for r in rows for t in r['rings']):reasons.append('capture loss or descriptor anomaly')
 report={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'runs':rows,'median_run_ratios':ratios,'qualification_rejection_reasons':reasons,'low_interference_qualification_passed':not reasons,'root_cause_found':False,'production_fix':False,'limitations':['ABBA has two runs per variant, not a statistical guarantee','loopback fresh test chain; original packets and scheduling unavailable','source probes do not capture std Formatter/Writer entry registers','only pre-registered owner threads have TLS rings; lost counters cover observed unregistered events','no scheduler trace; sampled process context switches are not event-order proof','overwrites deliberately retain only latest events; no deterministic replay']}
 report['debugger_disable_debug_heap']=NO_DEBUG_HEAP
 report['p95_only_gate_passed']=report['low_interference_qualification_passed']
 if ratios.get('query_ns_p50',0)>=1.2:
  report['qualification_rejection_reasons'].append('additional P50 review: query median regression >=20%; not accepted as generally low impact')
  report['low_interference_qualification_passed']=False
 rings=[t for r in rows for t in r['rings']]
 report['lifecycle_events_retained_at_end']=bool(rings) and all(any(t['retained_event_counts'].get(k,0) for k in ['1','2','3','4','20','21']) for t in rings)
 report['failure_evidence_ready']=False
 report['evidence_gaps']=['retained suffix is dominated by formatting; no retained lifecycle history in this run',
  'std Formatter/Writer entry registers not instrumented',
  'actual Carrier activity and sustained original 2048-record eviction not exercised',
  'original packet history, scheduler interleaving, production progress-file publication and multi-hour conditions not reproduced']
 if not NO_DEBUG_HEAP:
  report['qualification_rejection_reasons'].append('CDB created processes without -hd; debug heap may change allocation timing')
  report['low_interference_qualification_passed']=False
 p=OUT/('result-public.json' if NO_DEBUG_HEAP else 'result-debug-heap-public.json');assert not p.exists();p.write_bytes((json.dumps(report,indent=2)+'\n').encode());print(json.dumps({'ratios':ratios,'rejection_reasons':reasons}))
if __name__=='__main__':main()
