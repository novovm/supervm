# Native proportional NOV fee: local implementation and validation

Latest: the bounded v3 follow-up and its separate evidence are at the end of this
file. The original v2 results and `evidence.zip` below are retained as history,
not relabeled as validation of the changed v3 executable.

2026-11-06 delivery instruction: the user now requests freezing and delivering
this work as one mainline package. The local results below precede that freeze;
actual candidate source/artifact identities must come from the new release
manifests and its own CI, not from the historical 54bf0c16 package. No fee or
allocation candidates become deployment defaults by this instruction.

Date: 2026-11-05. Source base: `main@54bf0c16e36923ad31679ffc661a4d0a1b4e2456`
plus the uncommitted worktree, **not** a qualified `54bf0c16` release binary.
Prior identity-preparation and documentation edits are preserved. No branch,
push, cloud CI, deployment, actual SCM service or production charging was started.

## Implementation

- Original `DirectNovFeePolicy.amount_fee_v2` carries explicit integer rate bps,
  floor/ceil charging, service/network allocation and split-remainder recipient.
  Nothing selects 10/20 bps, 90/10, subscription 80/20 or a production rounding rule.
- Quote arithmetic uses the authenticated NOV amount, with no argument JSON or
  market-price lookup on the new path. Literal signed fee caps remain binding.
  Quotient/remainder arithmetic supports representable results at u128 limits.
- Original CPU preparation/settlement and AOEM full integer plan use the same
  quote and allocation; the integer compiler rebinds original split outputs,
  without a new kernel, ledger or Host effect callback.
- Existing reserve/fee budget views represent network/service responsibility
  under this explicit version; new risk allocation is zero. Their sum is counted
  once in the original collected balance. This is not provider-specific earnings
  or a completed provider payout. Existing mapped-asset operations stay disabled
  for this native-only policy.
- Policy/fee pages, receipt codecs, business IDs and semantic version are
  versioned. The derived protocol pin changes; old configurations are rejected
  before initialization instead of converting existing databases. Reference
  quotation remains distinguishable for historical regression, not fallback
  from invalid v2 configuration.
- Archive recognition consumes the shared semantic version; archived amount
  quotes also bind their basis to the actual signed transaction amount.

## Final local validation (Windows x86_64)

| Check | Result |
| --- | --- |
| Original native pipeline, single test thread | 572 passed, 0 failed; 149 ignored, 1147 filtered |
| Real ML-DSA proportional controller CPU/integer, failures and replay | 1 passed; both split-remainder choices covered |
| Proportional fee, four original in-process services, real UDP/four RocksDB stores, respend/replay/cold reopen | 1 passed |
| Existing controller/delegation CPU/integer equivalence | 1 passed |
| Existing four-service authority rotation/freeze/recovery and repeated cold reopen | 1 passed |
| Original initialization CLI, including real AOEM old/new policy approvals and forgery rejection | 14 passed, 0 ignored |

Strict Clippy (node/ctl lib, bins and tests; then the updated initialization CLI
test), workspace format and diff checks passed. No result here stands in for a
full cloud run or a final frozen package.

Test executable SHA256:
`d0c0e5c2e47c2dead258240992b31fececacaf60789daf8cbd6df63e389ccae1`.
Actual explicitly loaded package AOEM SHA256:
`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`.

Initial real effects failed with `unsupported NOV semantic version`. Synchronizing
the shared context/archive version exposed seven stale archive fixture failures;
those literal-version fixtures were corrected, not bypassed. A new quote-basis
counterexample and final successful runs are retained. One intermediate test
compile error (`PreparedCandidate` has no Debug for unwrap_err) is also retained.

`evidence.zip` contains only the explicitly listed build/test text logs: no keys,
credentials, custody files, private account material or databases. The four-store
fixture paths printed in logs refer to local test evidence, not A/B/C/D deployment.
Archive SHA256: `da109e5dffd4e0ee855eef24e4a7e9f7e9ebb5b6561c7d38ad34548ef2ce6c78`.

## Remaining boundaries

Actual rate, splits, rounding and deployment allocations are not selected. Tiny
amounts under explicit floor, or zero-value actions under the proportional formula,
can produce zero fee; independent non-transfer resource pricing is not calibrated
or claimed complete. No genesis for the A-side service test was generated, no
real service account was provisioned, and no existing service/library was replaced.
Actual SCM qualification, four physical-host chain acceptance, business proof
closure and TPS remain separate. The historical height-4 stall is not resolved
by these local passes.

## 2026-11-06 follow-up: explicit minimum and maximum, v3

The user-supplied proposal is archived verbatim in
[`2026-11-05-nov-transfer-bounded-fee-source.md`](../../../docs/NOVOVM-ECONOMICS/references/2026-11-05-nov-transfer-bounded-fee-source.md).
Its 20 bps, 0.002 NOV minimum, 50 NOV maximum and 90/10 allocation remain
**candidates**, not deployment defaults or fee activation approval.

The original policy now carries `amount_fee_v3: ProportionalFeeV3` with required
`minimum_fee_units` and `maximum_fee_units`. Charging rounds the proportional
amount first, then clamps it to those bounds. The floor is not added to the
proportion. All accounting and AOEM integer split inputs use the actual clamped
fee, once. Literal signed caps, failed-authorization behavior and existing
business-failure/nonce semantics remain unchanged. There is no price feed or
per-node dynamic pricing. Policy snapshots and quotations bind both bounds;
the online governance/scheduled update mechanism is not delivered by this change.

