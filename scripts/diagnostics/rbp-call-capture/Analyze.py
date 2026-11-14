"""Independent binary-record checker. Reads ONLY explicit isolated experiment dirs.

No register addresses are included in the public summary. Synthetic control
instruction RVA is published separately from any claim about the original bug.
"""
import argparse
import csv
import hashlib
import json
import math
import statistics
import struct
from pathlib import Path

FIELDS = ('entry_rbp entry_rsp return_address pre_rbp pre_rsp post_rbp post_rsp '
          'post_ip state target seq parent thread_id thread_created process_created depth '
          'stack_low stack_high exception_code exception_rip exception_rsp exception_rbp '
          'context_gap reserved').split()

def records(path):
    b = path.read_bytes()
    assert len(b) % 192 == 0, 'truncated record'
    return [dict(zip(FIELDS, v)) for v in struct.iter_unpack('<24Q', b)]

def validate(rows, allow_mismatch=False):
    ids = {}
    owner = None
    for r in rows:
        key = (r['process_created'], r['thread_id'], r['thread_created'])
        if owner is None:
            owner = key
        assert key == owner, 'thread identity changed'
        assert r['seq'] not in ids and (not ids or r['seq'] > max(ids)), 'call sequence reused'
        assert r['pre_rbp'] == r['entry_rbp']
        assert r['pre_rsp'] + 72 == r['entry_rsp']
        assert r['pre_rsp'] % 16 == 0 and r['entry_rsp'] % 16 == 8
        assert r['stack_low'] <= r['pre_rsp'] < r['entry_rsp'] < r['stack_high']
        if r['parent']:
            p = ids[r['parent']]
            assert p['depth'] + 1 == r['depth']
            assert r['entry_rsp'] < p['pre_rsp'], 'nested frame outside parent stack'
        else:
            assert r['depth'] == 0
        if r['state'] == 2:
            assert r['pre_rsp'] == r['post_rsp'] and r['post_ip'] != 0
            assert not r['context_gap'] and not r['exception_code']
            if not allow_mismatch:
                assert r['pre_rbp'] == r['post_rbp'], 'RBP preservation failed'
        else:
            assert r['state'] == 3 and r['context_gap'] == 1
            assert r['exception_code'] == 0xe0424242
            assert r['post_rbp'] == r['post_rsp'] == r['post_ip'] == 0, 'unwind mislabeled normal'
        ids[r['seq']] = r
    return len(ids)

def pe_bytes(path, rva, count):
    b = path.read_bytes()
    pe = struct.unpack_from('<I', b, 0x3c)[0]
    assert b[pe:pe+4] == b'PE\0\0'
    sections, optional = struct.unpack_from('<H', b, pe+6)[0], struct.unpack_from('<H', b, pe+20)[0]
    for i in range(sections):
        s = pe+24+optional+40*i
        vs, va, rawsize, raw = struct.unpack_from('<4I', b, s+8)
        if va <= rva and rva+count <= va+rawsize:
            return b[raw+rva-va:raw+rva-va+count]
    raise AssertionError('RVA outside captured file sections')

def quantile(values, q):
    return sorted(values)[max(0, math.ceil(q*len(values))-1)]

def metrics(path, concurrent=False):
    rows = list(csv.DictReader(path.open()))
    grouped = {}
    for mode in ('0','1'):
        subset = [r for r in rows if r['captured']==mode]
        vals = [int(r['wall_ticks' if concurrent else 'ticks'])/int(r['frequency'])*1e9/int(r['operations']) for r in subset]
        grouped[mode] = {
            'batches':len(vals),
            'batch_average_ns_per_operation_p50':statistics.median(vals),
            'batch_average_ns_per_operation_p95':quantile(vals,.95),
            'batch_average_ns_per_operation_p99':quantile(vals,.99),
            'total_operations':sum(int(r['operations']) for r in subset),
            'aggregate_operations_per_second':sum(int(r['operations']) for r in subset)/sum(int(r['wall_ticks' if concurrent else 'ticks'])/int(r['frequency']) for r in subset),
            'process_cpu_seconds':sum(int(r['cpu100ns']) for r in subset)*1e-7,
        }
    delta={}
    for q in ('p50','p95','p99'):
        field='batch_average_ns_per_operation_'+q
        delta[q+'_percent']=(grouped['1'][field]/grouped['0'][field]-1)*100
        delta[q+'_absolute_ns']=grouped['1'][field]-grouped['0'][field]
    return {'baseline':grouped['0'],'capture':grouped['1'],'delta':delta,
            'statistics_scope':'Percentiles of batch averages, NOT individual-call tail latencies; no production timing qualification'}

