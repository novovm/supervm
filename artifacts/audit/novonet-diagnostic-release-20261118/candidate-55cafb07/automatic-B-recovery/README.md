# B original relay automatic recovery

## Current — arm C/D observers once

B's original 55cafb07 transient service had `Restart=no`. The current boot's
targeted runtime drop-in is now loaded: `Restart=always`, `RestartSec=5s`,
start-rate limit 3 per 60s. The initial reload kept B's original PID,
invocation and process start unchanged. The [live policy receipt](B-policy-enabled.json)
records those facts. A management-only WSL smoke test on matching systemd 255
verified one automatic restart and a 5108.196 ms process gap; this is not B
or network acceptance. An intentional `systemctl stop` remains intentional;
the test signals the exact main process without asking systemd to stop the unit.

Each of C and D should first confirm that its last observer has exited, then
start exactly one original read-only `artifacts/audit/novonet-single-relay-20261111/Watch-Relay.ps1`
with adjacent `Read-RelayProgress.ps1`, using its qualified hidden administrator
PowerShell 7 wrapper and a new local EvidenceRoot. Do not restart or update
either node, send an object, or rerun concurrency. C/D remain on 5b656189.

```powershell
$watch = @{
    HostId = 'C' # D uses 'D'
    Source = '5b656189784c854ac356aae77610c931b9f2189d'
    NodeSha256 = '4b0d03e8ad7a5a93051cee2672d38aa9a4ec8c0cb99669ae89af62b8eeef4038'
    WorkName = 'novovm-common-5b656189-20261117'
    CommonCapacity8 = $true
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
# Pass these exact parameters to the qualified observer launcher.
```

Immediately push actual `ready.json`, `liveness.json`, observer PID/creation
time, actual node PID and result/error/exit presence into this directory's
`C-recovery-observer/` or `D-recovery-observer/`. Do not wait for terminal
events. Keep all raw snapshots, including opaque forwarding and the empty
pre-outage circuit. Return a precise blocker if the node is not healthy.

Once both receipts are fresh, A uses the existing owner to authenticate an
empty A→B→C→D circuit, then signals B's exact original PID once. The same
systemd owner must restart automatically with `existing`, the same original
state, identity and custody, and new process-owned UDP bindings. A must
observe circuit loss, then authenticate a new circuit/session under the
unchanged lease/timeout policy and send exactly one new diagnostic object.
Final acceptance requires systemd's monotonic exit/start timing, original
listener and state checks, matching authenticated edges, both actual relay
forwards, sender DONE and D's independently verified complete custody.
Object DONE is not a Business Receipt or consensus proof. No old test replay.

This policy is scoped to the current unit and current B boot; it does not
verify host reboot recovery or change node packages. The earlier failed
manual stop/start attempt and the accepted in-flight concurrency stay
independent and unmodified.
