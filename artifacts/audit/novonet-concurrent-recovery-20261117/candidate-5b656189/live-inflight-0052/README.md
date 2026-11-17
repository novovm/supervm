# Paired actual objects: both sender DONE, no in-flight overlap

## Final receiver/hop audit: both deliveries accepted; overlap still failed

A pulled actual C d2cf9b56 and D4c618286, independently checked15 original file
lengths/SHA256 and all5 raw snapshots. Both paths have unique same-object/session
target integrity events, original sender DONE and all3 adjacent authenticated
bindings matching. Both relays on each route forwarded9/9packets, failed writes0/0.
Independently derived receiver transfer tags match1/1fragment/hash verification,
zero duplicate data/repair; complete peer-time intervals lie inside original
validity. Both collectors exited0. [Independent raw correlation](DUAL-END-CORRELATION.json).

Both delivery chains are accepted. The measured461ms minimum separation and
zero overlap remain unchanged: these successes do not establish simultaneous
in-flight operation. C/D no further collection, new window, resend or restart.
Next missing implementation is original-owner bounded diagnostic release
coordination; do not repeat completed object tests or compensate by altering
payload size, production timeout, resource limits or receipt semantics.

A pulled fresh C/D READY throughd35e4eed, confirmed original exact5b owners,
two-circuit scope and no terminal outputs, then executed original -ConcurrentOnly
once. Controller exited0 at Beijing00:52:05.994, no B stop or accepted test repeat.
Both original objects reached sender DONE; receiver/hop originals were requested
from the SAME observers and are now independently checked above.

Actual objects and conservative timing are in [independent correlation](TIMING-CORRELATION.json).
A admission→DONE1794934320013→1794934320054 (41ms). B admission→DONE
1794934321333→1794934321375 (42ms) on B clock. Three management challenges before
and after give offset interval807..818ms. Even the earliest possible B admission
is461ms after A DONE. **Concurrency FAIL/not demonstrated**, not rounded to pass.

Original expiring CLI command files were published near-simultaneously (within
13ms after accounting for measured offset), but accepted87ms/602ms later. Source
`Control::poll` starts one bounded async read only every1s; independent read phases
explain why command synchronization cannot guarantee overlapping short objects.
This is an observed diagnostic scheduling limitation, not evidence that the
underlying relay cannot carry simultaneous objects. No authorization, TTL,
production quota/timeout or object size was changed.

Preserve this negative sample and original one-pair delivery results. No automatic
retry. Next implementation must coordinate bounded diagnostic release in the
existing owner rather than repeatedly racing file-poll phases, padding the
payload, slowing delivery or inventing another transport/test service. Any
product change needs exact-source regression and candidate qualification; no
unqualified update is performed here. C/D retain and return current raw snapshots,
collector exits and hashes; they must not rerun completed tests or restart nodes.
