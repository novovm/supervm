# a25452f4: approved existing updates, then real double relay

## CURRENT 2026-11-13 18:36 Beijing: C traces pointer mismatch; no native fix claimed

C independently confirmed exception/thread contexts and extended the captured
unwind-code comparison. Saved argument pointers differ from the crash-time
RSI; the first writer is still unknown. Official instruction-history tools
were built and a separate faulty-pointer fixture was checked. Recording UAC
returned cancelled: no trace, node restart or observer launch. A corrected
aligned-fixture entry is prepared but has not run.
The initial unaligned fixture failed as Rust abort and is preserved; the
separate aligned fixture passes its direct clean/AV controls, not TTD yet.

**C NOT READY; A must not send.** Original DB/identity/failures remain.
[Pointer evidence, tool provenance and exact limitations](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/pointer-followup/README.md).

## PRIOR 2026-11-13 17:44 Beijing: known fixes pushed; native AV remains unresolved

1f4bd204 contains the snapshot deadline-latch fix and diagnostic DateTime
cleanup correction, with full node check/strict Clippy and scoped regressions.
No candidate was installed. First-chance full dump remains private on C;
selected captured instructions/unwind metadata match disk, but the first bad
parameter/register writer is not identified. A bounded independent30-second
stack-capture experiment completed26,114,540captures without that AV; it is
not exact a254/PageHeap reproduction and does not clear the native fault.

Fresh read-only end state confirms no node/debugger/listeners or residual
PageHeap flags. Stop UAC cancelled and was never executed; no further prompt
or stop is needed for the captured process. Node normal exit code remains
unknown (debugger quit after capture), not replaced by debugger0. **C NOT READY;
A must not send.** C continues local native diagnosis, without automatic rerun.
[Offline evidence and limitations](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/offline-review/README.md).

## PRIOR 2026-11-13 17:25 Beijing: two fixes verified; native AV still open

C fixed the actual snapshot publisher's two-clock-check boundary: one latched
decision now controls network observation and progress emission. The original
reader is unchanged. Original-source regression fails on a254 and passes4/4
after the fix; full node check with iroh-transport, strict Clippy and rustfmt
all exited0. V4 debugger-cleanup DateTime comparison is also corrected and
5cases passed; original failed source retained. No new program was installed.

Original Watch-Relay failed16:59; the same bounded diagnostic later captured
the17:08first-chance AV in ntdll unwinding/heap stack capture. Full dump remains
private on C. This is not yet the first corrupting write or a proven common
cause with prior String faults. C continues independent source/dump diagnosis.
Node/debugger are absent; **C NOT READY, A must not send. No automatic rerun.**
[Verified fix scope and checks](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/known-fixes/README.md).

## PRIOR 2026-11-13 17:08 Beijing: C captured AV; original observer also failed

Original Watch-Relay exited1 at16:59:18 after missing `novonet` for the unchanged
15-second bound. Coordinator exited1; a DateTime/string identity comparison
defect skipped debugger cleanup. Separate stop UAC was cancelled, not executed.
The same instrumented node subsequently hit a first-chance write AV in ntdll
stack unwinding / heap stack-capture. CDB captured and verified a277,824,196-byte
full-memory dump by17:08:53, then quit; debugger exit0 is not node success.
Node normal exit is unknown. No new node was launched. The two-hour window failed.

C owns analysis. The snapshot publisher's duplicate-time-check defect has a
source-level fix and deterministic regression (old fails, fixed4/4); full node
check pending. That fix is not declared the cause/fix of the native memory crash.
Raw dump/log/config remain private. **relay_ready=false; A must not send.**
[Exact original errors, chronology, dump hash and limits](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/README.md).

## PRIOR 2026-11-13 16:56 Beijing: C original-observer fault diagnostic RUNNING

