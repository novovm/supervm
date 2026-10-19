# Complete-plan GPU contention investigation (candidate rejected)

This is not finalized-transaction acceptance. Product baseline: `9b2b000a`;
AOEM source baseline: `97070d17`. The CPU product path remains unchanged.

## Measured cause, before the candidate

The test replays one prepared 1,024-transaction complete candidate, one warm-up
and sixteen measured executions, comparing every exported value. It uses the
same unified integer resource as the product. Four-process runs replay the same
candidate independently on one GPU; they are NOT four-node consensus tests.

With no per-dispatch diagnostics, baseline average complete-plan time was
30,282.69 us alone and 50,733.81 / 51,190.19 / 51,050.88 / 50,499.38 us in four
processes. Device: NVIDIA GeForce RTX 5090 Laptop GPU (driver-reported discrete
device, vendor 0x10de). AC power was reported connected.

A separate aggregated timestamp diagnostic (17 executions including warm-up)
recorded the old scalar selector's 1,088 dispatches: GPU command intervals
65,628 us alone versus 65,366 us in one of the four processes; completion waits
106,314 versus 343,931 us. Device intervals and waits are nested/overlapping,
not additive chain costs. The scalar selector represents about three quarters
of the measured device intervals, and executes 64 times per candidate.

An earlier diagnostic printed every dispatch and materially perturbed timing;
its approximately 849 ms four-process result is INVALID for estimating normal
contention. It is retained as a failed diagnostic, not used as the baseline.

Local raw logs and frozen diagnostic driver are preserved under
`target/native-gpu-plan-cost-20261019/`; they are not all tracked artifacts.

## One combined candidate, rejected after measurement

1. The node compiler selects existing checked-i1024 scalar programs directly;
   it no longer computes all seven operations plus a selector for a literal
   compile-time mode. No policy, width, guard or dependency changes are intended.
2. Reuse AOEM resident composition from `da80f306`, keeping intermediate layers
   in the existing execution context. That implementation ALONE previously
   failed the tail-latency gate and was withdrawn; this is a new combined test,
   not a retroactive acceptance of it.
3. Explicitly advertise a generic sixteen-program resource budget; NOV uses
   fourteen small programs. Keep per-program input/output limit16, context
   handle64 and resident memory64MiB. Older libraries retain budget8 and cannot
   silently run the new plan. AOIP0 v10 / AOIR0002 remain unchanged.

Required before promotion: direct-program/oracle equality, full candidate
failure/economics equality, resource rejection/lifecycle tests, then frozen
old/new CPU and integer four-node native 65,536 real signed transaction runs.
Keep batch1,024, window8,192, shared recipient, fees, quorum, durable final
receipts and cold recovery unchanged. Diagnostics OFF for formal TPS/P95/P99.
Report all rounds; require integer throughput benefit without consistently
worse tails and investigate any CPU regression. No SDK promotion yet.

## Actual result and disposition

All eight native runs completed 65,536 successful transactions, zero business
failures, receipt fee total2,949,120, account/receipt-fee conservation, nonce
sequence and cold receipt/balance recovery. This does not add full fee-state
audit or business-proof coverage: both report flags remain false.

| Backend | Baseline rounds TPS | Candidate rounds TPS | Aggregate baseline/candidate |
| --- | --- | --- | --- |
| CPU | 7,707.04 / 7,650.98 | 7,671.74 / 7,667.22 | 7,678.91 / 7,669.48 (-0.12%) |
| Integer | 3,594.12 / 3,616.83 | 2,638.21 / 4,674.34 | 3,605.44 / 3,372.80 (-6.45%) |

Integer P95/P99 (seconds): baseline2.345/2.364 and2.293/2.417;
candidate10.858/11.561 and1.796/2.276. CPU tails remain in the measurements.
Aggregate TPS uses total successes divided by total measured duration.

The first height was observed by all four nodes at about0.413s in baseline1,
9.914s in candidate1 and0.779s in candidate2. This localizes the outlier to
initial progression; it does NOT prove driver compilation, queue contention,
or consensus timeout was its cause. No outlier was discarded or favorable
extra formal round added. Frozen baseline node e7e41e50 predates current
reliable-carrier feedback code; the native UDP fixture does not enable that
carrier branch. This is not a historical July/October controlled comparison.

Isolated warmed candidate replay improved from30.283ms to about12.1ms alone,
and four-process mean50.869ms to21.355ms, with all exports equal. Seven scalar
operators across128 bounded edge inputs each matched the historical selector;
full common/diverse candidate exports matched, as did fee rejection/dependency
repair and window/diagnostic edge tests. The new generic sixteen-resource
upper-bound test was added but NOT run before rejection; no claim of complete
AOEM regression acceptance is made.

The combined candidate therefore failed promotion. Active node/exec and AOEM
production edits were withdrawn; the bundled SDK was never replaced. Exact
rejected source is retained in `rejected-supervm-candidate.patch` and
`rejected-aoem-candidate.patch`, plus frozen local binaries/logs. Only the
ignored complete-plan contention regression fixture remains in source. It
warms once, and cannot substitute for the cold-first-use product measurement.

Next bounded investigation: initial complete-plan setup and first-height
execution/publication, using existing phase hooks on the SAME native path.
Do not redesign consensus, remove the first-height time from TPS, or accept
the warmed microbenchmark as proof that end-to-end contention is fixed.

## July versus October audit

July `4f45526d` native structural bulk was actually reproduced; see
`../july-native-reproduction-20261016/README.md`. Its combined receive/execution
sample is 161,740.96 rows/s, not merely engine arithmetic. However, the original
fixture uses preserved synthetic signature bytes without cryptographic checking,
core persistence none, and no four-validator finality. Its fixed template is
not the present complete NOV transfer relation.

October `a7db795` controller load includes real signature verification, business,
network, BFT and four durable heads; it excludes wallet signing/startup/cold
recovery from timing and differs from current native receipt observation.
There is no same-condition finalized transaction comparison proving either
month categorically faster. Reuse July native structural mechanisms and later
verified business/safety capabilities in ONE node/exec/AOEM path; do not infer
that the current full-plan GPU cost is a defect of July transport.
