# Single-relay real-object qualification

## CURRENT 2026-11-12: local candidate security blocker repaired before Full

Both original packager builds of1baa6dbc completed, but that source was NOT
qualified: root Cargo Deny rejected rustix1.1.4 (RUSTSEC-2026-0486). No Full
was dispatched and no node was updated. The minimal1.1.6 dependency patch and
Windows22/Linux20 local regressions are recorded in
[the security increment](rustix-security-20261112/README.md).
Build/qualify the corrected fixed source, not the superseded1baa packages.
The128KiB candidate authorization does not automatically authorize deployment.

## CURRENT 2026-11-12: completed single relay; extend the same owner for two relays

The real A -> C -> D sample is closed at a7e19fc8. See
[the raw three-host correlation](candidate-875d3307/A-single-relay-20261112-0947/README.md).
Do not repeat that direct/single-relay sample or open another C/D observer yet.

The next case is a single fixed object through two ordered intermediate peers.
Existing HopRoute/HopStream already allow three path identities (two relays and
the final recipient). The missing part is the original diagnostic owner, whose
local command and incoming guard were single-relay-only. This change extends
those same owners; it does not introduce a router, network daemon, wire protocol,
socket, queue or receipt. Normal business route priority remains NovoRUDP first.

`novovmctl native-network-probe prepare --via <relay1> --via <relay2>` emits one
bounded441-byte local command. Existing377-byte direct and409-byte single-relay
formats and tags are unchanged. New tags6/7/8 mean double arm/send/cancel; old
programs reject them. Send/cancel must match the original ordered reservation.
No empty/duplicate/unknown/local/recipient relay is accepted; at most two relays.
The original lease, message validity, connection limits and identity/domain
checks remain. The shared byte allowance change is separately authorized below.
End-to-end authentication binds the LAST path identity (target),
not a hardcoded first-relay successor. Intermediate peers still cannot claim
endpoint Object DONE or Business Receipt.

Local validation is complete. Initial Windows negative evidence is preserved:
command/state tests passed5/5, but the new four-peer case failed before readiness
with `diagnostic route must contain one known relay and one known target`.
The existing single-relay case passed. The incoming guard now accepts exactly
two or three known path identities, retaining its authenticated origin/path
checks and original per-owner occupancy. This was an incomplete diagnostic
extension, not an observed failure of the running four-machine network.

After fixing that guard, Windows and Linux both reached native authentication
and recipient time-reference readiness, then failed before probe delivery.
Linux's bounded owner error identifies the exact cause: requested1157 bytes,
limit65536, used64605, remaining931, received/admitted32881, sent31724,
reserved0, uncertain0, receive_dropped0, send_deferred1. This is exhausted real
rolling traffic allowance, not leaked outstanding reservations or a QUIC timeout.
The old64KiB failure is retained; it is not reclassified as passing.

User explicitly approved128KiB/min for CANDIDATE validation on2026-11-12. The
original shared NativeWireBudget now uses131072, and this node's signed locator
advertises the same allowance from the same constant. The generic Iroh endpoint
default (including other application consumers) is not changed. No per-hop or
per-command fresh quota, unbounded traffic or accounting reset is introduced.
Original quota-exhaustion/cancellation/incorrect-reservation checks stay active.
The new error only formats existing usage counters when reservation fails.
Windows's first128KiB single/double component cases both pass; final validation
also checks the signed allowance, unchanged four-session declaration, and the
original owner/clock/replay cases. Windows and Linux each pass18 targeted tests
plus5 existing UDP/carrier/reactor compatibility tests (23/platform, no ignored
tests), and strict Clippy for network/node/ctl with `-D warnings`.
See [exact counts, source/library hashes and raw archive manifest](double-relay-local-20261112/results.json)
and [original logs and runners](double-relay-local-20261112/local-evidence.zip).
The18 are5 diagnostic command/state,2 actual local single/double IO,1 original
file ingress,5 actual AOEM owner/time/cancel/concurrency,2 CLI,3 hop rejection and
shared accounting tests. The5 compatibility cases cover original NativeChannel,
UDP ingress during fallback, owner restart/reauthentication, wrong-chain rejection
and requested-only backup delivery. These are local tests, NOT physical hosts.

The existing Full component-IO step now enumerates the single/double and command
test names before running their original filters, plus the original hop budget
regressions. No new trigger, skip, weaker assertion or automatic deployment was
introduced. Local CI scope tests pass27/27 and workflow YAML parses. Ordinary
source push remains lightweight; this is not a Full result.