The reissued Windows UAC was actually accepted. LocalService clean/overrun
controls and a13,244,674-byte full-memory dump passed. Original a254/AOEM966
node PID17748 started16:52:00 under CDB12524, original unmodified Watch-Relay
PID37152 at16:52:04, same fresh original progress path. Prior progress was
archived byte-for-byte with hash verification; original DB existing/identity
retained. The original7200-second observation is active, expected through18:52,
with node/CDB bounds18:54/18:56. No fault or terminal result is claimed yet.

**Fault reproduction only: relay_ready=false, A_must_not_send=true.** Do not
send a double-relay object or treat the private original observer READY as a
service acceptance. C owns diagnosis; A/B/D are not operated. The first cancelled
UAC and all earlier failures remain. [Actual controls/startup and sampled liveness](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/README.md).

## PRIOR 2026-11-13 16:45 Beijing: original-workflow crash capture prepared

C prepared a single corrected PageHeap capture with the original unmodified
Watch-Relay/reader and7200-second observation. The node's bounded122minutes
covers startup/window plus shutdown margin; debugger124minutes. The old progress
file is archived and hash-verified before its original path is reused.21local
preparation checks passed; live LocalService controls/full-dump capture and
Windows UAC remain pending. Original a254/AOEM966, DB existing and identities
are retained. No new node or observer has yet started at this checkpoint.

**Diagnostic only: C NOT relay READY. A must not send an object.** No A/B/D
operation is requested. C owns reproduction/source diagnosis, with no automatic
retry. [Exact scripts, bounds and live gates](C-source-investigation-20261112/C-first-fault-20261113/relay-v4/README.md).

## PRIOR 2026-11-13 16:24 Beijing: C PageHeap run ended; fault unresolved

Scope correction from the user's original scenario: **C has already crashed
repeatedly**, and observation attempts have repeatedly failed. The30-minute
diagnostic did not cover the observed77/95-minute crash latencies and did not
run the original Watch-Relay loop. Do not interpret its non-reproduction as
clearing that workflow. [Exact chronology and next diagnostic target](C-source-investigation-20261112/C-first-fault-20261113/SCENARIO-COMPARISON.md).

The single C diagnostic node PID15012 exited at12:14:52.197 Beijing after
about30minutes8seconds. The original CDB process-exit event confirms **target
exit0**, separately from debugger/coordinator exit0. No first AV/verifier stop
was recorded after diagnostic readiness. Classification: bounded non-reproduction,
**not a fix**. The known v2 dump-path limitation and corrected isolated v3 dump
test are preserved. Four exact-source FFI descriptor layouts match; this does
not establish memory safety or the reason A/D have not reproduced the fault.

Owner end checks confirm protected identity/network files unchanged and original
DB `existing`. Fresh read-only collection confirms no node/debugger/listeners
and no residual PageHeap IFEO keys. Raw logs/dumps/config remain private;
original failures remain intact. **C is NOT READY. A must not send a double-relay
object. No observer or node has been restarted.** C retains source diagnosis;
no A/B/D action is requested by this result.

[Original public terminal evidence, actual exit event, limits and remaining gaps](C-source-investigation-20261112/C-first-fault-20261113/README.md).

## PRIOR 2026-11-13 11:56 Beijing: C bounded PageHeap diagnosis, NOT relay READY

C started one instrumented original-node run at03:44:44Z (PID15012/CDB37556),
using a254 + AOEM966 and the original `existing` database/identity. Actual
PageHeap loader enablement and deliberate-overrun trap were observed. One
tool limitation remains in this live run: CDB escaped dump-path backslashes;
fault text is retained but a full dump is not assured. Future tooling has been
corrected and its dump output independently checked, without restarting C.
The run is bounded to30/32minutes; no fault has yet been observed at checkpoint.
This is neither a fix nor permission for A to send a relay object. C owns the
investigation; original failures are retained and A/B/D require no action.
See [actual startup, capture limitation and corrected-tool tests](C-source-investigation-20261112/C-first-fault-20261113/README.md).

## PRIOR 2026-11-13 11:12 Beijing: C crashed again; double relay blocked

