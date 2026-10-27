# ML-DSA unified consumer candidate — 2026-10-27

## Latest: bounded cross-batch overlap integrated; performance NOT qualified

The original signature output now parks scheduling credit without releasing its
retained-byte charge. Exactly one authority capture can overlap one next signature
job/completed output, under the original total-byte/candidate reservation. FIFO
handoff, fresh full ParentPoint binding, whole-body selection and cached-tail
boundaries remain enforced. No new account, I/O owner, verifier or runtime.

Node SHA-256 `fd1f4166…da3cbcd7`, the same isolated AOEM `8779d014…455671`,
business `cpu_callbacks` fixed. Before is the previous bulk-capture node
`a6bad423…deedfd`, NOT the old per-key implementation. Same-corpus five-run
1024 and 4096 series passed all-four economic/fee/nonce/grant checks and original
store cold recovery. CPU 1024 was **1063.76 → 961.66 TPS**; GPU-required 4096
was **165.77 → 146.27 TPS**. These negative observations are retained. Large
before/after variation and first-GPU wait variation forbid a stable speedup claim.
The unchanged baseline also produced slow 4096 samples; root cause remains open.

Separate diagnostic 4096 CPU/GPU/CPU: **1117.60 / 590.89 / 1062.50 TPS**,
not interchangeable with headline data. Ingress recorded 127 next-signature
submissions AND completions during authority capture in each round; max two
retained stages on every node. Mean capture attempts remain about 14.2 ms, with
25–36 parent restarts. These demonstrate actual overlap, not restored 12K.
The all-four receipt-observation endpoint still includes initial wait; no switch
to three-node completion or internal-finality timestamps was made.

Release/all-target node Clippy, workspace fmt, 459 default native regressions,
real signature/current-root recapture and the real GPU-required service boundary
subcase passed. Four-process recovery also passed: healthy three reached height40,
the late node started after106413 ms, catch-up was killed at durable height1 while
height40 archive remained absent, then the SAME store/outbox restarted and cold
reopen passed. No deployment or 65536 acceptance is claimed. Two new test-fixture
failures (encoding conversion and oversized retained source) were fixed without
widening production budgets; raw failures remain in local evidence.

Both active UCA successor guards stay. Next work must reuse the existing durable
parent root, UCA full effects and SeedCache/Lookahead; receipt nonce alone is not
an authorization/budget overlay. No Ed25519 payment route, Auto change, shipped
AOEM replacement, main advancement or cloud CI was performed.

Full tables, hashes, timing definitions, negative samples and exact limitations:
[local overlap evidence](../artifacts/audit/uca-overlap-20261027/README.md).
Additional full network coverage exposed six default-parallel failures in
unchanged runtime-status/transport modules; the same executable passed 874 tests
serially. This is NOT a resolved parallel gate or a claim of historical debt;
scope fixtures passed13. The checkpoint keeps those failures open.
This supersedes older text below saying cross-batch overlap is still unimplemented;
those sections preserve their historical measurement scope.

## Latest: first UCA bulk-authority capture integrated and locally verified

The original `rpc/admission.rs -> UcaBatchStateRead -> BulkCapture ->
CandidatePipeline -> IoReadClient -> SAME IoService/DB` now captures one trusted
parent for the ordered signature batch. It retains the original signature-output
permit, deduplicates read-only declarations, authenticates frontier replies and
checks every original record/tail through the unchanged UCA decoder. Only after
the whole snapshot completes does the original checker reserve nonces in input
order. ParentPoint changes discard the snapshot and recapture unadmitted rows.
No grant, balance, fee or nonce consumption moved out of the candidate plan.

The old per-row/per-key I/O loop has been removed, including from the one-row
query wrapper. Frontier work is bounded to 64 edges/nodes/keys and 8 row decodes
per poll. Request count, original queue/owner and writer-service discipline are
unchanged. Read reply accounting now covers up to 64 nodes but is capped by the
owner's configured I/O byte budget. Capture and deduplicated value retention each
have explicit CaptureBudget bounds; this is NOT a global heap limit covered by
the older signature reservation. One authority capture is active. Cross-batch
signature overlap and full UCA successor state remain UNIMPLEMENTED; their
protective barriers remain in place.

