"""ONE original-binary, fresh-state NOVONET run, with an interference stop gate.
Never opens original state/keys. No replay claim, artificial callee, TTD or rebuild.
OFF/ON/OFF phases occur within the same process lifecycle; no automatic retry.
"""
raise SystemExit(
    "RETIRED: sustained CDB callbacks failed the interference gate and the "
    "NtContinue stop guard conflicted with shutdown. No target will be started."
)

import csv, ctypes, hashlib, io, json, os, pathlib, re, shutil, socket
import statistics, struct, subprocess, sys, time
from ctypes import wintypes as W
from datetime import datetime, timezone

repo = pathlib.Path(__file__).resolve().parents[2]
out = pathlib.Path(sys.argv[1]).resolve()
assert out.is_relative_to(repo / 'target') and not out.exists()
assert shutil.disk_usage(out.parent).free >= 8 * 1024**3
out.mkdir()
sid = list(csv.reader(io.StringIO(subprocess.check_output(['whoami','/user','/fo','csv','/nh'], text=True))))[0][1]
subprocess.run(['icacls',str(out),'/inheritance:r','/grant:r',f'*{sid}:(OI)(CI)F','*S-1-5-18:(OI)(CI)F','*S-1-5-32-544:(OI)(CI)F'],check=True,stdout=subprocess.DEVNULL)
(out/'executed-script.py').write_bytes(pathlib.Path(__file__).read_bytes())
def utc(): return datetime.now(timezone.utc).isoformat()
def save(name, value):
    with (out/name).open('x',encoding='utf-8',newline='\n') as f:
        json.dump(value,f,indent=2);f.write('\n')
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
exe=repo/'target/c-symbolized-original-20261114/novovm-node.exe'
runtime=pathlib.Path('C:/ProgramData/NOVOVM/releases/aoem-9662470d-c-20261113/aoem')
dll=runtime/'windows/core/bin/aoem_ffi.dll'
cdb=repo/'target/c-first-fault-20261113/sdk/extracted/Windows Kits/10/Debuggers/x64/cdb.exe'
test=repo/'target/c-joint-network-capture-20261114/run-state-private'
assert sha(exe)=='1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d'
assert sha(dll)=='5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'
assert sha(test/'test-setup-private.json')=='6c61c2c77bbb6a2bb11232be23602afc54124f0d36f30349ae632171d86f3c43'
nt=pathlib.Path('C:/Windows/System32/ntdll.dll');b=nt.read_bytes()
pe=struct.unpack_from('<I',b,60)[0];ns=struct.unpack_from('<H',b,pe+6)[0]
section=pe+24+struct.unpack_from('<H',b,pe+20)[0]
def code(rva,n):
    for i in range(ns):
        vs,va,size,raw=struct.unpack_from('<4I',b,section+i*40+8)
        if va<=rva and rva+n<=va+size:return b[raw+rva-va:raw+rva-va+n]
    raise AssertionError('unbacked instruction')
for rva,h in [(0x59dfc,'488d6c24b0'),(0x59e01,'4881ec50010000'),(0x5a108,'488b4d30')]:
    assert code(rva,len(bytes.fromhex(h)))==bytes.fromhex(h)

# Keep real kernel process handles before the debuggee leaves its first break.
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[W.DWORD,W.BOOL,W.DWORD];k.OpenProcess.restype=W.HANDLE
k.CloseHandle.argtypes=[W.HANDLE];k.CloseHandle.restype=W.BOOL
k.WaitForSingleObject.argtypes=[W.HANDLE,W.DWORD];k.WaitForSingleObject.restype=W.DWORD
k.GetExitCodeProcess.argtypes=[W.HANDLE,ctypes.POINTER(W.DWORD)];k.GetExitCodeProcess.restype=W.BOOL
k.GetProcessTimes.argtypes=[W.HANDLE,*([ctypes.POINTER(W.FILETIME)]*4)];k.GetProcessTimes.restype=W.BOOL
k.QueryFullProcessImageNameW.argtypes=[W.HANDLE,W.DWORD,W.LPWSTR,ctypes.POINTER(W.DWORD)];k.QueryFullProcessImageNameW.restype=W.BOOL
k.DebugBreakProcess.argtypes=[W.HANDLE];k.DebugBreakProcess.restype=W.BOOL
k.TerminateProcess.argtypes=[W.HANDLE,W.UINT];k.TerminateProcess.restype=W.BOOL
def ok(v):
    if not v:raise ctypes.WinError(ctypes.get_last_error())