PID37796 exited at10:15:56 Beijing with `0xC0000005`, after the one-hour
observer had ended at09:42. C's original dump and owner exit are preserved.
13/14 node return RVAs match the earlier PID27928 crash; the remaining frame
is in the same function. The repeated status/profile/hex/String-copy path
identifies the failure site, not its origin. New AOEM966 did not eliminate it.
The 180-second original-profile isolation passed without reproducing it;
**this is not a fix**. C remains down; no restart or relay observer was launched.

[Original exit, dump hash, stack comparison, collection limitation and current
read-only process/listener result](C-source-investigation-20261112/C-AOEM966-controlled-20261113/backtrace-exit-37796/README.md).
A must not send against the old C READY. C owns the continued source diagnosis.
No A/B/D action requested. Original DB, identity and earlier evidence preserved.

## PRIOR 2026-11-13 08:08 Beijing: C NOT READY; new AOEM run exited

Update 08:41 Beijing: the user explicitly authorized C to investigate/fix
locally and, if needed, run one controlled recovery with backtrace. C started
one new `existing` task with the same a254 executable, AOEM966, original
database/network/identities and `RUST_BACKTRACE=full`. PID37796 bound both
original ports. This is a diagnostic run, **not C relay READY or a fix**.
Old logs/tasks remain intact. C owns the investigation; A must wait for a
fresh relay-ready handoff before sending. Startup evidence and the one-shot
script are in the linked C report directory.

09:16 Beijing checkpoint: same PID37796/creation time, task Running, both
original listeners and stderr length 0 after about35 minutes. These are
process-liveness observations, not an authenticated health query or a fix.
The bounded read-only observer PID27056 started00:41:55Z for up to3600seconds;
on exit it preserves stderr/stdout/exit results without restarting. No relay
observer has been rearmed. The direct AOEM register-preservation isolation
and its deliberate clobber control passed; see `abi-isolation/README.md` in
the C report. The real IndexMap panic remains unresolved.

C continues the local diagnosis. If A still has the original a254 PDB, return
its hash/location for transfer: the fixed executable requests GUID
`E16D0B11-47CA-4EBF-B482-C91380CC57F8`, age1. C already has the older875 PDB;
it must not be substituted as exact a254 symbols. This is an artifact request,
not a transfer of diagnosis ownership or a prerequisite to C's ongoing work.

C's isolated AOEM `9662470d` task exited at 08:07:32 Beijing with node exit
code 1. Stderr reports a `novovm-native-tick` IndexMap out-of-bounds panic.
The read-only observer ended with no node or C UDP listeners; it did **not**
time out. Preserve this failure and do not use the earlier C READY to send an
A→B→C→D object. Do not silently restart C or re-arm an observer. See
[the C terminal result](C-source-investigation-20261112/C-AOEM966-controlled-20261113/README.md).

## PRIOR 2026-11-13: C AOEM-only existing recovery under observation

C used the unchanged a254 node executable, original database, protected
identity and network config with the newly built AOEM `9662470d` Fullmax
Windows runtime. The separate LocalService task loaded the new DLL and opened
the original database with `existing`; PID15612 bound41052/42052 and progress
matched the old height/head at the first two observations. The previous a254
task and its crash evidence remain intact. This **does not yet establish the
cause of the prior crash** or qualify A/B/D for an AOEM update. See
[C controlled runtime comparison and initial evidence](C-source-investigation-20261112/C-AOEM966-controlled-20261113/README.md).

A should query the current C process through the existing authenticated path,
then coordinate a bounded relevant workload. The user has confirmed that
historical crash-root attribution is not a prerequisite for C's continued
test-network use if it stays healthy; see the linked C report. C should retain
crash and health evidence without automatic restart. The four-host version inventory below is
still required to make later synchronized decisions; its historical status
describes the state before this C-only experiment.

## PRIOR 2026-11-13: AOEM version comparison before any new runtime update

