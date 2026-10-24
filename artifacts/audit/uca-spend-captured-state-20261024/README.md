# U3 same-parent captured input — bounded foundation

Baseline: `ec32c21c199472a2adf242d975218e1684733e99`; implementation belongs to the
commit containing this report. Windows x86_64 MSVC, Rust 1.94.0, release,
`CARGO_BUILD_JOBS=4`, `CARGO_INCREMENTAL=0`, locked/offline Cargo. No performance run.

## Delivered

Original `novovm-node::native_pipeline::business::uca_spend_state` consumes the
original `OwnedStateInput` and immutable tree. It reuses `UcaAccount`,
`PersonaBinding`, original balance/nonce keys and bounded `record_pages`.
One captured parent supplies account/binding/reverse owner, current spend epoch,
grant, explicit nonce and balances. Wrong hints cannot redirect assets; missing
records do not create an account or zero nonce. Serialization is bounded binary,
not JSON. The new module's declarations are read-only.

The shared policy now requires an independent `current_spend_epoch`. Advancing
that epoch rejects an old retained grant; neither its matching signature nor its
own epoch overrides current account state. Existing management epochs/timestamps
are not spending epochs. No new cryptographic implementation or AOEM code changed.

## Verification

All test commands below use `cargo test --release --locked --offline`:

| Selection | Result |
| --- | --- |
| `-p novovm-node --lib native_pipeline::business::uca_spend_state -- --test-threads=1` | 7 passed, 0 failed, 1 runtime test ignored here |
| `-p novovm-node --lib actual_mldsa_signature_and_same_parent_preflight_reject_stale_authority -- --ignored --test-threads=1` | 1 passed, 0 failed/ignored |
| `-p novovm-adapter-api --lib uca_spend_policy` | 5 passed |
| `-p novovm-adapter-api --lib` | 17 passed, including those same 5 |
| `-p novovm-protocol --lib uca_spend` | 8 passed |
| `-p novovm-exec --test uca_spend_runtime -- --ignored --test-threads=1` | 1 passed |
| `-p novovm-node --lib native_pipeline -- --test-threads=1` | 391 passed, 0 failed, 59 ignored; includes the 7 pure tests above |

`cargo clippy --release --locked --offline -p novovm-node -p novovm-adapter-api
--lib --tests -- -D warnings` passed (the lib-only check also passed). Root
`cargo fmt --all -- --check` and `git diff --check` passed.
Test counts overlap; do not add them as unique cases.
The 59 ignored scenarios were not executed by that general regression command;
only the explicitly listed runtime tests were run separately. No process-network,
cold database recovery, GPU performance or mobile acceptance is claimed here.

The real runtime fixture signs through the existing ML-DSA-65 exec facade and
checks the same captured parent. It rejects stale account epoch with old grant
retained, revocation, insufficient grant budget, changed nonce, recovering account,
expiry and reusing an old snapshot at the successor root. Its tree is explicitly
seeded by the test, not initialized by a real authorized registration transaction.
The fixture's safe bindings key generator runs as the test thread's first auto
load and requires its resolved library to equal the explicit packaged facade path;
this is not a general guarantee about previously cached auto-loader identities.

Pure tests also cover every missing used record, undeclared access, false address
hint, alias/reverse-owner mismatch, another genesis, canonical encoding/tails,
malformed nonce, revoked/non-NOV binding, maximum ID length, fixed-buffer rejection,
self-binding and corrupt authenticated tree nodes. Post-capture reads work with
the source reader dropped. The captured object cannot stage a nonce write.

Initial test compilation correctly rejected an unsafe direct DLL load under the
node's `forbid(unsafe_code)`. The restriction was retained; the final fixture reuses
the existing safe bindings loader and refuses unavailable capability. No failing
assertion was removed. Cargo's existing multiple-bin-source warning remains.

The CI workflow enumerates this module and explicitly runs its ignored runtime
test on the packaged Linux library. That is new CI wiring, not a Linux test result.
Baseline `ec32c21c` native-auth CI `40079062435` succeeded; main `40079062322`
was still in progress when checked. This commit's CI is independently pending.

## Still missing / next integration

This is **not a spending permit or a completed native transaction**. Root trust
must come from the original validated candidate/journal. The pure encoders confer
no registration, grant issuance, rotation or publication authority. The original
management `Mldsa87` enum is not relabeled as 65.

Next: authorized account/binding/grant initialization and rotation on the same
tree; versioned native ingress/APFL resource budgets; grant-budget and stable-nonce
prefixes inside the existing complete CPU/integer business plans; one original
stage/journal publication. Then verify the approved fee/budget rules, 100-budget
two-80 conflict, failure release, no incoming-budget inflation, real transfers,
restart/catch-up and ML-DSA load. Do not reopen a second execution/authority path.

App can continue consuming the shared signature contract, but must not treat this
preflight or a seeded fixture as chain permission, NOV arrival or finality.
Android/iPhone runtime qualification and real billing remain uncompleted.

Exact artifact identities and structured results: [evidence.json](evidence.json).
