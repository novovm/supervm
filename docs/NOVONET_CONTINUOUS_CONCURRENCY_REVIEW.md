# Continuous service concurrency: v102 follow-up review

## 2026-10-23 v106 follow-up: finite continuity, no new runtime change

Reuse the installed v106 App `33196fe` / SDK `c161003f`, existing observer and
frozen first-failure tools. No new sender change, diagnostic fields, APK/node
deployment, quota or deadline adjustment. A preplanned 20-minute mixed window
ends at 1,323.984s with original A18911/B26192 and five identity hashes each intact.
B receives new text while HOME for 379.937s: sender Delivered is verified BEFORE
foreground return, then receiver Saved. Tiny idle wake checks on A and B each
complete 8/8 clients, 24/24 custody cycles with zero errors, not sustained TPS.

Validated cellular text/image, a 20.922s data-switch outage, original-image catch-up,
full previews and device receipts pass without process restart or resend. Both
original processes survive quiet/background intervals. Seventeen read-only
samples per phone report no observation issues, but this is not leak/24h proof.
Network task replacement and transient L4-not-ready remain accurately recorded.

AFTER the same-process window, explicitly force-stop B and prove absent PID;
sender obtains an offline image's Stored. Exactly one ordinary App open restores
original running choice, fetches/previews that image and returns its device
receipt. B now has new PID 17148, A remains 18911. New reverse text also passes;
final identities/history, original network switches and L4/local deposit remain.
This is ordinary-open cold recovery, not autonomous revival, OS reclaim or Doze.
No Windows/cloud/KINGCLUB operation; no TLS-only/Iroh-isolation or physical-path
acceptance. Full finite scope and ignored evidence pointers are in the
[App checkpoint](../../NOVOCHAT/docs/implementation/P0_CHECKPOINT.md).

The causal active-tail-repair correction remains delivered. Historical 0/16,
unbound rejects, earlier nine-second progress, TLS 59.49s, safe clock and sustained
stability are NOT closed by these successes. Keep existing first-failure capture
for natural failures; do not repeat successful stress batches or expand observers.

## 2026-10-23 v106 active tail repair: causal fix and bounded phone regression

Reuse `novorudp/object_sender.rs`, the existing authenticated UDP test link,
`Reassembler` and `NativeWireBudget`; no new protocol, observer or scheduler.
The c804ef3c production sender is unchanged through the preceding9b446190.
New43-fragment/33,013B tests retain default1s ACK wait, pacing, copies/retries
and the original10s operation deadline, using real elapsed time and AEAD UDP.

Before the patch,13/14 sender tests pass: ordinary lost-tail recovery, blocked
write, budget resumption and internal-gap suppression work. The new progressive
ACK case fails at39: timeout already started the full38..42 repair; receiving38
changes the missing set to39..42; `on_ack` trims that remainder toNone and waits
another feedback interval despite ample time. This proves an active-repair
cancellation defect, not the invalid claim that near-deadline loss must recover.

The minimal change preserves a full tail repair already in progress when a
verified new ACK updates its missing set. Initial in-flight tails and trimmed
internal-gap plans still use the original suppression. Pending writes retain
their exact frame/generation; a blocked old38 completion cannot advance new39.
No acceptance/replay/integrity rule, timeout, retry count, pacing or quota changes.
Receiver test DONE follows full hash verification. Final-source release regression
passes154 related tests, including the formerly red progressive-ACK case and
blocked old-generation tail completion. Logs are retained locally as
`NOVOCHAT/artifacts/v106-tail-before.log` and `v106-tail-after.log`.
SDK fix `c161003f` and App build `33196fe` are now installed on the two original
phones through the official profile-diagnostics flow. Rebuilt probe uses the
same SDK fix.15 real UDP/TLS integration tests also pass (manual experiment is
ignored). Original signer, five identity hashes per phone, history and retained
v105/102/103/104 APKs are preserved. New installation PIDs18911/26192 then remain
unchanged throughout this bounded campaign; no Windows/cloud/KINGCLUB deployment.

Four finite native UDP loads pass1,920/1,92032KiB Put/Fetch/Consume cycles:
A first8x60(11.835 cycles/s), B first8x60(15.888), B same-process old peers16x30
(25.432, native records9->25), B real validated Wi-Fi->cellular->Wi-Fi then8x60
(18.849, records0->9). They use original deadlines and frozen first-failure capture;
no failure occurs. These are neither a controlled throughput comparison nor
cellular inbound-P2P acceptance. No further load-until-green repetition.
Real text with B's16-client load, reverse text, both1.6MB image directions,
sender-offline independent fetch/preview and original device receipt all pass.
Phones end running with original network choices, L4/local deposit ready.
Full limits, P95/P99, APK/probe hashes and visual evidence are in the
[App checkpoint](../../NOVOCHAT/docs/implementation/P0_CHECKPOINT.md) and ignored
`NOVOCHAT/artifacts/v106-*` records. This supports delivery of the specific
component-proven repair correction, not closure of every historical phone stall.

