# C only — fixed 408e9d58 existing update

## CURRENT — C sustained health and A authentication passed; arm recovery observers

C booted `existing` on the original database and identity as LocalService
PID 19964. The loaded EXE, AOEM and manifest match the fixed candidate. A
independently checked all 31 [progress samples](C-existing/progress-samples.jsonl):
one PID and creation time, 901.913 seconds, strictly increasing ticks
165875→1609193 and file mtimes, maximum progress age 4.978 seconds, original
chain ID/height with no execution failures or exit. A's independent
[authenticated Status](A-C-authenticated-status.json) passed at 17:47:34 BJT
in 131 ms, at height 1 with the original state root. The query client is
55cafb07; C is 408e9d58. This is health, not object delivery.

On C and D, confirm the prior observer has exited, then use the existing
qualified administrator PowerShell 7 launcher for `Watch-Relay.ps1` with its
adjacent `Read-RelayProgress.ps1`. Start only one read-only observer each,
with a new local EvidenceRoot and these original route pins:

```powershell
$watch = @{
    TerminalCircuitCount = 2
    MaximumWaitSeconds = 7200
    EvidenceRoot = '<new-local-root>'
    ExpectedOrigin = 'novovm-mldsa65:6144e3de68011031592a7f66422b077989e643b7281b8904384b6cf1c321d5b3'
    ExpectedPath = @(
        'novovm-mldsa65:3a44e06e371273a050641a80d9103ac4d357954aabc5336072cfb72864b4cf8f',
        'novovm-mldsa65:516fc66162a3caa1d0153da9c42b5bbae9eae1e8bdcfb097b70aaa4ee97dd9f2',
        'novovm-mldsa65:49ba6bfc9206a0bc5fcb93e83975c10d10a228dbf7cd9bf4a2d909ca7df0d946'
    )
}
# On C only (updated canary):
$watch.HostId = 'C'
$watch.Source = '408e9d5810aac52fc4e3069d71810d9f164b02dc'
$watch.NodeSha256 = '56919a303ba809b8fe123ac897f21d5d5096875b4fb0b62940333b2029a6f2fa'
$watch.WorkName = 'novovm-common-408e9d58-20261118'
$watch.CommonProgressWriter = $true
# On D instead, set only the following host-specific pins:
# $watch.HostId = 'D'
# $watch.Source = '5b656189784c854ac356aae77610c931b9f2189d'
# $watch.NodeSha256 = '4b0d03e8ad7a5a93051cee2672d38aa9a4ec8c0cb99669ae89af62b8eeef4038'
# $watch.WorkName = 'novovm-common-5b656189-20261117'
# $watch.CommonCapacity8 = $true
# Pass the common parameters and only your host-specific pins to the launcher.
```

Immediately push `ready.json`, `liveness.json`, node and observer PID/creation,
and presence of terminal files to this candidate's `C-recovery-observer/`
or `D-recovery-observer/`. Keep all raw snapshots for both the empty circuit
and the one new object. If a collector is already running or a node is
unhealthy, report that fact; do not start a duplicate. No node restart,
upgrade or old test replay. A acts only after both fresh receipts.

## Original C update instructions (completed)

Full CI [44224445560](https://github.com/novovm/supervm/actions/runs/44224445560)
attempt 1 passed on `408e9d5810aac52fc4e3069d71810d9f164b02dc`.
Use **only** the ZIP and pins in the adjacent `windows-archive.json` and
`windows-qualification.json`; do not use this handoff without both receipts.
Only C is authorized to change program in this phase. A/B/D keep their
current builds and owners; do not start a relay observer or send an object yet.

1. On C, independently verify archive length/SHA256 and each manifest member
   before touching the original owner. Last known 5b PID 22324 is evidence,
   not a stop target. Check the actual PID, creation, process account,
   executable/hash, scheduled task, ports and original config. Preserve the
   current stuck or exited evidence and the old logs.
2. Extract to a fresh side-by-side directory
   `C:\ProgramData\NOVOVM\releases\novovm-common-408e9d58-C-20261118`.
   Reuse C's qualified LocalService existing launch path and
   `run.json` / `progress.json` layout under
   `C:\Windows\ServiceProfiles\LocalService\AppData\Local\NOVOVM-Common408e-C-20261118`.
   Bind only the new EXE, AOEM/manifest and launch/report paths. Keep
   the original chain-state directory, keys, chain/domain, peer/network
   config, discovery cache and journal unchanged. If the original Windows
   program rule is path-scoped, copy only its exact existing scope; never
   relax the rule or disable a profile.
3. Stop only the freshly identified old owner if it is still live. Verify
   old process/ports are released before starting the new node once with
   `START=existing`, continuous lifetime and original LocalService SID.
   No initialize, deleting LOCK, parallel opening, old-version restart
   or blanket firewall change. Preserve a known consumed diagnostic command
   as evidence and prevent it from replaying; don't remove unknown commands.
4. Immediately publish the new PID/creation, actual EXE and mapped AOEM
   hashes, manifest hash, listeners 41052/42052, old exit evidence,
   database/config/progress/control paths (paths only, no contents/keys),
   unchanged identity/head/height and original recovery health under
   `C-existing/`. Mark this as started, not sustained health.
5. On that same PID, record progress mtime UTC and numeric ticks on startup
   and over at least 15 minutes, taking samples no more than 1 minute apart.
   Check the progress is still fresh, ticks strictly advance, no new exit
   or execution failure, and archive status fields keep their JSON types.
   This covers the recorded 11m28s recurrence interval on PID 22324.
   Push the series and `C_408E_EXISTING_READY_FOR_A_QUERY` only after success.
   If the new node stalls or exits, retain its evidence and stop; no blind
   retry/reinit, no old BASE/crash test. Any rollback is the original
   `existing` startup on one verified owner only.

A independently performs authenticated Status and validates chain/root,
then coordinates fresh original C/D observers for just the pending B
automatic-recovery workload. The 5b observer receipts and the failed
pre-outage path remain historical. Old concurrency, old deliveries and
old forced-stop tests are not rerun.
