"""One bounded ORIGINAL a254 execution in fresh loopback test state.
No production database, identity, service, registry, or binary is modified.
Uses stock CDB hardware execution breakpoints; no recorder/sample program.
Not an exhaustive instruction trace and not a promise of low interference.
"""
import csv, hashlib, io, json, os, pathlib, socket, struct, subprocess, sys, time
from datetime import datetime, timezone

repo = pathlib.Path(__file__).resolve().parents[2]
out = pathlib.Path(sys.argv[1]).resolve()
assert not out.exists(), 'new evidence directory required'
assert out.is_relative_to(repo/'target'), 'private output must stay in repo target'
out.mkdir()
sid = list(csv.reader(io.StringIO(subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True))))[0][1]
subprocess.run(['icacls',str(out),'/inheritance:r','/grant:r',f'*{sid}:(OI)(CI)F','*S-1-5-18:(OI)(CI)F','*S-1-5-32-544:(OI)(CI)F'],check=True,stdout=subprocess.DEVNULL)
(out/'executed-script.py').write_bytes(pathlib.Path(__file__).read_bytes())
def save(name,x):
    p=out/name;assert not p.exists();p.write_text(json.dumps(x,indent=2)+'\n',encoding='utf-8',newline='\n')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def stamp():return datetime.now(timezone.utc).isoformat()
exe=repo/'target/c-symbolized-original-20261114/novovm-node.exe'
runtime=pathlib.Path('C:/ProgramData/NOVOVM/releases/aoem-9662470d-c-20261113/aoem')
dll=runtime/'windows/core/bin/aoem_ffi.dll'
cdb=repo/'target/c-first-fault-20261113/sdk/extracted/Windows Kits/10/Debuggers/x64/cdb.exe'
test=repo/'target/c-joint-network-capture-20261114/run-state-private'
assert sha(exe)=='1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d'
assert sha(dll)=='5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'
assert sha(test/'test-setup-private.json')=='6c61c2c77bbb6a2bb11232be23602afc54124f0d36f30349ae632171d86f3c43'
# Fail before launch if system code no longer matches the dump's instruction RVAs.
nt=pathlib.Path('C:/Windows/System32/ntdll.dll'); b=nt.read_bytes();pe=struct.unpack_from('<I',b,60)[0]
ns=struct.unpack_from('<H',b,pe+6)[0];sections=pe+24+struct.unpack_from('<H',b,pe+20)[0]
def code(rva,n):
    for i in range(ns):
        vs,va,sz,raw=struct.unpack_from('<4I',b,sections+i*40+8)
        if va<=rva and rva+n<=va+sz:return b[raw+rva-va:raw+rva-va+n]
    raise AssertionError('RVA not backed by PE')
for rva,h in [(0x59dfc,'488d6c24b0'),(0x59e01,'4881ec50010000'),(0x5a108,'488b4d30')]:
    assert code(rva,len(bytes.fromhex(h)))==bytes.fromhex(h),f'ntdll changed at {rva:x}'
setup=json.loads((test/'test-setup-private.json').read_text())
ports=[];sockets=[]
for _ in range(4):
    s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);s.bind(('127.0.0.1',0));ports.append(s.getsockname()[1]);sockets.append(s)
env={k:v for k,v in os.environ.items() if not (k.startswith('NOVOVM_') or k.startswith('AOEM_'))}
env.update(NOVOVM_AOEM_ROOT=str(runtime),NOVOVM_AOEM_DLL=str(dll),NOVOVM_AOEM_MANIFEST=str(runtime/'manifest/aoem-manifest.json'),AOEM_DLL_MANIFEST=str(runtime/'manifest/aoem-manifest.json'),AOEM_DLL_MANIFEST_REQUIRED='1',NOVOVM_AOEM_RUNTIME_PROFILE=str(runtime/'config/aoem-runtime-profile.json'),NOVOVM_AOEM_PLUGIN_DIR=str(runtime/'windows/core/plugins'),AOEM_SEMANTIC_GRAPH_V2_WORKERS='1',NOVOVM_NODE_MODE='native_execution_tick',NOVOVM_NATIVE_EXECUTION_START='create',NOVOVM_NATIVE_EXECUTION_RUN_MS='60000',NOVOVM_PROFILE_NATIVE_DELIVERY='1',NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS='5000',RUST_BACKTRACE='full')
for i in range(4):
    cfg=dict(profile='native_execution_tick',library=str(dll),database=str(out/f'new-test-db-{i}'),rpc_addr=None,signing_key_file=str(test/f'test-validator-{i}.sealed'),transport_key_file=str(test/f'test-transport-{i}.sealed'),key_storage='protected_v1',udp_addr=f'127.0.0.1:{ports[i]}',peers=[dict(public_key=v['public_key'],address=f'127.0.0.1:{ports[j]}')for j,v in enumerate(setup['genesis']['validators'])if j!=i],learn_authenticated_peer_routes=False,novonet_config_file=None,native_client_limit=0,genesis=setup['genesis'],uca_initialization=None,workers=1,batch_size=1)
    save(f'config-{i}-private.json',cfg)