This is NOT proof of why the paired phone request took nine seconds to reach38,
nor attribution of its eight handshake rejections, v104's13 or historical0/16.
Those evidence gaps and continuous-stability acceptance remain OPEN. Existing
v105 APKs/identity/history and102/103/104 rollback artifacts are preserved.

## 2026-10-23 bounded v105 failure paired: partial request tail, no production fix

SDK/runtime remains `c804ef3c`; App installed build `530d2de`, v105. Host-only
first-failure tooling (`NOVOCHAT95d723e`) leaves native diagnostics and protocol
frozen. Nine host tests cover independent file output, zero-rejection trigger,
evidence failure and bounded profiler capture. No APK/node/KINGCLUB deployment.

One preserved-process idle case A16x30 observed no stall. One B real validated
Wi-Fi -> cellular -> Wi-Fi, no App restart, then8x60 FAILED7/8,429/480: client5,
round10 Put at original10.008s. No further load was run. Before budget is64MiB/min;
request refill40/s, native peer pool0 on restored Wi-Fi. Phone resources remain
normal; no manual capacity/rate/timeout change. This is not cellular inbound P2P.

A host file watcher captures the FIRST request failure regardless of rejection
count; phone snapshot completes1.407s after detection and15.625s before batch exit.
Probe stdout/cancellation proceeds independently, original100s guard is not hit.
Same authenticated full-object/session tag matches both sides. Body33,013B with
768B UDP chunks requires43; phone admitted/hash cursor and client ACK high-water
are38. It is a partial request missing final five fragments, BEFORE complete hash,
Put/Stored receipt or response. Existing30s reassembly TTL has not elapsed within
this10s request. Batch-end still has the same partial record. No support for
blaming the corrected hash-idle turn or SQLite in THIS sample.

The8 client-window rejections are handshake-only, still unattributed. First
reject6.133470s precedes actual ACK progress9.327578s; last reject9.683212s follows
it, all in one local observer. Rejection is not automatically the cause. Phone
global wire receive drops+249 are a separate scope; cannot assign them to the
Put. Work/store refusal and execution-backoff counters stay0. Historical13 and
0/16 remain unresolved. Phone record is present; omission0 and historical
retention counters are recorded, never used to infer absent work.

Remaining candidate: existing final-tail suppression/retry versus late feedback,
original deadline and actual transmission/loss. Fewer than1s remains after last
ACK progress, but intentional unconfirmed-tail ACK wait is not itself proven
wrong. No transport patch or causal red/green fixture yet; preserve auth/replay,
ACK checks and original deadlines. Stop load repetition, investigate this state.
Original phone PIDs798/21075/identity hashes and normal L3/L4 state remain, network
choices restored. Full [App record](../../NOVOCHAT/docs/implementation/P0_CHECKPOINT.md)
and ignored `v105-targeted-*` evidence retain the failed case. Stability OPEN.

## 2026-10-23 v105 phone diagnostic candidate installed; historical cause still OPEN

SDK `c804ef3c` and NOVOCHAT build `530d2de` were installed through the official
explicit profile-diagnostics flow on both original phones; APK SHA-256
`6fcacdeb74ec3a9c979920794a42cf0c0674996b0d9883910d0941ccc851ca79`.
Signer, installed hash, identity hashes/history and capacity-compatible102/103/104
rollback artifacts are preserved. No deployed Windows/Ubuntu node binary or
KINGCLUB service/config changed; those services retain their preceding runtime.

After147 related unit and15 real UDP/TLS I/O passes (one manual experiment
ignored), six bounded real native UDP phone loads completed2,880/2,88032-KiB
verified Put/Fetch/Consume cycles. A/B cold8x60 passed; each retained9 peer records
then passed16x30; A's subsequent25-record pool passed another16x30 (ending32,
not41 active peers); B's final16x30 follows offline-image network recovery (pool
1 ->17). App PIDs798/21075 stayed unchanged after upgrade. Two cold loads included
six UI swipes each; chat-overlap cases used actual single sends instead. Rates
were10.835/11.032/11.959/12.496/12.033/13.160 cycles/s, P95/P99 documented in the
[App checkpoint](../../NOVOCHAT/docs/implementation/P0_CHECKPOINT.md). These are
instrumented, unmatched adaptive-state samples, NOT a causal speed improvement.

Both phones sent new Delivered text while their own16-client load was active;
A also received under load. Online1.6-MB full image, external custody followed by
sender-offline independent receive/full preview/original DeviceReceipt passed.
Final identity/PID, normal resources, local deposit/L4 send and original network
choices were checked. Both used Wi-Fi; no new cellular/TLS-tail claim is made.
A chat-overlap harness failed a process-query race before Send while its B load
still passed; this retained fixture failure was not hidden as product success.
A verified unsent draft was resumed only once; a separate B own-load gate filled
the missing coverage. No request, probe deadline or production rule was changed.