def main():
    p=argparse.ArgumentParser();p.add_argument('fixture',type=Path);p.add_argument('single',type=Path);p.add_argument('concurrent',type=Path);p.add_argument('output',type=Path)
    a=p.parse_args();assert not a.output.exists()
    f=json.loads((a.fixture/'fixture.json').read_text())
    rs=records(a.fixture/'fixture-records.bin');assert len(rs)==4
    validate(rs,True)
    assert [i for i,r in enumerate(rs) if r['state']==2 and r['pre_rbp']!=r['post_rbp']]==[1]
    assert rs[1]['pre_rbp']==f['bad_before'] and rs[1]['post_rbp']==f['bad_after']
    assert f['bad_before']^f['bad_after']==0x4000
    instruction=pe_bytes(a.fixture/'qualify.exe', f['bad_write_rva'],7)
    assert instruction==bytes.fromhex('48 81 f5 00 40 00 00')
    # Malformed/mixed evidence must fail independently of the target's own checks.
    negative=[]
    for name,mutate in (
        ('changed_rsp',lambda r:r[0].__setitem__('post_rsp',r[0]['post_rsp']+8)),
        ('reused_sequence',lambda r:r[1].__setitem__('seq',r[0]['seq'])),
        ('mixed_thread',lambda r:r[1].__setitem__('thread_id',r[1]['thread_id']+1)),
        ('unwind_as_return',lambda r:r[2].__setitem__('state',2)),
    ):
        copy=[dict(r) for r in rs];mutate(copy)
        try:validate(copy,True)
        except AssertionError:negative.append(name)
        else:raise AssertionError('checker accepted '+name)
    owners=[];nested=0;reused_stack_count=0
    for i in range(4):
        r=records(a.fixture/f'thread-{i}.bin');nested+=validate(r)
        assert len(r)==700
        owners.append((r[0]['thread_id'],r[0]['thread_created']))
        reused_stack_count+=len(r)-len({x['entry_rsp'] for x in r})
    assert len(set(owners))==4 and reused_stack_count>0
    heap=records(a.single/'heap-last-batch.bin');validate(heap);assert len(heap)==4096
    total=0
    for i in range(4):
        r=records(a.concurrent/f'concurrent-thread-{i}.bin');total+=validate(r);assert len(r)==4096
    report={
        'schema':1,'original_fault_reproduced':False,'first_original_rbp_corruption_found':False,
        'controlled_fixture':{'instruction_rva':hex(f['bad_write_rva']),'bytes':instruction.hex(),
            'instruction':'xor rbp, 0x4000','scope':'deliberate isolated ABI violation only','matched_call_records':4,
            'unwind_context_gap':True,'negative_checker_cases_rejected':negative},
        'concurrent_lifecycle':{'threads':4,'records':nested,'reused_stack_addresses_with_distinct_sequences':reused_stack_count},
        'actual_system_heap':{'single_thread_last_batch_checked':len(heap),'concurrent_last_batch_checked':total,
            'rbp_return_mismatches':0,'scope':'new private heap; not NOVOVM traffic or original invocation'},
        'single_thread':metrics(a.single/'batches.csv'),
        'shared_heap_four_threads':metrics(a.concurrent/'concurrent.csv',True),
        'limits':['RSP destruction may prevent post-call collection','Unknown nonlocal restoration has no complete history',
                  'Violation returning normally is bracketed to a call, not its first instruction',
                  'Original internal cookie fast-fail may occur before wrapper return',
                  'Wrapper adds a frame and restores saved RBP after recording; isolated diagnostic only',
                  'No hooking or patching of original EXE, DLL or processes'],
        'input_hashes':{str(path.relative_to(path.parent.parent)):hashlib.sha256(path.read_bytes()).hexdigest()
             for root in (a.fixture,a.single,a.concurrent) for path in root.iterdir()
             if path.suffix in ('.bin','.csv','.json')},
    }
    a.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:report[k] for k in ('controlled_fixture','single_thread','shared_heap_four_threads')},indent=2))

if __name__=='__main__':main()
