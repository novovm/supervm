# First-height deadline ordering investigation

## Follow-up on b9d06282: queued control, not a new consensus protocol

Offline-copy inspection now also reads bounded first-height immutable outboxes.
The NVOUT002 envelope checksum/revision and the original proposal/vote signatures
are checked; complete historical replay evidence is NOT revalidated by this
diagnostic. Node 1 was the round-zero leader: it persisted a proposal and a
non-nil prevote. Nodes 0, 2 and 3 persisted nil prevotes. All four persisted nil
precommits; the round-one decision has exactly the SAME value as the first
proposal. Thus the failure was not an absence of any executed leader candidate.
The saved records contain no phase timestamps: follower execution readiness,
arrival and scheduling cannot retrospectively be separated.

A second concrete scheduling defect was reproduced on b9d06282. NativeLifecycle
handled just one complete control object before evaluating the deadline; the
underlying quantum of 64 is completion work, not 64 decoded messages. A rejected
first control could conceal a ready valid proposal. The new regression sends
the real persisted proposal over the existing NativeChannel UDP socket, places
a bad-signature message in the retained first slot, and checks the real durable
prevote. Before the correction it failed with `actual=None` (node 2), while
the expected value was the already executed candidate.

The correction reuses the preserved controller's bounded 32-event service rule
inside the original NativeLifecycle. No replacement controller is activated.
The same single retained slot is reused, backpressure stops that lane, data
lanes still get one turn each, and all existing verification and timeout rules
remain. This increases the bounded amount of control work per turn, not queue
capacity; it does not promise to process unlimited backlog or bypass a retained
backpressured request. CPU/GPU algorithms, library and SDK are unchanged.

The rejected GPU candidate also lazily created its resident composition on the
first execution, after readiness. That creation includes buffer initialization
and compute-pipeline creation in the background integer owner, not the control
thread. Warm component timings exclude it. This is a source-confirmed cost
boundary, NOT a measurement that it consumed the original 9.91 seconds. That
candidate remains rejected; no speculative network or timer change is justified.

Follow-up verification: old second-control regression FAILED (nil instead of
the candidate); corrected single/second-control regressions and CPU/algebraic
three-height cases all passed (4 tests, 2.01 s). Existing A/A/B recovery passed
(10.36 s, same-process four-store fixture, NOT four independent-process fault
acceptance). Ordinary consensus tests: 45 passed, 26 ignored. Release node built.

Same frozen driver and AOEM library, AC checked before each run, no competing
builds/benchmarks, diagnostics off. All eight formal runs preserved the initial
wait and completed 65,536 successful transactions, zero business failures,
2,949,120 receipt fees, amount/receipt-fee conservation, nonce and cold recovery.
First-height observation ranged 0.298–0.476 s; the historical outlier did not
recur. Full fee-state audit and complete business proof remain outside this gate.

| Backend | Old combined TPS | New combined TPS | Change | Old P95 / P99 range (ms) | New P95 / P99 range (ms) |
| --- | ---: | ---: | ---: | --- | --- |
| CPU | 7736.88 | 7671.68 | -0.84% | 1059.49–1066.04 / 1220.87–1254.16 | 1080.19–1102.95 / 1242.91–1258.65 |
| Integer | 3591.53 | 3587.90 | -0.10% | 2299.33–2328.58 / 2372.32–2427.96 | 2305.20–2317.74 / 2359.23–2395.50 |

Combined TPS is total successful count divided by summed measured time. Tail
ranges are the two individual rounds, not averaged percentiles. CPU both pairs
declined; no speedup or stable tail-latency improvement is signed off. Retain the
bounded deadline fix for its reproduced correctness case, not as a throughput
win. The exact historical first-round trigger remains unproven, not closed by
these successful normal runs.

[All follow-up measurements and binary identities](control-batch-measurements.json)
and [original four-store outbox audit](first-height-outboxes.json). Local old/new
failure and regression logs: `target/native-control-batch-20261019/`; original
stores and all unsuccessful earlier experiments remain protected. The active
production change is one bounded loop; no new mode, controller, SDK or DLL.

## Earlier round (3e445fce → b9d06282)

Base: `3e445fce`. One original NovoRUDP/APFL node/exec/AOEM path; no
network, signer, consensus rule, timeout value, execution backend or SDK change.

## Historical sample, not reconstructed timing

The rejected GPU candidate's retained run
`target/native-product-load/12816-1792410964640603800` first height was observed
by all four receipt readers at 9,913,691 us. Copies of all four stopped databases
were inspected using the existing CandidateStore, ChainRecord, archived decision
codec and VerifiedDecision verifier. All four records bind 1,024 executed
transactions to a **round 1 decision with three valid signatures**. Original
databases were not opened by the inspection: the ABI is not read-only, hence
explicit offline copies were used.

