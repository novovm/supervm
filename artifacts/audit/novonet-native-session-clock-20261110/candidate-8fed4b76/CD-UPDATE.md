# C/D: fixed 8fed session-clock existing update

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
Full and B/A cutover healthy**. At creation of this note those steps are pending.
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
