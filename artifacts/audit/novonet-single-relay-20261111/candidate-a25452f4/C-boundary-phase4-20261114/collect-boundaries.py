# Offline harness collector only. Checks are performed by check-boundaries.py.
# Begin/end symbols are identical folded markers; one breakpoint toggles the window.
from pathlib import Path
import json, os, subprocess, hashlib
root = Path('target/c-boundary-phase4-20261114').resolve()
image = root / 'build/x86_64-pc-windows-msvc/release/c-profile-boundary.exe'
mapping = json.loads((root / 'c-profile-boundary-function-map.json').read_text())
assert mapping['sha256'] == hashlib.sha256(image.read_bytes()).hexdigest()
functions = {}
for name in ['format_write', 'pad_integral', 'string_writer']:
    found = mapping['functions'][name]
    assert len(found) == 1, (name, found)
    functions[name] = int(found[0]['rva'], 16)
module = 'c_profile_boundary'
commands = ['.sympath ' + str(image.parent), '.reload /f c-profile-boundary.exe', 'r @$t0=0', 'r @$t1=0', 'r @$t2=0']

def quoted(text):
    return chr(34) + text.replace(chr(34), chr(92) + chr(34)) + chr(34)
commands += ['x ' + module + '!phase4_window*']
commands.append('bp ' + module + '!phase4_window_begin ' + quoted('.if (@$t0==0) { r @$t0=1; r @$t1=@$tid; .printf "WINDOW_BEGIN tid=%x window=%x", @$tid,@rcx; .echo; } .else { .printf "WINDOW_END tid=%x calls=%x", @$tid,@$t2; .echo; r @$t0=0; }; gc'))

def bp(name, offset, stage, fields, extra=''):
    address = functions[name] + offset
    fmt = 'REC stage=' + stage + ' tid=%x seq=%x rsp=%p ' + ' '.join((k + '=%p' for k, v in fields))
    args = '@$tid,@$t2,@rsp' + ''.join((',' + v for k, v in fields))
    body = extra + '.printf ' + chr(34) + fmt + chr(34) + ', ' + args + '; .echo; '
    commands.append('bp ' + module + '+0x%x ' % address + quoted('.if (@$t0) { .if (@$tid==@$t1) { ' + body + '} }; gc'))
bp('format_write', 0, 'FMT_BEGIN', [('receiver', '@rcx'), ('cap', 'poi(@rcx)'), ('buffer', 'poi(@rcx+8)'), ('len', 'poi(@rcx+0x10)')], 'r @$t2=@$t2+1; r @$t3=@rcx; ')
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
commands += ['sxe -c ".echo PHASE4_AV; .exr -1; r; kv; q" av', 'sxe -c ".echo PHASE4_EXIT; .lastevent; q" epr', 'g']
script = root / 'capture3.cdb'
script.write_bytes(('\n'.join(commands) + '\n').encode())
if os.environ.get('PHASE4_PREPARE_ONLY') == '1':
    print('commands prepared only')
    raise SystemExit(0)
env = os.environ.copy()
for name in ['NOVOVM_PROFILE_STAGES', 'NOVOVM_PROFILE_CRITICAL_PATH', 'NOVOVM_PROFILE_ASSET_TRACE']:
    env[name] = '0'
debugger = Path('target/c-first-fault-20261113/sdk/extracted/Windows Kits/10/Debuggers/x64/cdb.exe').resolve()
with (root / 'capture3-private.log').open('xb') as log:
    run = subprocess.run([str(debugger), '-cf', str(script), str(image)], env=env, stdout=log, stderr=subprocess.STDOUT, timeout=120)
(root / 'capture3-exit.json').write_bytes((json.dumps({'debugger_exit_code': run.returncode, 'timeout_seconds': 120}) + '\n').encode())
print('debugger exit', run.returncode)
