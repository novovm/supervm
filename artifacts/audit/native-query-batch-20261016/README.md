# Original native query handoff and bounded receipt batches

Source input `dcc6d1b6`; the containing commit records the implementation diff.
Same original tick/UdpTransport/NativeLifecycle, CandidatePipeline, AOEM and
RoundProgress/ValidatorJournal. No new execution engine, signer or authority.

## What changed

- Original idle hint now sees actual native sends, fresh fragments, completion,
  session changes and query custody transitions. Pending queues, rejected or
  duplicate frames and feedback alone cannot keep it busy. The original64-turn/
  250us grace and1ms idle sleep, query5s deadline, quorum and durability remain.
- Same `NVQUERY1` stream now supports explicit `NVQUERY2` receipt batches of
  1..128 ordered hashes. Single v1 bytes and1KiB wire limit stay unchanged.
  Batch length is checked before allocating; response count/type and full echoed
  request/session/sequence/hash order are verified. Full u128 fee width retained.
- Query lane capacity is explicitly8 bounded objects,1 per peer,up to32KiB each
  (256KiB aggregate per receive/send direction), not the old1KiB byte budget.
  This carries bounded larger responses, not more in-flight requests. The SDK
  still owns at most one pending request/session and never treats DONE as finality.
- Product service answers from its existing verified receipt projection. During
  recovery/unavailable projection it still rejects reads rather than fabricating
  finality. No JSON on this result wire; no balance writes or consensus changes.

## Frozen artifacts and conditions

Plugged-in Windows, four independent original nodes on one machine, CPU
callbacks, AOEM semantic workers4/node. Same DLL and strict manifest as baseline.
65536 real presigned transfers,1024 senders x64 nonces, shared recipient,
1024 execution batch,8192 submission window, IO quantum64. No diagnostics.
Start: first submission attempt. End: every transaction's matching finalized
receipt from ALL FOUR nodes. Startup/signature preparation and subsequent
balance audit/cold restart are excluded from TPS. Every row still verifies
fee/nonce/nonzero height/block hash and four-node equality; no reject inflation.

