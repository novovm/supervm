"""Offline review of one already-ended original-binary run; no target execution."""
import hashlib, json, pathlib, re, sys

root=pathlib.Path(sys.argv[1]);dest=pathlib.Path(sys.argv[2]);assert not dest.exists()
terminal=json.loads((root/'terminal-public.json').read_text())
raw=(root/'cdb-private.log').read_bytes();lines=raw.decode(errors='replace').splitlines()
samples=[json.loads(s) for s in (root/'samples-public.jsonl').read_text().splitlines()]
actions=[json.loads(s) for s in (root/'actions-public.jsonl').read_text().splitlines()]
pattern=re.compile(r'^EVENT (\w+) group=([\da-f]+) tid=([\da-f]+) rip=([\da-f]+) rsp=([\da-f]+) rbp=([\da-f]+)$')
groups={};malformed=[]
for line in lines:
    if not line.startswith('EVENT '):continue
    m=pattern.fullmatch(line)
    if not m:malformed.append(line);continue
    group,tid,rip,rsp,rbp=[int(s,16) for s in m.groups()[1:]]
    groups.setdefault(group,[]).append(dict(name=m[1],tid=tid,rip=rip,rsp=rsp,rbp=rbp,raw=line))
expected=['OUTER_CALL','LEA_BEFORE','LEA_AFTER','CHILD_BEFORE','CHILD_AFTER','COOKIE_PRECHECK']
complete=[];incomplete=[];bad=[]
for g,ev in groups.items():
    names=[e['name'] for e in ev]
    if names!=expected:
        incomplete.append(dict(group=g,events=names));continue
    outer,before,after,pre,post,check=ev
    tests=dict(same_tid=len({e['tid'] for e in ev})==1,
        outer_to_lea=outer['rbp']==before['rbp'],
        lea=after['rsp']==before['rsp'] and after['rbp']==before['rsp']-0x50 and after['rip']==before['rip']+5,
        pre=pre['rbp']==after['rbp'] and pre['rsp']==before['rsp']-0x150,
        child=pre['rbp']==post['rbp'] and pre['rsp']==post['rsp'] and post['rip']==pre['rip']+5,
        check=check['rsp']==post['rsp'] and check['rbp']==post['rbp']==check['rsp']+0x100)
    complete.append(g)
    if not all(tests.values()):bad.append(dict(group=g,tests=tests))
markers=[s for s in lines if s.startswith(('RBP_MISMATCH','EXCEPTION_CAPTURE','EXCEPTION code=','CONTROLLER_ACTION ','MONITOR_COUNTS ','MONITOR_ON','MONITOR_OFF'))]
errors=[s for s in lines if any(x in s for x in ['Numeric expression missing','Command file execution failed','Syntax error','Unable to insert breakpoint'])]
counts=[]
for line in markers:
    m=re.fullmatch(r'MONITOR_COUNTS groups=([\da-f]+) checks=([\da-f]+) hits=([\da-f]+) active_tid=([\da-f]+)',line)
    if m:counts.append(dict(zip(['groups','checks','hits','active_tid'],[int(v,16) for v in m.groups()])))
intervals=[]
own=[x for x in samples if x['index']==0]
for a,z in zip(own,own[1:]):
    intervals.append(dict(seconds=(z['file_unix_ns']-a['file_unix_ns'])/1e9,from_phase=a['phase'],to_phase=z['phase'],from_elapsed=a['elapsed'],to_elapsed=z['elapsed']))
all_phase_counts={p:len([x for x in own if x['phase']==p]) for p in ['baseline_off','monitor_on','recovery_off','sustained_on']}
last={str(i):next((x for x in reversed(samples) if x['index']==i),None) for i in range(4)}
result=dict(log_sha256=hashlib.sha256(raw).hexdigest(),complete_observed_groups=len(complete),incomplete_groups=incomplete,
    observed_event_count=sum(len(x) for x in groups.values()),bad_observed_groups=bad,malformed_events=malformed,
    command_errors=errors,markers=markers,monitor_counts=counts,debuggee_samples_by_phase=all_phase_counts,
    largest_report_interval=max(intervals,key=lambda x:x['seconds']) if intervals else None,
    last_numeric_progress=last,terminal=terminal,
    first_original_corruption_instruction_found=False,
    limits=['Only selected caller histories; global check has no prior history for unselected calls',
            'No per-instruction trace of inner callee; intact return does not prove every temporary RBP value',
            'Report write interval includes query/scheduling/serialization, not individual query latency',
            'Fresh loopback state and current user, not historical packet order or LocalService heap layout',
            'OS exit code alone cannot identify an external terminator; controller actions are separately retained'])
dest.mkdir()
(dest/'analysis-public.json').write_text(json.dumps(result,indent=2)+'\n',newline='\n')
selected=sorted(set(complete[:1]+complete[-1:]+[x['group'] for x in incomplete]))
excerpt=[e['raw'] for g in selected for e in groups[g]]
(dest/'event-excerpt-public.txt').write_text('\n'.join(excerpt+markers)+'\n',newline='\n')
print(json.dumps({k:result[k] for k in ['complete_observed_groups','incomplete_groups','bad_observed_groups','monitor_counts','debuggee_samples_by_phase','largest_report_interval']},indent=2))
