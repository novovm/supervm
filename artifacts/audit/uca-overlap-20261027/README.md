# Bounded UCA signature / authority overlap — local candidate

Raw success AND failure logs are preserved byte-for-byte in
`raw-validation-logs.tar.gz` (hash in `checkpoint.json`), not edited to remove
the original runner's blank lines or failed assertions.

This knife extends the EXISTING original admission owner. It does not change
ML-DSA-65, UCA policy, candidate atomic effects, quorum, body limits, I/O request
limits, publication authority, AOEM library or Auto. It does not enable UCA
successor execution. No deployment or cloud CI is performed here.

## Before / after identity

Before is the first bulk-capture node, NOT the pre-bulk sequential-key node:
`baseline-novovm-node.exe`, SHA-256
`a6bad4238df21e83c69e32f15a657153386b746ed56d117d95ee226b41deedfd`.
AOEM is the same explicit isolated candidate in both groups:
`8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671`.
The development branch remains `dev/mldsa65-identity-closure` based on
`d17ab2dace41761b34c1856b711b1fa650d0ca77`; main and shipped libraries are not
modified. These are working-tree candidates, not release qualifications.

## Reuse and actual change

- `pipeline/admission.rs` output hands scheduling credit to an authority phase
  under the original `Usage` mutex. The immutable source-byte charge stays live
  until the final permit Arc drops. Maximum one authority output plus one active
  signature output; the existing maximum candidate reservation remains intact.
- `service/rpc/admission.rs` polls the oldest exact-parent authority capture and
  one next resident signature job. Completed results/errors wait FIFO; only
  immutable signature facts are reused. The new authority batch rebinds to the
  full CURRENT `ParentPoint`, not the previous batch's authorization conclusion.
- Whole-body selection remains gated. A fully prepared UCA source moves into
  its own work; old completion cannot clear a newer incoming source. A cached
  final chunk cannot release an unfinished older authority prefix.
- Event counters report accepted/completed signature jobs while capture remains
  pending and maximum retained stages. They are local scheduling observations,
  not finality evidence or total-memory measurements.

## Validation and timing boundaries

Run only one build/performance slot. Before formal timing, verify AC power; the live
Windows power API reported `ACLineStatus=1`, 100 percent battery at preparation.

Reuse the original same-corpus five-run harness: before CPU, after CPU, before
GPU-required, after GPU-required, before CPU repeat. Business execution remains
`cpu_callbacks`; only signature backend changes within each before/after pair.
Include first native submission and first-height wait through ALL FOUR receipt
query observations. Startup, key generation, signing, untimed economics and cold
recovery are outside that stopwatch, identically for both versions. All original
fee, nonce, grant, balance, receipt and cold-reopen checks remain mandatory.

1,024 / 4,096 workloads remain bounded static-intent fixtures. No 65,536 workload,
multi-host, release or restored-12K claim follows. Diagnostics run separately.
After overlap, sequential uncorrelated SignatureDone/AuthorityDone events cannot
be paired into batch durations. Use the capture's own start/finish measurements
and scheduling counters; never sum overlapping stage spans as wall time.

## Measured results — NOT a successful performance qualification

New production-node SHA-256:
`fd1f4166692483ab8571de032008baa75b7ae5e0233a894739807af6da3cbcd7`.
Measured test-client SHA-256:
`7a830c597410e6b6d076a6d69f9c95d8ff2f2eb085de69aae2af120a01a23307`.
Build-time tracked source delta:
`9f386b00d934321812e1e529e3297425aec5f7c8e337b745a35c4798f0c51132`;
the separate untracked-source archive is also retained. Later documentation
does not retroactively alter the measured binary identities.

Same corpus within EACH size's five-round series, diagnostics OFF:

| Transactions | Round | TPS | P95 ms | P99 ms |
|---:|---|---:|---:|---:|
| 1024 | before CPU | 1063.76 | 592.22 | 597.76 |
| 1024 | after CPU | 961.66 | 716.33 | 718.47 |
| 1024 | before GPU-required | 214.50 | 4156.06 | 4255.86 |
| 1024 | after GPU-required | 427.76 | 1728.65 | 1730.66 |
| 1024 | before CPU repeat | 978.11 | 695.12 | 714.04 |
| 4096 | before CPU | 130.99 | 29160.83 | 29179.55 |
| 4096 | after CPU | 301.14 | 11633.78 | 11653.86 |
| 4096 | before GPU-required | 165.77 | 21687.23 | 21708.78 |
| 4096 | after GPU-required | 146.27 | 25170.17 | 25193.85 |
| 4096 | before CPU repeat | 110.91 | 35040.95 | 35054.52 |

