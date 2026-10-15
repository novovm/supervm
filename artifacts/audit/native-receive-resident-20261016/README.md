# Original native tick: reuse the existing receive buffers

Input `17c9479cce2452ef718b05b669ddcbcc03a42b19`. Same original native
SDK/APFL -> original tick/UDP owner -> node/exec/bindings/AOEM -> durable
RoundProgress/ValidatorJournal publication. No new transport, execution path,
AOEM library, consensus rule, ACK policy or HTTP listener.

## Evidence-directed scope

An initial hypothesis (repeated candidate registration) measured only54–58ms
per node, so no candidate-validation cache was introduced. Broader diagnostic
spans located much more work in the existing native I/O poll, not registration.
Existing `UdpTransport::recv_buf` was used only to obtain a length, then every
receive attempt allocated/zeroed a new full buffer even on WouldBlock. Endpoint
also zeroed64KiB on every poll. Both owners now reuse their initialized receive
scratch; the original bounded drain queue owns only the actual received bytes.

Exact length/source, AEAD, replay, handshake, drain budget, queues, windows,
timer/quorum/durability and ownership remain. Every Result error restores the
endpoint scratch. A poisoned original receive-buffer lock now fails explicitly
instead of allocating an unowned fallback. No unsafe/uninitialized memory.
The unchanged receiver still copies successful datagrams into bounded custody;
this is not a zero-copy or no-allocation claim. Endpoint retains64KiB per owner,
replacing its transient stack buffer; no measured peak-memory claim.

## Diagnostic boundaries

The saved progress snapshots are periodic, NOT final profiling totals:
baseline height63 (63/64 executions), candidate height33 (33/34 executions).
Do not divide their accumulated times to claim a whole-run speedup. The default
snapshot interval was5s. Native I/O mean inclusive wall time per poll was about
6.7–7.5us baseline versus3.6–4.2us candidate in these snapshots. OS scheduling,
different poll/traffic distributions and nested timings remain included.
No addition of phase times or claim that all end-to-end waiting was removed.

The baseline diagnostic binary is `eb4d9f50...`; candidate diagnostic was built
before formatting-only source changes. Their exact SHA256 and AOEM hashes are
in the reports. Formal, diagnostics-off results below use the final build.

By source and pointer/length tests: no new full scratch allocation or clearing
on an empty poll; each successful retained Vec has only its received length.
Allocation totals and hardware memory bandwidth were NOT independently profiled.
The initial larger cumulative reduction was corrected after checking snapshot
heights; this document does not repeat it as a comparable full-run result.

## Frozen comparison and validation

