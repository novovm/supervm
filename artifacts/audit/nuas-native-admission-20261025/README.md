# NUAS original native admission, query and durable respend

2026-10-25. Input `1727f6e1`, clean fast-forward to docs-only `0b462565`.
Windows x86_64, release, repository-shipped AOEM, localhost UDP. This is public-test
implementation and evidence, not a production launch or mainnet allocation.

## Reuse and actual replacement

- `service/client::NativeApplicationClient::prepare_uca` sends shared NUAPFL01
  through the ORIGINAL authenticated application lane. Transport tag 15 is an
  explicit input version; existing tag 14 is its candidate body. No HTTP listener,
  new network owner, transaction codec, controller, ledger or publication head.
- `service/rpc/admission` (the existing scheduler's historical module name, NOT
  an HTTP entrance) uses the resident pipeline signature worker, then the original
  immutable state-tree reader at one published parent. It replaces the previous
  blanket rejection of typed NUAS input; legacy V3/raw carriers still reject in
  the UCA configuration. The shared signature contract and full business plan are
  unchanged. No Ed25519 fallback for an invalid ML-DSA-65 asset signature.
- Paged UCA/account/grant codecs are reused for authoritative admission and query.
  Missing, malformed and unavailable state are not authorized/zero nonce. Exact
  signed intent, source ownership, genesis/domain and captured root are checked.
  All grants of one account share the original stable nonce reservation.
- Same bounded pool, repair hints, per-peer fanout, candidate selection and
  `CandidatePipeline -> exec/bindings/AOEM -> packet -> ValidatorJournal` publish.
  Admission does not consume funds or grant budget. Same-batch prefix, authorization
  rejection, actual fees, failure nonce and release of unused maximum-fee reserve
  remain in the existing full effects plan and its single candidate writeset.
- `Query::UcaSpendState` uses NVQUERY3 on the ORIGINAL query lane, canonical NUAS
  intent plus non-authoritative address hints. The result includes root/height,
  account/epoch/grant/nonce and payer/recipient balance. It is an authenticated
  serving-node observation, NOT an execution/light-client proof or spend permit.
  NVQUERY1/2 are unchanged. Eight bounded jobs, 30-second TTL, one read opportunity
  per turn; a completed read cannot be followed by another reservation that turn.
  This preserves admission/nonce hydration's opportunity on the shared I/O owner.

## Measured functional evidence

`native-services-first.log`: actual ML-DSA-65 signing and AOEM verification, one
nonleader native application ingress, four ORIGINAL services/sockets and four
independent DBs **inside one process**. No direct transaction injection or synthetic
vote. Tampered signature and wrong authorization epoch reject without publication;
Alice sends 100 to Bob, Bob spends 40 back; each transaction charges 45 test units.
All four typed receipts match. Alice ends at 895, Bob at 15; remaining grant budgets
855/915, stable account nonces 1/1. Original databases reopen and receipts/state match.
The values are isolated test smallest units, not approved mainnet distribution.

Initial build regression: 413 passed, 0 failed, 66 explicitly ignored; protocol
58 passed. Four explicit real-AOEM checks passed: asynchronous signature owner /
source and bad-row boundaries; CPU/integer full effects equality; four-store
journal replay and structural archive recovery; startup pins / legacy rejection.
Final rebuilt test results and artifact identities are recorded alongside this
file; the first and final checkpoint must not be confused.

First checkpoint test executable SHA256:
`4451caf7bdf80ce01ee92a31a59b2c575248f3f18b785e5b5500f0ecb24480d3`.
Unchanged shipped Windows AOEM SHA256:
`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.

Final rebuilt executable SHA256:
`241d75982a2eb48e4b975273ee1a2e489b8a28bd04e24b32e34f0b414b2adc2e`.
Final run repeats 413 native regressions and all six explicitly selected real-AOEM
checks successfully, including the unchanged V3 native UDP three-height recovery.
Native four-service scenario passes again in 5.52 seconds (functional test duration,
NOT TPS). Final strict Clippy and workspace fmt pass; CI routing 11 tests pass.
Workspace all-target release check passes (35.62 seconds); no sibling/mobile build
or cross-platform execution result is inferred from this Windows check.
See `evidence.json`, `final-*.log`, `native-regression-final.log`, and
`native-services-final-report.json`; neither DLL nor economic policy was changed.

The first strict Clippy run failed on `unnecessary_unwrap` in admission handoff;
the code now uses explicit matching. `clippy-first.log` is retained. Existing Cargo
duplicate-build-target warning is not hidden or treated as a new failure. The
renamed startup test still tests the same refusal boundary, not removal of a check.
Raw test logs preserve the runner's final blank line; Git whitespace check reports
those EOF lines. Source, workflow, documentation and JSON whitespace checks pass.

## Explicit limits / next bounded work

- Not four OS processes, four machines, failure-point process kill, live single-node
  NUAS archive catch-up, mainnet activation, mobile asset activation, full PQ consensus,
  dynamic management/rotation, complete ZK or service settlement qualification.
- Native input delivery ACK is only transport completion. Pool admission remains
  volatile; clients keep signed bytes and observe finalized receipts. Missing recent
  receipt is not proof of permanent rejection; the same original archive observer remains.
- Current UCA authority admission uses about 74 sequential bounded point reads per
  row; candidate preparation re-verifies signatures. Full UCA grant/budget successor
  overlay is not yet exposed, so the old V3 nonce-only successor path is NOT reused
  as if it authorized future UCA execution. CPU reference behavior remains available.
  This cut has NO new TPS/P95/P99 result and does not inherit Ed25519 performance.
- Next: run this exact native entry in independent node processes, including one
  node offline, authenticated reconnect, archive re-execution and interruption during
  catch-up. Then qualify ML-DSA-shaped batch/byte budgets and throughput. Reuse the
  existing fault harness; no alternate runtime, copied databases or reduced quorum.
- CI uses the existing shared ML-DSA step, adding only these two targeted live
  tests. Full remote result is separate from local results; cancelled != passed.

## Reproduce

Use repository toolchain/MSVC environment and trusted shipped library. Set
`NOVOVM_AOEM_TEST_LIBRARY` to the resolved `aoem/windows/core/bin/aoem_ffi.dll`.

```powershell
cargo test -p novovm-protocol --lib --release --locked
cargo test -p novovm-node --lib --release --locked native_pipeline -- --test-threads=1
cargo test -p novovm-node --lib --release --locked actual_native_uca_single_client_respend_and_cold_recovery -- --ignored --nocapture --test-threads=1
cargo test -p novovm-node --lib --release --locked actual_mldsa_uca_async_admission_keeps_source_owner_and_row_failures -- --ignored --test-threads=1
cargo clippy -p novovm-node -p novovm-protocol --lib --tests --release --locked -- -D warnings
cargo fmt --all -- --check
```

Generated test DBs/signing logs remain local under `target/runtime-rebuild/consensus-tests`;
only report/log evidence is copied here, never generated private keys or chain DBs.
