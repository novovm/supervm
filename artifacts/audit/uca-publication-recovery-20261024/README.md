# UCA initialization, original pipeline and journal checkpoint

Input: `2d504f07f5cc8def832825f6ca995beebe0829c5`, clean `main` at start.
Windows x64, Rust 1.94.0, packaged AOEM Windows core. No deployed chain, key,
data directory, mobile application or AOEM sibling source was modified.

## Reuse and actual change

The prior UCA implementation ended at `NovTransferInput::capture_uca` and a
candidate's combined economic effects. This checkpoint replaces that test-only
capture handoff with a typed input to the ORIGINAL `CandidatePipeline`:

`BatchRequest::from_uca -> original bounded compute owner -> exec ML-DSA-65
verifier -> NovTransferBody::prepare_uca -> bind -> original incremental
BatchPlan capture/I/O -> same complete NOV effects -> original packet/store ->
ValidatorJournal -> ArchiveRead -> same state-tree queries`.

The synchronous `capture_uca` convenience delegates to that same implementation;
there is no copied economic algorithm, second queue/controller/ledger or new
listener. NUAS and locator hints count against the original admission budget.
Hints do not grant permission: both account binding and reverse owner must match
the same captured parent. Each pipeline independently verifies signatures.
ML-DSA has no Ed25519 fallback. Its existing synchronous ABI does not provide
cancellation; the pipeline compute timeout is not a new hard ML-DSA deadline.

Original V3 CPU reference remains explicitly compatible and unchanged. UCA
CPU and integer backends use the prior full effects contract; this checkpoint
does not claim new GPU scheduling or throughput improvements.

## Initialization contract and cycle resolution

`service/config/uca_genesis.rs` compiles a separately approved NEW-domain
manifest. It reuses the original validator/policy/allocation validation,
UcaAccount/PersonaBinding, grant/nonce/page codecs and SHA-256 tree. It is NOT
activated in startup configuration and cannot mutate/register on an existing
chain. Manifest input alone is not permission to deploy a genesis.

- The signing domain commits to a canonical complete manifest, excluding the
  resulting state root and signatures. Accounts, grants and allocations are
  normalized; suite, purpose, policy, program and receipt pins are included.
- An independently expected domain must match. EVERY initial management key
  must sign the full manifest through existing exec/AOEM ML-DSA-65. Management
  and spending keys are separate. A spending-key signature cannot initialize
  its own permission. Missing approvals and changed-policy/grant approvals fail.
- Management signature bytes are randomized evidence, not deterministic state.
  After actual verification the new `GenesisInitializationV1` record contains a
  versioned manifest/key commitment, explicitly NOT a signature. Public original
  approvals remain initialization evidence; the runtime test saves manifest and
  approvals alongside its isolated databases, never secret keys.
- The final commitment binds domain and initial root. Explicit activation must
  pin this final commitment/root in service install/reopen configuration; that
  startup work is NOT claimed here. Tests use a trusted initial-root anchor in
  the original journal.
- The 32-account/8-grant and other bounds are this initialization compiler's
  local resource limits, not a lifetime chain-user capacity limit. Test funding
  is 1,000 smallest units, NOT the approved mainnet allocation or issuance.
- Appended MLDSA65/initialization proof tags are not relabelled MLDSA87. Legacy
  management paths explicitly reject these new tags until actually integrated.

## Original persistence contract

`NVCAND01` is retained for its existing V3 profile; `NVCAND02` binds the UCA
program/receipt profile and the original receipt commitment's authorization
code/remaining-budget suffix. The same store, immutable content and journal
are used. Exact NUAS/domain/nonce, output counts/codes, receipt continuity,
fee cap, success/failure balance effects and budget prefixes are checked on
load. Output bytes count against the existing packet budget.

Archive content validation is NOT re-execution, cryptographic verification,
trusted-parent authorization or business ZK. The existing pipeline completion
brand and journal quorum/parent checks still authorize publication. Old proof
relations explicitly refuse the new UCA contract.

## Tests and evidence