AOEM GitHub `main@9662470d` now contains the previously missing
`aoem_compute_result_take_v1` and the FFI ownership correction. C built and
component-checked its Windows Fullmax core, then produced a separate Windows
bundle candidate. **No host has installed this candidate. C node is still down;
the crash root cause remains open.** The older pinned-source ZIP is comparison
evidence only.

[Four-host coordinated read-only version instructions, exact hashes and
Windows/Linux collectors](C-source-investigation-20261112/AOEM-FOUR-HOST-VERSION-HANDOFF-20261113.md)
are the current action. A should pick one common capture window for A/B/C/D
and collect actual loaded-module and installed-manifest reports. B's Linux SO
hash is not expected to equal the Windows DLL hash, but source, ABI and release
identity must align. Do not update, restart or send a relay object merely to
complete this version inventory.

## C owns source diagnosis — 2026-11-12 23:00 Beijing

The user assigned C to investigate locally, without waiting for A's diagnosis.
[New original-dump comparison and pinned-AOEM/profile isolation](C-source-investigation-20261112/README.md)
are complete. The isolation passed but did not reproduce or fix the crash.
C remains NOT READY; no restart, observer or object was launched.

## C BLOCKER 2026-11-12 22:46 Beijing: NOT ARMED; a254 node already crashed

C's one observation launcher stopped before Watch invocation: no active node
or41052/42052 listeners. Original a254 PID27928 crashed at09:50:53Z (17:50:53
Beijing), Application1000 record546473 / c0000005 / VCRUNTIME140.dll+0x1126b;
task Ready/result3221225477 and original existing exit -1073741819 agree.
See [actual C preflight and terminal-file absence](C-double-relay-ready-20261112/README.md).
No READY/liveness/observer PID exists. No restart, update, capture retry or send.
A must not send on old C health or D-only readiness. Recovery/fix remains a
separate next action; the same module/offset is not a confirmed root cause.

## CURRENT 2026-11-12 16:54 Beijing: all four updated; arm C/D original observers

A fetched D1d7714c3, checked15 raw evidence hashes and update exit0.
D a254 PID20876/LocalService, original database/custody/progress unchanged;
A->D authenticated Status38ms, original root unchanged. See
[actual query](A-query-D-20261112/health-public.json). B/A/C/D are now a254.
No object has been submitted in this candidate. Do not repeat any update.

**C and D now:** each start ONE existing Watch-Relay.ps1 with adjacent
Read-RelayProgress.ps1 from the parent audit directory, administrator PowerShell7,
hidden independent process. Check no same-purpose observer is already running;
if one exists, report its actual identity/state rather than killing by broad name.
Use these fixed arguments (HostId C or D):

```powershell
-HostId C -Source a25452f4c49be37cd984e650f2079642e6c273b3 -NodeSha256 1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d -WorkName novonet-observation-a25452f4-20261112 -EvidenceRoot '<fresh-local-evidence-root>' -MaximumWaitSeconds 7200
```

Original observer SHA256 d0dce2979cb51c39edf545637d276a80d56b921bfd194130842b3a475df966d9;
reader SHA256 ccc8f9592a9fb12d68531dc2ace804efb544b9921e4f43c51147f2de57cbcd44.
No source change or new node/network process. Preserve actual stdout/stderr and
exit/result/error. Immediately publish ready.json AND current liveness.json,
observer PID/creation time, node PID and terminal-file presence; do NOT wait for
the2-hour bound. Put receipts under C-double-relay-ready-20261112 or
D-double-relay-ready-20261112 here. Reply C_RELAY_OBSERVER_ARMED /
D_RELAY_OBSERVER_ARMED only if real startup and liveness succeed. On failure
return actual error, no repeat observer/update/initialize. Do not send objects.

