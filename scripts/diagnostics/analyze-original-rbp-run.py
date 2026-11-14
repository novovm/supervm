"""Read an existing isolated CDB log. Never executes a target or reads keys/DBs."""
import hashlib, json, pathlib, re, sys
root=pathlib.Path(sys.argv[1]); dest=pathlib.Path(sys.argv[2]);assert not dest.exists()
raw=(root/'cdb-private.log').read_bytes();lines=raw.decode(errors='replace').splitlines()
pattern=re.compile(r'^EVENT (\w+) group=([\da-f]+) tid=([\da-f]+) rip=([\da-f]+) rsp=([\da-f]+) rbp=([\da-f]+)$')
groups={};public=[];sequence=[];returns=[]
for line in lines:
    m=pattern.fullmatch(line)
    if m:
        name=m[1];nums=[int(s,16) for s in m.groups()[1:]]
        group,tid,rip,rsp,rbp=nums
        groups.setdefault(group,[]).append(dict(event=name,tid=tid,rip=rip,rsp=rsp,rbp=rbp))
        public.append(line)
    elif re.fullmatch(r'HEAP_RETURN [\da-f]+',line):
        returns.append(int(line.split()[1],16));public.append(line)
    elif re.fullmatch(r'CHILD_SEQUENCE [\da-f]+',line):
        sequence.append(int(line.split()[1],16));public.append(line)
expected=['OUTER_CALL','LEA_BEFORE','LEA_AFTER','CHILD_BEFORE','CHILD_AFTER','COOKIE_PRECHECK']
comparisons=[]
assert sorted(groups)==list(range(1,9)), 'not eight complete groups'
assert len(returns)==8 and sequence==[1]*16
for group,events in groups.items():
    assert [r['event'] for r in events]==expected
    assert len({r['tid'] for r in events})==1
    outer,before,after,pre,post,check=events
    nt=before['rip']-0x59dfc
    tests={
        'same_instruction_rsp':before['rsp']==after['rsp'],
        'lea_result_correct':after['rbp']==before['rsp']-0x50,
        'post_rip_is_next_instruction':after['rip']==before['rip']+5,
        'local_allocation_0x150':pre['rsp']==before['rsp']-0x150,
        'child_registers_preserved':(pre['rsp'],pre['rbp'])==(post['rsp'],post['rbp']),
        'child_return_is_call_plus_5':post['rip']==pre['rip']+5,
        'cookie_frame_correct':check['rbp']==check['rsp']+0x100,
        'same_parent_stack':check['rsp']==post['rsp'],
        'heap_return_matches':returns[group-1]==nt+0x5bddd,
        'covered_rvas_match':(pre['rip'],post['rip'],check['rip'])==(nt+0x5a05b,nt+0x5a060,nt+0x5a108),
    }
    assert all(tests.values()), (group,tests)
    comparisons.append(dict(group=group,tid=hex(before['tid']),tests=tests,
        lea_before={'rsp':hex(before['rsp']),'rbp':hex(before['rbp'])},
        lea_after={'rsp':hex(after['rsp']),'rbp':hex(after['rbp'])},
        cookie_precheck={'rsp':hex(check['rsp']),'rbp':hex(check['rbp'])}))
stop_markers=[s for s in lines if s.startswith(('CONTEXT_GAP','FIRST_CHANCE','LEA_MISMATCH','CHILD_INTERVAL_MISMATCH','PRECHECK_MISMATCH','HIT_LIMIT_STOP','CODE_MISMATCH_STOP'))]
errors=[s for s in lines if any(x in s for x in ['Numeric expression missing','Command file execution failed','Syntax error','Unable to insert breakpoint'])]
assert not errors and not stop_markers
assert 'CAPTURE_GROUP_LIMIT' in lines
execution=json.loads((root/'execution-public.json').read_text())
assert all(p['exit_code']==0 and not p['controlled_stop'] for p in execution['processes'])
result=dict(schema=1,groups=8,events=48,all_observed_groups_valid=True,comparisons=comparisons,
    observed_wrong_rbp=False,first_original_corruption_instruction_found=False,original_fault_reproduced=False,
    capture_stopped_at_group_limit=True,command_errors=errors,exception_or_gap_markers=stop_markers,
    original_heap_instruction=dict(rva='0x59dfc',bytes='488d6c24b0',instruction='lea rbp,[rsp-0x50]',kind='legal frame establishment'),
    debugger_and_three_peer_exits=execution['processes'],debuggee_exit_code_separately_collected=False,
    wall_seconds=execution['wall_seconds'],log_sha256=hashlib.sha256(raw).hexdigest(),
    limits=['8 selected active calls only','not all child RBP instructions covered','fresh loopback state, not original packet replay',
            'unknown context restoration not fully covered','no direct per-hit pause timing or non-debugged control; not low-interference qualified',
            'CDB exit code is not separately retained OS debuggee exit code'])
dest.mkdir()
(dest/'analysis-public.json').write_text(json.dumps(result,indent=2)+'\n',newline='\n')
(dest/'register-events-public.txt').write_text('\n'.join(public)+'\n',newline='\n')
print(json.dumps({k:result[k] for k in ['groups','events','observed_wrong_rbp','first_original_corruption_instruction_found','wall_seconds']},indent=2))