All ten rounds finalized the expected funded effects on all four nodes, audited
all fee pages/authorizations/nonces/balances, and passed original-store cold
reopen. All slow samples are retained. CPU 1024 and GPU 4096 are NEGATIVE
observations, not noise declared away. GPU 1024 first-height observations were
3640.85 ms before / 1341.26 ms after; this variability forbids attributing the
entire TPS difference to overlap. The very slow 4096 cases also happened in the
unchanged before executable; their root cause is NOT established by this knife.
There were 672642 / 278267 / 527935 / 561484 / 786861 observation requests in
that series. Query counts are observed feedback under a delayed workload, not
proof that querying itself caused the original delay. No observation latency
has been removed, and no internal-finality substitute is reported.

Separate diagnostic 4096 CPU / GPU-required / CPU series (DIFFERENT corpus,
not interchangeable with headline measurements): 1117.60 / 590.89 / 1062.50
TPS, P99 1505.04 / 4418.09 / 1880.76 ms. All economics/cold checks passed and
all four final progress snapshots matched. At ingress node1, each round recorded
127 accepted NEXT signature jobs and 127 completions while the prior authority
capture was pending; every node's maximum retained stage count was two.
Ingress capture attempts were 164 / 153 / 153, parent restarts 36 / 25 / 25,
node-batch I/O requests 2117 / 2090 / 2053. Mean capture-attempt duration was
14.210 / 14.051 / 14.366 ms. These are capture-local spans, not additive wall
shares or physical disk-read counts. They show actual overlap and substantial
remaining capture work, NOT restored 12K or a stable throughput improvement.

## Local validation

- Release/all-target strict Clippy and workspace formatting passed after the
  final source changes. Default native pipeline: 459 passed / 75 ignored.
- Admission/permit filter: 14 passed / 2 manual AOEM gates ignored. The four new
  permit tests are included in both this count and the 459; do not add them twice.
- Explicit GPU-required original-service test passed actual ML-DSA source
  slicing, uncached-prefix/cached-tail deferral, newer-source preservation,
  bad-signature rejection, max-two-stage observation and no consensus advancement.
  It then reopened the SAME genesis databases and passed the existing real UDP
  transfer, recipient respend, fee/nonce/budget checks and cold equality.
  This boundary subcase is admission-only, not itself network/finality proof.
- Real signature reuse plus current-root revoke/budget/nonce/balance recapture
  passed. Explicit GPU-required four-process fault recovery passed in 131.56s:
  healthy three reached height40, late start after106413ms, catch-up killed at
  durable height1 with target40 archive absent, SAME original store/outbox resumed,
  all four final states agreed and cold reopen passed. Evidence is in
  `process-recovery-report.json`; this is not a new TPS or multi-host result.

Two NEW TEST-FIXTURE failures are retained: a missing encoding-error conversion
failed compilation, and an oversized three-row retained source correctly failed
the original small fixture's two-row byte budget (the hook then timed out).
The corrected fixture REDUCES its control-side quantum to one while leaving the
original pipeline/native byte budgets unchanged; it replays a two-row source.
Both strict checks and the real test were rerun. These are not historical debts
and no production capacity limit was raised to accept the fixture.

The real subcase does not force a whole-call Completed(error) while authority is
pending; that bookkeeping path has the pure helper/credit tests and source review,
not an injected resident-driver disconnect acceptance. No broad fault claim is
made for it.

Additional checkpoint coverage: CI scope fixtures 13 passed. Full network library
default-parallel run: 868 passed / 6 failed / 11 ignored; the SAME executable
serial diagnostic: 874 passed / 0 failed / 11 ignored. The six failures are in
unchanged `runtime_status.rs` / `transport.rs` (one native pending-history and
five RLPX/EVM tests). This is consistent with isolation/timing interference but
does NOT establish its root cause or fix the default-parallel gate. It remains
OPEN; serial success does not replace the failed run. No production/test deadline
or assertion was changed to hide it. The development checkpoint is NOT main/CI
qualification. The earlier bounded handshake draft remains transport scaffolding,
not ML-DSA transport authentication; its resource COMMENT was corrected to include
locally reserved routes after measurement (no runtime logic change).

## Next separate successor knife

The active owner is `consensus/bodies/successor.rs`, not the historical
controller. Both service and body guards remain. Existing UCA effects already
produce grant budget, balance, nonce and fees atomically. Reuse durable-parent
root capture and original `PostStateSeed` / `SeedCache` binding when adapting
`UcaBody`; do not infer a complete authority overlay from receipt nonce fields,
delete guards alone or introduce a second account-state owner.