Validation evidence retains intermediate failures rather than counting retries
as extra successes. One intermediate Linux wrapper exited1 with
`--nocapture: command not found` after its Cargo checks, because its script was
edited while running. The final immutable runner was executed separately; that
wrapper failure is not a network failure and is not counted as a passing run.
The initial WSL PowerShell attempt also did not execute tests (pwsh unavailable).
Windows final-source diagnostic/compatibility checks run the exact test binary
produced by the final Cargo node build, recording its SHA256 before/after; they
do not replace its build or strict Clippy. Component signing fixtures and real
AOEM owner regressions remain distinct, and all included owner tests execute
with `--include-ignored` plus the actual library/manifest.

Candidate/deployment: NOT produced or updated by this change yet. Old875 cannot
receive a two-relay diagnostic route; upgrade every participating owner to a
qualified candidate before attempting it. Reuse original existing-state update,
protected command file and bounded observers. On real machines correlate the
same object/session at both endpoints and the same circuit/route digest with
actual two-way completed forwarding at EACH intermediate. Candidate selection,
sender DONE alone or this local four-peer test cannot satisfy that gate. The
planned four-machine order must be checked against current authenticated
locators; no unverified address or remembered observer READY is a send permit.
C's original875 memory faults remain open and are not modified or explained by
this diagnostic extension. No new real object, restart or package update here.

## Historical preparation:875d3307 fixed packages and local owner gates

Windows/Linux each passed7 packaged identity/NAT,20 original-owner,6 packaged
node recovery,1 NUAS recovery and1 runtime startup/reopen. Actual packaged
arm/send/cancel commands also passed; all archive members and inherited AOEM
checksums were independently verified. Original updater checks passed12 Windows
groups and13 Linux tests. [Fixed candidate, logs, hashes and existing plan](candidate-875d3307/README.md).
Matching Full success and A/C/D deployment/real relay evidence are still pending.
Only C/D package copy/hash preparation has been requested. No new live object,
node update, firewall block, direct-test repeat or normal route-policy change.

## 2026-11-11 candidate63e9418e rejected; bounded pacing fix and security gate

No running A/B/C/D node was updated and no new real-machine object was sent.
Windows/Linux source qualification each passed19 owner,5 original probe,3 hop
boundary and1 CLI tests; all three strict Clippy targets passed on both platforms.
Both fixed packages built. Linux copied-package hashes/runtime KAT and7 packaged
identity/NAT tests passed; packaged arm/send/cancel409-byte local commands passed.
These successful samples did NOT qualify the candidate: the subsequent original
owner qualification with packaged AOEM failed18/19 before diagnostic readiness,
with `diagnostic time reply budget`. No probe object was sent in that failed case.
Windows package owner qualification and remaining Linux recovery qualification
were not completed. Candidate63e9418e must not be deployed.

The old combined error does not identify which guard predicate fired. Inspection
found a deterministic defect in its receive-side rate predicate: requests spaced
250ms at their origin may arrive249ms apart after network jitter. The old
`now >= next_reply` condition then terminates an otherwise authenticated session.
The correction keeps a single pending authenticated response, captures its time
at handoff, and dispatches only when the original250ms reply rate permits. Its
original2-second deadline is not refreshed; queueing delay stays in uncertainty.
Three samples, distinct challenges, context, flight and shared byte budgets stay
unchanged. A deterministic test covers early arrival, no early reply, unchanged
timestamps/deadline, occupied slot, wrong context, duplicate, sample limit and
expiry. This is not an assertion that every prior combined-guard failure has been
independently attributed to that one predicate.

The already-triggered Full42961080141 uses88032d91, whose sole difference from
program63e9418e is C's handoff Markdown. It is not cancelled and cannot qualify
the new pacing correction, regardless of its eventual result. The next candidate
must finish local package qualification before another Full is dispatched.

Original63e9 evidence: `local-63e9418e-evidence.zip`, SHA256
`e85bc8cb02816c1ec8baa9fa9fa739105e51fb651061d61193e0c2ac0fc12094`.
The archive retains the local successful samples, initial failures, actual
packaged-owner failure, build/runtime output and CI source-equivalence record.
R4 correction: Windows/Linux each passed20 original-owner tests (276.61s/262.71s)
and strict node Clippy. The four diagnostic boundary tests and actual three-node
component delivery also passed independently. These are NOT real A/C/D evidence.
The collector now samples at most64 snapshots one second apart, covering the
60-second reservation without exhausting its record limit halfway through.

