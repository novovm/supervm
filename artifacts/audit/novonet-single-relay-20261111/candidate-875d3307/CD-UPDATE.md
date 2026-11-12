# 875d3307: C then D existing update and real relay observation

Full42967820361 succeeded on headf47c05ba, attempt1. Program source is
875d3307a1fc2ca34b45464ac3bf51a03619adc2; the later changes are audit/docs.
The user has approved updating this candidate and one real A→C→D relay object.
Stop/update permission does not need to be requested again.

## Current release condition

### CURRENT 2026-11-12: C takes source investigation and justified fix ownership

The user explicitly asked C to investigate and fix this crash directly instead
of handing the diagnosis back to A. This supersedes the investigation ownership
in the historical entries below. C owns original-dump correlation, bounded local
source reproduction, and a minimal fix once a causal defect is established.
A should avoid overlapping edits to this crash path and review/integrate C's
findings rather than independently patching the same code.

The matching PDB and two original dumps are already available on C. Invalid
frame/String state is evidence of the fault, not proof that hex formatting is
the corrupting operation. Current source is not fixed or qualified. Local
isolation tests do not certify the complete node or exclude earlier FFI damage.
C remains NOT READY for relay delivery; this ownership update does not start a
node/observer, resend an object, replace keys/database, or authorize D changes.
Raw dumps, stack memory and private materials remain local.

C's [first source isolation result](C-source-isolation-20261112/README.md)
completed with exit0:80000 write attempts,501 snapshots,1014272 hash fields
checked. It did not reproduce the crash and is not full-node qualification or
a fix. Original875 fault evidence remains open; no runtime was restarted.

### C PDB FOLLOW-UP 2026-11-12: original symbols verified; pre-fault state inconsistent

C verified supplied original PDB size17281024, SHA256b46bc1763c06c7370733763391d56e083b2e52cc8dcd370a377be2f7928dac38,
GUID/age matching both dumps and875. Original-RVA symbolization confirms
indexmap::Core::insert_full for PID31296, whose captured frame relation differs
from the normal prologue, and String/fmt/Status for PID3228, whose writer consumes
an implausible length before the runtime copy. See
[PDB correlation and precise limits](C-pdb-analysis-20261112/README.md).
These observations do not identify the earlier corrupting operation or prove a
shared root cause. A owns native source investigation; C retained raw memory and
symbols locally. No node restart, replacement package, observer or object send.

### C FOLLOW-UP 2026-11-12: original crash dumps analyzed, exact875 PDB needed

C recovered the two existing875 minidumps locally and verified their hashes.
Both match packaged novovm_node.pdb GUID C94A5E99-4CFF-4094-8B63-0663EF0F9C21,
age1. PID31296 faults at node+0x65f59d on an observed misaligned movaps operand;
PID3228 faults writing2 bytes at VCRUNTIME140+0x1126b, with14 node caller frames.
These are instruction/context findings, not a source-level root cause or fix.
See [sanitized stack RVAs, limits and A symbol request](C-crash-analysis-20261112/README.md).
A: supply original matching875 PDB/path/hash or symbolize those exact RVAs;
do not substitute a newly rebuilt symbol file as original evidence. Raw dumps,
memory logs and WER stay local, not on GitHub. No node restart, package/config
change, observer retry, object send or D operation. C remains NOT READY.

### C RECEIPT 2026-11-12 08:41 Beijing: v3 NOT READY; node preflight failed

C synced c0a1dfb2 and passed20 readiness +13 reader checks. Old21548 was already
absent, with original exit0/wait_expired/zero snapshots preserved; no process was
terminated. Single approved transition stopped before Watch invocation because
original node3228 owner/path was not confirmed. No new READY/liveness exists.
See [C v3 failure and old terminal originals](C-relay-standby-v3-20261111/README.md).
No node restart, package change, object send, D operation or automatic retry.
A must not send on this receipt or old C READY; separately coordinate recovery
before a new observer window. The historical23:24 instruction was executed at
the actual next-morning time above, not backdated.

### CURRENT 2026-11-11 23:24 Beijing: bounded diagnostic standby, no node changes