Policy version is at least 3. Policy/fee pages, semantic version, business and
receipt identities are explicitly changed; prior v1/v2 protocol commitments
are rejected by configuration validation. No old database is converted, cleared
or claimed compatible. Balance and account nonce locations remain unchanged.

### Current local results

| Check | Result |
| --- | --- |
| Original native pipeline, one test thread | 575 passed; 0 failed; 149 ignored; 1147 filtered |
| Real ML-DSA controller CPU/integer equivalence | Passed, including minimum, capped fee, cap below minimum, wrong authority, business failure and replay |
| Four original in-process services/UDP/four stores and cold reopen | Passed twice within one test: zero-floor/large-cap fixture, then positive-floor/capped fixture |
| Existing real controller/delegation CPU/integer regression | Passed |
| Existing four-service key rotation, freeze/recovery, respend and repeated cold reopen | Passed |
| Original genesis CLI including real approval verification and forgery rejection | 14 passed, none ignored; metadata domain changes for rate/minimum/maximum |
| Strict Clippy, node/ctl lib/bins/tests; workspace format; diff check | Passed |

The exact candidate commercial-unit table and base-unit transitions are tested;
real funded service fixtures use separately stated small integer bounds, not
invented commercial allocations. Zero remains an explicit permitted floor for
isolation. A minimum can charge zero-amount actions too: their separate resource
price is not calibrated here. No extra charge is attributed to retransmissions.

Executable SHA256:
`6ec799e68fa2879302e05f5d2fb88fbf16eae52244b1cbf64b943006b7ce3762`.
The explicitly loaded AOEM DLL is the same frozen package file identified above:
`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`.
Source is still `main@54bf0c16` plus the recorded uncommitted worktree, not a
requalified release package. The cargo duplicate-bin manifest warning remains
pre-existing; no check was disabled to hide it.

### Reproducible commands and evidence

```powershell
cargo test --locked --release --target x86_64-pc-windows-msvc -p novovm-node --lib native_pipeline:: -- --test-threads=1
# Explicit trusted package DLL for the four ignored real-library tests above:
$env:NOVOVM_AOEM_TEST_LIBRARY='D:\WorksArea\SUPERVM\artifacts\product-overlay\native65-54bf0c16-windows-x86_64\aoem\windows\core\bin\aoem_ffi.dll'
$env:NOVOVM_AOEM_DLL=$env:NOVOVM_AOEM_TEST_LIBRARY
# Run each named test independently in the produced novovm_node test executable:
# actual_proportional_controller_fee_cpu_integer_and_replay
# actual_native_proportional_fee_respend_and_cold_recovery
# actual_mldsa_controller_cpu_integer_atomic_effect_equivalence
# actual_native_uca_single_client_respend_and_cold_recovery
# Arguments: --ignored --test-threads=1 --nocapture
cargo test --locked --release --target x86_64-pc-windows-msvc -p novovmctl --test native_genesis_cli -- --include-ignored --test-threads=1
cargo clippy --locked --release --target x86_64-pc-windows-msvc -p novovm-node -p novovmctl --lib --bins --tests -- -D warnings
cargo fmt --all -- --check
git diff --check
```

`bounded-v3-evidence.zip` contains only seven explicit `target/bounded-fee-*`
text logs: native, real-effects, four-services-cold, controller-regression,
authority-cold-regression, genesis-cli and clippy. It contains no private keys,
protected credentials or databases. 25,941 bytes; SHA256:
`6f71327127ac89da79cce4eef987fc36a9941460a01c1c270d1aa3157c1f474e`.
The original v2 archive was not overwritten.

No branch, commit/push, cloud CI, package/library replacement, A SCM deployment,
genesis, actual fee activation or B/C/D operation occurred. There is no new TPS,
provider payout, online parameter-update or historical height-4 fix claim.

## Delivery freeze: protected purposes and native identity regression

2026-11-06, before the consolidated mainline commit: original
`native_identity_cli` **7/7 passed**, including all ignored cases against the
actual rebuilt NAT binary, explicitly pinned candidate DLL and the unqualified
formal DLL negative control. `novovm-relay` purpose envelope tests **3/3 passed**.
No protocol or production test assertion was changed for these passes.

The first include-ignored invocation omitted `NOVOVM_PRODUCT_NAT_TEST_BINARY`
and `NOVOVM_AOEM_BATCH_UNAVAILABLE_TEST_LIBRARY`: 5 passed, 2 failed for missing
fixture inputs. The corrected invocation supplied both original required
inputs; the initial failure remains evidence, not reclassified as a product
defect or silently dropped.

Four text logs are retained in `delivery-source-evidence.zip`, 2,941 bytes,
SHA256 `8817f4ee423a8cb559e34c522582e9d5541705f7f2ebff759bec9bba81e1b0f2`.
This supplements the earlier 575/14/4 checks, without claiming cloud CI or
actual A service deployment. The new platform package manifests, generated
after this freeze, must identify this commit and the actual same-platform
runtime rather than the old package SHA.