The same original Full's security job130327715687 failed Cargo Audit: root lock
`openssl0.10.78` has RUSTSEC-2026-0474,0476,0477; the last requires>=0.10.80.
Its Linux dependency is lettre->native-tls->openssl (node/rollout-policy), not
AOEM or NovoRUDP authentication. Cargo updated ONLY openssl to0.10.80; sys remains
0.9.118 and runtime lock is unchanged. No advisory ignore or gate change was made.
Root audit fetched advisory DB28d73fc95e5972af3403225a217def57dcaba3c4 and passed.
Runtime's first fetch failed with a GitHub I/O error; both audits then passed
against that same already-fetched1446-advisory database, without stale/ignore
options. Root/runtime Deny passed with existing warnings preserved. Linux actual
consumer regressions after this update passed:20 owner,5 probe,3 hop,1 CLI and
1 rollout-policy; strict node/ctl/network Clippy passed. Windows20 owner and all
three Clippy targets passed. Both original fixed-source875d3307 package builds
completed; copied-package qualification is still pending. No real node changed.
Original CI failure and local fetch failure remain separate records; neither is
rewritten as passed. No runtime deployment follows from a local security result.

The original Full recovery job130327715690 separately failed NUAS original-process
recovery. Its artifact13829518517 reports a verified interruption at durable
height1 (target40, three QC signatures), then existing restart. The test passed
its subsequent wait_height call, but client3's first128-selector receipt query
timed out at5000ms (1063 total selectors, shared deadline still84762ms). The query
send completion was Delivered, not a business response. Existing-node stderr
only records native tick startup, with no matching per-query ingress/response
trace; the root cause is UNKNOWN. No database/journal is included in that CI
artifact. This is not attributed to diagnostic routing, pacing or OpenSSL, nor
declared fixed. The next package's original NUAS gate remains required, with its
actual result retained separately. No timeout, quorum or receipt assertion changes.

R4/R5 correction, security logs and the91-file public CI recovery artifact are
archived in `local-pacing-security-evidence.zip`, SHA256
`d371f4f3e6dafe19bc2136d7723780f7c4ca647aa5358fcdb946c31386f1711a`.
Its adjacent JSON indexes the63 direct local logs; the additional CI files are
under `ci-42961080141-recovery/` in the archive. No database, journal or private keys included.
Original logs also remain in `target/relay-constraint-20261111/`.
No fresh firewall block or repeated real direct test is required.

## Diagnostic constraint implementation in progress (2026-11-11)

User authorized a diagnostic-only path constraint in the original owner, followed
by local qualification and real single-relay object delivery. The prior UDP
isolation is complete and restored; it is not repeated or broadened.

Implementation uses the existing Driver reactor, verified locator candidates,
Iroh scope, HopStream authentication/forwarding, NativeEndpoint and
NativeObjectLink. It creates no socket, listener, daemon, business queue or receipt
protocol. A domain-separated diagnostic circuit is never installed through
`Connected`: normal business peer routes and NovoRUDP-first policy remain unchanged.

The original protected local command file supports bounded arm/send/cancel tags.
Arm pins one known intermediate validator and one known recipient, reserves one
session for at most60 seconds, and sends no probe. Send must match that reservation,
follow authenticated readiness, and use the original fixed non-business payload
with a fresh10-second lifetime. No arbitrary payload or asset submission is allowed.
The original shared connection/hop-byte budgets are not raised. Diagnostic events
retain at most64 entries; route digest/circuit, actual forwarding and native session
are observed by the original relay diagnostics. Object DONE remains custody, not
a business receipt or consensus result.

Local failures retained separately:

- Initial three-node fixture attempted a relay before its fresh next-hop locator
  was available. The fixture now loads actual signed peer records and waits for
  their verified availability; production discovery and timeouts are unchanged.
- The initial diagnostic measured relative time in both directions and exhausted
  the original shared relay-byte budget. The fixed probe is one-way: its recipient
  now performs the original three authenticated time exchanges; the sender replies.
  Receiver validity still requires its own established bounded reference. This
  removes unnecessary exchanges, not authentication, sample count or budget checks.
- Windows r3 targeted tests:3 boundary tests plus1 actual three-node relay test
  passed. The latter checks matching object/session, sender DONE, receiver integrity,
  middle-node forwarding, unchanged business sessions and subsequent business
  delivery. These are component tests, NOT A/C/D real-machine evidence.
- Linux r2 original-owner group:18 passed,1 failed (the pre-fix diagnostic budget
  case). This failure is not relabeled as passing; final-code checks are pending.
- Strict Clippy first identified a range-loop style error in the new test; fixed
  without suppressing the lint. Final two-platform results are pending.

Working logs: `target/relay-constraint-20261111/`. No candidate has been accepted,
no running node has been updated, and no new real-machine object has been sent by
this implementation work. Fixed-source qualification and actual relay/receiver
evidence remain required before closing the real-network task.

## Authorized live window result (2026-11-11)

User explicitly authorized at most five minutes of A↔D specific UDP isolation,
automatic restoration, and exactly one new object **only after** an authenticated
single-node relay route was established. That prerequisite was never satisfied.