### Same-corpus before/after product evidence

One Windows host, AC, four real original processes/databases, RTX 5090 Laptop,
same candidate AOEM, same genesis/root/approvals and exact signed inputs within
each five-run series. Business backend stays `cpu_callbacks`; signature policy
is explicit CPU or GPU-required. Order: before CPU, after CPU, before GPU, after
GPU, before CPU repeat. Frame/query batches 32, workers 24, body 384 KiB,
configured rows 1024, intent [1,257], grant expiry 1000 and quorum unchanged.
Headline diagnostics are disabled. First waits and all-four receipt-query
observations remain included; startup/signing/post-run audits remain excluded.
These timings are not internal durable-finality timestamps or peak capacity.

| Transactions / run | Successful TPS | P95 seconds | P99 seconds | First-height observation ms |
|---|---:|---:|---:|---:|
| 1024 before CPU | 200.09 | 4.338 | 4.405 | 97.513 |
| 1024 after CPU | 963.04 | 0.690 | 0.695 | 178.309 |
| 1024 before GPU | 116.43 | 8.086 | 8.149 | 4217.536 |
| 1024 after GPU | 436.90 | 1.654 | 1.709 | 1329.985 |
| 1024 before CPU repeat | 204.15 | 3.974 | 4.187 | 531.071 |
| 4096 before CPU | 221.09 | 14.458 | 14.721 | 101.668 |
| 4096 after CPU | 1129.73 | 1.717 | 1.744 | 133.999 |
| 4096 before GPU | 186.06 | 14.032 | 17.274 | 3554.450 |
| 4096 after GPU | 816.37 | 2.309 | 2.725 | 1287.075 |
| 4096 before CPU repeat | 143.09 | 20.142 | 20.416 | 225.359 |

All ten runs passed four-node receipt, balance, fee archive, authorization-budget,
nonce and original-store cold-recovery assertions. Different block schedules are
audited within each run; independent-run heights/roots are not required equal.
4096 baseline variation and GPU first-wait variation remain negative/uncertain
observations. Do not attribute all GPU wall improvement to capture alone or
claim a stable performance ratio. The 12K historical qualification is NOT
restored, Auto and shipped AOEM are unchanged, and 65,536 is NOT executed.

1024 ordered signed-byte SHA-256:
`03bd175bb26f4fbfddda063ed0098c737bc2fc20d9c1300e0294f01990a975da`.
4096 ordered signed-byte SHA-256:
`1535c07347480fb62cde1cabdf1545982082a44758ec75f180649893529e05d7`.
The 4096 series is a separate workload, never mixed with the 1024 series.

### Capture diagnostics (separate from headline runs)

The separate same-corpus diagnostic CPU/GPU/CPU series completed with zero
dropped critical events. At the submission node, all full frames had 2368
summed declarations and 659 unique keys per capture attempt. First CPU run:
32 completed authority batches, 37 capture attempts including 5 parent restarts,
87616 declared keys / 24383 unique-per-attempt keys, 514 accepted node-batch I/O
requests for 17371 node hashes. Mean full signature-done to authority-done time
was 14.378 ms/batch; capture-only attempt mean 11.765 ms, maximum 21.183 ms.
GPU diagnostic had 35 attempts / 3 restarts / 509 I/O requests; repeated CPU had
37 / 5 / 540. These count logical commands, NOT physical disk reads. The old
2368 point-request count is source-derived for one unchanged-parent full frame,
not a new instrumented physical-read measurement. Original tail checks remain.

### Safety, test fixture and artifact identity

- Final default native-pipeline regression: 453 passed, 75 explicitly ignored.
- Four manual actual-ML-DSA gates passed: authority recapture/revocation/budget/
  nonce/balance; CPU atomic effects; CPU/integer equality; original UDP service
  transfer/respend/cold recovery. The two old effects fixtures initially refused
  a non-shipped DLL BEFORE execution; the default packaged path is retained, and
  explicit isolated test-library selection now requires generator/execution
  canonical paths equal and the original loader/manifest checks. No assertion
  was removed and the SDK was not overwritten.