There was NO new transport failure or failure-only client snapshot. Therefore
historical13 rejection cause/order, v104427/480 and v1030/16 remain unresolved;
all new successes are retained as bounded functional evidence only. Normal
builds keep the observer disabled. No larger quotas, timeouts, retries, weakened
auth/replay checks or new scheduler is justified. Stop successful load repetition;
use the deployed bounded evidence on the next natural failure. Snapshot retention
counters and local monotonic origins retain the interpretation limits below.
Long-frame samples, safety clock,59.49s TLS tail and24h stability remain open.

## 2026-10-23 classified transfer evidence candidate; no stall fix claimed

This round reuses `novorudp/endpoint.rs` rejection sites, `object_link.rs`
verified fragment/ACK/hash progression, `mailbox_structured/native_session.rs`
work/response lifecycle, the existing native listener and peer probe. AOEM
execution, accounts, transport scheduling, storage and wire protocol are not
replaced. The gap is causal evidence across the same authenticated request,
not a demonstrated need for larger quotas/windows/deadlines.

Fixed rejection categories preserve every original acceptance/rejection decision
and aggregate count. Pre-auth route/nonce/replay checks cannot authenticate an
object association; all rejection timing is explicitly UNATTRIBUTED. Ordinary
stale-epoch/absent-job ACKs keep their existing non-rejection semantics. A fresh
session-salted full-object/stream tag links verified request and response only.
No packet header before authentication is treated as a trusted association.

An explicit optional local observer records first rejection and actual progress
(new fragment, verified ACK information high-water mark, bounded hash cursor,
whole-object verification, work entry/finish, stored-receipt creation, response
queue/transport completion and client reply validation). Same-gap/worse ACKs,
duplicates and successful local send calls do not count as forward progress.
Work entry is not admission success; work finish/reply validation may be a valid
business rejection. A stored receipt is still verified by the original client.
Neither transport completion nor diagnostic metadata is a DeviceReceipt.

The observer is off by default. It owns at most256 local records, expires evidence
after120 seconds on use/snapshot and exposes omission/eviction/expiry counters.
These are evidence bounds, not business TTL or capacity changes. No new writer,
background timer, daily packet log, address/identity/content field or route is
added. The probe takes one snapshot on failure before original cancellation;
NOVOCHAT may opt into a profile-only diagnostic build using its existing snapshot
entry. Snapshot clocks are local monotonic origins: tags associate sides, their
absolute microsecond values must not be subtracted or globally ordered. Missing
records with omission/expiry are unknown evidence, not proof of absent requests.

Acceptance: deterministic category/progress tests and real UDP/SQLite correlation
must pass before packaging. Then preserve original phone identities/history and
v104 plus capacity-compatible rollback APKs; observe cold and same-process idle/
old-connection load without changing original32-KiB workload,10s request,90s test,
rate/window or security checks. Capture a real failure with both sides' evidence
before proposing a production correction. Historical13 rejections cannot be
retroactively classified by new counters, and another16/16 alone is not a fix.
No new phone deployment or sustained-stability acceptance is implied by source.

Final-source local verification:147 related unit tests passed;15 real UDP/TLS
I/O tests passed (one manual rate comparison intentionally ignored). The new real
UDP/SQLite test verifies the exact authenticated tag chain and original signed
StoreReceipt for a32-KiB Put, then proves an Unauthorized Put still completes
its work/reply evidence without a Stored marker or additional custody. Existing
mixed-identity, forged temporal metadata, withdrawal and restart cases pass.
Logs remain ignored under NOVOCHAT artifacts/v105-classified-*-final.log.

## 2026-10-23 v104 phone follow-up: installed hash-progress correction, cold stall still open

