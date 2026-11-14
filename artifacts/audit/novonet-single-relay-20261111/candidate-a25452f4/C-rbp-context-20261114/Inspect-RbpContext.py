"""Offline PID34452 context/stack comparisons; private registers stay local.

Usage: python Inspect-RbpContext.py DUMP PRIVATE_OUTPUT_DIRECTORY PUBLIC_JSON
The output directory must already exist with the approved private ACL.
No processes, registry, database or debugger are touched.
"""
import contextlib
import hashlib
import io
import json
import pathlib
import runpy
import struct
import sys

dump, private, public = map(pathlib.Path, sys.argv[1:4])
assert private.is_dir()
private_context = private / 'rbp12-contexts-private.json'
assert not private_context.exists(), 'Preserve previous evidence; use a new private directory'
helper = pathlib.Path(__file__).parents[1] / 'C-wer-natural-20261114/Inspect-NaturalDump.py'
old_argv = sys.argv
try:
    sys.argv = [str(helper), str(dump), str(private / 'rbp12-prerequisite-public.json')]
    with contextlib.redirect_stdout(io.StringIO()):
        h = runpy.run_path(str(helper))
finally:
    sys.argv = old_argv
data, streams, read, q = (h[x] for x in ('data', 'streams', 'read', 'q'))
offsets = {'RAX':120, 'RCX':128, 'RDX':136, 'RBX':144, 'RSP':152, 'RBP':160,
           'RSI':168, 'RDI':176, 'R8':184, 'R9':192, 'R10':200, 'R11':208,
           'R12':216, 'R13':224, 'R14':232, 'R15':240, 'RIP':248}

def decode(b):
    assert len(b) >= 1232
    v = {k:hex(struct.unpack_from('<Q', b, off)[0]) for k, off in offsets.items()}
    v['ContextFlags'] = hex(struct.unpack_from('<I', b, 48)[0])
    # AMD64 FXSAVE at +0x100; XMM0 at +0x1A0, each register 16 bytes.
    v['XMM6_XMM15'] = {f'XMM{i}':b[416+i*16:432+i*16].hex() for i in range(6,16)}
    return v

e = streams[6][0]
size, rva = struct.unpack_from('<II', data, e+160)
exception_context = decode(data[rva:rva+size])
global_context = decode(read(0x7ffce33228e0, 1232))
a, z = streams[3]
thread_context = None
for i in range(struct.unpack_from('<I', data, a)[0]):
    p = a + 4 + i*48
    tid = struct.unpack_from('<I', data, p)[0]
    if tid == h['tid']:
        size, thread_rva = struct.unpack_from('<II', data, p+40)
        thread_context = decode(data[thread_rva:thread_rva+size])
assert thread_context is not None
private_context.write_bytes((json.dumps({
    'exception_stream': exception_context, 'ntdll_report_context': global_context,
    'thread_list_capture': thread_context,
}, indent=2)+'\n').encode())

# Verify the exact instructions that save the check input and capture RBP.
assert read(0x7ffce32768b0,5).hex() == '48894c2408'
assert read(0x7ffce3276c50,7).hex() == '4889a9a0000000'
report_unwind = read(0x7ffce330ad5c,8)
assert report_unwind.hex() == '010c02000c011100'
rsp, rbp = h['rsp'], h['rbp']
slots = [
    {'owner':'RtlpReAllocateHeapInternal', 'slot':rsp+0x180, 'expected_from_caller':0x7739fee130},
    {'owner':'node_RVA_84ed90', 'slot':0x7739fee1a0, 'expected_from_caller':0x7739fee230},
    {'owner':'node_RVA_84f050', 'slot':0x7739fee2c0, 'expected_from_caller':0x7739fee350},
]
for s in slots:
    s['value'] = q(s['slot'])
    s['matches_caller_frame_calculation'] = s['value'] == s['expected_from_caller']
    for k in ('slot','value','expected_from_caller'):
        s[k] = hex(s[k])

# Raw return addresses corroborate physical frame spacing; not entry history.
returns = [(rsp+0x188,0x7ffce31abddd),(0x7739fee058,0x7ff7d34848f2),
           (0x7739fee0a8,0x7ff7d191eed9),(0x7739fee1a8,0x7ff7d191f359),
           (0x7739fee2c8,0x7ff7d1ae6211)]
for address, expected in returns:
    assert q(address) == expected

result = {
    'pid':34452,'dump_sha256':h['sha'],'fault_thread_id':h['tid'],
    'exception_context_file_rva':hex(rva),
    'exception_RIP':exception_context['RIP'],'exception_RSP':exception_context['RSP'],
    'exception_RBP':exception_context['RBP'],
    'exception_equals_ntdll_report_registers':exception_context==global_context,
    'thread_list_RIP':thread_context['RIP'], 'thread_list_RSP':thread_context['RSP'],
    'thread_list_RBP':thread_context['RBP'],
    'context_origin':'RtlCaptureContext followed by report-frame RtlVirtualUnwind; not an unprocessed CPU trap',
    'report_unwind_only_allocates_0x88_no_RBP_restore':True,
    'saved_report_input_equals_actual_rbp_slot_xor_rsp':q(rsp)==h['actual'],
    'saved_report_input_differs_from_correct_cookie':q(rsp)!=h['cookie'],
    'expected_RBP':hex(rsp+0x100),'deviation':hex(rbp-rsp-0x100),
    'correct_cookie_slot':hex(rsp+0x130),'saved_caller_RBP_slot':hex(rsp+0x180),
    'actual_cookie_read_slot':hex(rbp+0x30),
    'saved_RBP_slots':slots,'physical_return_addresses_checked':len(returns),
    'private_context_sha256':hashlib.sha256(private_context.read_bytes()).hexdigest(),
    'historical_save_values_proven_unchanged':False,'first_RBP_change_instruction_found':False,
    'node_restarted':False,'production_fix_applied':False,
}
public.write_bytes((json.dumps(result,indent=2)+'\n').encode())
print(json.dumps(result))
