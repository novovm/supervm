# UCA startup pins and RustCrypto security migration

Input: `de9e5270b6959b827db4495a3b721040ddd2167f`, Windows, Rust 1.94.0.
Completed integration checks and the remaining product gaps are recorded below.
No production launch, existing-chain reset, mobile acceptance or TPS claim.

## Reuse and scope

- Existing `service/config/uca_genesis.rs` management-approved initialization,
  original `ResidentNode::start_on_transport`, CandidateStore and journal.
- Original native NUAS activation still requires coordinated APFL, bounded
  admission/pool, candidate, query and archive reconstruction adaptations.
  Startup pins alone do not open an asset-spending entrance.
- Security migration uses upstream stable k256 0.14.0, elliptic-curve 0.14.1,
  ecdsa 0.17.0 and patched crypto-bigint 0.7.6. No local arithmetic fork,
  advisory ignore, altered EVM signature preimage or disabled capability.
- Root, preserved runtime and its NOV proof host workspace lockfiles are
  resolved explicitly. The historical RISC0 guest-patched arithmetic source is
  a separate artifact, not qualified or changed by the host dependency update.

## Startup contract and remaining boundary

`ResidentConfig.uca_initialization` carries the management-approved manifest and
three operator-pinned values: signing domain, initial state root and final
commitment. The original `ResidentNode::start_on_transport` verifies all three
through the existing exec/AOEM ML-DSA-65 verifier BEFORE opening CandidateStore.
Fresh mode installs only that verified tree. Existing mode checks the original
initial nodes; it neither repairs nor overwrites existing state.

The manifest signing domain remains the transaction/storage domain; the final
root-bearing commitment is separately checked, not substituted into that domain.
The existing consensus validator fixture and protocol remain unchanged (including
their Ed25519 signatures). This is NOT full first-release PQ consensus, a mainnet
economic allocation, or an approved formal launch.

The original body owner now checks the full program/effect/receipt contract.
Until the full native NUAS adapter is ready, initialization does not open spending:
the original admission, checked-pool, ordinary-body and successor-body boundaries
reject legacy V3 submissions under the UCA program. No parallel listener or
execution path was added. The old development CPU path remains available under
its old, explicit contract; there is no UCA-to-Ed25519 fallback.

Missing native adaptations are recorded in the
[unique signature contract](../../../docs/NOVOVM_FIRST_RELEASE_SIGNATURE_CONTRACT.md).
They include bounded NUAS/APFL columns, shared admission/pool, actual UCA key
declarations, typed receipt/query and both cold/archive body reconstruction.
No application asset availability is claimed before these work together.

## Security checks and compatibility

- `audit-root.json`, `audit-runtime.json`, `audit-proof-host.json`: zero fatal
  advisories, exit 0, advisory database `21d909e4c8bd4e14c7aea58bf980c881a0053ab1`.
  Existing informational rand unsoundness and paste-maintenance warnings remain;
  this is not a claim that all dependency risks are gone.
- `deny-root.log`, `deny-runtime.log`: advisories/bans/licenses/sources pass.
  Existing warning output is retained, with no new exception or ignore rule.
- `bigint-root-tree.log`: the host chain now resolves patched 0.7.6 through the
  upstream elliptic-curve/k256 family. All five direct consumers were migrated.
- `workspace-check.log`: root workspace all targets, release/locked/offline,
  passed. Independent native-auth manifests retain locked metadata resolution.
- `clippy-root.log`: root workspace all-targets release strict Clippy passed
  (`-D warnings`, 41.61 s). Root and preserved-runtime formatting checks pass.
- `runtime-network.log`: preserved network consumer 201 passed, 0 failed,
  10 ignored (17.46 s). `clippy-runtime-network.log`: its all-targets release
  strict Clippy passed. These are compatibility checks, not an active second chain.
- `security-consumer-tests-3.log`: EVM core 54 passed, including the fixed old
  k256 0.13.4 RFC6979 signature/recovery-byte vector. ECDH tests use fixed SEC2
  public keys and a fixed shared-secret vector, not merely new self-sign/verify.
  Low-S rejection, Keccak/RLP preimages and public wire bytes were preserved.

## Retained failures

`security-consumer-tests.log` records an incorrect initial Cargo package name;
`security-consumer-tests-2.log` records the migration API compile errors before
the upstream fallible PrehashSigner and helper-name fixes.

