"""Single bounded, private, loopback-only capture. No deployed-node operations.

Full dumps and packet/boundary logs contain isolated test state: never publish them.
Run once from SUPERVM root AFTER build and function-map verification.
"""
from pathlib import Path
import os,subprocess,json,hashlib,datetime,time,struct,shutil,re
ROOT=Path('target/c-joint-network-capture-20261114').resolve()
IMAGE=Path('target/c-boundary-phase4-20261114/build/x86_64-pc-windows-msvc/release/c-joint-network-capture.exe').resolve()
CDB=Path('target/c-first-fault-20261113/sdk/extracted/Windows Kits/10/Debuggers/x64/cdb.exe').resolve()
DLL=Path('target/c-state-context-phase5-20261114/runtime/aoem_ffi.dll').resolve()
MODULE='c_joint_network_capture'
MAX_SECONDS=1500
def save(path,data):
    with path.open('xb') as f:f.write((json.dumps(data,indent=2)+'\n').encode())
def now():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def sha(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def environment():
    env=os.environ.copy()
    for k in list(env):
        if k.startswith(('NOVOVM_','AOEM_','PHASE','JOINT_')) or k=='CREDENTIALS_DIRECTORY':env.pop(k)
    env.update({'JOINT_ROOT':str(ROOT/'run-state-private'),'JOINT_LIBRARY':str(DLL),
                'NOVOVM_AOEM_DLL':str(DLL),'NOVOVM_PROFILE_STAGES':'0',
                'NOVOVM_PROFILE_CRITICAL_PATH':'0','NOVOVM_PROFILE_ASSET_TRACE':'0',
                'NOVOVM_PROFILE_NATIVE_DELIVERY':'1','AOEM_SEMANTIC_GRAPH_V2_WORKERS':'4'})
    return env
def debugger_run(script,env,log,timeout):
    with log.open('xb') as output:
        p=subprocess.Popen([str(CDB),'-cf',str(script),str(IMAGE)],env=env,stdout=output,stderr=subprocess.STDOUT)
        try:return p.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            if p.poll() is None:subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'],stdout=output,stderr=subprocess.STDOUT)
            p.wait(timeout=20)
            raise
def main():
    assert not (ROOT/'capture-start.json').exists(), 'single invocation; no retry'
    mapping=json.loads((ROOT/'c-joint-network-capture-function-map.json').read_text())
    assert sha(IMAGE)==mapping['sha256']
    assert sha(DLL)=='5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'
    assert shutil.disk_usage(ROOT).free>12*1024**3
    f={}
    for name in ['format_write','pad_integral','string_writer']:
        rows=mapping['functions'][name];assert len(rows)==1 and rows[0]['unwind_matches_original'],name
        f[name]=int(rows[0]['rva'],16)
    save(ROOT/'capture-start.json',{'utc':now(),'binary_sha256':sha(IMAGE),'runtime_sha256':sha(DLL),'maximum_seconds':MAX_SECONDS,'original_fault_replay':False})
    env=environment();env['JOINT_MODE']='gate'
    gate=ROOT/'gate.cdb';dump=ROOT/'gate-private.dmp'
    gate.write_bytes(('\n'.join([
        '.sympath '+str(IMAGE.parent),'.reload /f',
        'x '+MODULE+'!*joint_hash_begin*','x '+MODULE+'!*joint_hash_end*',
        'bp '+MODULE+'!joint_capture_gate ".echo CAPTURE_GATE; .dump /ma '+dump.as_posix()+'; q"','g'])+'\n').encode())
    gate_rc=debugger_run(gate,env,ROOT/'gate-private.log',120)
    log=(ROOT/'gate-private.log').read_text(errors='replace')
    assert gate_rc==0 and 'CAPTURE_GATE' in log and dump.is_file(),'gate not reached'
    with dump.open('rb') as h:
        header=h.read(32)
        count,directory=struct.unpack_from('<II',header,8)
        h.seek(directory);entries=[struct.unpack('<III',h.read(12)) for _ in range(count)]
        assert all(rva+size<=dump.stat().st_size for _,size,rva in entries),'truncated stream'
        memory64=[rva for kind,size,rva in entries if kind==9]
        assert len(memory64)==1,'full captured-memory stream missing'
        h.seek(memory64[0]);n,base=struct.unpack('<QQ',h.read(16))
        assert memory64[0]+16+16*n<=dump.stat().st_size
        total=sum(struct.unpack('<QQ',h.read(16))[1] for _ in range(n))
        assert base+total<=dump.stat().st_size,'truncated memory ranges'
    assert header[:4]==b'MDMP' and struct.unpack_from('<Q',header,24)[0]&2,'full memory gate missing'
    assert 'Dump successfully written' in log,'dump completion missing'
    for name in ['joint_hash_begin','joint_hash_end']:
        assert re.search(r'[0-9a-f`]+\s+.*diagnostic_capture::'+name,log),name+' debug symbol missing'
    save(ROOT/'capture-gate-public.json',{'debugger_exit_code':gate_rc,'gate_marker_reached':True,'full_memory_flag':True,'dump_bytes':dump.stat().st_size,'dump_sha256':sha(dump),'hash_markers_resolved':True,'gate_is_node_failure':False})
    commands=['.sympath '+str(IMAGE.parent),'.reload /f','r @$t0=0','r @$t1=0','r @$t2=0','r @$t3=0','r @$t8=0']
    fatal='.echo FIRST_CAPTURE_STOP; .exr -1; r; kv; .dump /ma '+(ROOT/'failure-private.dmp').as_posix()+'; q'
    def bp(address,body):commands.append('bp '+address+' "'+(body+'; gc').replace('"','\\"')+'"')
    bp(MODULE+'!joint_query_begin','r @$t1=@$tid; r @$t8=@rcx; .printf "QUERY_BEGIN tid=%x query=%x owner=%p",@$tid,@$t8,@rdx; .echo')
    bp(MODULE+'!joint_query_end','.printf "QUERY_END tid=%x query=%x calls=%x",@$tid,@rcx,@$t3; .echo; r @$t0=0')
    bp(MODULE+'!novovm_network::diagnostic_capture::joint_hash_begin','.if (@$tid==@$t1) { r @$t0=1; }')
    bp(MODULE+'!novovm_network::diagnostic_capture::joint_hash_end','.if (@$tid==@$t1) { r @$t0=0; }')
    def point(name,offset,stage,fields,extra='',condition=None):
        fmt='BND stage='+stage+' tid=%x query=%x call=%x rsp=%p '+' '.join(k+'=%p' for k,v in fields)
        args='@$tid,@$t8,@$t3,@rsp'+''.join(','+v for k,v in fields)
        body=extra+'.printf "'+fmt+'",'+args+'; .echo; '
        if condition:body+='.if ('+condition+') { '+fatal+'; }; '
        bp(MODULE+'+0x%x'%(f[name]+offset),'.if (@$t0) { .if (@$tid==@$t1) { '+body+' } }')
    point('format_write',0,'FMT_BEGIN',[('receiver','@rcx')],'r @$t3=@$t3+1; r @$t4=@rcx; ')
    point('format_write',349,'FMT_CALL',[('formatter','@rdx'),('receiver','poi(@rdx)')],condition='poi(@rdx)!=@$t4')
    point('pad_integral',0,'PAD_ENTRY',[('formatter','@rcx'),('receiver','poi(@rcx)'),('text','poi(@rsp+0x28)'),('len','poi(@rsp+0x30)')],condition='poi(@rcx)!=@$t4')
    args=[('receiver','@rcx'),('text','@rdx'),('len','@r8'),('rax','@rax'),('r9','@r9'),('rbp','@rbp')]
    point('pad_integral',245,'TAIL_JUMP',args,condition='@rcx!=@$t4')
    point('pad_integral',1060,'PAD_CALL',args,condition='@rcx!=@$t4')
    point('string_writer',0,'WRITER_ENTRY',args,condition='@rcx!=@$t4')
    point('string_writer',46,'COPY_CALL',[('receiver','@rdi'),('dest','@rcx'),('text','@rdx'),('len','@r8')],condition='@rdi!=@$t4')
    for code in ['av','0xc0000374','0xc0000409']:commands.append('sxe -c "'+fatal+'" '+code)
    commands+=['sxe -c ".echo TARGET_EXIT; .lastevent; q" epr','g']
    script=ROOT/'joint.cdb';script.write_bytes(('\n'.join(commands)+'\n').encode())
    env['JOINT_MODE']='prepare'
    with (ROOT/'setup-private.log').open('xb') as h:setup=subprocess.run([str(IMAGE)],env=env,stdout=h,stderr=subprocess.STDOUT,timeout=120)
    save(ROOT/'setup-exit.json',{'exit_code':setup.returncode,'utc':now()})
    assert setup.returncode==0,'fresh isolated setup failed'
    peers=[];handles=[];debugger=None;reason=None;started=time.monotonic()
    try:
        env['JOINT_MODE']='run'
        for i in [1,2,3]:
            own=env.copy();own['JOINT_INDEX']=str(i)
            h=(ROOT/f'peer-{i}-private.log').open('xb');handles.append(h)
            peers.append(subprocess.Popen([str(IMAGE)],env=own,stdout=h,stderr=subprocess.STDOUT))
        own=env.copy();own['JOINT_INDEX']='0'
        h=(ROOT/'boundary-private.log').open('xb');handles.append(h)
        debugger=subprocess.Popen([str(CDB),'-cf',str(script),str(IMAGE)],env=own,stdout=h,stderr=subprocess.STDOUT)
        save(ROOT/'running-private.json',{'utc':now(),'debugger_pid':debugger.pid,'peer_pids':[p.pid for p in peers],'max_seconds':MAX_SECONDS})
        while debugger.poll() is None:
            if any(p.poll() is not None for p in peers):reason='peer exited before target';break
            if time.monotonic()-started>MAX_SECONDS:reason='capture wall-time bound';break
            if (ROOT/'boundary-private.log').stat().st_size>512*1024**2:reason='boundary file bound';break
            if shutil.disk_usage(ROOT).free<10*1024**3:reason='free-space bound';break
            time.sleep(1)
        if reason and debugger.poll() is None:
            subprocess.run(['taskkill','/PID',str(debugger.pid),'/T','/F'],stdout=h,stderr=subprocess.STDOUT)
        debugger.wait(timeout=30)
    finally:
        if debugger is not None and debugger.poll() is None:
            subprocess.run(['taskkill','/PID',str(debugger.pid),'/T','/F'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            debugger.wait(timeout=30)
        (ROOT/'run-state-private/STOP').write_bytes(b'bounded capture finished')
        for p in peers:
            try:p.wait(timeout=20)
            except subprocess.TimeoutExpired:p.kill();p.wait(timeout=10)
        for h in handles:h.close()
        save(ROOT/'run-exit.json',{'end_utc':now(),'elapsed_seconds':time.monotonic()-started,'debugger_exit_code':None if debugger is None else debugger.poll(),'peer_exit_codes':[p.returncode for p in peers],'bound_or_peer_failure':reason,'root_cause_fixed':False})
    print('Bounded run ended; inspect actual target exit and capture before interpreting')
if __name__=='__main__':main()
