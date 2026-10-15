# Original native tick: bounded successor overlap

Input `f29e551945709a5ed3c920bc22654fd9034a7bd2`. The containing commit
records the implementation; this is not a new controller or execution mode.

## Actual change and limits

Original native SDK/APFL -> original tick -> shared service selection ->
CandidateBodies/NativeLifecycle -> existing CandidatePipeline -> exec/bindings/
AOEM -> original RoundProgress/ValidatorJournal. The same CPU and complete
integer-plan backends remain available. No HTTP listener or AOEM DLL change.

The next scheduled proposer can authenticate and compute ONE private successor
against its locally executed, durable parent candidate. It reuses the existing
owned authentication/background submission/seed mechanisms. Selection overlays
actual receipt nonces on the exact published projection; pending inputs and
published state are not advanced. Full parent point, source context and source
round are bound. Only the existing journal ACK for that exact parent and next
context permits promotion into ordinary current-height work. Reuse the same
ticket/completion; peers still independently execute the original proposed body.

Current work retains priority: optional authentication AND execution count as
inflight, reserve one foreground slot, charge bytes and the read-only receipt
nonce projection. Stale source work is retired and its accepted tickets drained;
unknown storage outcomes freeze the existing owner. No new future-body wire,
second signer, consensus rule, authority head, ledger or unbounded queue.

This is local next-proposer overlap, not all-peer speculative execution. It does
not remove all serial economic work or all network/publication/observer waits.
Existing `received` remains volatile admission, not durable receipt.

## Frozen programs and conditions

- Baseline node SHA256: `4f0d01cad009d66c4eb59def416e76950655c970f736ff407f885a1b0ae994f2`.
- Candidate node SHA256: `3b6ee279e247fdefff237d23cc6ba189c5d3abde80293a3c9b6c75df1298623b`.
- UNCHANGED driver SHA256: `223b37a9931b7e6479fe127b4ad49e68a00db008d458a358298121a594c2dc7b`.
- UNCHANGED AOEM SHA256: `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.

Plugged-in Windows, one machine/four independent original node processes,
CPU callbacks, four AOEM semantic workers per node, single native submission
entry (not first proposer), 65,536 real presigned transfers, 1,024 senders x64
nonces, one shared recipient, batch1,024/window8,192/query128/IO64. Formal
diagnostics OFF. Only one build/performance slot; no competing agent builds.
Timing begins at first native submission attempt and ends at all four matching
final receipt replies for every transaction. Signature preparation, startup,
post-run balance audit and cold restart excluded. Same600s deadline/quorum/
durability/fee rules. No new window or observation shortcut.

## All formal rounds (A/B/A/B)

| sample | successful TPS | seconds | P95 seconds | P99 seconds |
| --- | ---: | ---: | ---: | ---: |
| [baseline1](baseline-1.json) |6004.221535|10.914987|1.393624|1.568041|
| [candidate1](candidate-1.json) |6982.366159|9.385930|1.204816|1.436294|
| [baseline2](baseline-2.json) |6164.418094|10.631336|1.367922|1.573156|
| [candidate2](candidate-2.json) |6997.673777|9.365398|1.205366|1.428708|

Combined successful count / combined time: **6083.265344 -> 6990.011587 TPS
(+14.905584%)**. Both pairs improve TPS and tails; this is two finite local
pairs, not stable capacity or recovery of historical9K/July hundred-thousand
performance. Do not directly compare this native observer with old HTTP runs.
P95/P99 include submission-window and query-observation scheduling, not pure
consensus latency. No aggregate percentile is invented from per-run percentiles.

Every formal round:65,536 successful,0 business failures,0 transport failures,
0 deadline expirations; every final receipt's fee/nonce checked on four nodes,
balances and amount/receipt-fee conservation, matching heads and cold recovery
PASS. Baseline1/candidate1 full reported heads, balances and recovered statuses
also match exactly. Full persisted fee-state audit and business validity proof
remain false; this workload does not itself exercise business failures.

## Separate diagnostic (not a formal performance sample)

[diagnostic](diagnostic.json), node snapshots [0](diagnostic-node-0.json),
[1](diagnostic-node-1.json), [2](diagnostic-node-2.json), [3](diagnostic-node-3.json).
Actual four-process totals:63 early authentications,35 execution submissions
before parent ACK,63 promotions,0 discards. Every node executed64 batches and
published64 decisions. **Zero** successors completed before parent ACK in this
load; do not claim complete hiding of child compute latency. Early completion
before ACK is instead exercised by the explicitly held-parent correctness test.
Diagnostics are local only, absent from business roots/signatures.

## Correctness and remaining acceptance

- Release pipeline regression:360 PASS /45 ignored before added drain tests.
- Actual AOEM held-parent/exact-ACK/restart and single-inflight tests:2 PASS.
  Four sockets/stores in one process; real quorum progression, no fabricated QC.
- Actual four-process original native query/transaction/genesis/minority/replay/
  pending-signature-conflict/recovery tests: CPU2 PASS (2.29s), integer-plan2
  PASS (7.27s), using the measured candidate node.
- All four successor tests PASS (1.13s) after the added accepted-auth/accepted-
  execution source-round retirement cases. Controlled peers sign their own
  higher-round votes; the real collector/journal durably changes round. Existing
  tickets drain, a fresh background permit becomes available, no child is
  registered/published, and repeated empty polls cannot revive work. These are
  bounded fault injection, not complete network competing-parent/A/A/B tests.
- CI runs these four ignored-by-default real-AOEM tests explicitly, with test
  name assertions to reject an empty filter. Linux result remains separate.
- Node/protocol all-target Clippy `-D warnings`, workspace fmt check and scoped
  diff check PASS. Final test binary SHA256:
  `183dd59d9989b7dc893e61ff466018caf9513da6554fe209efc5c886ad8c9d0f`.
  Candidate programs retained in `target/frozen-native-lookahead-20261016/`.

Raw local stores/observations remain in `target/native-product-load/` under
`800-1792099975791493100`, `22728-1792100696693112900`,
`19268-1792100733746082400`, `27000-1792100761797452300`,
`3864-1792100788021281500`. No database, private test key or configuration is
copied here. Baseline executable retained in
`target/frozen-native-lookahead-baseline-20261016/`.

Not signed off: complete competing-parent/network A/A/B on this integration,
hard-crash unknown-write injection here, multi-machine capacity, stable9K/100K/
million TPS, GPU speedup, business proof, privacy/PQ release or production.
Outstanding rustls advisory and final commit CI remain release gates.

## Shared-main integration

Implementation commit `192d483b`; merge `1063c471` preserves remote `f4d6760a`
and `7075b765`'s feature-gated bound-carrier codec. Both machines' doc sections
were retained. No conflicting production source hunks. The default mainchain
release build was checked after merging; its executable remains EXACTLY
`3b6ee279...` as measured above (no timestamp-only extrapolation). Default
NovoRUDP regression99/99 PASS0.77s on merged source. Optional Iroh/TLS results
remain the other commit's recorded evidence, not a claimed local rerun here.