D's actual heartbeat23:19:26 was read by A at23:21:38, only52 seconds before
the original observer deadline23:22:30. Its v2 sender guard rejected the late
window; A did not invoke prepare/send. No object/session/DONE exists for this
window. C recovery and35ms authenticated health remain passed; do not redo direct
tests. Receipt5f1d564b preserves D's active sample, not continued liveness.

The original Watch-Relay now supports `-MaximumWaitSeconds 7200` (v3), defaults
still1800. It uses its original monotonic timer, same64 changed snapshots,
1-second sampling,4MiB read bound and terminal+3-second exit. Only diagnostic
idle standby increases; no production network/object/session/authorization TTL,
forwarding budget, program, key, database or node is changed. READY and heartbeat
carry the actual wait budget; heartbeats also carry monotonic elapsed seconds.
The original A sender checks both identities, current heartbeat<=600 seconds,
no terminal receipt, and at least120 seconds remaining by BOTH elapsed clocks.
v2 keeps its prior25-minute freshness gate. Late readiness is rejected, not
extended on paper.20 readiness tests and13 reader tests passed locally.

**C/D now:** sync this handoff. Preserve old observer evidence. D confirms its
expired13168 observer/wrapper terminal result; C verifies observer21548 by PID,
creation time and exact Watch-Relay command. If C's old observer is still active,
end ONLY that confirmed observer, record the termination, never stop node3228.
Do not overlap observers. Start ONE hidden independent administratorPS7 original
Watch-Relay with the unchanged875 parameters and `-MaximumWaitSeconds 7200`,
adjacent original reader, and a fresh EvidenceRoot. D node36464 stays untouched.

Immediately return real v3 ready.json, current liveness.json, process creation
identity, terminal-file presence and launch/old-observer exit evidence under
`C-relay-standby-v3-20261111` / `D-relay-standby-v3-20261111` here. Do not wait
for collector completion to publish READY. If a node is gone or unhealthy,
return that failure instead of restarting/recovering it in this observer step.
After A dispatch, return existing snapshots and terminal outputs, no new capture
or resend. A will use the same original one-shot A->C->D sender and require
actual C forwarding plus matched A/D object/session/DONE; no fallback direct test.

### CURRENT A 2026-11-11 23:14 Beijing: C recovery authenticated; relay pending D refresh

A read C84024d10 recovery and crash receipt. The19 public raw result/observer
members match their hashes; README/.gitattributes checkout bytes differ from C's
listed hashes and are not represented as byte-preserved raw evidence. C875
PID3228 / LocalService opened the original database, same custody/config and
node/AOEM hashes. Application1000 record546255 identifies priorPID31296 access
violation c0000005 in novovm-node.exe offset0x65f59d; root cause remains unknown.

A's fixed875 SDK authenticated query at15:14:08.3964666Z succeeded35ms, original
state root f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333,
height0/current_height1, no recovery/projection failure. Raw outputs are archived
as C-current875-recovered-query-original.zip, SHA256
2387e5cb5b0281d981916f4705ba3b813813045664e34a64820d64437530f140.
This was health, not a direct object retest or relay acceptance. C observer21548
is armed from23:11:27; actual published liveness23:12:42. D13168's22:52 window
nominally ends23:22:30, but its last published sample22:53 is stale. D has been
asked only to refresh that observer's actual liveness/terminal state, not restart
it. A prepared the one-shot sender without starting another preliminary countdown.
No new object has yet been sent.

### C RECEIPT 2026-11-11 23:12 Beijing: recovery exit0; fresh observer ARMED

C executed 9a0aec5e recovery helper once: actual exit0; current875 PID3228,
LocalService, original database existing, config/custody unchanged, both UDP
41052/42052 restored. One original v2 observer PID21548 started after recovery;
real READY at23:11:27, maximum window until23:41:27 Beijing. See
[C recovery and actual READY/liveness](C-current875-recovery-20261111/README.md).
At handoff no observer result/error/exit exists. Recovery does not fix or explain
the c0000005 crash; original logs/event/task evidence remain local. A must query
recovered C and check both C/D current liveness before sending the one object.
No C object send, D operation, initialize or configuration change occurred.

### CURRENT 2026-11-11 23:08 Beijing: authorized C875 crash evidence + same-owner recovery

User explicitly authorized completing C crash evidence and recovering current875
with existing, then A->C->D once. This supersedes the prior observation-only hold.
No new program ZIP, old8fed launch, database initialization or Full run is needed.