Final release test executable SHA-256:
`62ea9ea4986bdf4406955f50dc6a83f883cb3a6200cf20ea0a467fd8a89c8f5c`.
The earlier pre-fixture-correction checkpoint was
`4feb68838bf23bc25ae5b66724abee59f3348174080c87b902da8ec2994c2937`;
its logs remain retained, not confused with the final rebuilt artifact.
Actual packaged AOEM SHA-256:
`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.
This is a test executable, not a rebuilt/deployed node binary.

- `journal-final.log`: 1 passed / 0 failed / 0 ignored, 0.78 s. The repeated
  management signatures happened to be identical on this runtime; equal roots
  were tested, but this does NOT claim observed randomized signatures. The
  state construction explicitly excludes approval signature bytes.
- `native-regression.log`: 402 passed / 0 failed / 63 ignored, 0.40 s. Includes
  the new three manifest, five packet and two admission tests. Ignored process,
  performance and other hardware fixtures are NOT counted as passed.
- `uca-cpu-integer.log`: both prior real ML-DSA candidate tests passed, 2.42 s;
  ten scenarios each, including authority/budget/nonce/failure equivalence.
- `adapter-api.log`: 18 passed; `clippy.log`: node/exec/adapter-api lib/tests,
  release, `-D warnings` passed.
- `pq-signature.log`: 13 passed. `network.log`: 149 NovoRUDP-related tests
  passed after the compatible Tokio/mio updates (2.21 s).
- Final rebuilt artifact: `journal-rebuilt.log` 1 passed (0.78 s),
  `native-regression-rebuilt.log` 402 passed / 63 ignored (0.32 s),
  `uca-cpu-integer-rebuilt.log` 2 passed (0.46 s). Earlier timings are not TPS.
- `native-lifecycle-final.log`: 6 passed / 0 failed / 0 ignored, 3.23 s;
  CPU/integer native socket preparation/reuse, three-height and deadline tests.
  Final strict Clippy/format check is captured separately in `clippy-final.log`.

Commands use `CARGO_INCREMENTAL=0`, build jobs 4, release/locked/offline and
`NOVOVM_AOEM_TEST_LIBRARY` explicitly set to the packaged Windows library.
The three main runtime commands:

```powershell
cargo test -p novovm-node --lib --release --locked --offline actual_mldsa_uca_initialized_spend_respend_journal_recovery -- --ignored --nocapture --test-threads=1
# The following two filters ran the same just-built test executable directly:
novovm_node-8b1b0f6d8115fb2d.exe native_pipeline:: --test-threads=4
novovm_node-8b1b0f6d8115fb2d.exe actual_mldsa_capture_uca_ --ignored --nocapture --test-threads=1
```

Final local database/public initialization evidence directory (not committed):
`target/runtime-rebuild/consensus-tests/uca-publication-3212-1792842685203753500/`.
Earlier successful checkpoint: `uca-publication-23580-1792842197530753800`.
Do not delete or reuse its signing history under a cleared domain.

The new test uses four ORIGINAL pipelines, four independent RocksDB databases,
four original journals and a 3-of-4 certificate, with in-process message delivery.
Consensus keys remain the existing Ed25519 fixture: this is NOT the full ML-DSA
consensus upgrade, native SDK activation, four node processes or multi-host proof.

It checks legitimate management initialization, actual ML-DSA spending, a
same-nonce rejection, recipient respend in height 2, independent execution and
identical published roots/receipts. Original `quote_and_settle` is the independent
fee reference; cold queries compare exact balances, grant budgets, stable nonce,
all fee/window/diagnostic records, signed bodies and both archived receipt sets.

Recovery boundaries: reopen after candidate persistence but before publication;
enqueue a decided transition, drain storage without observing its ACK, then reopen;
finally close/reopen all databases and journals. These are NOT forced process
kill, physical power loss, ambiguous commit injection or independent node catch-up.
No assertions equate candidate persistence/enqueue with finality.

Failures retained:

- `build.log`: initial integration caught forbidden direct unsafe DLL load and
  a wrong receipt accessor. Fixed by borrowing the existing exec facade and
  using the actual accessor, not by relaxing `forbid(unsafe_code)`.
- `journal-initial.log`: cold fee-reference assertion incorrectly assumed all
  page changes were Put; the original codec explicitly deletes unused pages.
  The test now checks both exact values and absent tail pages.
- Review caught randomized approval bytes changing the initial root; removed
  from authoritative state as above and added repeated valid approval/root test.
- `native-lifecycle.log`: additional old native regressions found 4 passed and
  2 failed at `preparation emitted protocol output`. The unchanged recovery
  code queues an unsigned exact-current-context RequestDecision for each peer
  on its first poll; the older overlap fixture incorrectly required an empty
  queue. A test-only checker now requires precisely that known request's
  original encoded identity per peer, no durable control/outbox, while retaining
  the original no-head/no-signing-phase and actual independent execution checks.
  No network/consensus production behavior or timer was changed.

No TPS/P95/P99 or new performance qualification is produced by these tests.
The new actual-journal test is explicitly wired into the existing Linux shared
ML-DSA CI step with its library path. Local Windows pass does not predict that
step or full remote CI outcome.
Security dependency results are [separate](../security-dependencies-20261024/README.md);
crypto-bigint remains an open blocking advisory, not an ignored green check.

## Next actual product boundary

Wire the reviewed initializer/final-root pin into the original fresh-domain
configuration and NUAS into the same native SDK/APFL ingress, pool, propagation,
re-execution/catch-up and typed queries. Budget the larger signatures explicitly.
Do not open HTTP/JSON-RPC or another mode, or reinterpret old Ed25519 wire.
Then repeat actual-node publication, recipient respend and interrupted independent
catch-up; separately complete authorized grant changes/revocation/recovery,
first-release consensus signing, mobile artifacts and economic settlement.