- Exact application: installed8fed novovm-node.exe, A original PID13996.
- Two temporary Block rules (inbound/outbound), only A192.168.71.74 and
  D192.168.71.59, local UDP41051/42051 and remote UDP41053/42053.
  Other peers, TCP, profiles, original rules and node configuration were unchanged.
- Independent SYSTEM restore task was armed **before** rules were added, with
  deadline03:42:51Z. The primary runner removed its rules sooner, then unregistered
  the now-unneeded watchdog. The watchdog itself did not need to run.
- Rule window03:38:22.447Z–03:42:24.166Z: **241.72 seconds**, under five minutes.
  Privileged final ActiveStore query: remaining_rules=0, restored=true.
- A's23 interval snapshots and final pre-restoration snapshot retained zero relay
  circuits. Installed bound-carrier count changed0→1; this is not a node-hop route.
  Existing probe accepted_commands stayed1 (the previous direct object); no new
  prepare command, test object, Object ID or DONE was created. execution_failures=0.
- B's70 snapshots03:37:26Z–03:42:04Z retained only the same two historical dropped
  circuits and zero forwarded frames. This sampling ended about19 seconds before
  restoration; it is not continuous coverage of the final seconds. A still recorded
  no relay circuit at window end; a separate B post-window projection is retained.

Final runner status: **NO_AUTHENTICATED_SINGLE_RELAY_NO_OBJECT_SENT**.
The isolation experiment completed; single-relay object delivery is **not executed**,
not PASS. No D receiver action is required because no new object was sent.
No direct test was repeated, and no failure or historical counter was erased.

### Actual remaining execution gap

Blocking those UDP coordinates does not exclude other Iroh reachability paths.
The fixed owner can establish a bound Carrier without constructing a NOVOVM
HopStream route. The code advances to node-relay candidates on connection failure;
it does not force an intermediate node when the Carrier succeeds. These observations
do not identify the exact underlying Iroh path and do not prove a relay defect.

Do not expand the firewall experiment to arbitrary relay/DNS/TCP traffic or repeat
it until a desired outcome appears. Deterministic single-node relay qualification
needs a bounded original-owner diagnostic path constraint (with its own affected
validation/candidate qualification), or a separately agreed reachable real topology.
The actual product's healthy NovoRUDP priority, existing authentication/limits and
business routing must not be silently changed. No such source change is made here.

`udp-window-original.zip` contains the executed script, A original window results
and snapshots, and B original read-only observations. `udp-window-SHA256SUMS.json`
records exact source bytes and archive hash. No private configuration or key material.

## Historical preflight (before the above authorization)

2026-11-11, after the completed A→D native UDP object. No repeat of that
delivery, new probe command, node restart, locator modification, firewall rule,
clock change or protocol change was performed.

## Actual observations

- A: 8fed original owner PID13996, original UDP ports41051/42051;
  fresh progress at03:26:08Z, collected03:26:09Z. relay_forwarding=true,
  installed_bound_peer_routes=0, relay_circuits=[]; execution_failures=0.
- B: original8fed unit active/running, PID3133881. Current public projection
  has installed_bound_peer_routes=0 and two retained relay observations.
- Those B observations are **historical**, about12,313–12,355 seconds old at
  capture. Both authenticated an upstream route but have downstream=null,
  downstream_ready_us=null, zero forwarded frames/bytes, and stream_dropped_us.
  They are not a new attempt, a live usable circuit, or evidence of this task's
  object delivery. The failed downstream substage is not determined here.

`original-preflight.zip` preserves A's privileged read-only projection and B's
SSH public progress projection. Only public network observations are included.
Digests are in `SHA256SUMS.json`. No secret key, run config or database is included.

## Actual fixed-owner execution constraint

The existing service/network/relay.rs::fallback_path rotates direct/single/two-hop
routes on failed connection turns. service/network.rs enters that work only on
fallback/bound peer state and retains healthy native UDP priority.
The existing fixed probe command selects sender/recipient, not a forced relay.
No suitable active A relay circuit was observed. Sending immediately would not
prove a single-relay route and could merely repeat the already-passed direct case.

The existing real-IO hop regression makes its test topology by signing deliberately
unreachable coordinates inside its fixture. That is not permission to rewrite
live signed locators, and is not a four-machine result. Iroh's external relay alone
also does not prove a NOVOVM intermediate node forwarded this object.

Next requires a scoped route experiment: either explicitly authorized bounded
network-path isolation, or a diagnostic-only path constraint in the existing
owner with qualified deployment. Do not silently change production routing,
disable healthy direct paths, or implement another transport to manufacture PASS.
After an actual authenticated single-relay circuit exists, capture its matching
origin/relay/target route digest, endpoint bindings, native session, real forwarded
writes and receiver integrity acceptance with sender Object DONE. No result is
currently signed for that test.