C Codex: sync this main handoff. In administrator PowerShell7, run ONCE from
the repository (replace only the local repository prefix and fresh EvidenceRoot):

```powershell
& .\scripts\windows\Resume-Novonet875C.ps1 -EvidenceRoot 'D:\WEB3_AI\SUPERVM\target\c-875-current-recovery-20261111'
```

Use a fresh directory if that name already exists; do not overwrite it. The helper
pins the installed v2 ExistingOwner, package, task action and LocalService owner;
reads the matching Application1000 event, task result and exit; requires the
reported875 PID31296 / c0000005 run; checks no executing database/port owner using
the original kernel process barrier. It retains the crash logs in a uniquely
named sibling under the exact875 work directory, recreates the empty original
logs directory with the same ACL, and starts the SAME original existing task once.
It does not alter task/config/key/database/cache/firewall or program. A mismatch
stops before restart. A failed restart is NOT retried or rolled back automatically.

Return actual exit code, crash-public.json, before-public.json, result-public.json
or error-public.json and helper hashes. Keep crash-event-local.xml, task XML and
all original logs locally; no database, credentials or dump uploads. If available,
retain existing WER/dump paths locally for root-cause follow-up, without exporting
their contents. Recovery success is NOT a claim to have fixed the access violation.

On C_875_RECOVERED_READY_FOR_A_QUERY, immediately start ONE original v2 Watch-Relay
in a hidden independent administrator PowerShell7 with a new EvidenceRoot, after
confirming prior C observerPID1700 is exited. Return real READY/current liveness
and launch identity immediately, not at window completion. Use the unchanged875
parameters below. Publish recovery and observer receipts under a fresh
`C-current875-recovery-20261111` directory here. Do not send an object from C.

D: keep the current22:52 observer; refresh its actual liveness and terminal-file
presence only. Do not restart the node or observer. If it expires, return that
fact and evidence; no blind repeated window. A will query recovered C, check both
actual observers, and start its sender only when they are ready (no preliminary
A30-minute countdown). Never dispatch with stale READY or a stopped C node.

Local helper checks:22 passed (task/action/authority/exit counterexamples, same
directory log preservation + ACL and single restart/no forbidden mutation).
These are helper checks, not C recovery, crash resolution or relay acceptance.

### CURRENT C 2026-11-11 22:57 Beijing: NOT READY; confirm operability before observation