This proves a round change occurred; it does not reconstruct the time of every
proposal, GPU dispatch, vote or publication. The original formal run had no
per-phase trace. Neither shader compilation nor the deadline-ordering defect
below is proven to be the exclusive cause of that historical sample.

## Bounded reproductions

The same frozen rejected node (`fed093c1…`) and library (`2cd1dee4…`) ran twice
with existing stage diagnostics. These are NOT formal performance comparisons:

| Run directory under target/native-product-load | First height observed us | Successful TPS | P95 us | P99 us |
| --- | ---: | ---: | ---: | ---: |
| 16976-1792411952231005600 | 791072 | 4631.631 | 1792735 | 2199256 |
| 4140-1792412134506856800 | 773542 | 4536.614 | 1856457 | 2137348 |

Both completed 65,536 successful transfers and cold recovery. The second run's
existing 100 ms progress snapshots showed round 0 throughout height 1; first
integer-wire maxima were 66.18, 120.46, 385.01 and 387.85 ms. Inclusive counters
are not additive and these snapshots do not explain the old 9.91 s event.

## Reproduced defect and minimal reuse

NativeLifecycle previously called RoundProgress (including Pacemaker) before
consuming its bounded control slots. At the proposal deadline, an already
received valid proposal could therefore lose to a staged nil prevote. The
older preserved controller already processed eligible ingress before deadlines.

The correction reuses that ordering, not the old controller: durable ACK and
context advance first; existing bounded native receive next; existing
drive_consensus and Pacemaker last. Same journal, outbox backpressure, three
receive lanes, budgets, monotonic time and timeout values. CPU/GPU execution
remain unchanged. No lock reset, synthetic vote, signature bypass or new head.

`native_lifecycle_ready_proposal_wins_deadline_without_disabling_timeout` uses
real AOEM execution and a real durable leader proposal. The old executable
failed: ready node 2 signed nil instead of the candidate. The control case
requires a node with no proposal to retain its ordinary nil timeout behavior.
This is a deterministic ordering regression, not a simulated TPS result.

The corrected test passed (0.31 s; old test failed in 0.26 s). Those durations
are fixture execution times, not consensus latency or throughput. The test
also verifies the resulting signature/phase/round and rejects single-vote
finality. Test executable SHA256:
`8eb89e72d480a162af740322c231672d809408e9793ee603c687dd327c43d2a5`.

## Formal comparison and disposition

Windows/AC, four original processes and databases, one native SDK ingress,
65,536 real signatures, batch 1,024, window 8,192, shared recipient, query batch
128. Same library and same frozen load driver; diagnostics disabled. Baseline
was rebuilt from current production source, not the earlier e7e41e50 binary.
Only the deadline ordering differs in the production candidate. See
[exact identities and every result](measurements.json).

| Mode/round | Before TPS | After TPS | Before P95/P99 ms | After P95/P99 ms |
| --- | ---: | ---: | ---: | ---: |
| CPU 1 | 7684.10 | 7723.66 | 1081.878 / 1317.214 | 1071.296 / 1267.724 |
| CPU 2 | 7719.28 | 7602.26 | 1079.248 / 1262.547 | 1148.503 / 1343.947 |
| Integer 1 | 3634.71 | 3611.42 | 2290.924 / 2399.381 | 2319.107 / 2387.749 |
| Integer 2 | 3588.89 | 3564.92 | 2295.380 / 2441.488 | 2354.889 / 2410.042 |

Total successes / total measured time: CPU 7701.648 -> 7662.478 (-0.51%);
integer 3611.655 -> 3588.018 (-0.65%). No throughput improvement or general
tail-latency improvement is claimed. Retain the proven deadline correctness
fix, not a performance win. All eight runs: 65,536 successful, zero business
failures, receipt fees 2,949,120, balance/fee conservation and cold recovery pass.
First-height observations span 326.379–470.586 ms; both versions failed to
reproduce the old 9.91 s event. This is not proof that every cause is eliminated.
Initial readiness statuses in all runs were round 0 (before APFL preparation).

Additional passes: CPU/algebraic original lifecycle three-height tests; existing
A/A/B four-store/same-process progress recovery; load observer/count/percentile
contracts; consensus ordinary suite 45 passed, 25 ignored (ignored is not passed).
The older `native_application_apfl_fees_failure_and_cold_restart_without_http_submission`
fixture failed before node start with `HTTP fixture requires historical-http`:
its setup still selects historical configuration. It is NOT recorded as passed
and no HTTP listener was enabled to work around it. Formal native load succeeded.
The first runner's post-run report reader mishandled the Windows extended-path
prefix; that completed baseline result was retained, not rerun or discarded.

All first-submission waiting remains inside TPS/P95/P99; no warm-up transaction
is removed. Complete fee-state audit and business proof remain outside this
load's acceptance scope. The previous combined GPU candidate remains withdrawn;
no AOEM source or bundled SDK was changed in this slice.
