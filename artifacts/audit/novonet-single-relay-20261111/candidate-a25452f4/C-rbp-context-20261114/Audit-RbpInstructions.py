"""Verify existing offline disassemblies against pinned dump bytes.

Usage: python Audit-RbpInstructions.py DUMP PRIVATE_DIR CAPSTONE_DIR PUBLIC_JSON
This catalogs static candidates, NOT a trace of executed instructions.
"""
import contextlib
import io
import json
import pathlib
import re
import runpy
import sys

dump, private, capstone_path, output = map(pathlib.Path, sys.argv[1:5])
sys.path.insert(0, str(capstone_path))
from capstone import Cs, CS_ARCH_X86, CS_MODE_64

helper = pathlib.Path(__file__).parents[1] / 'C-wer-natural-20261114/Inspect-NaturalDump.py'
old = sys.argv
try:
    sys.argv = [str(helper), str(dump), str(private/(output.stem+'-precondition.json'))]
    with contextlib.redirect_stdout(io.StringIO()):
        h = runpy.run_path(str(helper))
finally:
    sys.argv = old
read = h['read']
decoder = Cs(CS_ARCH_X86, CS_MODE_64)
decoder.detail = True
logs = ['frame-private.log','rbp12-private.log','rbp12-slots-private.log',
        'rbp12-callees-private.log','rbp12-last-private.log',
        'rbp-branch-aligned-private.log']
groups = {}
for name in logs:
    command = None
    for line in (private/name).read_text(errors='replace').splitlines():
        if line.startswith('0:001> '):
            command = line.split('> ',1)[1]
        if not command or not command.startswith(('uf ','u ')):
            continue
        m = re.match(r'^([0-9a-f]{8}`[0-9a-f]{8}) ([0-9a-f]+)\s+', line)
        if not m:
            continue
        address = int(m[1].replace('`',''),16)
        raw = bytes.fromhex(m[2])
        assert read(address,len(raw)) == raw, 'Dump/code mismatch at '+hex(address)
        inst = list(decoder.disasm(raw,address))
        assert len(inst) == 1 and inst[0].size == len(raw)
        g = groups.setdefault(command, {})
        ins = inst[0]
        regs = [ins.reg_name(x) for x in ins.regs_access()[1]]
        g[address] = {'address':hex(address),'instruction':ins.mnemonic+' '+ins.op_str,
                      'writes_rbp':any(x in regs for x in ('rbp','ebp','bp','bpl')),
                      'explicit_rsp_change':('rsp' in regs and ins.mnemonic not in ('call','ret','push','pop'))}
result = {'dump_sha256':h['sha'],'executed_path_reconstructed':False,
          'scope':'Only disassemblies named below; child paths may not have executed', 'groups':[]}
for command, ins in groups.items():
    result['groups'].append({'command':command,'verified_instruction_count':len(ins),
        'rbp_writes':[x for x in ins.values() if x['writes_rbp']],
        'rsp_changes':[x for x in ins.values() if x['explicit_rsp_change']]})
output.write_bytes((json.dumps(result,indent=2)+'\n').encode())
print(json.dumps({'groups':len(groups),'dump_bytes_match':True,
    'internal_rbp_writes':result['groups'][0]['rbp_writes']}))
