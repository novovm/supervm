# C/D: fixed 8fed session-clock existing update

## CURRENT 2026-11-11: D update and A-to-D query complete; A send blocked locally

A pulled D `61e08288`, verified every returned evidence digest, fixed node/AOEM,
LocalService PID19756 and existing recovery. Fixed new SDK authenticated D in
**34ms**, original root `f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`,
height0/current_height1, no recovery/projection failure. See
`A-D-authenticated-after-20261111.json` and `D-QUERY-A-SEND-BLOCKER.json`.
**D must not repeat its update. C also needs no operation.**

No diagnostic object has been sent in this step. A's current desktop tool account
cannot read the protected original task/progress, and its one new Status query to
A timed out. The attempted read-only UAC inspection was cancelled. These facts do
not establish A's exit cause or even a successful privileged process inspection.
Do not call the D query a message from the A node: it used the approved SDK client.

Next: complete A's read-only elevated task/process/progress inspection, then restore
its current 8fed owner with existing only if actually needed. Before sending, D
will run `Watch-D-Receiver.ps1 -EvidenceRoot <new-local-directory>` in administrator
PowerShell 7. **Do not arm it yet while A is blocked.** It observes the original
8fed progress only, waits at most 30 minutes using a monotonic stopwatch, records
at most 64 public snapshots and exits shortly after a new A receive/reject event.
It creates no socket and changes no node/time/configuration. Readiness is not
acceptance; A will match actual Object ID, Session ID, nonce, path, transport
observations and integrity result against sender DONE. Collector syntax was
checked locally; actual D execution and dual-end object acceptance remain pending.
This section supersedes all earlier update/run instructions below. No new Full.

## D returned success: A query pending (2026-11-11)

D executed the approved RecoverInterrupted entry once, exit 0 at 10:50:58
Beijing. Fixed 8fed PID19756 / LocalService opened the original database with
existing; custody and prior height/head comparison passed, execution_failures=0,
recovery_in_progress=false. UDP 192.168.71.59:41053/42053.
[D original result, after, old-exit and preflight](../../validator-d-8fed-interrupted-recovery-20261111/README.md).
Status: `D_8FED_EXISTING_READY_FOR_A_QUERY`. Leave D running for A's fixed-client
authenticated query; do not repeat the update. Old interruption cause is still
unknown, clean_shutdown_proven=false. This is not authenticated query or Object
DONE acceptance. C/A/B were not operated.

## CURRENT 2026-11-11: D proceeds with NEW 8fed using interrupted recovery

A pulled `ce2637ea`. D's old task is terminated (267014/0x41306), with no
node/listener and valid original progress. Its exit cause remains unknown; this
is neither a clean drain nor proof of C0000005. **Do not repair or restart 1ef
first. Start the approved 8fed program with the existing database.**

The original Windows updater now has `-RecoverInterrupted`. It checks the exact
terminated-task result, LocalService, no restart/triggers, no active node/ports,
valid progress/timeline, fixed package/runtime, original database and custody.
It rechecks process/port absence immediately before startup. It does not clear
LOCK, initialize, adjust clocks or classify the interruption as a clean exit.
If new recovery fails, it stops only the new candidate and leaves evidence;
it does not automatically restart the unexplained old owner.

Only the updater changed. Program **8fed4b76**, its Full qualification and the
150,603,521-byte Windows package are unchanged. **No program recopy or new Full.**
The small tool archive is committed beside this file:
`windows-interrupted-recovery-tools-v2.zip`, **18,565 bytes**, SHA256
`0354764a6cc3432a6ed09b2108a00e6fc16cfafc698d52b98debe51e00e63fc9`.
See `interrupted-tools-manifest.json` for all three member hashes. The old tool
archive remains historical; do not use its missing interrupted entry.

D Codex: sync main, then run in administrator PowerShell 7 from repository root:

```powershell
$audit=Join-Path (Get-Location) 'artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76'
$zip=Join-Path $audit 'windows-interrupted-recovery-tools-v2.zip'
if((Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant() -cne '0354764a6cc3432a6ed09b2108a00e6fc16cfafc698d52b98debe51e00e63fc9'){throw 'Wrong tools archive'}
$run=Join-Path (Get-Location) ('target/d-8fed-interrupted-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
if(Test-Path -LiteralPath $run){throw 'Use a fresh output path'}
$tools=Join-Path $run 'tools'
Expand-Archive -LiteralPath $zip -DestinationPath $tools
$manifest=Get-Content -LiteralPath (Join-Path $audit 'interrupted-tools-manifest.json') -Raw|ConvertFrom-Json
foreach($file in $manifest.files){if((Get-FileHash -LiteralPath (Join-Path $tools $file.path)).Hash.ToLowerInvariant() -cne $file.sha256){throw 'Wrong tool member'}}
$package='D:\WEB3_AI\SUPERVM\target\d-8fed-preparation-20261110\package\novonet-observation-8fed4b76-windows-x86_64'
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -File "$tools\Test-Update-Windows.ps1" -PackageRoot $package
if($LASTEXITCODE -ne 0){throw 'Updater tests failed'}
& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -File "$tools\Update-Windows.ps1" -HostId D -PackageRoot $package -EvidenceRoot (Join-Path $run 'evidence') -RecoverInterrupted
$updateExit=$LASTEXITCODE
# Return the real exit code even on failure; do not retry or run initialization.
Write-Output "D_UPDATE_EXIT=$updateExit"
```

Return `D_8FED_EXISTING_READY_FOR_A_QUERY` only on successful result, with exit
code, result.json, after.json, old-exit.json and stopped-owner-preflight.json.
Keep before.json/config/custody private; publish public projections only. On
failure return error and remaining process state. A then performs authenticated
A-to-D Status before object delivery. C/A/B need no operation for this change.

Local verification: full original updater gates, actual returned D receipt,
11 interrupted-state cases, 6 actual mode-guard cases, fixed 77-member package
through generated updater, and 13 process-barrier cases passed. Logs are the
three `interrupted-tools-*-tests.log` files. This is not D runtime acceptance.
This section supersedes all older waiting/live-only instructions below.

## D actual preflight returned: update not started (2026-11-11)

D ran the fixed updater tests successfully, but its original 1ef owner is no
longer running: task Ready/result 267014 (0x41306), no node/listener, exit.json
absent, stdout/stderr empty, valid old progress, no matching Application 1000
crash event. [D original evidence](../../validator-d-8fed-preflight-stopped-20261111/README.md).
The live entry was not invoked; neither the supplied clean-stop nor crash guard
qualifies this observation. A must select the existing recovery entry from D's
own evidence; no blind C flags, initialization, LOCK deletion or retry. C stays
untouched. D is NOT yet ready for the new-client authenticated query.

## CURRENT 2026-11-11: C query passed; D executes now

A pulled C receipt `2db1fead`, checked its actual 8fed node/AOEM hashes and
custody, and completed an external authenticated query with the fixed new
client at **10:32 Beijing**: **43ms**, original state root
`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`,
current_height=1/finalized_height=0, not recovering and no projection failure.
[Original SDK response](A-C-authenticated-after-20261111.json),
[C-query/D-release record](C-QUERY-D-RELEASE.json).

**C is complete for this update/query step. Do not restart C or repeat its recovery.
D now executes its approved same-version existing update.** No package retransmit,
new Full CI or repeated stop approval is needed. D uses its already verified
local package/tool paths from its receipt, administrator PowerShell 7:

```powershell
$tools='D:\WEB3_AI\SUPERVM\target\d-8fed-preparation-20261110\tools'
$package='D:\WEB3_AI\SUPERVM\target\d-8fed-preparation-20261110\package\novonet-observation-8fed4b76-windows-x86_64'
$evidence='D:\WEB3_AI\SUPERVM\target\d-8fed-cutover-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
& "$tools\Test-Update-Windows.ps1" -PackageRoot $package
# Only after all updater tests pass and the original D owner is still running:
& "$tools\Update-Windows.ps1" -HostId D -PackageRoot $package -EvidenceRoot $evidence
```

Check D's actual task/process before choosing the entry. If its old owner has
already exited, do not run the live-process entry or copy C's recovery flags
blindly: return the exact task result/exit/progress and matching crash event,
then select the existing stopped/crashed entry from D's own evidence. No clear,
initialize or LOCK deletion. Failure is reported, not repeatedly retried.