Baseline node `3b6ee279e247fdefff237d23cc6ba189c5d3abde80293a3c9b6c75df1298623b`.
Unchanged driver `223b37a9931b7e6479fe127b4ad49e68a00db008d458a358298121a594c2dc7b`.
Unchanged AOEM `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.
Plugged-in Windows, four original processes, CPU4workers/node, single native
submission,65536 real presigned transfers,1024 senders x64 nonces/shared
recipient,batch1024/window8192/query128/IO64. Same final four-node receipt
observation, economics and cold restart. Formal diagnostics disabled; one build/
performance slot. Signature generation, startup and post-run audits excluded
from TPS; no pending/rejected work counted as successful transactions.

Three new release tests PASS: exact long/short/empty UDP payloads with both
drain policies; scratch retained through WouldBlock/Err/invalid quantum and
oversized length rejection; real authenticated long/short frames see only their
received prefix. Existing NovoRUDP87 and original-ingress3 tests PASS, including
loss/reorder/replay/source/session/resource bounds. No live network deployment.

## Formal results (A/B/A/B, diagnostics OFF)

Candidate node SHA256:
`4c752b0c245af9d1f303405d33cbdd26db9503233a93956e4c85220ac5b4a4fa`.
Frozen executable: `target/frozen-native-receive-20261016/novovm-node.exe`.

| sample | success TPS | seconds | P95 seconds | P99 seconds |
| --- | ---: | ---: | ---: | ---: |
| [baseline1](baseline-1.json) |6838.547931|9.583321|1.312958|1.492974|
| [candidate1](candidate-1.json) |7297.666419|8.980405|1.136298|1.295532|
| [baseline2](baseline-2.json) |7089.748014|9.243770|1.178395|1.419641|
| [candidate2](candidate-2.json) |7338.649046|8.930254|1.135824|1.277416|

Combined success count / combined elapsed: **6961.882747 ->7318.100356 TPS,
+5.116685%**. Both pairs improved TPS and tails, but these are finite local
samples, not stable capacity or the performance ceiling. Do not pool percentiles.
All four runs65536 successful,0 business failures/nonce rejection/transport
failures/deadlines; four matching final receipt fees/nonces, balances, cold
recovery PASS. Baseline1/candidate1 complete reported heads, balances and
recovered statuses also match. This normal load does not test business failures;
full persisted fee-state audit and business validity proof remain false.

Query requests changed77127/74883 ->43385/42722 despite unchanged driver and
observer contract. Pending observations are NOT successful TPS. Changed queue/
poll scheduling can change query work, so do not attribute all end-to-end gain
to raw memory clearing alone or claim the observer is the only bottleneck.

Original local report directories under `target/native-product-load/`:
`984-1792101941183960100`, `28504-1792102381764832800`,
`28012-1792102405061889000`, `5952-1792102436788028700`.
Diagnostics: `27116-1792101691042768600`, `28244-1792101877365730500`,
`19052-1792102176794411300`. No databases/private keys copied into this audit.

This checkpoint does not sign off historical9K/100K/million capacity, GPU
speedup, full business proof or production. Same-path pipeline overlap,
publication, observer and CPU/GPU structure costs still require further work.
## Product regression and known failures

- Release native_pipeline suite:360 PASS/47 ignored,1.16s. Initial run caught
  the diagnostic enum's old last-phase/count assertion (27 instead of32); only
  that test was corrected, then the whole suite rerun. No business assertion
  removed or validation threshold changed.
- Same candidate binary, real four original processes, CPU native-only2 PASS
  (2.06s), complete algebraic backend `aoem_integer_v7`2 PASS (7.22s): genesis,
  native queries, signatures/replay,2/4 minority no-publication and cold recovery.
  One attempt used invalid test backend name `integer_program` and rejected
  immediately,0 executions; corrected to the existing supported name and rerun.
- Four real-AOEM successor/ACK/source-round drain regressions PASS (1.15s).
  They do not replace the still-outstanding combined real-network A/A/B case.
- Test executable SHA256:
  `0c6949c1b330d666e04ed05a2cb34d3b3ac181aebde4604f4ca496155436c2ff`.
  Retained alongside the measured node in `target/frozen-native-receive-20261016/`.
- Network/node all-target Clippy `-D warnings`, workspace fmt check and scoped
  diff check PASS. Shared-main artifact verification follows integration.

The prior17c9479c CI38813464884 was cancelled; its Rust security job failed
Cargo audit for rustls0.23.45 advisories0346–0349. This is an unresolved release
gate, not a native performance result. No advisory ignore or security downgrade
was added. Remote NOVOCHAT carrier changes are integrated separately; no mobile
runtime deployment or acceptance is claimed here.

## Shared-main integration verification

Implementation7615369d merged remote fa58bf4b at7a02c7a6. Kept the remote
carrier admission/reset semantics and initialized the same scratch in its new
from_bound_channel constructor. No carrier/mobile functionality was removed.
TLS-feature release NovoRUDP88 tests PASS; completed_bound4 tests PASS,
including actual endpoint adoption and reset propagation.

Final rebuilt node SHA256
`f46d0c6157e2f4e926ea7d49a384730160f14dde616cd990583806f617c1f808`.
Same frozen driver/AOEM, plugged-in, diagnostics off: `merged-final.json`
records65536 successes/0 business failures,8.961309s,7313.217299 successfulTPS,
P95/P99 1.128376/1.271884s,42763 query requests. Four final receipts, reported
fees/nonces/balances/heads and cold recovery PASS. This final-build sample is
separate from the pre-merge paired performance claim, not a new capacity result.
Source report: target/native-product-load/15832-1792103166244265900/report.json.
Same final node actual CPU2 tests PASS2.06s and algebraic2 tests PASS7.32s,
using the preserved test executable above. Security advisory remains unresolved.