- Four-process GPU-required recovery passed: healthy three reached 40 heights,
  missing node started after 102764 ms, was killed at durable height 1 before
  height-40 archive existed, restarted original DB/outbox and all four reached
  height 41; fee/nonce/budget/receipt and cold reopen checks passed.
- Strict release/all-targets node Clippy and workspace format passed. No cloud
  CI or real multi-host deployment was performed. AOEM prior full-format and
  experimental RingCT limits remain independent; this round did not edit AOEM.

Before node SHA-256:
`882ba1feab5d1ee3092b40d4712cb1729859cb8f42d9b69dc3e3d10714e07b4e`.
After node SHA-256:
`a6bad4238df21e83c69e32f15a657153386b746ed56d117d95ee226b41deedfd`.
Measured test-client SHA-256:
`2c6723a92c9740b86cb595c566f43d632dd95ad4b64ca266e65c33f59b82e4c0`.
AOEM remains `8779d014...455671` as fully identified below. Raw reports/logs
are copied under `artifacts/audit/uca-bulk-admission-20261027`; no private fixture
key material is exported. This is the first read-organization fix, NOT closure
of the pending overlap/successor work or release approval.

## Current measurement batch (in progress)

The user has deferred Ed25519 payment product work: first qualify the existing
ML-DSA-65 product performance and identify its actual bottleneck. No alternate
payment authority or algorithm downgrade is being implemented.

`CPU / GPU / CPU` below means THREE SEPARATE benchmark runs, not three stages
inside one verification. The GPU-required run retains the complete resident GPU
cryptographic pipeline and compact verdict readback; it does not send GPU
intermediates back to CPU for cryptographic completion. Business execution stays
fixed across all three runs.

The original `native_product_load/uca.rs` now reuses one generated initialization
and exact signed transaction corpus across these three runs. Every run retains
its own database and journal. This is disconnected cfg(test) acceptance replay
with test-generated protected identities and loopback endpoints, never a recipe
for clearing deployed journals or re-signing a retired deployment domain.
The retained 4096-transaction cap and [1,257] intent window are not expanded to
manufacture a 65,536-transaction claim. All-four receipt observations remain the
main endpoint; they are not relabelled as internal durable timestamps.

The 18 default-feature Clippy constructs have been matched against AOEM HEAD;
all existed before this ML-DSA consumer round. Minimal fixes are under validation.
Two private AI trainer enums use one-time session-owned boxes; ABI/wire contracts
and per-operation cryptographic or authorization rules are unchanged. SUPERVM
handshake whitespace was formatted without protocol edits; its full workspace
format check now passes. AOEM-wide formatting still includes unrelated APFL and
vendor differences and is not claimed green.

Status: local integration candidate on `dev/mldsa65-identity-closure`, not a
release, main merge, deployed package or finalized transaction TPS qualification.

## One existing consumer

`UcaSpendVerifierV1 -> OwnedMldsaVerifier -> aoem_execute_ops_wire_v1`
now uses `compute.crypto.mldsa_verify_v1` (opcode 115, AOMV0001), followed by
the existing context-owned `aoem_compute_result_take_v1` / AOMR0001 lifecycle.
No second pool, execution engine, account authority or direct-ABI fallback was added.
Signing and other unmigrated scalar consumers are outside this narrow change.

The original facade frames `0x00 || context_length || context || message` once.
Eligible framed messages retain the aggregate 1 MiB bound; keys/signatures and
candidate-body bounds remain separate. Result identity, sequence, row count,
completion flags, actual backend and valid count are checked. Execution/result
errors fault that owner; invalid signatures remain row-local rejections.
Signature-only evidence never grants trusted-parent spending authority.