Return `D_8FED_EXISTING_READY_FOR_A_QUERY`, exit code and public result/after/
old-exit records. A then queries D before the four-host object test. C's Status
success is not Object DONE, consensus completion, or proof that the old access
violation root cause was fixed. This current section supersedes all older wait
and C-run instructions below.

## C completed: C_8FED_EXISTING_READY_FOR_A_QUERY (2026-11-11)

C executed the approved `44cd9ffb` RecoverCrashed entry once, exit 0, at
2026-11-11 10:26:41 Beijing. New 8fed PID7816 / LocalService opened the original
database with existing; custody unchanged, old progress height/head comparison passed,
execution_failures=0, recovery_in_progress=false. UDP 192.168.71.125:41052/42052.
[C public result and original records](../../validator-c-8fed-existing-recovery-20261111/README.md).
The new process remains running for A's fixed-client authenticated query. D still
waits for that query. No CI, initialization, key rotation or object send occurred;
the original 1ef access violation remains unresolved.

## Completed C recovery instruction (do not rerun)

A pulled receipt `243ffb25` and evaluated its actual fields with
`Assert-CrashedOwnerEvidence` extracted from the pinned updater
`1abe055135ac1469bac56b91d813844befc14173e8461edbf61e003e2f641a98`.
It passed: original LocalService task Ready, C0000005 result, no restart/trigger,
matching original 1ef process path/PID24376 and creation/crash timeline. The
administrator collection found no process or 41052/42052 listener and no read
errors. The original progress is valid, not the historical all-zero report.

**Proceed with the NEW 8fed program now. No old-version repair, new package,
new Full CI, clock adjustment or another permission request is needed.**
This supersedes the read-only/pending instructions below. Recovery means opening
the existing database with the new qualified program after the old process exited;
it does not mean restoring an old database or making the old program work first.

C Codex: use the already verified tools/package in administrator PowerShell 7:

```powershell
$tools='D:\WEB3_AI\SUPERVM\target\c-8fed-preflight-20261110\tools'
$package='D:\WEB3_AI\SUPERVM\target\c-8fed-preflight-20261110\package\novonet-observation-8fed4b76-windows-x86_64'
$evidence='D:\WEB3_AI\SUPERVM\target\c-8fed-crashed-existing-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
& "$tools\Update-Windows.ps1" -HostId C -PackageRoot $package -EvidenceRoot $evidence -RecoverCrashed
```

The original updater rechecks actual lifecycle, fixed hashes, empty process/port
ownership, protected custody and database before starting 8fed in existing mode.
Do not combine ResumeStopped/RecoverMissingProgress/ResumePrepared; do not rerun
the ordinary live-process entry or an initialize task. Existing updater tests
already passed; A's new check used the real returned evidence, not another fixture.

Return `C_8FED_EXISTING_READY_FOR_A_QUERY`, exit code and public result/after/
old-exit/crash-recovery-preflight records if successful. If it fails, return the
original error and actual process/rollback state without repeated retries. A then
checks C with the fixed new client; D follows after that query. The original
access-violation cause remains unresolved; successful new-version recovery must
not be described as proof that it was fixed, or as object/asset acceptance.

## C old-owner collection returned (2026-11-11)

C ran the `30a4684f` Inspector once, administrator PowerShell 7, exit 0.
[Public result, exit code and script digest](../../validator-c-1ef-old-owner-20261111/README.md).
Original 1ef task result and exit.json both record C0000005. Application event
545473 matches the 1ef executable and PID 24376 at 2026-11-10T04:25:26.1118322Z.
No active/exited node or UDP listener was found; collection errors were empty.
The old progress is valid JSON; stdout was not valid JSON, stderr empty.
No updater or recovery mode was invoked. A should review this original crash
evidence before selecting the next existing recovery entry; D remains waiting.

## Completed prior action: one administrator read-only old-owner collection

A read and fast-forwarded C's failure receipt `f01f849f`. The fixed package and
updater tests passed; no old process was stopped and no 8fed node was started.
Do not interpret this as an 8fed startup crash. The old 1ef owner is currently
unconfirmed, and its exit cause is unknown. **The earlier "C proceed now" command
below is suspended by this section; D remains waiting.** No new package is needed.