A is already elevated; no sender-UAC wait is required. A will check fresh C/D
readiness using the existing guard, refresh B's original progress immediately
before sending, then issue one unique A->B->C->D diagnostic through the original
owner. B/C are opaque forwarding roles, D final receiver. B's original bounded
relay records are read before/after the attempt, not a new socket or service.
The original one-shot sender is parameter-adapted for two ordered --via values;
35s setup observation /15s DONE observation and original cancellation unchanged.
Observer2-hour standby changes no message/session/protocol deadline.

Only ordered route, each actual relay forwarding, sender Object DONE and D
integrity/time/custody correlated to Object/Session/Circuit count as this sample.
This is the original Iroh hop-stream relay/native-object channel, NOT a new
claim of native UDP multihop or outer-IP forwarding. Direct native UDP and prior
single relay remain separate accepted samples; do not repeat them here.

## CURRENT D 2026-11-12 16:49 Beijing: D updated; A query pending

`D_A254_EXISTING_READY_FOR_A_QUERY`: original fixed updater called once, exit0.
Actual old875 PID36464 was healthy/Running, so no recovery switch was used.
New a254 PID20876 / LocalService opened original DB existing, custody/progress
unchanged, correct actual node/AOEM hashes and41053/42053 listener ownership.
See [D public original result/exit and projections](D-update-20261112/README.md).
No initialization, observer, object send or C operation. Do not repeat D update;
A now performs authenticated Status/root verification. Both observers remain
unstarted pending coordination; this local receipt is not double-relay delivery.

## CURRENT 2026-11-12 16:44 Beijing: C accepted; D execute now

A fetched C27efa42c and verified all22 public receipt hashes. C a254 PID27928 /
LocalService opened the original database existing, custody/progress unchanged.
A->C original authenticated Status passed31ms at08:44:02Z, original root
f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333.
See [A actual query](A-query-C-20261112/health-public.json) and its raw response.
The query uses pinned875 operator client; actual C node source is a254.
This is health acceptance, not object DONE; old875 c0000005 remains unresolved.

**D now:** synchronize this receipt, use the already received a254 program ZIP
and existing-update-tools-a25452f4-v1.zip below. Inspect actual875 owner then
execute the already approved original existing updater ONCE with HostId D and
only its evidence-supported recovery switch. No need to start the old version,
recopy a matching program archive, initialize, rekey or reset cache/database.
On success return D_A254_EXISTING_READY_FOR_A_QUERY plus actual public result,
exit code, process/runtime hashes and owner/listener evidence under
D-update-20261112. On failure preserve evidence and stop. C stays running;
neither host starts an observer or sends an object yet. A will query D next,
then coordinate the one real double-relay sample. This supersedes the D-wait
instructions below; no new Full was requested.

## C RESULT 2026-11-12 16:34 Beijing: C_A254_EXISTING_READY_FOR_A_QUERY

C executed the fixed original updater once, exit0. Actual875 PID13020 had
already crashed at02:01:50Z (Application1000 record546395, c0000005); original
task/exit/kernel/port checks supported RecoverCrashed. New a254 PID27928,
LocalService, original DB existing, custody/progress unchanged, ports41052/42052
confirmed. See [C raw public results, exit and hashes](C-update-20261112/README.md).
No old875 launch, initialize, observer, object or D operation. Existing875 crash
cause remains OPEN. A: authenticate C now; D remains package-only until C health
query passes. This local receipt does not claim external authentication.

## CURRENT 2026-11-12: B and A healthy; C execute next, D package-only

User explicitly approved fixed a25452f4 B->A->C->D existing updates and a real
double-relay object. Full43155828234 succeeded on the exact program SHA.
B and A actual original-state recovery and external authenticated Status passed;
see README and raw public receipts. Do not wait for Android/mobile work.

**C now:** synchronize main without overwriting other work; verify the fixed
program ZIP and this small tools ZIP, inspect actual875 owner, then run the
original updater once with the evidence-supported mode. Do not start the old
program merely to stop it again. No initializer, rekey, DB/cache/journal reset.
No observer or object send yet. D may copy/verify only until A validates C.

