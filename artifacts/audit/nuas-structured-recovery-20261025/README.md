# NUAS structural candidate and recovery integration

Input: `3927926c`, clean `main` matching `origin/main` before this cut. During
implementation, fast-forwarded three disjoint handoff-only commits through
`a4b97505`. Windows, Rust 1.94.0, release, one build owner. This is pre-production
integration, not launch authorization.

## Reuse and replacement

- Reuse `novovm-protocol::{uca_spend, apfl_column}`: the signed NUAS v1 intent,
  ML-DSA-65 key/signature contract, and first-use dictionary columns.
- Add explicit `NUAPFL01`/1 structural encoding and native body tag 14. Old
  NVAPFLV3 and tags 1–13 are unchanged. Same signed inputs are reconstructed
  exactly; hints locate balances, never authorize them.
- `NativeChannel -> NativeLifecycle -> CandidateBodies -> BatchRequest ->`
  original resident compute owner -> AOEM verifier -> original UCA effect plan
  -> candidate store / ValidatorJournal remains the only execution/publication
  route. No listener, controller, ledger, account store or AOEM opcode added.
- Replace hardwired V3 reconstruction in `RoundProgress::poll_recovery` and
  `NativeLifecycle::poll_archives` with one strict-profile `stored_body` adapter.
  Original ArchiveRead/QC, bounded peer caches, exact parent, signing history,
  pipeline re-execution and candidate-locator equality remain mandatory.
- Recover locator hints from content-checked UCA receipt deltas. The receiver
  checks account binding and reverse owner again against its own exact parent.
- The existing native current-height body handoff consumes the structural batch;
  it stays structural until the resident verifier projects canonical rows. The
  existing UCA business compiler still materializes canonical signed records for
  its plan/archive boundary. This is NOT zero-copy or zero-materialization.

## Verification

Final local results (2026-10-25 +08:00):

| Check | Result / evidence |
|---|---|
| Shared protocol | 56 passed, including 3 new codec tests; `protocol-regression.log` |
| Original native pipeline | 406 passed, 64 ignored, including 4 new transport tests; `final-native-regression.log` |
| Real ML-DSA structural execution and unresolved recovery | 1 passed; `final-real-uca.log` |
| Actual startup pins and closed asset admission | 1 passed; `final-startup.log` |
| Existing V3 three-height native UDP / unresolved recovery | 1 passed; `final-v3-recovery.log` |
| Existing CPU/integer UCA atomic-effect equivalence | 1 passed; `final-cpu-integer.log` |
| Strict Clippy, protocol + node lib/tests, release/locked | Passed; `clippy.log` |
| `cargo fmt --all -- --check`, source/documentation whitespace check | Passed |

Codec fixtures use dummy signature bytes only; real cryptographic checks use
the existing packaged AOEM integration fixture. The existing CI native_pipeline,
protocol uca_spend and explicit real UCA publication filters already select these
tests; no extra workflow or duplicate full job was added. Remote CI is separate,
not claimed passed by these local results.

Raw captured test logs preserve the runner's trailing blank lines; the staged
whole-change whitespace check reports those EOF warnings, not source errors.

Final node test executable (not a deployable node binary):
`target/runtime-rebuild/release/deps/novovm_node-3fbd7fac9a29c2ec.exe`, SHA256
`cc6109de70f34fdc36bc3988d5954a3bafb130981c8a585f65ad342b6eb06945`.
Protocol test executable SHA256:
`4b9899e90f5a69942bae04e86a41fd2cbc70e5dbf4f0e8dd4bb9619fcfa90d47`.
Same packaged AOEM `aoem/windows/core/bin/aoem_ffi.dll`, SHA256
`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.
No DLL or SDK manifest was changed. The containing Git commit pins source;
test hashes identify the actual executed builds, not a published package.

Preserved intermediate failure: `v3-recovery.log` stopped with `wrong replayed
body type`. The old test demanded raw `Message::Body`; unified reconstruction
now returns the original APFL representation. It was replaced with the stronger
assertion that the entire recovered APFL object equals the original object AND
its canonical body ID is unchanged. The same fault test then passed, with its
signed-proposal replay-byte assertion retained. Earlier `real-uca.log`,
`native-regression.log`, `startup.log` are intermediate evidence, not the final
binary qualification. No timeout, quorum or economic assertion was weakened.

The extended four-database fixture tests raw/structural candidate-locator
equivalence, bad signature/hint refusal, native bounded body execution, replay
rejection, funded recipient respend, an unresolved durable precommit reopened
through RoundProgress, archive/body identity and cold economic state. Consensus
messages are still in-process fixture delivery, not four independent processes.

## Remaining release gates

- Native client/application NUAS admission, owner-branded admission results,
  nonce and grant-aware pool selection, source retention and directed gossip.
- UCA early/successor preparation on the existing overlap path; it remains
  explicitly closed. This cut does not reactivate a serial replacement product.
- Actual UCA declaration/retained-byte limits and typed query/receipt extensions.
- Real process/network and independent catch-up/kill tests, then same-condition
  successful TPS and latency. No performance improvement is claimed here.
- Full first-release consensus/network signature migration, mobile artifacts,
  approved economic genesis and launch remain separate gates.

The original user asset entrance is deliberately still closed. A decoded body,
valid signature, candidate result or green CI is not final NOV asset admission.

## Other-machine handoff acknowledged

Read `d3d60297`/`f13de543` and the KEY ONE Mobile handoff. The reported physical
PUB/REQ/SIG exchange is the other machine's evidence, not independently rerun
here. Isolated test-NOV actual submission/expenditure is authorized; production
assets and charging remain prohibited. The optical fixture is not automatically
a trusted funded UCA. Continue shared native admission/query/budget integration
here; do not duplicate Android custody or optical transport work.

Also acknowledged `a4b97505`: optical cancellation/expiry and stale callback
guards are the other machine's component evidence; physical authentication
cancellation remains unverified. Upstream delivery must provide test genesis
pins, trusted permission state, native submission and confirmation interfaces.
This cut does not yet provide those client-facing interfaces; the App must keep
execution unavailable rather than invent a substitute. No mobile device touched.