C Codex: sync this handoff, then use administrator PowerShell 7 to run the
committed `Inspect-C-OldOwner.ps1` beside this document once, with a fresh
EvidenceRoot. Example from the C repository root:

```powershell
$evidence='D:\WEB3_AI\SUPERVM\target\c-8fed-old-owner-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
& .\artifacts\audit\novonet-native-session-clock-20261110\candidate-8fed4b76\Inspect-C-OldOwner.ps1 -EvidenceRoot $evidence
```

It reuses the original public progress reader and pinned kernel process helper.
One collection includes original task result/SID/timing/restart policy, live and
kernel-terminated processes, ports 41052/42052, bounded original owner/progress/
stdout/exit structure, stderr metadata/known failure markers and matching bounded
Application crash events. It does not open the database, read purpose keys,
change clocks, run an updater or stop/start any process. Return
`C-old-owner-public.json`, actual exit code and script SHA256. Missing or unreadable
evidence stays missing; no failure is converted into a clean stop.

A will select the existing recovery entry only after this evidence: proven clean
stop -> original ResumeStopped; matching crash -> original RecoverCrashed; a live
owner/path mismatch or unknown exit -> diagnose the actual record. Do not add
RecoverMissingProgress or any recovery flags speculatively, rerun initialization,
clear locks, or rerun the ordinary live-process update into old evidence.
This is one batch of exit evidence, not another package/CI/network qualification.

## C attempted once: original process identity preflight failed

At 2026-11-10 18:00:56 Beijing, C's fixed tests passed (exit 0), but the
updater exited 1 with `Original process identity not confirmed`, before stopping
the old node or attempting existing startup. [C original failure receipt](../../validator-c-8fed-preflight-failed-20261110/README.md).
Post-failure read-only enumeration found no node process or 41052/42052 listener;
exit cause is not established. C did not retry or choose recovery flags. C is
NOT_READY; D still waits. Preserve this failure and inspect old-owner exit evidence
before determining the next recovery action.

## Earlier release: B/A healthy; C attempted the approved update (see current action above)

Full **42776380753**, attempt 1, exact program source **8fed4b76**, completed
successfully, including Rust checks, CI scope result and Full candidate result.
The approved B -> A cutover completed at **2026-11-10 17:32:06 Beijing**.
Both original databases opened with `existing`; custody and the original state
root remained unchanged. External fixed-client Status passed: B **79ms**, A
**27ms**. A PID **21708**, B PID **3133881**; both same new processes were
observed running again when this receipt was prepared.

Evidence: [AB-CUTOVER-PUBLIC.json](AB-CUTOVER-PUBLIC.json) and
`AB-cutover-evidence.zip` (original CI response, controller, update/query outputs).
The earlier candidate manifest is a preparation-time snapshot, not current
deployment status. The pending B/A instructions below are historical where they
conflict with this current section.

**C: proceed now; do not wait for another permission or retransmit the package.**
Reuse the verified preparation paths and administrator PowerShell 7:

```powershell
$tools='D:\WEB3_AI\SUPERVM\target\c-8fed-preflight-20261110\tools'
$package='D:\WEB3_AI\SUPERVM\target\c-8fed-preflight-20261110\package\novonet-observation-8fed4b76-windows-x86_64'
$evidence='D:\WEB3_AI\SUPERVM\target\c-8fed-cutover-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
& "$tools\Test-Update-Windows.ps1" -PackageRoot $package
# Execute the next command only if every preceding updater test succeeded.
& "$tools\Update-Windows.ps1" -HostId C -PackageRoot $package -EvidenceRoot $evidence
```

Return `C_8FED_EXISTING_READY_FOR_A_QUERY`, actual exit code and public
result/after/old-exit records. Failure is not readiness: preserve its exact error
and process state, do not loop the updater. A then uses the fixed new client to
query C. D's package readiness is accepted; **D waits only for C startup and A's
authenticated C query**, then uses the same approved procedure with HostId D.
No clock change, new identity, initialization or further Full CI is required.
These startup/query results do not replace real object DONE or asset acceptance.

## D package preflight returned

