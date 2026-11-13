from pathlib import Path
import subprocess,os,json,hashlib,datetime
b=Path(__file__).parent;r=b/'unwind-isolation-01'
exe=r/'build/release/c-stackcapture-profile.exe'
cdb=b/'sdk/extracted/Windows Kits/10/Debuggers/x64/cdb.exe'
env=os.environ.copy()
for k in ['NOVOVM_PROFILE_STAGES','NOVOVM_PROFILE_CRITICAL_PATH','NOVOVM_PROFILE_NATIVE_DELIVERY']:env[k]='1'
rows=[]
for mode in ['clean','trace']:
 out=r/mode;out.mkdir(exist_ok=False)
 dump=out/'fault-private.dmp'
 cmds=out/'capture.cdb'
 cmds.write_text(f'sxe -c ".echo ISOLATION_FIRST_AV; .exr -1; .ecxr; r; kv; .dump /ma {dump.as_posix()}; q" av\nsxe -c ".echo ISOLATION_EXIT; .lastevent; q" epr\ng\n')
 with (out/'debugger-private.log').open('wb') as log:
  p=subprocess.Popen([str(cdb),'-cf',str(cmds),str(exe),mode],stdout=log,stderr=subprocess.STDOUT,env=env)
  try:p.wait(timeout=75)
  except subprocess.TimeoutExpired:
   p.kill();p.wait();raise RuntimeError('Isolated debugger exceeded bound; no retry')
 text=(out/'debugger-private.log').read_text(errors='replace')
 fault='\nISOLATION_FIRST_AV\n' in text
 rows.append({'mode':mode,'debugger_exit':p.returncode,'first_AV':fault,'target_exit0':'Exit process ' in text and ', code 0' in text,'stack_capture_line':next((x for x in text.splitlines() if x.startswith('STACK_CAPTURE ')),None),'dump_created':dump.exists(),'log_sha256':hashlib.sha256((out/'debugger-private.log').read_bytes()).hexdigest()})
 if fault:break
result={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Isolated actual profile/JSON source with a bounded global-System-allocator stack-capture hook; no AOEM, PageHeap registry, node, DB or original identities. Does not duplicate exact node binary/stack.', 'runs':rows,'node_started':False,'registry_changed':False,'native_crash_fixed':False}
(r/'result-public.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
