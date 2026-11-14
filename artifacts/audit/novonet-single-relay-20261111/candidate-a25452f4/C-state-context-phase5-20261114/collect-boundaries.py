# Offline harness collector only. Checks are performed by check-boundaries.py.
# Phase5 distinct begin/end markers; real RpcLifecycle owner with test-only state.
from pathlib import Path
import json, os, subprocess, hashlib
root = Path('target/c-state-context-phase5-20261114').resolve()
image = Path('target/c-boundary-phase4-20261114/build/x86_64-pc-windows-msvc/release/c-phase5-status.exe').resolve()
mapping = json.loads((root / 'c-phase5-status-function-map.json').read_text())
assert mapping['sha256'] == hashlib.sha256(image.read_bytes()).hexdigest()
functions = {}
for name in ['format_write', 'pad_integral', 'string_writer']:
    found = mapping['functions'][name]
    assert len(found) == 1, (name, found)
    functions[name] = int(found[0]['rva'], 16)
module = 'c_phase5_status'
commands = ['.sympath ' + str(image.parent), '.reload /f c-phase5-status.exe', 'r @$t0=0', 'r @$t1=0', 'r @$t2=0']

def quoted(text):
    return chr(34) + text.replace(chr(34), chr(92) + chr(34)) + chr(34)
commands += ['r @$t10=0', 'x ' + module + '!phase5_*']
commands.append('bp ' + module + '!phase5_begin ' + quoted('r @$t0=1; r @$t1=@$tid; r @$t8=@rdx; r @$t9=@rcx; .printf "WINDOW_BEGIN tid=%x query=%x owner=%p", @$tid,@rcx,@rdx; .echo; ~; gc'))
commands.append('bp ' + module + '!phase5_end ' + quoted('.printf "WINDOW_END tid=%x query=%x owner=%p calls=%x", @$tid,@rcx,@rdx,@$t2; .echo; r @$t0=0; gc'))
commands.append('bp ' + module + '!phase5_producer ' + quoted('.printf "PRODUCER_BOUNDARY tid=%x stage=%x task=%x active=%x", @$tid,@rcx,@rdx,@$t0; .echo; gc'))
inner=int(mapping['functions']['format_inner'][0]['rva'],16)
commands.append('bp '+module+'+0x%x '%inner+quoted('.if (@$t0) { .if (@$tid==@$t1) { .if (@$t10==0) { r @$t10=1; .echo ORIGINAL_FORMAT_INNER_STACK; kv; } } }; gc'))

def bp(name, offset, stage, fields, extra=''):
    address = functions[name] + offset
    fmt = 'REC stage=' + stage + ' tid=%x seq=%x rsp=%p ' + ' '.join((k + '=%p' for k, v in fields))
    args = '@$tid,@$t2,@rsp' + ''.join((',' + v for k, v in fields))
    body = extra + '.printf ' + chr(34) + fmt + chr(34) + ', ' + args + '; .echo; '
    commands.append('bp ' + module + '+0x%x ' % address + quoted('.if (@$t0) { .if (@$tid==@$t1) { ' + body + '} }; gc'))
