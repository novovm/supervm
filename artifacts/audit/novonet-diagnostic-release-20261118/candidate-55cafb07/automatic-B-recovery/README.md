# B original relay automatic recovery

## Current — C original owner is alive but progress is stalled

C reports the same 5b node PID 19196 and creation time in both samples:
19 threads, scheduler state `Running`, and unchanged progress
tick 135019 / mtime `2026-11-18T00:20:17.280Z` at
`00:36:52.557Z` and `00:36:59.702Z`. No new exit event, result or
PID-matching dump was found, and node stderr had zero bytes. No restart.
The last progress mtime is 15.36 s before the observer's error.

Source review at C's actual 5b656189: NOVONET stepping, the product poll
that serves native Status, and synchronous progress status/encoding/file
output all run on the same original tick thread. The stale report and A's
earlier Status timeout are consistent with this tick not serving requests,
but do not establish whether it is blocked in networking, a product stage,
observation/output, or another operation. An independent network fault is
not ruled out. Process liveness does not establish tick health.

C next: record the original PID's visible wait chain, two main-thread
CPU/wait-state samples, and whether its original UDP ports are still bound
to that PID. No attach, restart, dump publication or new observer for this
check. B/D remain untouched; the automatic restart trigger and object
send are still unexecuted.

## Earlier C/D observer and query correlation

Pulled C `db2beb19` and D `a0bdd4e7`. Verified all five returned C file
lengths and SHA256 digests. C observer PID 28548 exited 1 at 08:20:32 BJT
with `Stale progress` and zero snapshots. This precedes B's first empty-circuit
authentication (approximately 08:21:01 BJT) by 29 seconds. D observer PID
53576 was still active at 08:33:37 BJT with zero snapshots and no terminal
files. D recorded no matching circuit; the missing C view does not prove
that B→C was never attempted or distinguish B→C from C→D.

A's one current authenticated Status query to C timed out after 30.14 s,
exit 1 (`native query deadline; no fresh node observation`). This is separate
from the earlier historical C crash; no new C exit is proven by these
observations. B still has PID 3167799 and zero restarts. No object was sent.

C next: use the existing original node and original progress path for a
read-only check of live PID/owner, two UTC/mtime/tick samples, and any
matching new exit event or stderr. Do not reset the stale threshold, reopen
the observer or restart the node before identifying the actual state. D has
no new action. B remains untouched until a healthy observable empty route
can be confirmed.

## Pre-outage failure — historical run evidence

Pulled the C/D readiness receipts through `43b41baf`; C observer 28548 was
bound to the recovered node PID 19196, D observer 53576 to node PID 34148.
Original readiness validation passed on both. C's earlier exit and its
separate `existing` restart remain recorded in the C directory.

The 08:21 Beijing pre-outage empty route did not complete. B authenticated
the incoming A hop, did not observe downstream ready, and closed the new
circuits below without forwarding an object. A recorded
`carrier stream IO failed: connection lost: closed by peer: scope finished (code 0)`.
B showed no Carrier admission rejections at the readback. This localizes the
block to downstream setup from B, but does not distinguish B→C from C→D.

| B circuit | Authenticated to close | Downstream ready | Forwarded packets |
| --- | ---: | --- | --- |
| `87dd3954749e70c583b7322f826709fe` | 15005.223 ms | no | 0 / 0 |
| `b2ebc0a61d9ef7c45d3ba1dd7363f722` | 15006.062 ms | no | 0 / 0 |

Controller exit 1, before invoking the B trigger. B remained active at
PID 3167799 with zero restarts; no B process gap, post-restart authentication,
object send or DONE was measured. No rerun of the concurrency sample.

C/D: keep the same observers. Return any existing raw snapshots and hashes,
latest heartbeat and terminal file presence, including an explicit no-match
if applicable. No new observer, node restart or send. A corroborates any C
inbound and C→D setup observations against those two circuits before moving
the B recovery attempt forward.

A originals are locally preserved under
`target/novonet-B-auto-55cafb07-live-20261118`:

- `before-B-auto-exit-first-failure.json`: `fe9a7261845daae4904e83a6087c3c68b804af8740a8d2e30250628f337d48f8`
- `B-preflight.json`: `ce14cab7d12e48dc45ac9dcf42ab2bdef79a1479945394e9cf98fdc9db534726`
- `B-after-preflight-failure.json`: `504bdfe3ec80644d9c538401e178f3d4b3ec78b7515d0035105b29f1d872c9d0`
- `B-carrier-owner-projection.json`: `b97db31633cf2f8e324eb0e50a8c89569267bc12a96900640785bc171aa1b9d5`

## Observer handoff (executed)

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