D independently verified its existing Windows ZIP, the pinned release manifest,
all 77 artifacts and all 8 tools; fresh extraction and PowerShell syntax checks
passed. [D public preparation receipt](../../validator-d-8fed-package-ready-20261110/README.md).
Status: `D_8FED_INPUTS_VERIFIED_UPDATE_NOT_STARTED`. Updater tests and update
have not run; no node or clock operations occurred. D waits for the fixed Full,
B/A healthy cutover, then C startup and A's authenticated C query. This receipt
does not authorize an early switch or claim an updated D node.

## C package preflight returned

C verified the original Windows ZIP already present in its E: receiving directory,
all 77 package artifacts and all 8 tool hashes. Fresh extraction is ready.
[C public preflight](../../validator-c-8fed-package-ready-20261110/README.md).
Status: `C_8FED_PACKAGE_TOOLS_VERIFIED_WAITING_A`. No updater, node stop/start,
or clock operation was executed. C waits for the fixed Full and B/A healthy notice;
D must independently confirm receipt and verification.

## Current action

The user has approved updating all four nodes. Do not request stop permission
again. Copy/verify the files now; execute C, then D, **after A reports the fixed
Full and B/A cutover healthy**. That prerequisite is now satisfied above; the
original creation-time pending state is superseded. C/D file checks already passed.
This is not an instruction to launch an old initialization/recovery task.

Program source: `8fed4b76f518935b30fcfb13930ccd3d7aae7381`.
Required Full: `42776380753`, attempt 1, same source. Existing installed baseline
is 1ef31893. A documentation-only later main commit is not another program build.

## Files

A has the Windows program archive at:

`C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-8fed4b76-20261110\novonet-observation-8fed4b76-windows-x86_64.zip`

Copy it to the existing C/D receiving directory (C previously used
`E:\区块链四机通讯\`). It is **150603521 bytes**, SHA256
`29bee97601a7fb8ba698bf8bb5bff356e1c97c8e8d2a750be871f3c928fe73e4`.
Do not use the failed e7 archive or the earlier 1ef package.

The small public tool bundle is committed beside this document:
`existing-update-tools-8fed4b76.zip`, **33017 bytes**, SHA256
`63855bf03bcc8912fc07068503344e3a887a8d1f6d57a318ebcc0cb2ab96d255`.
Verify every extracted script against `update-tools-manifest.json`.
Use fresh extraction directories. The package archive has the enclosing directory
`novonet-observation-8fed4b76-windows-x86_64`; PackageRoot is that directory,
containing release-manifest.json.
Do not transfer protected keys, configuration secrets or databases.

## Execution by C/D Codex

Use administrator PowerShell 7. Resolve actual absolute paths for `$tools`,
`$package` and a new `$evidence` directory. Check that the actual old node/task
is the original 1ef LocalService owner and its original database, with no second
instance. The original updater performs the detailed exact checks itself.

```powershell
& "$tools\Test-Update-Windows.ps1" -PackageRoot $package
# Only after the preceding command succeeds and A reports B/A healthy:
& "$tools\Update-Windows.ps1" -HostId C -PackageRoot $package -EvidenceRoot $evidence
```

D uses `-HostId D` only after C reports local existing startup and A completes
its external authenticated query. Do not pass RecoverMissingProgress,
RecoverCrashed or ResumeStopped automatically; an unexpected现场 must be read
before selecting any recovery mode. Do not rerun a failed one-shot updater into
the same evidence directory or delete LOCK. No system clock adjustment is needed.

The original updater stops the exact old process, retains original protected
custody and database, installs fixed hashes, copies the same firewall rule scope,
and starts the original node with `existing`. Native handshake profile 3 requires
the new fixed client; old 1ef clients cannot be used to declare the upgrade failed.
No silent authentication downgrade is available.

Return `C_8FED_EXISTING_READY_FOR_A_QUERY` (or D), actual exit code, public
result/after/old-exit records and script digest. Never publish before/config,
protected material or database files. If it fails, preserve the exact error and
current process state; do not report rollback requested as rollback completed.

After all four are current, A will issue one fixed A→C object through the
original owner. Receiver integrity, Object ID, Session ID and sender DONE must
match; Status or Transport ACK does not replace this real delivery evidence.