def ticks(t):return (t.dwHighDateTime<<32)|t.dwLowDateTime
def proc_info(h):
    t=[W.FILETIME() for _ in range(4)];ok(k.GetProcessTimes(h,*[ctypes.byref(x) for x in t]))
    return dict(created_filetime=ticks(t[0]),cpu_seconds=(ticks(t[2])+ticks(t[3]))/1e7)
def open_target(pid):
    h=k.OpenProcess(0x1FFFFF,False,pid);ok(h)
    size=W.DWORD(32768);buf=ctypes.create_unicode_buffer(size.value)
    ok(k.QueryFullProcessImageNameW(h,0,buf,ctypes.byref(size)))
    assert pathlib.Path(buf.value).resolve()==exe.resolve()
    assert k.WaitForSingleObject(h,0)==258
    return h,proc_info(h)
def exited(h):
    wait=k.WaitForSingleObject(h,0)
    if wait==258:return None
    assert wait==0, ('wait failure',wait)
    value=W.DWORD();ok(k.GetExitCodeProcess(h,ctypes.byref(value)))
    return dict(unsigned=value.value,hex=f'0x{value.value:08x}',signed=ctypes.c_int32(value.value).value)

setup=json.loads((test/'test-setup-private.json').read_text());ports=[];reserved=[]
for _ in range(8):
    s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);s.bind(('127.0.0.1',0));reserved.append(s);ports.append(s.getsockname()[1])
env={a:v for a,v in os.environ.items() if not (a.startswith('NOVOVM_') or a.startswith('AOEM_'))}
env.update(NOVOVM_AOEM_ROOT=str(runtime),NOVOVM_AOEM_DLL=str(dll),NOVOVM_AOEM_MANIFEST=str(runtime/'manifest/aoem-manifest.json'),AOEM_DLL_MANIFEST=str(runtime/'manifest/aoem-manifest.json'),AOEM_DLL_MANIFEST_REQUIRED='1',NOVOVM_AOEM_RUNTIME_PROFILE=str(runtime/'config/aoem-runtime-profile.json'),NOVOVM_AOEM_PLUGIN_DIR=str(runtime/'windows/core/plugins'),AOEM_SEMANTIC_GRAPH_V2_WORKERS='1',NOVOVM_NODE_MODE='native_execution_tick',NOVOVM_NATIVE_EXECUTION_START='create',NOVOVM_NATIVE_EXECUTION_RUN_MS='4500000',NOVOVM_PROFILE_NATIVE_DELIVERY='1',NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS='5000',RUST_BACKTRACE='full')
for i in range(4):
    network=dict(probe_control_file=str(out/f'probe-{i}.nvprobe'),relays=[],nameserver='127.0.0.1',bind_address=f'127.0.0.1:{ports[i+4]}',public_record_file=str(out/f'locator-{i}.json'),carrier_identity_file=str(out/f'carrier-{i}.sealed'),peer_records=[str(out/f'locator-{j}.json') for j in range(4) if j!=i],discovery=False,discovery_seeds=[],relay_forwarding=False,fallback_after_ms=1000,retry_ms=1000,udp_recovery_ms=10000)
    save(f'network-{i}-private.json',network)
    cfg=dict(profile='native_execution_tick',library=str(dll),database=str(out/f'new-test-db-{i}'),rpc_addr=None,signing_key_file=str(test/f'test-validator-{i}.sealed'),transport_key_file=str(test/f'test-transport-{i}.sealed'),key_storage='protected_v1',udp_addr=f'127.0.0.1:{ports[i]}',peers=[dict(public_key=v['public_key'],address=f'127.0.0.1:{ports[j]}')for j,v in enumerate(setup['genesis']['validators'])if j!=i],learn_authenticated_peer_routes=False,novonet_config_file=str(out/f'network-{i}-private.json'),native_client_limit=0,genesis=setup['genesis'],uca_initialization=None,workers=1,batch_size=1)
    save(f'config-{i}-private.json',cfg)