# One active invocation, up to eight groups. No cross-thread/global-state merge.
# All other threads' hits are counted and capped, not silently ignored as free.
def script(name,lines):
    (out/(name+'.cmd')).write_text('\n'.join(lines)+'\n',encoding='ascii')
def use(name):return '$$><'+(out/(name+'.cmd')).as_posix()+';'
def hit_guard(body):return 'r @$t19 = @$t19 + 1; .if (@$t19 > 0n1000) { .echo HIT_LIMIT_STOP; q } .else { '+body+' }'
def hw(slot,rva,condition,name):
    condition='('+condition.replace(' && ',') & (')+')'
    return f'ba{slot} e 1 ntdll+{rva:x} "'+hit_guard(f'.if ({condition}) {{ {use(name)} }} .else {{ gc }}')+'"'
def event(name):return f'.printf "EVENT {name} group=%x tid=%x rip=%p rsp=%p rbp=%p\\n", @$t10, @$tid, @rip, @rsp, @rbp'
dump=f'.dump /ma {out}/captured-private.dmp; q'
script('arm',['ba0 e 1 novovm_node+84eed4 "'+hit_guard('.if (@rdx != 0) { '+use('outer')+' } .else { gc }')+'"','g'])
script('outer',['bc 0','r @$t10 = @$t10 + 1','r @$t0 = @$tid','r @$t1 = @rsp','r @$t11 = 0',event('OUTER_CALL'),
    'bp10 ntdll!RtlUnwindEx ".echo CONTEXT_GAP_UNWIND; q"',
    'bp11 ntdll!RtlRestoreContext ".echo CONTEXT_GAP_RESTORE; q"',
    'bp12 ntdll!NtContinue ".echo CONTEXT_GAP_CONTINUE; q"',
    'bp13 novovm_node+84eed9 ".if ((@$tid == @$t0) & (@rsp == @$t1)) { .echo OUTER_RETURN_WITHOUT_COMPLETED_GROUP; q } .else { gc }"',
    hw(1,0x59dfc,'@$tid == @$t0 && @rsp < @$t1 && poi(@rsp+38) == ntdll+5bddd','lea_before'),'g'])
script('lea_before',['bc 1','r @$t4 = @rsp','r @$t5 = @rbp','r @$t12 = poi(@rsp+38)',event('LEA_BEFORE'),'.printf "HEAP_RETURN %p\\n", @$t12','u @rip L1',hw(2,0x59e01,'@$tid == @$t0 && @rsp == @$t4 && poi(@rsp+38) == @$t12','lea_after'),'g'])
script('lea_after',['bc 2',event('LEA_AFTER'),'.if (@rbp != (@$t4 - 50)) { .echo LEA_MISMATCH; '+dump+' }',hw(0,0x5a05b,'@$tid == @$t0 && @rsp == (@$t4 - 150)','child_before'),hw(1,0x5a060,'@$tid == @$t0 && @rsp == (@$t4 - 150)','child_after'),hw(3,0x5a108,'@$tid == @$t0 && @rsp == (@$t4 - 150)','check'),'g'])
script('child_before',['.if (@$t7 != 0) { .echo CONTEXT_GAP_NESTED_CHILD; q }','r @$t11 = @$t11 + 1','r @$t6 = @rbp','r @$t7 = 1',event('CHILD_BEFORE'),'.printf "CHILD_SEQUENCE %x\\n", @$t11','u @rip L1','g'])
script('child_after',['.if (@$t7 != 1) { .echo CONTEXT_GAP_CHILD_WITHOUT_ENTRY; q }',event('CHILD_AFTER'),'.printf "CHILD_SEQUENCE %x\\n", @$t11','.if (@rbp != @$t6) { .echo CHILD_INTERVAL_MISMATCH; '+dump+' }','r @$t7 = 0','g'])
script('check',[event('COOKIE_PRECHECK'),'.if (@$t7 != 0) { .echo CONTEXT_GAP_UNFINISHED_CHILD; q }','.if (@rbp != (@rsp + 100)) { .echo PRECHECK_MISMATCH; kv; '+dump+' }','bc *','r @$t7 = 0','.if (@$t10 < 8) { '+use('arm')+' } .else { .echo CAPTURE_GROUP_LIMIT; g }'])
script('init',['n 16','.sympath '+str(repo/'target/c-symbolized-original-20261114')+';'+str(repo/'target/c-first-fault-20261113/symbols'),'.reload /f ntdll.dll','.if ((dwo(ntdll+59dfc) != 246c8d48) | (by(ntdll+59e00) != b0) | (dwo(ntdll+5a108) != 304d8b48)) { .echo CODE_MISMATCH_STOP; q }','r @$t10 = 0','r @$t19 = 0','r @$t7 = 0',f'sxe -c ".echo FIRST_CHANCE_AV_CONTROLLED_STOP; {dump}" av',f'sxe -c ".echo FIRST_CHANCE_FAST_FAIL_CAPTURE; {dump}" 0xc0000409','sxe -c ".echo CONTEXT_GAP_CPP_EXCEPTION; q" eh',use('arm')])
save('preflight-public.json',dict(utc=stamp(),node_sha256=sha(exe),aoem_sha256=sha(dll),ntdll_sha256=sha(nt),loopback_only=True,original_database_accessed=False,original_identity_accessed=False,original_node_started=False,run_ms=60000,maximum_wall_seconds=110,group_limit=8,all_hardware_hit_limit=1000,limitations=['eight selected invocations only','outer source has no exact source PDB','no original packet replay','fresh loopback genesis','hardware debug exceptions perturb execution','no full instruction trace','abnormal unwind of tracked frame may leave an unmatched group']))
# Validate the exact nested include form OFFLINE before launching any node.
# This does not execute an invented callee or mutate the dump.
script('syntax-include',['.echo NESTED_INCLUDE_OK'])
script('syntax-preflight',['n 16','r @$t19 = 0',hit_guard('.if ((1 == 1) & (2 > 1)) { '+use('syntax-include')+' .echo NESTED_RETURN_OK } .else { .echo BAD_CONDITION }'),'.if ((dwo(ntdll+59dfc) != 246c8d48) | (by(ntdll+59e00) != b0) | (dwo(ntdll+5a108) != 304d8b48)) { .echo BAD_BYTES } .else { .echo BYTE_GUARD_OK }','q'])
old_dump=repo/'target/c-wer-natural-full-20261114T122549Z/novovm-node.exe.34452.dmp'
with open(out/'syntax-console-private.txt','wb') as f:
    syntax=subprocess.run([str(cdb),'-z',str(old_dump),'-cf',str(out/'syntax-preflight.cmd'),'-logo',str(out/'syntax-private.log')],stdout=f,stderr=subprocess.STDOUT,timeout=30,creationflags=subprocess.CREATE_NO_WINDOW)
