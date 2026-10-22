# Continuous service concurrency: v102 follow-up review

## 2026-10-22 v103 candidate: default rate uses the original resource owner

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
`artifacts/v103-controller-wire-fix-iroh-tests.log`. Phone/App candidate
acceptance remains pending.

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
