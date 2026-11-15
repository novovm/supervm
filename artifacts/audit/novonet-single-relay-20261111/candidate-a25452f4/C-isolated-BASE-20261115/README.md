# C isolated BASE: stopped early by diagnostic controller

## Terminal result (2026-11-15 11:31 Beijing)

This is NOT a completed 75-minute BASE and NOT a natural NOVOVM crash. One run only; no retry.
LocalService independently generated eight new test purpose keys; prepare exit 0. Four original a254 EXEs used new isolated databases and identities. All four actually loaded AOEM966 SHA256 5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52.

At elapsed 135.7303592 seconds the controller classified `load_not_qualified` and killed the four isolated processes. All four real exit codes are -1, explicitly `forced_by_controller=true`. No natural failure code was observed. End UTC03:31:42.607; no DMP generated; WHEA query after run returned no new events. No result supports hardware health or an original-crash fix.

### Why this run stopped

Owner.ps1 line68 required every sampled report to contain `novonet`. Peer index3 had this field in the previous eight samples; in the last sample the field was absent while transport remained present and data writes grew 1709 to1941. Its process was alive. Target index0 had111124 ticks,1968 data writes,437 accepted data,1496 history data and0 execution failures.

Original a254 source has two separate elapsed-time checks: first collecting `network.observation()`, then after `product.poll()` deciding to emit `product.status()`. The second check can cross the threshold when the first did not, legitimately leaving `novonet` absent in that report. This statically proves the diagnostic predicate is insufficient; the precise timing of this particular iteration was not captured. Missing one optional field is not evidence that networking stopped. No production source change was made.

Launch-Once.ps1 also checks result before sleeping, then checks task state without rechecking result. It emitted `owner task ended without result` at03:31:50 although result.json had already been written at03:31:42. Both records are preserved; bootstrap error does not invalidate the saved target exit codes. Scheduled-task final exit/cleanup could not be read from the ordinary account (access denied); removal is not claimed. Task has no trigger or auto-retry. A subsequent process query found no original or isolated node running.

### Evidence limits and next action

9 samples are retained, not additional test executions. No exact network.observation/product.status invocation counts or allocation/free counts were instrumented. Loopback input, fresh state and four local peers are not historical packet replay. This short run does not cover the66-minute failure window. Full-dump capability was configured but not exercised by a natural crash here.

Preserve this failed run. Before a separately authorized new run, correct only the diagnostic qualification predicate to use previously established capability plus actual counter progress and explicit errors; recheck result after the bootstrap wait. Do not change production reporting to satisfy the controller. Do not restart this run or original nodes. Original DB, identity and previous evidence untouched; double relay paused. Private keys, configs and dumps stay local; diagnostic scripts/binaries remain under ignored target, outside release source.

## Initial launch record (historical; superseded by terminal result)

# C isolated BASE — started, not an acceptance result

2026-11-15 Beijing11:29:14. LocalService generated eight fresh sealed purpose keys
using the existing verified helper in prepare mode only (exit0). Test genesis and
carrier keys are isolated; no original key/database was read or reused. Original
DB is not opened with create/initialize: all four databases are new private test
paths. Helper executable SHA27140551...d2caf73 matches the archived build.

Four loopback native processes started once: target8088, peers22668/10600/42992.
Byte-identical original a254 EXE renamed novovm-base-a254.exe to use only the already
qualified isolated WER rule, and AOEM966. Full hashes in started.json. No CDB/PT/TTD,
modified standard library, new build, hardware stress or production configuration
change. Same executable used by all peers. No probe object commands or relay test.

Configured native duration4480000ms (74m40s), owner hard bound4490s (74m50s), latest
expected node boundary12:44:04 Beijing. One manual LocalService task, no restart.
At a natural nonzero exit, owner stops only surviving new-run peers, retains actual
exit codes/forced status, then checks isolated dumps. Owner also stops on missing
network load, memory4GiB combined private budget or C free-space below8GiB. Initial
free-space precondition12GiB passed. Original settings/ACLs remain unchanged; only
new public evidence directory grants LocalService output access. Test roots remain
private; identities and raw dumps never go to Git.

Initial reports show real native network records/data writes/accepted history and
periodic status in all four processes. Samples read existing files every15s and
record CPU/private bytes/working set/handles/threads and named transport counters.
Exact network.observation()/product.status() calls and object allocation/free
counts are not supplied by this unmodified binary and remain unknown. No report
count is relabelled as those function calls. Private bytes is process private
commit, not total system commit. Workers1/batch1 and loopback topology are explicit
isolation settings, not an exact historical four-machine packet replay.

WER image rule: Full Dump2, retain2. Existing isolated dump directory was empty
and LocalService write check passed. Concurrent multiple crashes could exceed the
two-dump cap; final inventory must report what actually exists, not promise four
full dumps. FullMemory flag alone will not prove every required page is present.

Before-run WHEA history has14 entries, latest110869. Final WHEA, exit codes, load
summary and dump inventory will be added after this one bounded run. As of this
commit the run is in progress; original fault NOT fixed, double relay paused.