SDK `7a8b22d8` was packaged through NOVOCHAT's official profile build in App
`c571a70`, version104. Both original phones verified the installed APK SHA-256
`b0765d8a3dc5a73d82f9921ffbc43f1be577ef151b6389ad0259b69da6b62f99`,
original signer and five identity hashes each. No node binary or KINGCLUB service
was changed. The full phone matrix and evidence are in
[NOVOCHAT's P0 checkpoint](../../NOVOCHAT/docs/implementation/P0_CHECKPOINT.md).

Same final delta probe,32-KiB verified Put/Fetch/Consume, original10s request and
90s probe, sequential loads/no competing build. All post-install loads retained
A PID27575 / B PID9313; installation itself created new processes, so pre-upgrade
v103 peers are not represented as retained. A cold8x60 failed7/8,427/480; B cold
passed8/8,480/480. A idle8x60 then passed480/480 without restart. Both prescribed
old-connection16x30 cases passed480/480 (A17.145/B22.462 cycles/s; P95/P99 A1086/
2161ms, B901/2685ms). Those later passes and their higher learned budgets do NOT
replace the cold failure or establish a causal performance gain over v103.

The final diagnostic probe now actually captured A connection7/round8 Put at its
10.015s deadline: outbound1/33013B, inbound0, rejected13, data_sent262, with zero
receive/send I/O errors or admitted response fragments. Rejections may be endpoint
source/session/auth/replay/handshake/shape or object ACK/fragment errors; they are
not mailbox business rejection. Normal stale-epoch/absent-job ACK does not add
this count. Valid ACK acceptance is not counted by fragment fields, and262 means
initial+repair sends, not distinct fragments or retry count. No relaxation of
replay/authentication/ACK checking or larger timeout/retry is justified.

A whole-node wire receive-drop/send-deferral increased638/1273 during cold load;
work/store refusals and execution backoffs remained zero. This sample therefore
also has wire-budget pressure absent in historical A0/16. The aggregate cannot
attribute those drops to this Put, and successful B cases also had wire refusals.
The hash-gap source defect is fixed/delivered, but intermittent transport failure
is NOT fixed by implication. Next evidence: typed failure-only rejection categories
and actual same-request progression, before another production change.

New bidirectional text/device receipts, text while B served16 clients, online
1.6-MB image/full preview and sender-offline independent image/full preview plus
original DeviceReceipt all passed. Final original PIDs and identities remained;
L3:2/L4:1, local deposit/send ready, thermal status0 each, networks restored.
A35.516s unloaded whole-App CPU sample was22.07%/18.53% of one core (A/B); no
matched old-version control or isolation of hashing, so idle/power acceptance
remains limited. v104 remains a functional candidate, not sustained/24h acceptance.
Capacity-compatible v102 and v103 rollback APKs remain verified. Clock integration,
historical0/16 and59.49s cellular TLS tail remain open.


## 2026-10-23 preserved-process stall follow-up: one local defect reproduced and fixed

The final delta probe `b320e27d...` was run against the SAME v103 phone processes
(A PID24523, B PID22164), without restart, reinstall, network change or a competing
build. Both used Wi-Fi and the original32-KiB verified Put/Fetch/Consume workload,
10-second request and90-second probe deadlines. Six original UI swipes overlapped
each run. This is a new diagnostic sample, not acceptance of sustained stability.

| Phone / initial existing peers | Clients x rounds | Completed clients / cycles | Cycles/s | P95/P99 ms |
| --- | --- | --- | ---: | --- |
| A /17 |16x30|16/16;480/480|16.798|1168/1464|
| B /25 |16x30|16/16;480/480|20.136|1196/1266|

Both ended with32 peers; resource-pressure retirement can reclaim old idle peers,
so this does not prove all initial peers survived throughout. No new work/store
or SDK wire refusals, and no execution backoff, appeared. The263 sampled Flutter
frames per phone all had total span below16.667ms (P95 A10.392ms/B6.362ms); this is
bounded UI evidence, not an overall FPS claim. Learned rate/wire budgets and old
connection conditions differ from previous cases: do not attribute these speeds
to a source change, and do not replace the historical0/16 with these passes.
The final probe has still not captured a failed phone request. Own-UID Android
UDP netlink statistics were permission-denied; OS packet loss remains unmeasured.

### Reused implementation, demonstrated gap and minimal correction

Reused `novorudp/fragments.rs` -> `object_receiver.rs` -> `object_link.rs` ->
`product_rendezvous_native.rs::serve_link`, also used by NOVOCHAT's existing L4
listener. No AOEM transaction execution, new App transport or second owner is
needed. The reassembler already hashes a bounded number of received chunks,
but returned `None` both for missing input and for real hash work not yet forming
a whole object. The service owner could mistake that second case for idle and
wait200ms after network/feedback drained.32KiB needs more than the owner's32
native-UDP fragment quantum. This is a local scheduling defect, not yet a proof
that it caused the historical all-client stall or all its outgoing symptoms.

A deterministic real-UDP fixture receives a complete authenticated32-KiB Put,
drains the server socket and ACK output, and manually polls the production owner
once. Before the fix, real service `put.started` remained0 (1 test failed); after
the fix it is1 (same test passed). No timing threshold, fake service, manual
clock advance or new production test API is used. The crate-private progress
result now reports actual hash work through the existing layers. `serve_link`
continues its original cooperative loop on that signal. Missing chunks do not
report progress; blocked work still waits. Public completion results, original
quantum, ACK behavior, byte reservations, deadlines, authority and TTL stay intact.

The relevant release unit groups passed130/130, including original fragment,
ACK/repair, cancellation, session replacement, resource, UDP failure and carrier
preservation checks. New negative cases cover a missing middle chunk without a
busy loop, unchanged TTL and byte release, bounded hash quantum, hash rejection
and empty completion. This is source/component validation only; no new APK or
node binary has been installed during this round.

Both phones were rechecked at their original PIDs, five identity hashes unchanged
each, account running, L3:2/L4:1, local deposit/L4 send ready and resource pressure
normal. Existing v103 remains the test candidate; verified capacity-compatible
v102 remains rollback. Chat/image/offline functional evidence from the preceding
phone gate is retained, not claimed as a new device regression of this source fix.
No KINGCLUB source, service, configuration or80/443 listener was changed.

Real production-owner UDP/carrier/TLS I/O regressions then passed14/14; the
manual sequential rate benchmark remained explicitly ignored. These include
custody/restart, active-session declaration rotation, shared-execution waiting,
forged temporal metadata rejection and the borrowed L4 endpoint.

Local ignored evidence: `v103-stall-{A-old17,B-old25}-16x30-delta`,
`v103-stall-post-state.json`, `v103-native-hash-progress-red.json` and
`v103-native-hash-progress-regression.log`. Next gate is phone validation of this
source candidate and capture/attribution of any remaining stall with old live
connections; do not sign off the historical stall,24-hour stability or a general
throughput gain from this local correction. TLS59.49s and clock work remain open.


## 2026-10-23 v103 phone follow-up: partial gate, stalled transport not yet fixed

The v103 candidate is installed on both original phones and the existing Windows
and Ubuntu services; the retained accepted rollback baseline is v102. This
supersedes the source-only "phone pending" status below. Android's official
profile APK hash is `bb5dd38dea8fd62fc38e589afa1614d53bb24ee53ac594a5a4ae2bce09e3c409`.
Actual runtime SDK is281b7e64. Original identities/history/custody were preserved.
Cloud protected configuration/nginx, four existing KINGCLUB containers and three
healthy endpoints were unchanged. Final read-only checks kept both nodes serving
and all four existing containers running. No KINGCLUB entry was modified.

Same32-KiB verified Put/Fetch/Consume native UDP workload, original deadlines,
one performance slot and no competing build:

| Phone / case | Clients x rounds | Completed clients / cycles | Cycles/s | P95/P99 ms | Gate |
| --- | --- | --- | ---: | --- | --- |
| A first cold |8x60|7/8;425/480|11.450*|753/3395*|FAIL |
| B first cold |8x60|7/8;426/480|11.055*|735/6783*|FAIL |
| A warm |8x60|8/8;480/480|10.221|954/2088|PASS |
| B warm |8x60|8/8;480/480|11.350|904/2017|PASS |
| A second cold |8x60|8/8;480/480|10.882|916/1991|PASS |
| B second cold |8x60|8/8;480/480|7.713|3380/9398|PASS, tail retained |
| A after idle / existing peers |16x30|0/16;18/480|0.806*|9989/9989*|FAIL |
| B existing peers |16x30|16/16;480/480|11.264|2408/3338|PASS |
| A fresh v103 |16x30|16/16;480/480|13.829|1445/3391|PASS |
| A fresh v102 control |16x30|16/16;480/480|8.842|3782/6325|PASS |

`*` Successful-subset statistics, not accepted throughput. Do not replace failed
cases with later passes. A diagnostic-cache failure stopped another v102 control
before any business request; preserve it as INCOMPLETE. Intentional cold starts,
rollback/reinstallation and client diagnostic-build differences are recorded in
NOVOCHAT's P0 checkpoint and ignored `v103-phone-acceptance-summary.json`.
The fresh pair is not sufficient to attribute the earlier failure or certify a
general throughput gain. Sustained phone acceptance is incomplete.

The default policy genuinely adapted (fresh A16: refill32->50, work8->16,
burst64->128). Warm8 admission wait was nearly absent while service throughput
remained10-11 cycles/s. Increasing request quota again would target the wrong
bound. A16 failures instead include retained outbound Put/Consume and incomplete
inbound Fetch responses. All16 handshakes/window negotiations passed; work/store
and SDK wire refusals did not increase during that failed run. SDK counters do
not account for every possible network/OS socket loss.

Read-only comparison8142e1ce->281b7e64 found no changes in NovoRUDP/object repair,
ACK, native_session, native service loop, bound_session or dependencies. Only
mailbox policy/metrics/owner wiring and execution/wire feedback separation changed.
Execution backoffs were zero in the failed phone16 case, so that wire branch did
not explain it. Work8->10 does not renegotiate the established capacity grant;
capacity/business contexts and slots remain separate. These rejected hypotheses
are not a new proof of the unknown root cause.

`examples/mailbox_native_peer_probe.rs` now adds round/operation context and one
failure-only snapshot of existing outbound/inbound counts and seven fixed
`NativeLinkPoll` counter deltas. No new SDK API, timer, packet, retry, deadline,
payload/identity log or production runtime path. Deltas describe a client request
window, potentially including residual work, not a cryptographic same-object
proof. The phase probe1fb9b50d and final delta probeb320e27d are distinct from the
retained v102 probe992b3717; source/build provenance remains explicit. Both new
diagnostic variants compiled in normal release withiroh-transport,tls-transport.
No phone failure was yet reproduced with the final delta probe, so its counters
have not established the failing sender's retry/ACK state. Keep the original
finite retry and TTL contract; do not increase it based on speculation.

Mixed real-phone text, image/full preview and sender-offline independent image
receipt passed; final bidirectional post-load texts have visible device receipts.
Identity hashes/history and L3/L4 readiness remain intact. This is a functional
candidate with open intermittent transport stalls, not a production release or
24-hour pass. Continue that existing path before promoting the adaptive policy.
The clock-domain work and historical59.49s cellular TLS tail remain separate.

## 2026-10-22 v103 source/component checkpoint (phone results supersede status below)

The default request bucket is now bound to the SAME `ServiceConcurrencyV1`
used by the native listener, TLS/compatible carrier and Application owner.
`service_concurrency.rs` delegates its bounded rate state to
`service_request_rate.rs`; `mailbox_rpc.rs` retains the original shared token
bucket. `product_rendezvous_owned.rs`, the relay owner and NOVOCHAT's L4 owner
bind it before publication. There is no extra task, timer, queue, wire protocol,
account or authorization. Explicit `with_continuous_request_rate_v4` settings
remain fixed; reconnecting/rebinding cannot reset credit or replace the owner.
The old production hardcoded64/32 calls are removed, rather than kept as a
parallel mode. Generic unbound SDK fixtures retain their original default.

The managed rate bootstraps at32/s, then samples on arriving requests over
bounded two-second windows. Two clean demand/quota-wait windows are required
before a25% probe. Success, actual request mix, encrypted byte size, existing
service cost, execution backoffs, wire pressure and retained memory participate.
Idle Watch duration does not count as execution cost. Two comparable windows
with flat throughput and rising operation cost roll back one probe with a
cooldown. Failed/different work invalidates the slope. Platform pressure and
sustained execution pressure reduce the managed rate; normal platform recovery
restores the proven bootstrap without filling tokens. This is a bounded
finite-difference heuristic, not a guarantee of an optimum or linear scaling.
The existing execution controller itself is otherwise unchanged.

An extended8x60 component comparison exposed a separate feedback coupling:
fixed32, fixed128 and adaptive cases all retained request timeouts. In the
sampled fixed32 case, three execution backoffs lowered the initial wire budget
from1GiB/min to128MiB/min, immediately followed by507 receive drops and678 send
deferrals. The remaining443/480 cycles were retained, not labeled a PASS.
Other failed runs and all offered clients stay in the evidence. Component
execution/SQLite scheduling time is not a measurement of available bandwidth.
The original wire controller now treats execution backoff as a growth veto;
execution/request admission handles that pressure without also halving wire
capacity. Actual platform pressure still halves the allowance, and existing
metering, pacing, byte accounting and bounds stay enforced. No larger budget
or longer deadline is introduced. The final correction passed the7 native-I/O
regressions and the following full same-workload comparison. The final34 controller tests also passed with the necessary`iroh-transport`
feature; all tests in that module executed.

Old-interval credit is accrued before changing the refill interval; increased
burst capacity never mints tokens. Sampling and token updates share one locked
Instant. Default rate growth has no128/s policy ceiling; the explicit fixed
SDK API still accepts its existing1..128 range. Work, connection, memory and
wire limits remain separate and authoritative. Original request deadlines,
identity, revocation, TTL, custody and receipt rules are unchanged.

Local validation:33 controller tests and27 mailbox/API tests passed, including
fixed-owner preservation after adaptive growth, same/foreign owner binding,
credit accounting, cancellation, bulk Watch classification, cooldown edges,
workload changes and counter restart. Native real-I/O regression7 passed with
one manual comparison ignored by default. The first cooldown test run retained
one fixture failure (inconsistent supplied work/window duration); the fixture
was corrected without changing production logic, and the33-test rerun passed.
No full SDK clean-suite, new APK or phone throughput claim follows from this.

Final same-path release comparison (8 clients x60 cycles x32KiB, sequential,
no competing build/load, same initial1GiB/min TEST wire policy and unchanged
production feedback/deadlines):

| Request policy | Verified cycles | Cycles/s | Cycle P95/P99 ms | Summed admission seconds | Final requests/s |
| --- | ---: | ---: | ---: | ---: | ---: |
| Explicit fixed32 |480/480|11.039|885/1025|216.304160|32|
| Explicit fixed128 |480/480|27.152|509/1019|0.000013|128|
| Resource-managed default |480/480|16.143|773/952|103.384537|62|

All three admitted1456 requests including8 preflight metadata requests,
verified signed custody/decryption/cleanup and ended with zero stored objects,
active execution/body reservations, wire deferrals, wire drops and business
errors. Execution backoffs remained9/8/9 respectively and work targets ended
7/5/6: execution protection was NOT disabled to make the wire test pass.
The adaptive rate began32 and changed through40/50/62; burst ended48 with the
current work target, without minting credit. Relative to fixed32 this one run
improved complete-cycle throughput by about46% and reduced admission waiting;
it does not establish an optimum, steady-state ceiling, phone gain or24-hour
stability. Fixed128 is a test comparator, not the shipped policy.

Evidence: NOVOCHAT ignored `artifacts/v103-rate-comparison-wire-fix.log` and
`v103-rate-comparison-wire-fix.json`; earlier failed comparisons and bounded
resource sequences remain in `v103-rate-failed-comparisons.json`. The full-feature
SDK unit-test executable compilation twice exited with0xc0000005 (normal and
package opt1), before running tests; neither is a PASS. A base-feature build
matched0 controller tests because its module requires`iroh-transport`; also not
a PASS. Use the actual selected-test count and keep compiler failures distinct
from executed assertions. The7 final production-feature native regressions and
manual comparison used normal release optimization; no workaround build is used
for the reported throughput. Profile-session guards6 passed. The final normal-release`--features iroh-transport --lib service_concurrency`
run selected34 tests and passed34/34, including the execution/wire separation
and real platform-pressure guard. It retains one pre-existing optional-feature
unused-import warning, not a clean whole-SDK claim. Evidence:
`artifacts/v103-controller-wire-fix-iroh-tests.log`. This was the source-only
checkpoint; subsequent phone results, including failures, are recorded above.

2026-10-22. The accepted phone baseline remains v102: App `f281cda`,
SDK runtime `8142e1ce`. This review does not announce a new phone release,
continuous marginal-benefit controller, maximum device capacity or mainchain TPS.

## First separate policy throttling from resource saturation

The current L4 constructor is
[`l4_mailbox.rs`](../../NOVOCHAT/crates/chat-core/src/l4_mailbox.rs),
`with_continuous_request_rate_v4(64, 32)`. The bucket in
[`mailbox_rpc.rs`](../crates/novovm-network/src/mailbox_rpc.rs) is shared by all
local and remote requests, and is not reset by a new connection. Structured
requests call `request_local_timed` once per operation. The
[`native probe`](../crates/novovm-network/examples/mailbox_native_peer_probe.rs)
registers once per client and then executes Put, receipt verification,
Fetch/decryption and Consume: three admitted requests per complete cycle.

Consequently its long-run policy ceiling is about **32 / 3 = 10.67 cycles/s**.
With a full 64-token starting bucket the optimistic lower duration for N requests
is `(N - 64) / 32`. The initial bucket balance and other App traffic were not
captured, so this is a bound, not an assertion about actual initial tokens.

| Existing real-phone run | Counted requests including Register | Measured seconds | Optimistic quota floor, seconds |
| --- | ---: | ---: | ---: |
| PKL110 8 x 60 | 1448 | 49.683 | 43.250 |
| PCLM50 8 x 60 | 1448 | 47.092 | 43.250 |
| PKL110 16 x 30 | 1456 | 44.788 | 43.500 |
| PCLM50 16 x 30 | 1456 | 46.588 | 43.500 |

These results strongly support policy throttling as a major explanation for the
plateau. They do not isolate every network/database cost. In particular the
8-to-16-client throughput and latency changes cannot estimate a hardware
throughput derivative while this independent policy cap is active.
Source-derived accounting: NOVOCHAT ignored `artifacts/v102-rate-floor-audit.json`;
original reports remain in its four `v102-*-8x60*` / `v102-*-16x30` directories.

## Reuse and current implementation boundaries

- [`ServiceConcurrencyV1`](../crates/novovm-network/src/service_concurrency.rs)
  already owns demand, measured service duration, resource pressure, retained
  memory, connection admission and native wire feedback. Its current execution
  algorithm is success/demand-driven additive growth and saturation/pressure
  backoff. It does **not** estimate marginal throughput, and CPU parallelism is
  an estimate rather than measured CPU utilization. The current two-times-CPU
  execution ceiling is likewise a resource heuristic, not a universal 16 limit.
- The same mailbox owner's `timing_snapshot_v1` already records aggregate
  admission and operation elapsed time. Reuse it through the existing App
  snapshot; no new timer, request logs, remote telemetry service or identifiers.
  Operation counters include store queueing/commit and are not pure CPU time.
  Concurrent totals are summed request-time, not elapsed wall-clock time.
- AOEM `aoem-cc/src/scheduler_integration.rs` maps transaction cache pressure to
  discrete capacities; `aoem-optimizations/.../adaptive_concurrency_engine.rs`
  chooses OCC/MVCC from conflict samples. Neither is a drop-in network service
  throughput controller. Do not introduce transaction semantics into chat.
- [Netflix concurrency-limits](https://github.com/Netflix/concurrency-limits)
  uses bounded latency sampling (Vegas and short/long-average Gradient2).
  [Envoy's gradient controller](https://www.envoyproxy.io/docs/envoy/latest/configuration/http/http_filters/adaptive_concurrency_filter.html)
  compares sampled latency with a baseline and a variance buffer. Their useful
  lesson is stable feedback, not transplanting an HTTP filter or periodically
  forcing phones to minimal concurrency to obtain a baseline.

## Bounded next implementation boundary

1. Compare the same existing native UDP listener, authenticated clients, 32 KiB
   AEAD objects, signed receipts and SQLite commits with explicit **test-only**
   rate policies, unchanged workload, burst and deadlines. Count actual admitted
   requests and separate admission time. This is a component comparison, not an
   Android throughput improvement. Manual fixture: `native_service_rate_policy_comparison`
   in `tests/mailbox_native_node.rs`; run alone with `--ignored --exact --nocapture`.
2. Preserve explicit node-owner rate preferences. A future resource-managed
   default must be distinguishable from an intentional fixed owner quota;
   do not silently increase an owner's limit or merely change 32 to 128.
3. Extend the original controller only after identifying actual saturation.
   Use comparable workload/window samples, policy-wait attribution, successful
   useful work, damping and resource-pressure constraints. Demand alone, idle
   connections, Watch waiting, failed work or remote ACK time do not justify
   CPU growth. Different work types/bytes must not masquerade as a throughput
   improvement. No per-packet proof, new executor, extra timer or global history.
4. Keep connection retention, execution and bytes in flight separate. Existing
   leases/custody survive shrinking; queued requests keep original deadlines,
   byte reservations and identity/TTL/revocation checks. Resource expansion must
   preserve normal chat response under simultaneous file and L4 work.
5. Before phone acceptance, compare at equal network/workload and record all
   failures, useful throughput, tail latency and bounded frame/thermal samples.
   Sustained mixed operation is a separate gate; no 24-hour PASS is inferred
   from these short runs. Safe clock-domain integration and the historical
   59.49-second first-cellular-TLS tail stay independent open items.

## Completed component comparison (not a phone upgrade)

Windows release-build probe SHA-256
`992b37172a92722cc2649037ee10cf4eeb3c1374e9191a5194b9b69a9f3a4c78`.
Sequential cases, same 8 clients x 16 cycles, 32 KiB plaintext, burst64, existing
native listener/SQLite and original probe deadlines. A deliberately identical
TEST-only 1 GiB/min wire allowance avoids confusing wire pacing with request
quota. The existing probe supplies authentication, encryption, receipt checks,
Fetch/decryption, Consume and completion ACK progress; no duplicate transmitter.

| Test-only requests/s | Completed cycles | Cycles/s | Cycle P95 ms | Admission summed request-time, seconds | Rejections |
| --- | ---: | ---: | ---: | ---: | ---: |
| 32 | 128/128 | 12.21 | 833 | 49.917805 | 0 |
| 128 | 128/128 | 29.08 | 295 | 0.000002 | 0 |

Each case admitted 400 requests: 392 Register/Put/Fetch/Consume requests plus
8 existing preflight metadata requests. Probe elapsed excludes preparation;
the service timing totals include it. Finite-run cycles/s can exceed32/3 because
of the initial burst. Admission time is SUMMED over overlapping requests and
must not be subtracted from the single elapsed wall-clock measurement.
Both verified all signed storage receipts/decrypted objects and left no custody
or execution/body reservations. No wire drop/deferral or work refusal occurred.
The 32-rate case recorded one execution backoff (target before/after8); the128
case recorded none. This counter is retained, not a claim of zero feedback
adjustment or a newly diagnosed production defect.

This comparison demonstrates substantial policy waiting on this component path.
It is one sequential two-case run, not maximum capacity or a mobile speedup.
It does not implement an adaptive rate policy or justify replacing32 with128.
The initial manual-client fixture timed out and is excluded: it did not reuse
the diagnostic's final ACK progression and also had a second wire-pacing bound.
The failure remains in `artifacts/v102-rate-comparison-release.log`, not relabeled
as a product failure. Final comparison:
`artifacts/v102-rate-comparison-release-run2.log`; the7 existing native-node
regressions passed, with the manual comparison intentionally ignored by default.
The same command must set `NOVOVM_NATIVE_PEER_PROBE` to the reviewed release probe.

The original phones were only read in this follow-up: both remain v102, original
PIDs25586/24442, five identity hashes unchanged each, running L3:2/L4:1 with local
return/send ready. No APK, node binary, user network choice or KINGCLUB service
was changed. App timing snapshot wiring is a source candidate, not yet installed.