## Fixed inputs

- Program source: a25452f4c49be37cd984e650f2079642e6c273b3.
- Windows ZIP: novonet-observation-a25452f4-windows-x86_64.zip;
  150573543 bytes; SHA256
  fa198636fa33021ea542850e56f1df17140a5174bfb93fca1ade47f599f14978.
- A copy source:
  C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-a25452f4-20261112\.
- Original C/D receiving directory: E:\区块链四机通讯\.
- Tools in this Git directory: existing-update-tools-a25452f4-v1.zip;
  19935 bytes; SHA256
  a64f3cb94418f97e1f20ba074b47c45e4caf3798b8110ee31c1326340cbc757e.
- Per-tool pins: update-tools-manifest.json. Generated from the original updater
  by Prepare-Windows.ps1; original guard/package tests passed (windows-updater-tests.log).
  Program not rebuilt or changed by this deployment-tool handoff.

Extract to new, distinct program/tools directories, never over a running release.
Check archive entries remain inside the destination and match manifest hashes.
The original updater rechecks all77 release members and source/pins before stop.
Do not use tools pinned to875,8fed,1ef or3fa against this program ZIP.

## Actual875 preflight and execution

Use administrator PowerShell7. C/D current expected old owner:

- Task NOVOVM-C-875d3307-Existing-20261111 (D substitutes D).
- LocalService SID S-1-5-19; no other active node or unknown DB/port owner.
- Old node C:\ProgramData\NOVOVM\releases\novonet-observation-875d3307-windows-x86_64\bin\novovm-node.exe;
  SHA9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7.
- Original work C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-875d3307-20261111;
  DB sibling chain-state; D substitutes D. Keep its actual run/network/custody/cache.
- Check task result, kernel process state, old progress/exit/Application1000,
  protected-material locations, original ports41052/42052 (D41053/42053).

Call the extracted original Update-Windows.ps1:

```powershell
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -ExecutionPolicy RemoteSigned -File '<new-tools>\Update-Windows.ps1' -HostId C -PackageRoot '<extracted>\novonet-observation-a25452f4-windows-x86_64' -EvidenceRoot '<fresh-evidence-directory>'
```

Choose ONLY the original matching lifecycle mode, based on live evidence:
running exact875 => no recovery switch; Ready/result0/no active process or ports
with original progress => -ResumeStopped; original task-termination evidence
267014 => -RecoverInterrupted; exact supported crash evidence => -RecoverCrashed
(heap c0000374 also requires matching original exit record). Original guards
must accept the evidence. Do not guess a flag, use RecoverMissingProgress for
unrelated data, or weaken validation. Unknown state => return evidence and stop.
Run once; failure => preserve error/recovery-stop evidence, do not loop/reinitialize.

## Return immediately after C finishes

Return C_A254_EXISTING_READY_FOR_A_QUERY only if updater exited0 and result
confirms same-db existing, source a254, custody/progress unchanged. Publish
result.json, public after projection (no private config/custody material),
old-exit / matching stopped/crash preflight, firewall summary, actual exit code,
source/program/AOEM/tool hashes and task/listener state. Put C receipt under
C-update-20261112 in this directory and push main. Do not publish before.json
wholesale: it contains full local configuration and protected-material metadata.

A then queries C with original authenticated Status; only after PASS does D use
the same procedure with HostId D, returning D_A254_EXISTING_READY_FOR_A_QUERY.
Any node/task/path discrepancy is a real blocker to report, not permission to
run an old initialize task or force the database open.

## Double-relay phase

After all four actual versions/health are verified, use the original bounded
double-relay command and existing observation owners. Record ordered two-hop
path, relay forwarding, Object ID, Session ID and sender/final recipient DONE.
New NVLOCR02 signed128KiB declarations require coherent readers; no signed
sequence rollback/cache wipe. No replay of accepted direct/single-relay samples.
Object custody is not Business Receipt, NOV transfer, quorum finality or TPS.
