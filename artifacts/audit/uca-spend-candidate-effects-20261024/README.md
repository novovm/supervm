# U3 candidate effects on the original NOV compiler

Input: `6f2114d4`; resumed on `144812cf` after the independent EVM fixes and
mobile handoff merged. No AOEM library, transport, consensus or live chain data
is changed. This report belongs to the commit containing it.

## Scope and reuse

`SignatureCheckedUcaSpendV1` from the existing exec ML-DSA-65 verifier enters
`NovTransferInput::capture_uca`. The original BatchPlan / OwnedStateInput captures
account, independent current spend epoch, grant, stable UCA nonce, balances and
fee state at ONE parent root. Location hints cannot replace authenticated
account/binding/reverse-owner records. The compiler derives every write access;
clients cannot supply a grant or effect patch.

The SAME `full_effects` composition compiles authorization masks, per-grant
effective budget prefixes and shared account nonce alongside existing fee,
window, diagnostic and balance effects. Unauthorized rows preserve all of them.
Authorized success consumes amount plus actual fee (also for self-transfer);
business failure consumes actual fee only. Unused maximum-fee reservation does
not remain deducted. Incoming money does not replenish a grant.

The original CPU backend remains the independent reference; the integer backend
does not call `finish` or return intermediate outcomes for Host business repair.
Both feed ONE original `stage_results` / `input.stage` with grant pages, balances,
nonce and fees. This produces an **unpublished candidate**, not a durable commit,
finalized receipt, spending endpoint or second ledger.

New business/effect/receipt pins separate UCA semantics from the unchanged V3
reference contract. Receipts commit authorization code and remaining budget.
The legacy proof relation explicitly refuses the new contract; a candidate
commitment is not a business validity proof. A signed UCA max_fee of zero is
literal zero, not the legacy fee API's unlimited sentinel.

## Verification record

Windows x86_64 MSVC, Rust 1.94.0; release, locked/offline Cargo, 4 build jobs,
incremental disabled. Packaged AOEM SHA-256:
`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.
No throughput run. Final node **library-test executable**, not a deployed node:
`1f3f0c935c3e26a1e642196bd3f8d54ceab0c62d4a7bd7dddee34f7a8e220839`.

All commands below use `cargo test --release --locked --offline`, default product
features, with `NOVOVM_AOEM_TEST_LIBRARY` set to the packaged Windows library.

| Selection | Final result / log |
| --- | --- |
| `-p novovm-node --lib actual_mldsa_capture_uca_ -- --ignored --nocapture --test-threads=1` | 2 passed, 0 failed/ignored; [final-candidates.log](final-candidates.log) |
| `-p novovm-node --lib resident_uca_authorization_guards_nonce_budget_and_diagnostics -- --ignored --test-threads=1` | 1 passed; [final-guards.log](final-guards.log) |
| `-p novovm-node --lib resident_integer_matches_cpu_complete_effects_and_global_fee_dependency_repair -- --ignored --test-threads=1` | 1 passed; [final-reference-effects.log](final-reference-effects.log) |
| `-p novovm-node --lib resident_complete_plan_windows_diagnostics_and_capacity_edges -- --ignored --test-threads=1` | 1 passed; [final-reference-windows.log](final-reference-windows.log) |
| `-p novovm-node --lib native_pipeline -- --test-threads=1` | 392 passed, 0 failed, 62 explicitly ignored; [final-native-regression.log](final-native-regression.log) |
| `-p novovm-adapter-api --lib` | 18 passed, 0 failed/ignored; [policy-regression.log](policy-regression.log) |

The two candidate tests each cover the same ten test-owned seeded-state scenarios:
100 budget/two reservations of 80 (amount30 + cap50), sufficient budget, obsolete
epoch before a valid same-nonce intent, strict zero fee cap, revoked grant,
expired grant, same-batch nonce replay with a distinct signed intent, recipient
overflow retaining actual fees, gross-budget self-transfer, and two independent
ML-DSA-65 keys/grants sharing one UCA nonce with separate budgets. The integer
test compares full receipts, fee state, root, staged node bytes and commitments
against CPU; both reread the resulting tree. A grant balance is not merely
asserted from an in-memory preflight. These are ten scenarios, not ten deployed
nodes or ten network-finality tests.

The separate plan guard test additionally preserves old fee diagnostics/windows
when every authorization fails. Existing reference tests retain capacity failure,
economic repair equivalence and fee-window behavior. Counts overlap; ignored
process/recovery/performance cases were not silently counted as passing.

Strict `cargo clippy --release --locked --offline -p novovm-node
-p novovm-adapter-api --lib --tests -- -D warnings` passed, as did root fmt and
diff checks. The first lint run rejected an unnecessary unwrap; it was replaced
with a match, without suppressing the lint, and ALL five explicit runtime tests
plus the native regression were rerun on the final rebuilt executable. Earlier
logs, including [clippy.log](clippy.log), remain for provenance.

CI now explicitly selects the CPU candidate test on the packaged Linux runtime;
this is workflow wiring, not a Linux result. Vulkan parity was run locally only.
No new Android/iPhone run, process-network test, durable UCA restart, formal TPS,
production configuration activation or release was performed.

The initial build failed with `rustc-LLVM ERROR: IO failure on output stream:
no space on device`. Cache removal was rejected by environment policy; the user
subsequently released space. The old binary is NOT acceptance of this patch.

## Not activated / next seam

- The fixture seeds test-owned UCA/grant state. It is not an authorized genesis,
  registration, grant-issuance, rotation or recovery transaction.
- There is a real initialization dependency to version explicitly: UCA state
  keys and stable nonce currently bind `intent.genesis`, whereas the service's
  genesis commitment includes the initial state root. Direct insertion creates
  a cycle. Reuse the canonical genesis manifest with a separately defined
  signing-domain seed (or a properly authenticated subsequent initializer),
  never a random placeholder or relabeled old signing domain. No live reset or
  new mainnet genesis is performed here.
- Native ingress/APFL still uses its existing transaction contract and bounds;
  this patch does not pass NUAS as old V3, add a listener or enable App spending.
- Durable journal publication, independent restart/catch-up of these new effects,
  live authorization mutation, full proof and ML-DSA load remain integration
  acceptance, not inferred from candidate-tree equality.
- No mobile runtime/custody or billing acceptance is claimed here; consult the
  separately updated mobile handoff for its actual device results.

The original publication owner and exact-parent guards remain the only path for
future activation. No database copy, signing-log reset, Ed25519 downgrade or
unapproved real grant was used.
