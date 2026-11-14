"""One bounded ABBA qualification; fresh loopback state, no deployed processes."""
from pathlib import Path
import datetime,hashlib,json,os,re,subprocess,time,psutil,sys
ROOT=Path('target/c-lowimpact-capture-20261114').resolve()
assert sys.argv[1:]==['--without-debug-heap'],'new runs require explicit -hd; old captures retained'
SERIES=ROOT/'heap-disabled'
CDB=Path('target/c-first-fault-20261113/sdk/extracted/Windows Kits/10/Debuggers/x64/cdb.exe').resolve()
DLL=Path('target/c-state-context-phase5-20261114/runtime/aoem_ffi.dll').resolve()
def now():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def save(p,d):
 with p.open('xb') as f:f.write((json.dumps(d,indent=2)+'\n').encode())
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def env(root):
 e=os.environ.copy()
 for k in list(e):
  if k.startswith(('NOVOVM_','AOEM_','JOINT_','LOW_','PHASE')) or k=='CREDENTIALS_DIRECTORY':e.pop(k)
 e.update({'JOINT_ROOT':str(root),'JOINT_LIBRARY':str(DLL),'NOVOVM_AOEM_DLL':str(DLL),'LOW_SECONDS':'45','AOEM_SEMANTIC_GRAPH_V2_WORKERS':'4','NOVOVM_PROFILE_NATIVE_DELIVERY':'1','NOVOVM_PROFILE_STAGES':'0','NOVOVM_PROFILE_CRITICAL_PATH':'0'})
 return e
def one(index,variant):
 run=SERIES/f'qualification-{index}-{variant}';run.mkdir();state=run/'state-private'
 image=ROOT/variant/'c-lowimpact-capture.exe';e=env(state);e['LOW_MODE']='prepare'
 with (run/'prepare-private.log').open('xb') as h:prepared=subprocess.run([str(image)],env=e,stdout=h,stderr=subprocess.STDOUT,timeout=60)
 save(run/'prepare-exit.json',{'exit':prepared.returncode,'utc':now()});assert prepared.returncode==0
 e['LOW_MODE']='run'; processes=[];handles=[];samples=[];reason=None;exception_at=None;start=time.monotonic()
 try:
  for i in range(4):
   script=run/f'debugger-{i}.txt';dump=run/f'failure-{i}-private.dmp'
   fatal='.echo LOWTRACE_EXCEPTION; .exr -1; .ecxr; r; kv; .dump /ma '+dump.as_posix()+'; q'
   lines=['.sympath '+str(image.parent)]+['sxe -c "'+fatal+'" '+c for c in ['av','0xc0000374','0xc0000409']]+['sxe -c ".echo TARGET_EXIT; .lastevent; q" epr','g']
   script.write_bytes(('\n'.join(lines)+'\n').encode());h=(run/f'node-{i}-private.log').open('xb');handles.append(h)
   own=e.copy();own['JOINT_INDEX']=str(i)
   processes.append(subprocess.Popen([str(CDB),'-hd','-cf',str(script),str(image)],env=own,stdout=h,stderr=subprocess.STDOUT))
  save(run/'started.json',{'utc':now(),'debugger_pids':[p.pid for p in processes],'binary_sha256':sha(image),'runtime_sha256':sha(DLL),'max_wall_seconds':120})
  while any(p.poll() is None for p in processes):
   if any(run.glob('failure-*-private.dmp')) and exception_at is None:
    exception_at=time.monotonic();reason='exception'
    (state/'STOP').write_bytes(b'exception in isolated peer; stop workload')
   # Let the debugger finish its full dump. Do not kill on file creation.
   if exception_at is not None and time.monotonic()-exception_at>60:
    reason='exception dump/exit grace expired';break
   if exception_at is None and time.monotonic()-start>120:reason='wall bound';break
   if psutil.disk_usage(str(ROOT)).free<12*1024**3:reason='space bound';break
   row={'elapsed':time.monotonic()-start,'nodes':[]}
   for i,p in enumerate(processes):
    try:
     for child in psutil.Process(p.pid).children():
      if child.name().lower()=='c-lowimpact-capture.exe':
       cpu=child.cpu_times();cs=child.num_ctx_switches()
       row['nodes'].append({'index':i,'pid':child.pid,'cpu_seconds':cpu.user+cpu.system,'rss':child.memory_info().rss,'threads':child.num_threads(),'context_switches':cs.voluntary+cs.involuntary})
    except (psutil.NoSuchProcess,psutil.AccessDenied):pass
   samples.append(row);time.sleep(0.5)
  for p in processes:
   if p.poll() is None and reason:subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
   p.wait(timeout=20)
 finally:
  for p in processes:
   if p.poll() is None:subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);p.wait(timeout=20)
  for h in handles:h.close()
  save(run/'resource-samples-public.json',samples)
  save(run/'exit-public.json',{'utc':now(),'debugger_exits':[p.returncode for p in processes],'reason':reason,'elapsed_seconds':time.monotonic()-start})
 # Exit of debugger alone is not target success. A partial/failed run stops ABBA.
 assert reason is None,reason
 for i in range(4):
  text=(run/f'node-{i}-private.log').read_text(errors='replace')
  assert re.search(r'^Last event:.*Exit process.*code 0\s*$',text,re.M),f'target {i} exit not proved'
  assert (state/f'metrics-{i}.json').is_file(),f'target {i} result absent'
 print(f'Completed {index} {variant}',flush=True)
def main():
 assert not SERIES.exists(),'single corrected qualification only'
 SERIES.mkdir()
 assert sha(DLL)=='5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'
 save(SERIES/'comparison-start.json',{'utc':now(),'debugger_disable_debug_heap':True,'sequence':['baseline','instrumented','instrumented','baseline'],'seconds_per_run':45,'query_period_seconds':10,'qualifier':'not original input replay; no business transfer; no relay restart','predeclared_gate':'reject for >=20% median query or poll P95 regression, >=20% loop throughput drop, missing target/probe evidence or capture errors; variance/incomplete context prevents root-cause inference'})
 for i,v in enumerate(['baseline','instrumented','instrumented','baseline']):one(i,v)
 save(SERIES/'comparison-complete.json',{'utc':now(),'root_cause_fixed':False})
if __name__=='__main__':main()