bp('format_write', 0, 'FMT_BEGIN', [('receiver', '@rcx'), ('cap', 'poi(@rcx)'), ('buffer', 'poi(@rcx+8)'), ('len', 'poi(@rcx+0x10)')], 'r @$t2=@$t2+1; r @$t3=@rcx; .if (@$t2==1) { .echo ORIGINAL_STATUS_FIRST_FORMAT_STACK; kv; }; ')
bp('format_write', 349, 'FMT_CALL', [('formatter', '@rdx'), ('receiver', 'poi(@rdx)')])
bp('pad_integral', 0, 'PAD_ENTRY', [('formatter', '@rcx'), ('receiver', 'poi(@rcx)'), ('text', 'poi(@rsp+0x28)'), ('len', 'poi(@rsp+0x30)')], 'r @$t4=poi(@rsp+0x28); r @$t5=poi(@rsp+0x30); ')
nonvolatile = [(reg, '@' + reg) for reg in ['rbx', 'rbp', 'rdi', 'rsi', 'r12', 'r13', 'r14', 'r15']]
bp('pad_integral', 199, 'PREFIX_BEFORE', nonvolatile)
bp('pad_integral', 204, 'PREFIX_AFTER', nonvolatile)
bp('pad_integral', 988, 'PAD_PREFIX_BEFORE', nonvolatile)
bp('pad_integral', 993, 'PAD_PREFIX_AFTER', nonvolatile)
args = [('receiver', '@rcx'), ('text', '@rdx'), ('len', '@r8'), ('rax', '@rax'), ('r9', '@r9'), ('rbp', '@rbp')]
bp('pad_integral', 229, 'TAIL_READY', args, 'r @$t6=@rsp+0x98; ')
bp('pad_integral', 245, 'TAIL_JUMP', args)
bp('pad_integral', 1060, 'PAD_CALL', args + [('target', 'poi(@rdi+0x18)')], 'r @$t6=@rsp-8; ')
bp('pad_integral', 1063, 'PAD_RETURN', [('result', '@al')])
bp('string_writer', 0, 'WRITER_ENTRY', args, 'r @$t7=@rsp; ')
bp('string_writer', 46, 'COPY_CALL', [('receiver', '@rdi'), ('dest', '@rcx'), ('text', '@rdx'), ('len', '@r8'), ('buffer', 'poi(@rdi+8)'), ('oldlen', 'poi(@rdi+0x10)')])
commands += ['sxe -c ".echo PHASE5_AV; .exr -1; r; kv; q" av', 'sxe -c ".echo PHASE5_EXIT; .lastevent; q" epr', 'g']
script = root / 'capture1.cdb'
script.write_bytes(('\n'.join(commands) + '\n').encode())
if os.environ.get('PHASE5_PREPARE_ONLY') == '1':
    print('commands prepared only')
    raise SystemExit(0)
env = os.environ.copy()
for name in list(env):
    if name.startswith(('NOVOVM_', 'AOEM_', 'PHASE5_')) or name=='CREDENTIALS_DIRECTORY':
        env.pop(name)
env.update({'PHASE5_TEST_ROOT':str(root/'run1-state'),'PHASE5_LIBRARY':str(root/'runtime/aoem_ffi.dll'),
            'NOVOVM_AOEM_DLL':str(root/'runtime/aoem_ffi.dll'),'NOVOVM_PROFILE_STAGES':'1',
            'NOVOVM_PROFILE_CRITICAL_PATH':'1','NOVOVM_PROFILE_ASSET_TRACE':'0','NOVOVM_PROFILE_NATIVE_DELIVERY':'1',
            'AOEM_SEMANTIC_GRAPH_V2_WORKERS':'2'})
assert not Path(env['PHASE5_TEST_ROOT']).exists()
debugger = Path('target/c-first-fault-20261113/sdk/extracted/Windows Kits/10/Debuggers/x64/cdb.exe').resolve()
import datetime
started=datetime.datetime.now(datetime.timezone.utc).isoformat()
with (root/'capture1-private.log').open('xb') as log:
    p=subprocess.Popen([str(debugger),'-cf',str(script),str(image)],env=env,stdout=log,stderr=subprocess.STDOUT)
    (root/'capture1-launch.json').write_text(json.dumps({'debugger_pid':p.pid,'start_utc':started,'image':str(image),'script':str(script),'maximum_seconds':180},indent=2))
    try:
        rc=p.wait(timeout=180);timed_out=False
    except subprocess.TimeoutExpired:
        # This PID belongs to this exact Popen; CDB launched only the isolated test binary.
        stopped=subprocess.run(['taskkill','/PID',str(p.pid),'/T','/F'],stdout=log,stderr=subprocess.STDOUT)
        rc=p.wait(timeout=20);timed_out=True
(root/'capture1-exit.json').write_text(json.dumps({'debugger_exit_code':rc,'timeout':timed_out,'maximum_seconds':180,'start_utc':started,'end_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()},indent=2))
print('debugger exit',rc,'timeout',timed_out)