syntax_log=(out/'syntax-private.log').read_text(errors='replace')
markers=['NESTED_INCLUDE_OK','NESTED_RETURN_OK','BYTE_GUARD_OK']
syntax_ok=syntax.returncode==0 and all('\n'+s+'\n' in syntax_log for s in markers) and not any(s in syntax_log for s in ['Numeric expression missing','Command file execution failed','Syntax error'])
save('syntax-public.json',dict(exit_code=syntax.returncode,passed=syntax_ok,markers=markers,log_sha256=sha(out/'syntax-private.log')))
assert syntax_ok,'offline nested callback syntax failed; no node launched'
procs=[];handles=[];start=time.monotonic();history=[];stop_reason='completed_or_wall_limit'
try:
    for s in sockets:s.close()
    for i in (1,2,3,0):
        e=env.copy();e['NOVOVM_NATIVE_EXECUTION_CONFIG']=str(out/f'config-{i}-private.json');e['NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH']=str(out/f'progress-{i}-private.json')
        stdout=open(out/f'process-{i}-stdout-private.txt','wb');stderr=open(out/f'process-{i}-stderr-private.txt','wb');handles.extend((stdout,stderr))
        argv=[str(exe)] if i else [str(cdb),'-G','-logo',str(out/'cdb-private.log'),'-cf',str(out/'init.cmd'),str(exe)]
        p=subprocess.Popen(argv,cwd=out,env=e,stdout=stdout,stderr=stderr,creationflags=subprocess.CREATE_NO_WINDOW)
        procs.append((i,p));save(f'launch-{i}-public.json',dict(utc=stamp(),pid=p.pid,debugger=i==0))
    while any(p.poll() is None for _,p in procs) and time.monotonic()-start<110:
        log=out/'cdb-private.log'
        if log.exists():
            text=log.read_text(errors='replace')
            if any(x in text for x in ('Numeric expression missing','Command file execution failed','Syntax error','Unable to insert breakpoint')):
                stop_reason='debugger_command_error';break
        for i,p in procs:
            path=out/f'progress-{i}-private.json'
            if path.exists():
                st=path.stat();key=(i,st.st_mtime_ns,st.st_size)
                if key not in history:history.append(key)
        # If a non-debugged original EXE rejects configuration, stop without retries.
        if any(i and p.poll() not in (None,0) for i,p in procs):stop_reason='peer_exited_nonzero';break
        if any(i==0 and p.poll() is not None for i,p in procs) and time.monotonic()-start<59:stop_reason='debugger_ended_early';break
        time.sleep(1)
finally:
    exits=[]
    for i,p in procs:
        forced=p.poll() is None
        if forced:subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        try:code=p.wait(timeout=10)
        except subprocess.TimeoutExpired:code=None
        exits.append(dict(index=i,pid=p.pid,exit_code=code,controlled_stop=forced))
    for f in handles:f.close()
    save('execution-public.json',dict(end_utc=stamp(),wall_seconds=time.monotonic()-start,processes=exits,stop_reason=stop_reason,progress_file_changes=history,original_fault_reproduced='not_determined',root_cause_fixed=False))
print(json.dumps(dict(evidence=str(out),processes=exits)))