The local node configuration now passes `signature_backend` (`auto`, `cpu`,
`gpu_required`) through the existing resident pipeline to its verifier owner.
This is independent of business `execution_backend`; default Auto remains CPU.
Explicit GPU cannot retry on CPU. A library without the new contract fails
closed: the unchanged shipped SDK is NOT qualified by these candidate tests.

## Isolated candidate identity

AOEM source base: `4c57372ad0cc12123df5ca9e163a130039bfb599` plus local candidate
changes. Build: release, default features retained, plus
`mldsa-gpu-vulkan,rocksdb-persistence`. This is not a FULLMAX release claim.

DLL SHA-256:
`c9a98aed02231b089488ceb963d551ec4f1accfdff2c1b51194c1f44c30bd376`.
The isolated manifest requires its hash, ABI 1 and unified/GPU capabilities.
`AOEM_DLL_MANIFEST_REQUIRED=true`; no hash or licensing bypass.
No SDK files, Auto policy, main or A–D services were changed.

## Same-input consumer comparison

Evidence: `artifacts/audit/mldsa-unified-consumer-20261027/compare-*.json`.
One DLL, same signed corpus within each size, 31 samples, alternating ABC/CBA,
RTX 5090 Laptop, Rayon 24, CPU parallel threshold 128, diagnostics disabled.
Key generation/signing and caller input cloning excluded equally; first call
after qualification is retained separately. Inputs are newly generated for each
size invocation, not one shared corpus across every size.

A is direct-ABI CPU diagnostic reference with equivalent canonical preparation,
not a retained product route. Its ABI lacks an actual-backend receipt; CPU is
explicitly configured, not inferred. B/C use the original UCA consumer and assert
actual backend 1/4 and completed row count on every call. B/C also construct the
original signature-only evidence; A does not. Consequently A/B is not a pure
ABI-only cost measurement. None of these timings includes state or finality.

| Rows | A direct CPU P50 µs | B semantic CPU P50 µs | C semantic GPU P50 µs | B P99 µs | C P99 µs |
|---:|---:|---:|---:|---:|---:|
| 1 | 30.4 | 32.6 | 581.0 | 56.8 | 687.8 |
| 8 | 239.2 | 247.7 | 581.5 | 328.5 | 700.5 |
| 16 | 479.4 | 488.2 | 603.5 | 539.0 | 683.0 |
| 32 | 970.6 | 970.1 | 633.7 | 1089.4 | 736.5 |
| 64 | 1923.8 | 1941.7 | 672.5 | 2062.9 | 874.8 |
| 71 | 2129.4 | 2155.7 | 716.9 | 2327.8 | 800.3 |
| 1024 | 5034.7 | 8455.4 | 8067.8 | 10415.0 | 8275.7 |

The original consumer now realizes the GPU benefit around 32–71 rows in this
sample. Tiny batches remain faster on CPU. The 1024-row A/B gap is a negative
observation requiring separate attribution, not an improvement to hide.
31 samples do not establish stable production P99. These are not chain TPS.

## AOEM bounded absorption

The original prepare shader retains the continuous SHAKE state in resident
storage and consumes at most 1024 rate blocks per dispatch. A 4 MiB message
requires 31 dispatches; the 32 MiB upper bound requires at most 241. State costs
272 bytes per row, at most 1,114,112 bytes, with existing packed admission intact.
No CPU cryptographic fallback, intermediate readback or message rehash substitution.

This bounds individual dispatch work, not total batch latency. The synchronous
batch still holds the original owner and returns together: early short-row
return and cross-call fairness remain unimplemented. Long-message wall time did
not improve; do not call this a throughput gain or a complete fairness solution.

## Validation and remaining gates

Actual isolated DLL CPU/GPU contract tests passed for 44/65/87, including framing,
context/message/key/signature tampering, row-format/budget checks, concurrent
owner use, wrong context/request/sequence and double-take rejection.
AOEM product_tests: 25 passed / 5 manual ignored; the new absorption boundary
gate and long-message gate were separately run and passed, including 4 MiB.
This is a filtered current-build count, not the earlier 119-test full scope.

