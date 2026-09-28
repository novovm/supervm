# LAN diagnostic launchers

These two existing scripts test network-only overlay paths, not AOEM execution,
block finality, phone chat discovery or the Product Mainline runtime:

- `scripts/novovm-overlay-cross-machine-process-gate.ps1`
- `scripts/novovm-adaptive-overlay-cross-machine-smoke.ps1`

Their shared binary resolver now stops on Cargo failure even if an old executable
exists. Both build and lookup use the same explicit target directory: an absolute
`CARGO_TARGET_DIR`, a repository-relative `CARGO_TARGET_DIR`, or `<repo>/target`
when unset. Directories may contain spaces. The executable suffix follows the
host OS. These are native debug builds, not cross-compilation/package selectors.
`-SkipBuild` means explicitly reuse the selected existing binary; it does not
verify its source revision. Single-role/adaptive runs and background gate jobs
now propagate nonzero process exits rather than treating completion as success.

No drive name is required. No existing service is stopped or database migrated.
This repository is still pre-launch: test balances are synthetic accounting
fixtures, not evidence of real user holdings. LAN execution testing remains
separate from these network diagnostics and must not be marked passed by them.

Regression command (no compiler or network required):

```powershell
pwsh -NoProfile -File scripts/tests/overlay-gate-build.tests.ps1
```

The regression mocks Cargo and uses empty executable sentinels to check default,
relative and absolute targets, build failure, missing binary, and location
restoration. It does not execute these sentinels or claim real network coverage.
Windows and Linux CI run the same launcher regression. Existing example network
configs are not automatically rewritten or deployed by this change.

## Aggregate report acceptance

The adaptive script now requires boolean success, the adaptive-node report scope,
and matching sender/receiver node identities. Delivered routes require exactly
one configured target listener, unique listeners and the route's expected relay
count (direct=0, relay=1, multihop=2). Sender transmission totals and every
participating listener's success/counters must agree. A failed listener, wrong
node report or a missing target cannot be hidden by matching frame totals.
Queue fallback remains queued-only evidence, not delivery.

`scripts/tests/adaptive-overlay-aggregate.tests.ps1` invokes the actual aggregator
against synthetic JSON files: four positive route cases and seventeen rejection
cases. No sockets or services are started. These checks are not cryptographic
report authentication or wire freshness guarantees. None of this
proves production mainline execution, blockchain finality or real LAN acceptance.

## Isolate each diagnostic attempt

Run `-Action commands` once to generate commands sharing a fresh random `-RunId`.
Distribute those commands to the intended machines; do not generate independent
IDs on each machine. `run-node`, `send` and `aggregate` now require an explicit
ID (1..80 ASCII letters, digits, underscores or hyphens). Default reports go to
`artifacts/network-overlay-gate/<RunId>`; the old static config `run_id` is no
longer used as the attempt directory. Each new attempt needs a new ID.

Node reports record `diagnostic_run_id` and `diagnostic_case` directly in the
binary output. Aggregation rejects missing labels, another attempt or another
case. Old binaries used with `-SkipBuild` will therefore not pass aggregation.
Launch refuses an existing per-node report before build/start, rather than
overwriting previous evidence. Copy remote reports into the matching run/case
directory before aggregation. Existing services and old reports are not removed.

These fields are operator-supplied diagnostic labels, not signed or wire-bound
proofs. Reusing an ID, editing a report or delayed packets from an old run is not
prevented cryptographically. This change prevents accidental report mixing only.
Local real-binary queue smoke verified that the labels survive report generation
and aggregation: four queued frames, zero sent frames/bytes, no delivery claimed.
