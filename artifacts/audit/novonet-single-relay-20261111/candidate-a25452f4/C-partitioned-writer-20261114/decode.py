"""Normal-stop strict decoder. Public report excludes addresses and input contents.
This is NOT a crash-dump decoder; interrupted latest slots cannot be trusted.
"""
from pathlib import Path
import collections,hashlib,struct
def decode(path):
 data=path.read_bytes();at=0
 def take(n):
  nonlocal at
  out=data[at:at+n];assert len(out)==n,'truncated capture';at+=n;return out
 assert take(8)==b'NCPART02'
 tid,total,words,number,calls,mismatches=struct.unpack('<6Q',take(48));assert words==20 and number==4
 channels=[];events=[]
 for c in range(4):
  tag,capacity,writes,retained,overwritten,sampled,lost,unregistered=struct.unpack('<8Q',take(64))
  assert tag==c and retained==min(capacity,writes) and overwritten==max(0,writes-capacity)
  own=[struct.unpack('<20Q',take(160)) for _ in range(retained)]
  assert all(e[4]==tid and e[17]==writes-retained+i+1 for i,e in enumerate(own))
  assert all(b[0]>a[0] and b[1]>=a[1] for a,b in zip(own,own[1:]))
  events+=own;channels.append(dict(channel='ABCD'[c],capacity=capacity,writes=writes,retained=retained,overwritten=overwritten,sampled_or_intentionally_dropped=sampled,reentrant_failed=lost,unregistered_failed_process_total=unregistered,kinds=dict(collections.Counter(str(e[2]) for e in own))))
 assert at==len(data) and sum(c['writes'] for c in channels)==total
 groups=collections.defaultdict(list)
 for e in events:
  if 100<=e[2]<200:groups[(e[4],e[5],e[16])].append(e)
 full=0;bad=0;text_bad=0;missing=0
 for group in groups.values():
  kinds={e[2] for e in group};ordered=sorted(group,key=lambda e:e[0])
  if not {101,102,103,104,105,106}.issubset(kinds):missing+=1;continue
  full+=1;expected=next(e[6] for e in group if e[2]==101)
  bad+=sum(e[6]!=expected for e in group)
  pending=None
  for e in ordered:
   if e[2]==103:pending=e
   if e[2]==104 and pending:
    text_bad+=int((e[15],e[18])!=(pending[15],pending[18]));pending=None
 lifecycles=[e for e in events if e[2] in (2,3)]
 created={e[7] for e in lifecycles if e[2]==2};dropped={e[7] for e in lifecycles if e[2]==3}
 public=dict(bytes=len(data),sha256=hashlib.sha256(data).hexdigest(),os_thread_id=tid,total_written=total,target_calls=calls,online_receiver_mismatches=mismatches,channels=channels,complete_sampled_source_groups=full,incomplete_retained_groups=missing,receiver_mismatches_in_complete_groups=bad,text_mismatches_at_pad_writer_in_complete_groups=text_bad,creation_and_drop_pairs=len(created&dropped),drops_without_retained_creation=len(dropped-created))
 return public,events