Original node business/recovery results are recorded below. Whole-workspace
format and historical RingCT failures remain separate. No cloud CI was run.
Ed25519 payment product work is deferred by the latest user decision. Existing
component evidence is retained; an alternate payment scheme is not a gate for
this ML-DSA-65 CPU/GPU product comparison.

### Original node results on the candidate DLL

- `actual_native_uca_single_client_respend_and_cold_recovery`: passed once with
  explicit CPU and once with GPU-required. Both use real NovoRUDP/APFL sockets,
  four original services and four databases in one process; transfer, recipient
  respend, invalid signature/authorization rejection, replay and cold state
  equality are checked. These two fixture runs generate new identities and are
  correctness comparisons, not identical-input timing A/B.
- `native_uca_process_late_validator_interrupted_catchup_respend_and_cold_recovery`:
  passed with GPU-required in all four serialized node configurations. Four real
  node processes on ONE Windows machine. Three healthy nodes advanced 40 heights;
  missing node started after 107,302 ms; interrupted at durable height 1 with
  height-40 archive absent; original database restart caught up; all four final
  heights 41; receipt/state/economics and cold reopen checks passed. No database
  copy or wallet reinjection to the laggard. Total test time is NOT a TPS measure.
- Node policy default/invalid-value test passed. Final strict Clippy for
  aoem-bindings, novovm-exec and novovm-node, release/all-targets/no-deps, passed.

Node binary SHA-256:
`0654a40ae7547a412ef150b223b72ca94c7bdd110d24c304316e0059f7748416`.
See `artifacts/audit/mldsa-unified-consumer-20261027/native-summary.json` for
original evidence paths and immutable library/node identities. Test keys stay in
their isolated local fixture directories, not in this document or source archive.

This establishes product wiring and tested recovery, not finalized throughput,
multi-host acceptance, all signature-consumer migration or release qualification.
Next performance gate must freeze one native transaction/genesis corpus for
both signature backends and retain internal durable-confirmation timestamps
separately from client receipt observation. Do not compare different generated
chain fixtures or the old 12K result as if they were an identical-input A/B.

### Final local check limitations

The candidate AOEM release/all-targets/no-deps strict Clippy run with default
features plus `mldsa-gpu-vulkan,rocksdb-persistence` FAILED: 17 library diagnostics
and one additional test diagnostic. They concern AI session lifetimes, enum
sizes, contains/range idioms and an integer-resource test dangling pointer.
See `AOEM/artifacts/mldsa-semantic-consumer-20261027/clippy-full-candidate.log`.
This wider-feature failure must not be replaced by an earlier narrower-feature
green result. The current ML-DSA runtime successes remain separately recorded.

SUPERVM `cargo fmt --all -- --check` also failed on existing working-tree
`novorudp/endpoint.rs` and `handshake_continuation.rs`/tests formatting. Those
files were not modified by this consumer/absorption round; their prior changes
were preserved. No lint suppression, test removal or production-limit change
was used to hide either failure. Whole-package release qualification is pending.

### Follow-up closure (same day)

The previously failed default-feature AOEM strict Clippy command now passes,
including all targets, `mldsa-gpu-vulkan,rocksdb-persistence`, and `--no-deps
-D warnings`. Existing dependency warnings remain outside that claim. Related
AI/integer and product filters passed 126 tests; five product manual gates stayed
ignored. One mistakenly named resource filter matched zero and is not counted.

All 18 original lint constructs were present at AOEM HEAD: 2 causal-session
lifetimes; 3 supervision-mask contains checks; 3 integer version-range idioms;
lifted trainer enum size plus 2 lifetimes; 2 predicate-program lifetimes; 1 SGM
zero-count check; typed-relation trainer enum size plus 2 lifetimes; and 1 test
non-null dangling pointer sentinel. Fixes preserve valid-version sets, mask
rejections, returned-reference ownership, output checks and error propagation.
Only private AI enum storage changes (one allocation per session), not public
layout or per-operation allocation. Related Vulkan all-arm tests passed.

