# One new original-status scenario with a full, wrapped retained delivery window.
from pathlib import Path
import json,os,subprocess,hashlib,datetime
root=Path('target/c-real-load-phase6-20261114').resolve()
image=Path('target/c-boundary-phase4-20261114/build/x86_64-pc-windows-msvc/release/c-phase6-status.exe').resolve()
mapping=json.loads((root/'c-phase6-status-function-map.json').read_text())
assert hashlib.sha256(image.read_bytes()).hexdigest()==mapping['sha256']
f={}
for name in ['format_inner','format_write','pad_integral','string_writer']:
 matches=mapping['functions'][name];assert len(matches)==1 and matches[0]['unwind_matches_original'],name
 f[name]=int(matches[0]['rva'],16)
module='c_phase6_status'
commands=['.sympath '+str(image.parent),'.reload /f c-phase6-status.exe','r @$t0=0','r @$t1=0','r @$t2=0','r @$t3=0']
def quote(s):return '"'+s.replace('"','\\"')+'"'
def point(idx,address,body):commands.append('bp '+str(idx)+' '+address+' '+quote(body+'; gc'))
point(0,module+'!phase6_begin','r @$t0=1; r @$t1=@$tid; .printf "WINDOW_BEGIN tid=%x query=%x owner=%p",@$tid,@rcx,@rdx; .echo; ~')
point(1,module+'!phase6_end','.printf "WINDOW_END tid=%x query=%x owner=%p calls=%x",@$tid,@rcx,@rdx,@$t2; .echo; r @$t0=0')
# Selected windows: records 603..605 and 1529..1531 (zero based).
# 604 is a conditional last-snapshot correspondence, NOT a proven original failing record.
windows=[(603*64+1,606*64),(1529*64+1,1532*64)]
body='r @$t2=@$t2+1; '
for start,end in windows:
 body+=f'.if (@$t2==0x{start:x}) {{ '+ '; '.join('be '+str(i) for i in range(3,10))+'; .printf "SELECT_BEGIN ordinal=%x",@$t2; .echo; }; '
 body+=f'.if (@$t2==0x{end+1:x}) {{ '+ '; '.join('bd '+str(i) for i in range(3,10))+'; .printf "SELECT_END ordinal=%x",@$t2; .echo; }; '
body+='.if (@$t2==1) { .echo ORIGINAL_RETAINED_STATUS_STACK; kv; }; '
point(2,module+'+0x%x'%f['format_inner'],'.if (@$t0) { .if (@$tid==@$t1) { '+body+' } }')
def record(idx,name,offset,stage,fields,extra=''):
 fmt='REC stage='+stage+' tid=%x ordinal=%x seq=%x rsp=%p '+' '.join(k+'=%p' for k,v in fields)
 args='@$tid,@$t2,@$t3,@rsp'+''.join(','+v for k,v in fields)
 body=extra+'.printf "'+fmt+'",'+args+'; .echo; '
 point(idx,module+'+0x%x'%(f[name]+offset),'.if (@$t0) { .if (@$tid==@$t1) { '+body+' } }')
record(3,'format_write',0,'FMT_BEGIN',[('receiver','@rcx'),('cap','poi(@rcx)'),('buffer','poi(@rcx+8)'),('len','poi(@rcx+0x10)')],'r @$t3=@$t3+1; ')
record(4,'format_write',349,'FMT_CALL',[('formatter','@rdx'),('receiver','poi(@rdx)')])
record(5,'pad_integral',0,'PAD_ENTRY',[('formatter','@rcx'),('receiver','poi(@rcx)'),('text','poi(@rsp+0x28)'),('len','poi(@rsp+0x30)')])
args=[('receiver','@rcx'),('text','@rdx'),('len','@r8'),('rax','@rax'),('r9','@r9'),('rbp','@rbp')]
record(6,'pad_integral',245,'TAIL_JUMP',args)
record(7,'string_writer',0,'WRITER_ENTRY',args)
record(8,'string_writer',46,'COPY_CALL',[('receiver','@rdi'),('dest','@rcx'),('text','@rdx'),('len','@r8'),('buffer','poi(@rdi+8)'),('oldlen','poi(@rdi+0x10)')])
record(9,'pad_integral',1060,'PAD_CALL',args+[('target','poi(@rdi+0x18)')])
commands += ['bd '+str(i) for i in range(3,10)]
commands += ['sxe -c ".echo PHASE6_AV; .exr -1; r; kv; q" av','sxe -c ".echo PHASE6_EXIT; .lastevent; q" epr','g']
script=root/'capture1.cdb';script.write_bytes(('\n'.join(commands)+'\n').encode())
if os.environ.get('PHASE6_PREPARE_ONLY')=='1':print('prepared');raise SystemExit(0)
runtime=Path('target/c-state-context-phase5-20261114/runtime/aoem_ffi.dll').resolve()
assert hashlib.sha256(runtime.read_bytes()).hexdigest()=='5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'
env=os.environ.copy()
for k in list(env):
 if k.startswith(('NOVOVM_','AOEM_','PHASE5_','PHASE6_')) or k=='CREDENTIALS_DIRECTORY':env.pop(k)
env.update({'PHASE6_TEST_ROOT':str(root/'capture1-state'),'PHASE6_LIBRARY':str(runtime),'PHASE6_RETAINED_INPUT':str(root/'retained-input-private.json'),'NOVOVM_AOEM_DLL':str(runtime),'NOVOVM_PROFILE_STAGES':'0','NOVOVM_PROFILE_CRITICAL_PATH':'0','NOVOVM_PROFILE_ASSET_TRACE':'0','NOVOVM_PROFILE_NATIVE_DELIVERY':'1','AOEM_SEMANTIC_GRAPH_V2_WORKERS':'1'})
assert not Path(env['PHASE6_TEST_ROOT']).exists()
debugger=Path('target/c-first-fault-20261113/sdk/extracted/Windows Kits/10/Debuggers/x64/cdb.exe').resolve()
started=datetime.datetime.now(datetime.timezone.utc).isoformat()
with (root/'capture1-private.log').open('xb') as log:
 p=subprocess.Popen([str(debugger),'-cf',str(script),str(image)],env=env,stdout=log,stderr=subprocess.STDOUT)
 (root/'capture1-launch.json').write_bytes((json.dumps({'debugger_pid':p.pid,'start_utc':started,'image_sha256':mapping['sha256'],'script_sha256':hashlib.sha256(script.read_bytes()).hexdigest(),'maximum_seconds':300,'windows_one_based_format_ordinals':windows},indent=2)+'\n').encode())
 try:rc=p.wait(timeout=300);timeout=False
 except subprocess.TimeoutExpired:
  subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'],stdout=log,stderr=subprocess.STDOUT)
  rc=p.wait(timeout=20);timeout=True
result={'debugger_exit_code':rc,'timeout':timeout,'maximum_seconds':300,'start_utc':started,'end_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
(root/'capture1-exit.json').write_bytes((json.dumps(result,indent=2)+'\n').encode());print(result)
