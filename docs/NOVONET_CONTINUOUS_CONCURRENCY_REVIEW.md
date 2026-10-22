# Continuous service concurrency: v102 follow-up review

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
