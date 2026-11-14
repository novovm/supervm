from pathlib import Path
import json
from decode import decode
ROOT=Path('target/c-partitioned-writer-20261114/retention-output')
before,events=decode(ROOT/'before-overflow.bin');after,_=decode(ROOT/'after-overflow.bin');other,_=decode(ROOT/'unregistered.bin')
assert before['channels'][0]['writes']==2 and before['creation_and_drop_pairs']==1
assert before['complete_sampled_source_groups']==1 and before['receiver_mismatches_in_complete_groups']==0
assert before['channels'][3]['sampled_or_intentionally_dropped']==200000
assert before['channels'][3]['writes']==0
assert before['channels'][0]==after['channels'][0]
assert after['channels'][1]['writes']==8206 and after['channels'][1]['overwritten']==14
assert [c['unregistered_failed_process_total'] for c in other['channels']]==[1]*4
# The decoder must reject a deliberately truncated copy without accepting history.
bad=ROOT/'truncated.bin';bad.write_bytes((ROOT/'before-overflow.bin').read_bytes()[:-1])
try:decode(bad)
except (AssertionError,ValueError):rejected=True
else:rejected=False
assert rejected
out=Path(__file__).parent/'retention-public.json';assert not out.exists()
out.write_bytes((json.dumps({'collector_only':True,'synthetic_event_fields':True,'not_original_fault_reproduction':True,'registered_hot_path_allocations':0,'ordinary_events_did_not_evict_AB':True,'truncation_rejected':rejected,'before':before,'after_same_channel_overflow':after,'unregistered_thread':other},indent=2)+'\n').encode())
print('Partition retention, same-channel overflow, unregistered loss and truncation checks passed')