- Node SHA256 `526f0e13a1c7737be237fd06c55885fdf21511d7a43416213d4324e05055a660`.
- AOEM SHA256 `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.
- New test driver SHA256 `223b37a9931b7e6479fe127b4ad49e68a00db008d458a358298121a594c2dc7b`.
- Single1 deliberately uses the UNCHANGED frozen baseline driver
  `365b76fbaeb700a64dd6746e5c3900cf4bdd492adf8bc47843f544db82170ca8`.
  Single2 and both batches use the new driver (separate per-row counts added).
- Binaries preserved at `target/frozen-native-batch-20261016`; originals remain
  at `target/frozen-native-single-20261016`. Reports below bind exact identities.

## Results (all rounds retained)

| sample | observation batch | successful TPS | seconds | P95/P99 seconds | cold receipt seconds |
| --- | ---: | ---: | ---: | --- | ---: |
| [baseline](../july-main-tick-20261016/release-single-query-baseline.json) |1|219.127|299.077304|37.102617/37.391049|305.766176|
| [single1](single-1.json) |1|5969.458|10.978552|1.377163/1.660356|8.003440|
| [single2](single-2.json) |1|5919.700|11.070832|1.390981/1.658951|8.013099|
| [batch1](batch-1.json) |128|6142.435|10.669385|1.369263/1.556837|0.324198|
| [batch2](batch-2.json) |128|6177.299|10.609167|1.384407/1.554588|0.327660|

Every run:65536 successful,0 business failures,0 deadline expirations,0 transport
failures; matching four heads/receipts, balances, fee conservation, nonce sequence
and cold recovery passed. Full persisted fee-state audit and business proof remain
false. Aggregate single5944.474 /batch6159.818 TPS use total successful/total time.
This is a finite local result, not stable capacity, recovered historical9K or
hundred-thousand TPS. Batch observation is explicitly a different observation
configuration; do not describe its gain as faster AOEM computation.

Cold observation isolates querying existing receipts, not execution. Batch cold
needs2048 requests rather than262144. Timed batch runs still issue86677/87204
requests and observe10832512/10899968 not-final rows: eager polling remains, so
do NOT present2048 as the actual full-load request count. No confirmations are
dropped or counted early. Existing `received` remains volatile admission.

Full raw observations/DBs remain at `target/native-product-load/`:
`2268-1792098824729200200`, `18508-1792098923696196700`,
`19408-1792098872568330100`, `11896-1792098951130534700`.
The original failed-performance baseline is `10920-1792097520681486100`.

## Correctness / build

- Protocol5 tests pass: old byte goldens, maximal128/full-width, count before
  element decoding/allocation, wrong type/count/version/trailing rejection.
- Release native pipeline354 passed/43 ignored. New scheduling tests reject
  false progress; observer regression requires all4 nodes and no duplicate count.
- Actual release original-node CPU2 cases2.33s and algebraic2 cases7.52s pass:
  [CPU closure](cpu-closure.json),[minority](cpu-minority.json),
  [algebraic closure](algebraic-closure.json),[minority](algebraic-minority.json).
  These now compare max128 batch responses with single receipts, reversed order,
  duplicates and unknown hash, before/after cold recovery. No physical GPU/TPS
  claim from these small functional tests.
- Node lib/tests/bin check and node/protocol all-target Clippy `-D warnings`
  pass. Three previous CI test-only Clippy errors corrected without weakening
  assertions. Protocol codec negatives are not hostile-client socket acceptance.
- Retained remote mailbox/locator regressions rerun:16 structured mailbox and10
  native endpoint tests pass. No mobile files or other machine's work changed.

## Shared carrier merge verification

`cb4bb9eb` merges the implementation with remote `d99b8b23` typed datagram
provenance. Original UDP APIs/shared receive owner remain active; the carrier
type does not activate TLS/WSS/Iroh on the mainchain. Merged source node check
and 99 NovoRUDP tests pass. Release node rebuilt in 1m45s, SHA256
`4f0d01cad009d66c4eb59def416e76950655c970f736ff407f885a1b0ae994f2`.
The unchanged frozen test/SDK driver is `223b37a9...` identified above; this is
also a compatibility check against that driver, not a rebuilt-client claim.

Same plugged-in conditions, batch128: [raw report](merged-batch-1.json),
65536 successes, zero business failures/transport failures/deadlines,
10.672071s, **6140.888680 TPS**, P95 1.397271s/P99 1.569527s.
All four final receipts, balances, nonce and cold recovery pass; cold receipt
observation 0.330677s. Timed queries88618/not-final rows11080960 remain visible.
This single merged sample does not establish stable capacity or restore 9K.
Actual-process CPU2 cases2.20s and algebraic2 cases7.28s also pass; reports are
`merged-cpu-closure.json`, `merged-cpu-minority.json`,
`merged-algebraic-closure.json`, `merged-algebraic-minority.json`.
No AOEM binary change, physical GPU or business-proof claim.

## Remaining, not hidden by this improvement

Current native selection still waits for published parent/nonce projection;
cross-height early auth and successor overlap are NOT active. Reuse preserved
early_body/successor mechanisms inside existing CandidateBodies/NativeLifecycle,
with one bounded background permit and exact durable parent before promotion.
Current driver already uses `advance_with_seed`/`SeedCache::try_install`; do not
rebuild that capability. Historical Controller remains inactive, not reinstated.
Native A/A/B/offline, full proof, sustained/multi-machine and production remain open.

CI `38806144112` ended cancelled overall with failed Rust checks (3 test Clippy
errors, fixed here) and security audit (rustls0.23.45 advisories0346-0349;
reported fix>=0.23.46, NOT yet upgraded here). Missing legacy artifact upload
followed the early check failure. Do not call CI green or suppress the advisories.
