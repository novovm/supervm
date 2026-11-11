# 875d3307: C then D existing update and real relay observation

Full42967820361 succeeded on headf47c05ba, attempt1. Program source is
875d3307a1fc2ca34b45464ac3bf51a03619adc2; the later changes are audit/docs.
The user has approved updating this candidate and one real A→C→D relay object.
Stop/update permission does not need to be requested again.

## Current release condition

### CURRENT: C uses v2 RecoverCrashed once; no new program ZIP

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