def cmd(name,lines):(out/(name+'.cmd')).write_text('\n'.join(lines)+'\n',encoding='ascii',newline='\n')
def use(name):return '$$><'+(out/(name+'.cmd')).as_posix()+';'
def event(name):return f'.printf "EVENT {name} group=%x tid=%x rip=%p rsp=%p rbp=%p\\n", @$t10, @$tid, @rip, @rsp, @rbp'
def stop(reason):return '.echo CONTROLLER_ACTION debugger_q '+reason+'; q'
def guard(body):return 'r @$t19 = @$t19 + 1; .if (@$t19 > 0n100000) { '+stop('hit_limit')+' } .else { '+body+' }'
def hw(slot,rva,condition,name):return f'ba{slot} e 1 ntdll+{rva:x} "'+guard('.if ('+condition+') { '+use(name)+' } .else { gc }')+'"'
mismatch='.echo RBP_MISMATCH; '+event('MISMATCH')+'; kv; .dump /ma '+(out/'mismatch-private.dmp').as_posix()+'; bc *; .echo MISMATCH_CAPTURED_CONTINUE_TO_NATIVE_CHECK; g'
cmd('arm',['ba0 e 1 novovm_node+84eed4 "'+guard('.if (@rdx != 0) { '+use('outer')+' } .else { gc }')+'"','g'])
cmd('outer',['bc 0','r @$t10 = @$t10 + 1','r @$t0 = @$tid','r @$t1 = @rsp','r @$t4 = 0','r @$t7 = 0','r @$t11 = 0',event('OUTER_CALL'),hw(1,0x59dfc,'(@$tid == @$t0) & (@rsp < @$t1) & (poi(@rsp+38) == ntdll+5bddd)','before'),'g'])
cmd('before',['bc 1','r @$t4 = @rsp','r @$t12 = poi(@rsp+38)',event('LEA_BEFORE'),hw(2,0x59e01,'(@$tid == @$t0) & (@rsp == @$t4) & (poi(@rsp+38) == @$t12)','after'),'g'])
cmd('after',['bc 2',event('LEA_AFTER'),'.if (@rbp != (@$t4-50)) { '+mismatch+' }',hw(0,0x5a05b,'(@$tid == @$t0) & (@rsp == (@$t4-150))','child_before'),hw(1,0x5a060,'(@$tid == @$t0) & (@rsp == (@$t4-150))','child_after'),'g'])
cmd('child_before',['.if (@$t7 != 0) { '+stop('nested_child_gap')+' }','r @$t7 = 1','r @$t11 = @$t11+1','r @$t6 = @rbp',event('CHILD_BEFORE'),'g'])
cmd('child_after',['.if (@$t7 != 1) { '+stop('missing_child_entry')+' }',event('CHILD_AFTER'),'.if (@rbp != @$t6) { '+mismatch+' }','r @$t7 = 0','g'])
# This check remains armed for ALL threads/callers while instruction monitoring is ON.
cmd('check',['r @$t18 = @$t18+1','.if (@rbp != (@rsp+100)) { '+mismatch+' }','.if ((@$t0 != 0) & (@$tid == @$t0) & (@rsp == (@$t4-150))) { '+event('COOKIE_PRECHECK')+'; .if (@$t7 != 0) { '+stop('unfinished_child')+' }; bc 0; bc 1; bc 2; r @$t0 = 0; '+use('arm')+' } .else { gc }'])
cmd('on',['r @$t0 = 0','r @$t4 = 0','r @$t7 = 0','r @$t10 = 0','r @$t18 = 0','r @$t19 = 0','bp10 ntdll!RtlUnwindEx "'+stop('unwind_gap')+'"','bp11 ntdll!RtlRestoreContext "'+stop('restore_gap')+'"','bp12 ntdll!NtContinue "'+stop('continue_gap')+'"','bp13 novovm_node+84eed9 ".if ((@$t0 != 0) & (@$tid == @$t0) & (@rsp == @$t1)) { '+stop('outer_return_without_group')+' } .else { gc }"',hw(3,0x5a108,'1','check'),'.echo MONITOR_ON',use('arm')])
cmd('off',['.printf "MONITOR_COUNTS groups=%x checks=%x hits=%x active_tid=%x\\n", @$t10, @$t18, @$t19, @$t0','bc *','.echo MONITOR_OFF','g'])
exception='.echo EXCEPTION_CAPTURE; .printf "EXCEPTION code=%x chance=%x pid=%x tid=%x eventip=%p rip=%p rsp=%p rbp=%p\\n", @$exr_code, @$exr_chance, @$tpid, @$tid, @$eventip, @rip, @rsp, @rbp; .exr -1; .lastevent; r; kv; .if (@$t17 == 0) { r @$t17=1; .dump /ma '+(out/'exception-private.dmp').as_posix()+'; }; .echo EXCEPTION_CONTINUE_NOT_HANDLED; gn'
cmd('exception',[exception])
cmd('init',['n 16','.sympath '+str(repo/'target/c-symbolized-original-20261114')+';'+str(repo/'target/c-first-fault-20261113/symbols'),'.reload /f ntdll.dll','.if ((dwo(ntdll+59dfc) != 246c8d48) | (by(ntdll+59e00) != b0) | (dwo(ntdll+5a108) != 304d8b48)) { '+stop('code_mismatch')+' }','r @$t17 = 0',f'sxe -c "{use("exception")}" -c2 "{use("exception")}" av',f'sxe -c "{use("exception")}" -c2 "{use("exception")}" 0xc0000409','.printf "DEBUGGEE_PID %x\\n", @$tpid','.echo HANDLE_GATE_WAIT'])
save('preflight-public.json',dict(utc=utc(),node_sha256=sha(exe),aoem_sha256=sha(dll),ntdll_sha256=sha(nt),new_loopback_novonet=True,probe_commands_written=False,relay_forwarding=False,original_state_accessed=False,baseline_seconds=120,monitor_gate_seconds=120,recovery_off_seconds=60,maximum_node_seconds=4500,maximum_wall_seconds=4560,interference_limit=0.25,minimum_progress_samples=10,hardware_hit_limit=100000,log_limit_bytes=512*1024**2,stop_without_retry=True))