The parallel network run in `security-consumer-tests-3.log` records 837 passed,
20 failed and 11 ignored. Six failures include global chain-state/environment
and timing-sensitive RLPx cases; fourteen subsequent failures report a poisoned
test mutex. Source review found shared test chain IDs and environment mutation,
but this turn did not prove every failure's cause. The unchanged suite rerun
serially in `network-serial.log` passed 857, failed 0, ignored 11 (58.07 s).
No timeout, production scheduling, assertion or CI gate was relaxed. The parallel
failure is not erased by the serial result; test isolation remains an open item.

## Actual AOEM and product verification

- `node-native.log`: 402 passed, 0 failed, 64 explicitly ignored (1.10 s).
- `startup-test.log` and formatted final `startup-final.log`: the actual original
  startup test passed, final run 0.41 s. Three independent pins, valid management
  approval, public config loading, unknown-field rejection, fresh/reopen and five
  invalid-material cases are checked. Invalid startup does not create a database.
  V3/NUAS rejection leaves no pool, nonce reservation, signature job or publication.
- `uca-publication.log`: original four pipelines/four stores/two heights of UCA
  spending, replay rejection, recipient respend, unpublished-candidate reopen,
  unobserved-decision-ACK drain/reopen and cold balances/nonce/budget/fee/receipts
  pass (0.78 s). The regenerated approvals were identical in this runtime; no
  claim of observing different randomized signatures is made.
- `uca-effects.log`: original actual ML-DSA CPU and CPU/integer atomic-effect
  equivalence tests, 2 passed (2.48 s).
- `native-lifecycle.log`: original CPU/integer three-height, follower preparation
  and proposal/deadline tests, 6 passed (3.21 s).

Final node **lib-test** executable SHA-256:
`fe81295bb224120df4a183d28168ceee99087e7541c9653ec7eb25f0e7e5253d`.
This is not a deployed node executable. No TPS, real four-node process,
independent-device, mobile or launch qualification is claimed here.

Reproduction uses Rust 1.94.0, the VS amd64 build environment,
`CARGO_BUILD_JOBS=4`, `CARGO_INCREMENTAL=0`, and
`NOVOVM_AOEM_TEST_LIBRARY=<repo>/aoem/windows/core/bin/aoem_ffi.dll`.
AOEM SHA-256: `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.
No dynamic library, actual validator key or chain database is replaced.

```powershell
cargo audit --json
cargo audit --file runtime/Cargo.lock --no-fetch --json
cargo audit --file runtime/proofs/nov-transfer/Cargo.lock --no-fetch --json
cargo deny check
cargo deny --manifest-path runtime/Cargo.toml check
cargo check --workspace --all-targets --release --locked --offline
cargo test -p novovm-network --lib --release --locked --offline -- --test-threads=1
cargo test -p novovm-node --lib --release --locked --offline native_pipeline:: -- --test-threads=1
cargo test -p novovm-node --lib --release --locked --offline actual_uca_startup_three_pins_reopen_and_closed_asset_admission -- --ignored --nocapture --test-threads=1
cargo test -p novovm-node --lib --release --locked --offline actual_mldsa_uca_initialized_spend_respend_journal_recovery -- --ignored --nocapture --test-threads=1
cargo test -p novovm-node --lib --release --locked --offline actual_mldsa_capture_uca_ -- --ignored --nocapture --test-threads=1
cargo test -p novovm-node --lib --release --locked --offline native_lifecycle_ -- --ignored --nocapture --test-threads=1
cargo clippy --workspace --all-targets --release --locked --offline -- -D warnings
cargo test --manifest-path runtime/Cargo.toml -p novovm-network --lib --release --locked --offline --target-dir target/runtime-rebuild -- --test-threads=1
cargo clippy --manifest-path runtime/Cargo.toml -p novovm-network --all-targets --release --locked --offline --target-dir target/runtime-rebuild -- -D warnings
```

The startup test generates temporary test-only approvals in memory and uses a
known non-production validator fixture. Its configuration/database directory is
under `target/runtime-rebuild/consensus-tests/uca-startup`, not this evidence set.
It calls the actual original service in the test process. Rejection injections
exercise the bounded consumers but are not a native socket delivery test. Only
the initialized height-zero database is reopened; post-transaction publication
recovery is checked separately by the original four-pipeline test.