The rebuilt DLL is pinned separately at
`AOEM/artifacts/mldsa-native-paired-20261027/aoem_ffi.dll`, SHA-256
`8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671`.
It does not retroactively inherit the old DLL's performance results. SUPERVM
workspace formatting now passes after formatting the existing handshake files.
AOEM full-workspace formatting still reports pre-existing APFL/vendor/storage
differences; historical RingCT fused experimental failure remains unresolved.

### First same-corpus product measurement — NOT a 12K qualification

Original four processes, one Windows host on AC; 1024 identical signed NUAS
transactions, 16 senders, frame/query batches 32, business `cpu_callbacks` fixed,
Rayon/semantic workers 24, diagnostics disabled; only signature backend changes.
Candidate/body/TTL/quorum and [1,257] intent-window bounds are unchanged.
All three rounds passed all-four economics, authorization, nonce, fee archive
and original-store cold recovery. Ordered signed-byte hash is
`3e608d50a522a94c1cfe06753174839986f70b414dc7e55a4c0d3492ba0d51ac`.

| Independent run | successful TPS | total seconds | P95 ms | P99 ms | blocks | mean candidate rows |
|---|---:|---:|---:|---:|---:|---:|
| CPU first | 223.115 | 4.589554 | 3754.620 | 3875.601 | 31 | 33.03 |
| GPU-required | 170.354 | 6.011016 | 5249.574 | 5411.820 | 26 | 39.38 |
| CPU repeat | 195.854 | 5.228380 | 4616.381 | 4890.627 | 29 | 35.31 |

These are bounded all-four receipt-observation results including first wait, not
internal-finality TPS, steady-state ceilings or 65,536-run acceptance. GPU lost
in this product sample; the 71-row consumer advantage must not override this.
First-height observations were 85.160 / 1315.608 / 786.805 ms respectively.

The historical 12,324 aggregate used 65,536 V3 transfers, 1024 senders, 1024-row
full blocks, frame size 1024 and query batch 128, with an earlier node/AOEM and
signature/authorization contract. The new NUAS body is 5,571,968 signed bytes for
1024 transactions, constrained to at most about 71 candidate rows by the same
384 KiB budget. Current average candidates are only 33–39 rows. Comparing the
reported TPS directly as a controlled algorithm regression would be invalid;
calling the new product performant because the old one reached 12K is equally
invalid. The new-path performance gap remains open.

Separate opt-in GPU diagnostic run (different generated corpus, not a headline
timing sample): all-four/cold checks passed, critical buffers dropped 0 events.
On ingress node0, signature-batch completion to authority-admission completion
took 123.75 ms for the first 32-row input, then about 135.24 ms for the next.
`rpc/admission.rs::poll_uca_admission` indeed retains one current row/read and
starts the next `UcaStateRead` only after admitting the prior row. This identifies
a real serialized authority-read boundary to fix, not proof that it alone
explains the full TPS gap. The first admission span was 3147.83 ms; it includes
qualification/preparation and must not be labelled pure GPU kernel execution.
Diagnostic valid-strict-rows/batch-calls ratios at nodes1–3 were about 6.6–9.9,
not a measured batch histogram. Candidate row lists are archived separately.

Node SHA-256: `882ba1feab5d1ee3092b40d4712cb1729859cb8f42d9b69dc3e3d10714e07b4e`.
Measured client test-exe SHA-256:
`110fc14f29e9139163d23f5b6a33a9776445d3617cee7b18c9290d03587d16df`.
Raw paired report: `target/native-product-load/paired-22880-1793059406611.json`;
diagnostic report: `target/native-product-load/uca-15304-1793059433318164700/report.json`.
No private fixture keys are exported with these reports. Logs live under
`artifacts/audit/mldsa-native-paired-20261027`.

Additional final validation: resource tests 11 passed (6 explicitly ignored),
actual candidate-DLL consumer CPU/GPU contracts 2 passed, new paired comparison
unit 1 passed. Final SUPERVM release/all-targets strict Clippy passes after a
test-only type alias for the protected corpus-key tuple. That alias was added
after measurement; the measured client hash above remains the actual old
execution identity, not a claim of rerun timing for a later client rebuild.