processes=[];handles={};files=[];actions=[];samples=[];seen={};target=None;debug=None
phase='starting';start=time.monotonic();phase_start=start;reason='not_started';error=None;failure_deadline=None
def action(kind,**values):
    row=dict(utc=utc(),elapsed=time.monotonic()-start,kind=kind,**values);actions.append(row)
    with (out/'actions-public.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(row)+'\n')
def send(command):debug.stdin.write((command+'\n').encode('ascii'));debug.stdin.flush()
def interrupt(command, marker=None):
    action('debugger_break_request',next_command=command)
    ok(k.DebugBreakProcess(target));send(command)
    if marker:
        until=time.monotonic()+15
        while time.monotonic()<until:
            if re.search('^'+re.escape(marker)+r'\s*$',(out/'cdb-private.log').read_text(errors='replace'),re.M):return
            if debug.poll() is not None:break
            time.sleep(.1)
        raise RuntimeError('phase marker not observed: '+marker)
def metric(which):
    rows=[x for x in samples if x['index']==0 and x['phase']==which]
    if len(rows)<10:return dict(available=False,samples=len(rows))
    gaps=[z['file_unix_ns']/1e9-a['file_unix_ns']/1e9 for a,z in zip(rows,rows[1:])]
    dt=rows[-1]['elapsed']-rows[0]['elapsed']
    return dict(available=True,samples=len(rows),median_report_interval_seconds=statistics.median(gaps),ticks_per_second=(rows[-1]['ticks']-rows[0]['ticks'])/dt,cpu_seconds_per_wall=(rows[-1]['cpu_seconds']-rows[0]['cpu_seconds'])/dt,first_elapsed=rows[0]['elapsed'],last_elapsed=rows[-1]['elapsed'])
def collect():
    for i,h in handles.items():
        path=out/f'progress-{i}-private.json'
        if not path.exists():continue
        st=path.stat()
        if seen.get(i)==st.st_mtime_ns:continue
        try:p=json.loads(path.read_text())
        except (OSError,ValueError):continue
        seen[i]=st.st_mtime_ns
        probe=p.get('native_network_probe') or {};tr=probe.get('transport') or {};recv=tr.get('receive_totals') or {};sp=tr.get('send_polls') or {}
        row=dict(elapsed=time.monotonic()-start,utc=utc(),index=i,phase=phase,file_unix_ns=st.st_mtime_ns,ticks=p.get('ticks',0),novonet_present=isinstance(p.get('novonet'),dict),probe_present=isinstance(p.get('native_network_probe'),dict),objects=len(tr.get('objects') or []),expired=tr.get('expired_records'),evicted=tr.get('evicted_records'),send_active_calls=sp.get('active_calls'),reported_data_writes=sp.get('reported_data_writes'),accepted_data=recv.get('accepted_data'),history_data=recv.get('history_data'),execution_failures=p.get('execution_failures'),**proc_info(h))
        samples.append(row)
        with (out/'samples-public.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(row)+'\n')
try:
    for s in reserved:s.close()
    for i in (1,2,3,0):
        e=env.copy();e['NOVOVM_NATIVE_EXECUTION_CONFIG']=str(out/f'config-{i}-private.json');e['NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH']=str(out/f'progress-{i}-private.json')
        stdout=open(out/f'process-{i}-stdout-private.txt','wb');stderr=open(out/f'process-{i}-stderr-private.txt','wb');files.extend([stdout,stderr])
        argv=[str(exe)] if i else [str(cdb),'-G','-logo',str(out/'cdb-private.log'),'-cf',str(out/'init.cmd'),str(exe)]
        p=subprocess.Popen(argv,cwd=out,env=e,stdout=stdout,stderr=stderr,stdin=subprocess.PIPE if i==0 else subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
        processes.append((i,p));action('launch',index=i,pid=p.pid,debugger=i==0)
        if i:handles[i],info=open_target(p.pid)
        else:debug=p
    log=out/'cdb-private.log';deadline=time.monotonic()+30
    while time.monotonic()<deadline:
        text=log.read_text(errors='replace') if log.exists() else ''
        m=re.search(r'^DEBUGGEE_PID ([0-9a-f]+)\s*$',text,re.M)
        if m and 'HANDLE_GATE_WAIT' in text:break
        if debug.poll() is not None:raise RuntimeError('debugger exited before handle gate')
        time.sleep(.1)
    else:raise RuntimeError('debuggee handle gate timeout')
    target_pid=int(m[1],16);target,info=open_target(target_pid);handles[0]=target
    save('debuggee-handle-public.json',dict(pid=target_pid,created_filetime=info['created_filetime'],image_sha256=sha(exe),opened_before_run=True))
    action('debuggee_handle_retained',pid=target_pid);send('g')
    phase='baseline_off';phase_start=time.monotonic();reason='running'
    while time.monotonic()-start<4560:
        collect();elapsed=time.monotonic()-phase_start
        text=log.read_text(errors='replace')
        if any(x in text for x in ['Numeric expression missing','Command file execution failed','Syntax error','Unable to insert breakpoint']):reason='debugger_command_error';break
        if re.search(r'^CONTROLLER_ACTION debugger_q ',text,re.M):reason='debugger_guard_stop';break
        if re.search(r'^(EXCEPTION_CAPTURE|RBP_MISMATCH)\s*$',text,re.M):
            reason='exception_or_mismatch_observed'
            if exited(target) is not None:break
            if failure_deadline is None:
                failure_deadline=time.monotonic()+60;action('exception_capture_wait')
            if time.monotonic()>=failure_deadline:reason='exception_capture_wait_timeout';break
            time.sleep(.5);continue
        if debug.poll() is not None:reason='debugger_exited';break
        if any(i and p.poll() not in (None,0) for i,p in processes):reason='peer_exited_nonzero';break
        if sum(p.stat().st_size for p in out.glob('*private*.txt'))+log.stat().st_size>512*1024**2:reason='log_budget';break
        if shutil.disk_usage(out).free<4*1024**3:reason='disk_budget';break
        if phase=='baseline_off' and elapsed>=120:
            base=metric(phase);own=[x for x in samples if x['index']==0]
            if not base['available'] or not own or not all(x['novonet_present'] and x['probe_present'] for x in own[-3:]) or not any((x['reported_data_writes'] or 0)>0 for x in own):reason='load_not_qualified';break
            save('baseline-public.json',base)
            interrupt(use('on'),'MONITOR_ON');phase='monitor_on';phase_start=time.monotonic();action('phase',value=phase)
        elif phase=='monitor_on' and elapsed>=120:
            on=metric(phase);base=metric('baseline_off')
            degraded=not on['available'] or on['median_report_interval_seconds']>base['median_report_interval_seconds']*1.25 or on['ticks_per_second']<base['ticks_per_second']*.75
            save('interference-gate-public.json',dict(baseline=base,monitor=on,exceeds_limit=degraded,limit=.25,not_query_latency=True,not_randomized=True))
            if degraded:
                interrupt(use('off'),'MONITOR_OFF');phase='recovery_off';phase_start=time.monotonic();action('phase',value=phase)
            else:phase='sustained_on';action('phase',value=phase)
        elif phase=='recovery_off' and elapsed>=60:reason='interference_budget_exceeded';break
        time.sleep(.5)
    else:reason='wall_budget'
except BaseException as exc:
    error=f'{type(exc).__name__}: {exc}';reason='runner_error'
finally:
    if debug and debug.poll() is None:
        # Explicit terminal action distinguishes deliberate diagnostic stop from crash.
        try:
            if target and exited(target) is None:interrupt('.echo CONTROLLER_ACTION debugger_q final_stop; q')
            else:send('q')
            debug.wait(timeout=12)
        except (OSError,AssertionError,subprocess.TimeoutExpired):pass
    exits=[]
    for i,p in processes:
        if p.poll() is None:
            action('controller_taskkill',index=i,pid=p.pid,reason=reason)
            subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        try:p.wait(timeout=10)
        except subprocess.TimeoutExpired:pass
        exits.append(dict(index=i,pid=p.pid,kind='debugger' if i==0 else 'node',exit_code=p.poll()))
    if target and k.WaitForSingleObject(target,0)==258:
        action('controller_target_terminate',pid=target_pid,reason=reason,created_filetime=info['created_filetime'])
        if k.TerminateProcess(target,0xE0000001):k.WaitForSingleObject(target,10000)
    os_exits={}
    for i,h in handles.items():
        try:os_exits[str(i)]=exited(h)
        except OSError as exc:os_exits[str(i)]=dict(api_error=str(exc))
    for h in handles.values():k.CloseHandle(h)
    for f in files:f.close()
    save('terminal-public.json',dict(utc=utc(),wall_seconds=time.monotonic()-start,reason=reason,error=error,processes=exits,node_os_exit_codes=os_exits,phases={s:metric(s) for s in ['baseline_off','monitor_on','recovery_off']},original_fault_fixed=False,original_state_accessed=False,double_relay_resumed=False))
print(json.dumps(dict(evidence=str(out),reason=reason,error=error,exit_codes=os_exits)))
