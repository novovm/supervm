# C/D: fixed 8fed session-clock existing update

## C old-owner collection returned (2026-11-11)

C ran the `30a4684f` Inspector once, administrator PowerShell 7, exit 0.
[Public result, exit code and script digest](../../validator-c-1ef-old-owner-20261111/README.md).
Original 1ef task result and exit.json both record C0000005. Application event
545473 matches the 1ef executable and PID 24376 at 2026-11-10T04:25:26.1118322Z.
No active/exited node or UDP listener was found; collection errors were empty.
The old progress is valid JSON; stdout was not valid JSON, stderr empty.
No updater or recovery mode was invoked. A should review this original crash
evidence before selecting the next existing recovery entry; D remains waiting.

## CURRENT C action: one administrator read-only old-owner collection

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