User updated coordination: first confirm C can operate immediately, then open
the observer window; A sends only after receiving fresh actual READY/liveness.
C's one new observer attempt exited1 before READY (`Original LocalService owner
not confirmed`). Elevated read-only inspection found no node process or UDP
41052/42052 listener; original875 task/exit agree on 0xC0000005 termination.
No automatic recovery or observer retry occurred. This is **C_NOT_READY**, not
permission for A to send. A's old21:48 sender deadline is already past; do not
reuse old C readiness or infer current A availability. See
[C original failure and readiness evidence](C-relay-after-B-20261111/README.md).
Node recovery needs separate coordination; this round does not restart/update
nodes, change configuration, send objects, or operate D.

### CURRENT 2026-11-11 21:18 Beijing: A discovery recovered; new relay window now

A readback confirms verified locators0->3 and actual A->B Carrier/direct
47.103.59.206:42050 -> bound auth -> publication/lookup -> completed209ms.
Original state and failure counters preserved; no direct object retest or peer
file injection. See B-discovery-recovery-20261111/README.md and raw recovery ZIP.

C's prior observer ended normally20:50, exit0, zero snapshots. D's latest
published active sample is20:21 and its original window has elapsed; this is
not a current READY. Do not reuse either old receipt. A's elevated one-shot
sender waiter is already running (21:18:23, PID28600, expires about21:48:23),
so no initial A UAC wait is needed after the new C/D handoff.

**C/D action:** verify each old observer's exact PID, creation identity and
exit/result files first, preserving all original outputs. Do not start a second
overlapping collector. If the prior collector has exited as expected, launch
ONE new original v2 Watch-Relay.ps1 with adjacent Read-RelayProgress.ps1, same
875 parameters below, fresh EvidenceRoot, hidden independent adminPowerShell7.
No node restart/update/initialize, no object send. If the prior helper remains
unexpectedly active or identity is unclear, report that rather than killing an
unknown process or launching another collector.

Immediately push byte-preserved actual ready.json and current liveness.json,
launch/helper PID and script hashes, current terminal-record presence. Publish
under distinct C-relay-after-B-20261111 / D-relay-after-B-20261111 directories
inside this candidate directory (never overwrite previous evidence). Keep the
new30-minute window running and report any exit promptly. A will recheck the
same fixed owner and authenticated route, then send exactly one object via C
to D. On failure preserve/cancel exact reservation, no automatic resend.

After completion return existing relay snapshots, result/error, read-anomaly
metadata, liveness and wrapper exit/stdout/stderr. Require actual circuit/path,
C forward writes, matched A/D object/session and D integrity/time acceptance
plus original DONE. B discovery completion alone is not this relay acceptance.

### CURRENT 2026-11-11 20:45 Beijing: B bootstrap restored; A discovery check pending

Read-only inspection proved B's8fed node cleanly exited at17:31 after its
configured24-hour run. No41050/42050 listener remained. A used the original
updater's new resume-current mode to resume that same fixed8fed executable,
original custody/config/database with existing, without a process run-duration
cap. B PID3766709 now owns both ports; A->B authenticated Status passed74ms,
original state root unchanged; inbound Carrier TLS/Stream Ready observed.
Details and original evidence: [B discovery recovery](B-discovery-recovery-20261111/README.md).

No C/D update or new object has occurred. A is checking automatic locator
reacquisition; its read-only observation awaits UAC. C/D only refresh current
v2 liveness/terminal status, preserving this same observer window. If it has
ended, report actual exit, do not silently re-arm. Once actual C/D locators and
active windows are verified, A proceeds to the single relay object. B health
alone is not discovery or relay acceptance.

### CURRENT 2026-11-11 20:30 Beijing: one A relay preparation failed before dial

C/D v2 receipts were verified from main60299780. A performed one prepare;
original owner reported `no verified diagnostic locator`, verified count0.
No test object was sent; no Session/Object ID or DONE exists. Exact reservation
was cancelled successfully; no retry, direct fallback test or restart.
See [A raw failure and discovery evidence](A-relay-attempt-20261111T122654Z/README.md).

A's sole discovery entry is B; current observations show Carrier connect
deadlines despite a candidate already containing B's public address and Relay.
This blocks obtaining the C/D locators required before relay dialing. The
exact Carrier timeout cause is still unproven; this is not a C/D forwarding
failure. A must fix/diagnose original discovery before another object attempt.

C/D: keep existing v2 evidence; do not re-arm, update, restart or send. Return
existing result/error, relay snapshots if any, read-anomaly files, liveness and
wrapper stdout/stderr/exit after this bounded collector completes. No new
test window is requested. A did not touch either node.

The diagnostic receipt handoff freshness was explicitly corrected from3 to10
minutes before dispatch because Git/manual handoff exceeded3; READY still has
the25-minute age ceiling within30-minute collection. This does not alter any
production timeout or prove a collector remained active. Actual three-host
evidence is still mandatory, and this failed attempt is not accepted.

### CURRENT 2026-11-11 20:10 Beijing: re-arm read-only v2 observers; no node update

A verified D's six raw receipt hashes. D's READY was superseded by exit1 at
20:02:36, with zero snapshots. **No A relay object was sent.** C's original
collector may still be running; do not launch a second overlapping collector.

The original progress writer checks its reporting deadline before and after
`product.poll`. Crossing the deadline inside poll can publish a valid Status
without the optional `novonet` observation. This is a source-proven omission
window, **not a proven attribution of D's individual read**: those bytes were
not retained. The running program is unchanged; writer correction remains a
separate source issue, not declared repaired by this collector change.

`../Watch-Relay.ps1` now uses adjacent `../Read-RelayProgress.ps1`. Synchronize
both files, not just Watch. Within the existing15-second observation freshness
bound it waits for a complete report; it never synthesizes diagnostic fields
or treats omission as success. Persistent omission, stale data, wrong chain,
execution failure, disabled diagnostics and missing transport remain failures.
First/latest rejected-read metadata (hash, size, top-level keys, reason) are
bounded; no arbitrary raw report or secret-bearing configuration is exported.
Local reader regression:13 checks passed. No binary, node, production deadline,
authentication, route budget or protocol change; no new Full or package needed.

**C/D action once:** inspect the exact previous observer PID and creation time
from its launch receipt. If still running that exact Watch helper, stop only
that observer (not the node or unrelated PowerShell); preserve its existing
files and record the deliberate stop. D's exited collector must not be stopped
again. Start v2 with the same fixed875 parameters below in a fresh EvidenceRoot,
as one hidden independent administrator PowerShell7 helper. Do not restart,
update, initialize, send objects or modify any node state.

Immediately publish raw `ready.json` **and current `liveness.json`**, launch PID,
script/helper hashes, stdout/stderr available so far, and explicit exit/result/
error absence or their actual files. `observer_version=2` is mandatory. Leave
the30-minute collector running. Terminal liveness is false. If it exits, report
that promptly; READY alone is not readiness. Use -text Git attributes or a ZIP
to preserve evidence bytes. A requires both matching active liveness receipts
sampled within3 minutes and no subsequent terminal report before one A→C→D send.

After the send, return result/error, all relay snapshots, first/latest read
anomalies, final liveness and wrapper exit logs. Gaps are reported, not hidden;
missing relay/receiver evidence cannot become a pass. Existing direct tests
are not repeated. Detailed failure record: OBSERVER-V2.md.

### CURRENT 2026-11-11 20:02 Beijing: D observer failed; A must hold send

D launched the original observer once after main4f44244e. It emitted READY at
19:59:46, then exited1 at20:02:36 with zero snapshots: Watch-Relay.ps1 line28
reported missing `novonet` property in the read progress object. D is **not
currently armed**. No rerun, restart, package change or object send was performed.
The failed progress bytes were not retained, so the underlying cause is unknown.
See [D raw READY and subsequent failure](../../validator-d-875-relay-armed-20261111/README.md).
Do not treat the historical ready.json as a currently active window or send
A->C->D until the observation failure is resolved and both windows are active.

### CURRENT 2026-11-11 19:53 Beijing: D authenticated health passed; arm C/D observers now

A verified all11 D original evidence files from mainb6958aea. New875 PID36464,
LocalService, actual node/AOEM hashes match; original database/custody/progress
unchanged. Fixed875 client A→D authenticated Status passed **44ms** at
11:53:19.6599495UTC, original chain/state root and height0/current_height1;
no recovery or projection failure. C already passed45ms. See D-QUERY-RELAY-ARM.md.

**Do not run any more update/recovery commands on A/C/D.** C and D now each start
one original `../Watch-Relay.ps1` observer in administrator PowerShell7, using
the actual installed875 owner. No network probes, node restart or clock changes.
Use distinct fresh local EvidenceRoot directories and the command in the
"Arm only after A confirms both updated nodes healthy" section below.
Parameters: HostId C or D; Source875d3307a1fc2ca34b45464ac3bf51a03619adc2;
NodeSha2569174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7;
WorkName novonet-observation-875d3307-20261111.

Launch as an independent **hidden** helper, not a tool invocation killed on
timeout; record its actual PID and capture stdout/stderr and exit code. Return
the generated ready.json immediately after it exists, while leaving the helper
running. It waits at most1800 seconds and saves at most64 snapshots. Preserve
ready.json raw bytes (ZIP or -text Git attributes), include its SHA256, source,
host and actual helper/node PID. Push each real READY promptly; do not wait until
the30-minute observer exits to report that it is armed.

Required statuses: `C_RELAY_OBSERVER_ARMED` and `D_RELAY_OBSERVER_ARMED`.
If readiness fails, publish the actual error/exit instead; do not hand-write READY
or repeatedly launch collectors. Once A has both current READYs, A alone uses
its original owner's one diagnostic arm/send route **A→C→D**. No object is sent
by C/D and no previous direct delivery is repeated.

After the send, publish existing result/error, relay-*.json snapshots, collector
exit/stdout/stderr without rerunning or clearing history. Acceptance requires
matching route_digest/circuit at all3 hosts, C's actual authenticated upstream
and downstream plus completed writes in both directions, matching A/D native
session and unique Object ID/nonce, D integrity/time acceptance and original
DONE feedback. Original HopStream/Iroh carrier relay is not native_udp direct;
transport ACK, object custody DONE and business execution remain distinct.

Earlier CURRENT sections below are historical; updates are complete.

### CURRENT 2026-11-11 19:50 Beijing: D updated; A query pending

`D_875_EXISTING_READY_FOR_A_QUERY`: D's original v2 updater exited0 on
main c4d11f57, ordinary existing mode with no recovery switches. Actual old8fed
PID19756 was healthy and running; the updater replaced it with875 PID36464,
LocalService, original database/custody/progress unchanged. No initialization,
key change, collector or test-object send. C was not operated on.
See [D original public results](../../validator-d-875-existing-update-20261111/README.md)
for byte-preserved result/after/old-exit/firewall, inspection and execution logs.
A now verifies authenticated A->D Status/root. Do not repeat D update or arm
collectors yet; local startup is not external-query or relay-delivery acceptance.

### CURRENT 2026-11-11 19:37 Beijing: C healthy; D updates now

C's v2 recovery returned `C_875_EXISTING_READY_FOR_A_QUERY`, PID31296,
LocalService, fixed875 node/AOEM, unchanged original database/custody/progress.
A verified the original C archive and all11 member hashes, then queried C with
the fixed875 native client: **45ms**, chain91129655606029, height0/current_height1,
state root `f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`,
not recovering or projection-failed. See `C-QUERY-D-RELEASE.md` and raw archive.

**D is released for its already-approved875 existing update. C stays running.**
Use the same local875 ZIP/package already received; no retransmission. Use v2
tools below (19929 bytes and manifest hashes unchanged), HostId **D**, a fresh
tool/evidence directory, and the original updater. No old initialization task.

Before the single update, check D's actual current process/task/ports and old
progress/exit. If the exact old8fed LocalService node is live and healthy, use
the ordinary update with no recovery switch. If it has stopped, use only the
original evidenced branch: Ready/exit0/no live owner/valid progress permits
ResumeStopped; Ready/result267014 with the same guards permits RecoverInterrupted;
matched task+Application1000 crash permits RecoverCrashed (c0000374 additionally
requires the matching original exit.json). Unknown/conflicting evidence stops.
Do not first restart old8fed and do not use C-specific RecoverMissingProgress.
The updater independently rechecks all guards; this is not permission to bypass
them. If it fails, retain the original error/result and stop without retry.

Return `D_875_EXISTING_READY_FOR_A_QUERY` only with actual updater exit0 and public
result/after projection, old-exit, firewall, any recovery preflight and stdout/
stderr. A then checks D's authenticated Status. Do not start Watch-Relay yet:
both C/D observers will be armed together only after D health verification.
No new object has been sent and no single-relay test is yet signed off.

### Historical C recovery instruction (completed; do not rerun on C)

### C uses v2 RecoverCrashed once; no new program ZIP

A verified C's mainc4bcc87d original archive and every member. C's old8fed
PID7816 exited at05:40:51UTC, with matching task/event/exit code **c0000374**;
there is no live node or41052/42052 owner. Original progress is present and valid.
The original tool recognized only c0000005, so **875 has not yet started**.

The original updater source now supports this second evidenced crash class:
task result3221226356 must match Application1000 c0000374, the same old
executable/creation timeline/LocalService task, and original exit.json with
start=existing and the same signed/unsigned32-bit exit value. No unknown code
or weakened process/port/state guard is accepted. On failed new recovery, stop
the new instance and retain evidence; do not auto-start the heap-crashed8fed.
This does not diagnose or fix the old process crash, and does not assert its
database healthy until the new original owner actually opens it.

**C now executes one `-RecoverCrashed` update using v2**, with the already
verified local875 package and a fresh evidence directory. Do not first start
old8fed, run the consumed prior wrapper, or reuse its old updater hash pin.
Use the existing wrapper flow with the v2 tool hash and fresh output paths;
capture exact stdout/stderr/exit and the public result fields defined below.

- Fetch `existing-update-tools-875d3307-v2.zip` from this Git directory:
  **19929 bytes**, SHA256
  `10cbc7d6688aaddcf048e15651e98247ad7c6330e85fed434782e299b1d16dd9`.
- Extract into a fresh local tools directory and verify every member with
  `update-tools-v2-manifest.json`. Update-Windows.ps1 SHA256:
  `f7c185500abd955089c92b8bd0edefb3cf20a20df65bfbfe53c1c9c0bcd55cb2`.
- Administrator PowerShell7:

```powershell
& '<new-v2-tools>\Update-Windows.ps1' -HostId C -PackageRoot '<already-verified-local875-root>' -EvidenceRoot '<fresh-C-recovery-evidence>' -RecoverCrashed
```

Original updater rechecks live task/process/ports, original exit and event before
opening the original database with875. Any mismatch stops; do not change flags
or retry. Return `C_875_EXISTING_READY_FOR_A_QUERY` only after actual success.
No D update, collector, initialization or object send in this step.

Only deployment scripts changed; actual875 program, AOEM and their Full42967820361
are unchanged. Both original-source Windows tests and the derived updater's
fixed-package tests passed. Twenty heap-evidence cases plus failure-stop
selection passed; the actual C record rejects under v1 and accepts under v2.
Original local outputs are in `recovery-v2-local-evidence.zip` and its manifest.
The optional Linux execution of this Windows suite failed on its original
C-drive mapping after passing the new evidence cases; not recorded as full
Linux success. No Linux updater changed. The first new XML test fixture error
was corrected; both final Windows runs passed. No new Full was requested.

The prior inspection instructions below are retained as history; C should not
repeat a separate inspection round instead of using the now-reviewed v2 entry.

### C preflight failed: inspect once, then use only an evidenced original recovery mode

A reviewed main277625b1 and independently verified the SHA256 of C's original
archive and every member. The exact error was `Original process identity not
confirmed`; `old_node_stopped=false`, `existing_start_attempted=false`.
875 has **not** been started on C. The error combines active-process count,
path and LocalService identity checks; the receipt does not identify which failed.
Do not label this a new-program crash, database fault or permission change.

C should now collect one complete current-owner observation in administrator
PowerShell7, without first restarting the old program:

1. Use the already hash-verified `NovonetExistingProcess.ps1` from the original
   tool bundle and call `Get-NovonetNodeExecutionState`. Record active and
   kernel-signaled exited processes, PID/parent/creation time, executable path,
   actual executable SHA256 and GetOwnerSid result. Do not export command lines.
2. Read task `NOVOVM-C-8fed4b76-Existing-20261110`: state, principal/SID,
   LastRunTime/LastTaskResult, restart count, triggers and execution-time limit.
   Check for other C NOVOVM task owners, not just a process with the same name.
3. Record actual owners of UDP41052/42052. Read old work
   `C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-8fed4b76-20261110\logs`:
   exit.json if present; progress existence/size/hash/time and only public
   entry/chain_id/current_height/head/execution_failures/recovery_in_progress.
   Preserve existing stdout/stderr locally; return relevant bounded failure lines,
   not full configurations, secrets or database contents.
4. If a crash is indicated, collect the matching Application1000 event fields
   used by the original updater: AppPath, ExceptionCode, ProcessId,
   ProcessCreationTime, event UTC/record ID and report ID. An old unrelated crash
   or a task code alone is not proof of the current process cause.

After recording that evidence, C may complete **one** update through the original
unchanged tool if exactly one existing recovery branch is conclusively applicable:

- No executing node, no target-port owner, original task Ready/LocalService,
  restart_count0/trigger_count0, valid original progress/timeline, task result0:
  original `-ResumeStopped` branch. Task result0 alone does not certify node drain.
- The same stopped-owner conditions but task result267014 (0x41306), valid
  original progress: original `-RecoverInterrupted` branch. Record interruption
  cause unknown, not clean shutdown. Do not first start the old8fed program.
- Task result3221225477 with a matching original8fed c0000005 crash event and
  all original crash/timeline guards satisfied: original `-RecoverCrashed` branch.

Invoke `Update-Windows.ps1 -HostId C -PackageRoot <existing verified875 package>
-EvidenceRoot <fresh directory>` plus that **single evidenced switch**. The
original updater must independently enforce every precondition; no bypass or
relaxation. Use the already copied internal-disk ZIP/extraction, not removable E.
Keep the previous failed evidence unchanged. Do not rerun Run-C-875-Once.ps1;
it correctly rejects its consumed evidence directory. Capture this invocation
and its actual output/exit/public result under a fresh local wrapper/directory.

If any process ownership is unknown, multiple live nodes exist, task/exit evidence
conflicts, progress is missing/corrupt, or no branch qualifies, return the complete
observation and stop. Do not guess switches, delete LOCK, initialize, regenerate
keys or repeatedly retry. Do not use C's old1ef-specific RecoverMissingProgress.
If a healthy live old8fed is actually present, return the exact mismatch and
observation first rather than blindly repeating the same failed default branch.

On success return `C_875_EXISTING_READY_FOR_A_QUERY` and the original public
update evidence below. A performs authenticated Status, then releases D.
Do not start the relay collector yet. D remains unchanged.

**A_875_EXISTING_AND_AUTHENTICATED_STATUS_PASS is recorded in [A-UPDATE.md](A-UPDATE.md)
at 2026-11-11T10:26:01UTC. C is released to execute this update now.**
C updates first and returns its original result. A checks authenticated
Status before releasing D's update. D follows the same procedure only after
A records C health. Do not run collectors merely because the archive is ready.

## Update using the original owner

Use administrator PowerShell7. Reuse the already verified original Windows ZIP
in `E:\区块链四机通讯\`; no retransmission or repeat package test suite is needed.
Before extraction, check its current length150585096 and SHA256
`e8e4e2a192c127ca9a902779cb04228290d2e037ce9cbfc83e8ee52fd96aa5d8`.
Extract to a new local candidate directory, or reuse an existing verified
extraction without overwriting it. Locate the directory containing
`release-manifest.json`; that is PackageRoot, not the ZIP's parent.

Extract this directory's `existing-update-tools-875d3307.zip` into a new tools
directory after checking length29869 and SHA256
`1094cab94db24e9c4cbfcc5d39168a800b9dabf949541f3ea768090460583ed0`.
Check all extracted scripts against `update-tools-manifest.json`.

Execute once, substituting actual local directories and C or D:

```powershell
& '<tools-directory>\Update-Windows.ps1' -HostId C -PackageRoot '<verified-package-root>' -EvidenceRoot '<new-evidence-directory>'
```

Use the original updater unchanged. It checks the actual old8fed process,
LocalService, database, identities, original configuration and fixed77 members;
copies the same firewall scope to the new program; stops only the exact old
node; and starts875 with `START=existing`. It checks recovery, progress,
loaded AOEM, custody and observation fields. Do not run initialize, erase
cache/LOCK/journal or choose recovery switches speculatively. On failure return
the actual error and existing-start/rollback outcome; stop rather than retry.

Return `result.json`, `error.json` if present, `old-exit.json`, `firewall.json`,
and actual command exit/stdout/stderr. Project `after.json` into public fields:
host/source/time/PID/executable/hash/SID/database/start/AOEM/listeners,
custody_unchanged, chain/current_height/head/execution_failures/recovery state,
probe enabled and relay observation availability. Do not upload full before.json,
run/network configuration, protected key files, database or journal.
Success label: `C_875_EXISTING_READY_FOR_A_QUERY` / `D_875_EXISTING_READY_FOR_A_QUERY`.
This is not yet external authenticated health or object acceptance.

## Arm only after A confirms both updated nodes healthy

Use the existing `../Watch-Relay.ps1` in administrator PowerShell7 with:

```powershell
& '<repository>\artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1' -HostId C -Source 875d3307a1fc2ca34b45464ac3bf51a03619adc2 -NodeSha256 9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7 -WorkName novonet-observation-875d3307-20261111 -EvidenceRoot '<new-collector-evidence>'
```

D substitutes HostId D. Launch as an independent hidden helper with redirected
stdout/stderr, retained PID and exit record so a tool timeout does not kill it.
Promptly push the actual ready.json, not a handwritten READY. Wait window is
30minutes; bounded snapshot count64. Do not restart or resend to obtain logs.
After A's one send, publish result/error, relay snapshots, exit/stdout/stderr.
These are public observation projections; no protected material is requested.

Acceptance requires the same route digest/circuit at A/C/D, authenticated C
upstream/downstream and actual writes both ways, and matching A/D native session,
Object ID/nonce, sender DONE and D time/integrity acceptance with original
feedback. This is original HopStream/Iroh relay plus end-to-end NativeObjectLink,
not a repeat native-UDP direct test and not an asset Business Receipt.
