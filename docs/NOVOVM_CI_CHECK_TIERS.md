# CI scope and candidate validation

## 2026-11-04 C LFS audit and handoff: A implements, C does not edit CI

**User-selected ownership:** validator-A owns implementation of CI LFS download/cache optimization; validator-C only verifies and returns this plan. This decision does not authorize C to edit workflows, a second downloader, AOEM, credentials, billing budgets or test semantics. A continues its integration/test work and can implement the following in the same main owner. No optimization is delivered by this prose update.

Audit baseline: main **`f5a866d825b7180fe3be7acc9340188d5a0dc05a`**. The workflow Git blobs are ci.yml `dcbeddb5a1f38a75ada9c42fa99516fed84e485a` and mainline-nightly-soak.yml `7584e17f493a38d177f213ac0c2ee01c93de4dd8`. C fetched/synchronized with process-scoped `GIT_LFS_SKIP_SMUDGE=1`; metadata inspection used `git lfs ls-files --json` and did not fetch LFS objects or launch builds. The current workflow files are unchanged by this round.

### Recomputed current object and per-job request sizes

Current main lists **44 LFS paths / 14 different SHA256 objects / 722,741,064 bytes = 689.260 MiB**. This is the current revision's distinct-object set, not all historical storage or a bill. Identical OIDs at multiple paths are counted once **inside each job**, but independent cold jobs repeat their requested objects.

| Existing job | Cold requested bytes | MiB |
| --- | ---: | ---: |
| Rust checks (current full LFS checkout) | 722,741,064 | 689.260 |
| Native transport qualification / Linux | 313,628,440 | 299.099 |
| Native transport qualification / Windows | 93,050,880 | 88.740 |
| Preserved runtime release / Linux | 153,339,776 | 146.236 |
| Preserved runtime release / Windows | 44,705,792 | 42.635 |
| Five-job total | 1,327,465,952 | 1265.970 |

The five-job total is **1.2363 GiB per full cold run**. It assumes empty LFS stores, each selected fetch completes, no cancellation before download, and independent jobs. It excludes nightly, manual/local/fork/archive downloads, historical objects, retries and any cache-product costs. It is not observed billed traffic. The supplied 52-run / 40-checkout / 24-cancelled / 108-fetch log counts were not independently replayed in this audit; retain them as the user's prior historical investigation, not newly measured C results. C has no usage invoice or owner billing breakdown and cannot assign all quota consumption to CI.

Verified source locations at this baseline:

- `.github/workflows/ci.yml:223`: Rust checkout has `lfs: true`; its Cargo cache is not an explicit `.git/lfs/objects` restore/save.
- `ci.yml:328` uses the formal Linux core for existing resident/private-profile checks; `:339` subsequently pins the isolated Linux candidate. **A Linux-only filter must include the isolated candidate, not just aoem/linux/**.
- `ci.yml:881` fetches one platform's formal core plus candidate for each native-auth job; the formal core is an intentional negative counterexample and must not be removed or silently replaced by the candidate.
- `ci.yml:1000` fetches the platform core again for each runtime-release job.
- `.github/workflows/mainline-nightly-soak.yml:56` also uses `lfs: true`. It is self-hosted: inspect its retained local LFS store before estimating actual repeated downloads; do not assume every nightly is cold.
- The audited workflow files contain no explicit LFS object cache. Local/runner caches, concurrent requests and cancelled jobs must be observed rather than inferred from a cache step name or final run status.

The four distinct core objects that must remain separate:

| Current path | LFS object SHA256 | Bytes |
| --- | --- | ---: |
| `aoem/linux/core/bin/libaoem_ffi.so` | `4d06f36a67f41035661100ac0fd18c67dcae60bf2cdd5df28db33b555c538309` | 153,339,776 |
| `artifacts/runtime-candidates/native-transport-mldsa65-20261103/linux/libaoem_ffi.so` | `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` | 160,288,664 |
| `aoem/windows/core/bin/aoem_ffi.dll` | `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947` | 44,705,792 |
| `artifacts/runtime-candidates/native-transport-mldsa65-20261103/windows/aoem_ffi.dll` | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` | 48,345,088 |

### Proposed implementation order for A

1. **Map each job's actual dependencies first.** For the Rust Linux job, a conservative starting set is all currently tracked `aoem/linux/**` objects plus the isolated Linux candidate: **501.870 MiB / 7 OIDs**. This is a platform-filter scenario, **not a proven minimal dependency closure**. Check dynamic plugin loading, ignored tests, fixture readers and cross-platform negative cases before excluding individual plugins. Native-auth matrices retain their formal and candidate objects; runtime release retains its real core and any dependencies its original tests actually load. Audit the nightly host's same dependency boundary separately. Missing required material fails; it must not cause tests to disappear or be replaced by a mock.
2. **Restore before any LFS download.** Use checkout without automatic LFS materialization, with process/job-scoped skip-smudge if necessary, then restore a dedicated object cache. Use the existing original fetch/materialization owners for only the allowed paths. Do not cache the whole .git directory, credential helpers, tokens, private test custody or runtime state. Cache data is not authority: validate the current tracked pointer's OID and size, materialized bytes, platform/header and existing independent runtime manifest/qualification gates. A cache hit must not bypass these checks or turn a formal SDK into a candidate.
3. **Save immediately after download and successful verification**, before long Rust builds/tests. Use an explicit restore/save sequence rather than depending solely on end-of-job post-save. Later cancellation/failure must not throw away the only opportunity to save already paid-for, verified downloads. A cancellation between fetch and save still has a cost; no design can undo a completed LFS download. Key caches by platform plus a stable digest of the relevant OID/size set, not the moving commit SHA, timestamp or a static platform label alone. Prove changed OIDs fetch only missing objects and preserve old-object isolation.
4. **Avoid parallel cold-download races.** Several independent jobs restoring the same missing key can all download before any save completes. Prefer one bounded platform preparation/download owner (or another existing, verified single warm-up point), then let the original test jobs consume the verified object cache. Keep test matrices, assertions, all ordinary/ignored cases, fail-closed result aggregation and native runtime qualification intact. Do not disable main CI, add skip markers, cancel unrelated candidate checks or change product timeouts/quorum/permissions. Confirm cache scope/access on the relevant push/PR contexts; a cache snapshot that is inaccessible/evicted is a miss, not a success.

Sizing scenarios from the **same current** objects:

| Scenario | Cold LFS GiB | Scope of estimate |
| --- | ---: | --- |
| Current five independent cold jobs | 1.2363 | Verified configured object sets; not billed traffic |
| Only constrain Rust to conservative Linux set; other jobs unchanged | 1.0533 | About 14.80% fewer requested bytes; plugin closure still needs testing |
| One Linux-set + one required Windows-core-set download, downstream reuse succeeds | 0.5768 | 590.610 MiB unique union; about 53.35% lower than current cold requests; not yet implemented |
| All required objects available from a verified accessible cache | No missing-object LFS fetch required | Not zero Actions traffic, zero cache cost or a promise that every run is warm |

Sharing through Actions cache/artifacts has its own storage, transfer and runner-time costs; it is not free just because it no longer consumes LFS bandwidth. A should choose the simplest reuse arrangement that avoids fetching unrelated platform/plugins and demonstrate its real effect in logs.

### Required evidence after A's implementation

- Diff the pre/post workflow job/platform/test-command coverage; runtime qualification, formal negative controls, current hash pins, root/native/retained runtime/security gates and result validation remain intact.
- Demonstrate actual cold and warm runs for Linux and Windows, including the Rust job's formal resident phase and later candidate phase. Record required OIDs/bytes, restored-object hits, actual missing fetches and save success separately from compile/test status.
- Verify changed-OID and invalid/missing cached-object rejection, cancellation **after** early save, parallel cold consumers, and nightly self-hosted reuse behavior without manufacturing a passing product result. A failed or cancelled test remains failed/cancelled despite a good cache.
- Do not report the estimation table as realized cost savings; compare actual fetch evidence to owner usage reports if available. No price/budget increase or bill approval is implied.

### Audit-return pushes also select CI

`scripts/ci_scope.py:16-25` currently allows only eight exact reviewed prose files as docs-only. **New files under artifacts/audit select full by design.** C's prior public evidence commits also fall into this full-selection class; they must be included when attributing CI traffic, rather than treating all report pushes as free. This observation does not prove any particular checkout completed or was billed. Do not broadly exclude artifacts/**: that tree also contains loaded DLL/SO candidates, manifests and inputs consumed by tests.

This audit is placed in the existing reviewed prose CI document and the existing joint handoff, without changing PROSE_ONLY, triggers, cancellation or tests. The current router therefore selects its existing docs path for this prose-only change. This is not a product CI pass. Any future scope change needs a separate dependency review, not an unreviewed escape from full candidate qualification.

Primary references: [GitHub LFS billing](https://docs.github.com/en/billing/concepts/product-billing/git-lfs) confirms Actions LFS downloads count against the repository owner's bandwidth; [restore action](https://github.com/actions/cache/blob/main/restore/README.md) and [save action](https://github.com/actions/cache/blob/main/save/README.md) support explicitly placed restore/save steps. This report uses these only for the proposed download/cache lifecycle, not to infer actual invoice amounts.

A please return `received_commit / CI_file_owner / audited_path_sets / planned_cache_key_and_early_save / cold_warm_run_refs / remaining_blocker`. Git publication is not an A acknowledgment. C has changed no workflow/helper/product file, dispatched or cancelled no CI run, downloaded no LFS object for this audit and modified no billing setting.


2026-10-25. Source baseline: `91a9a587`. This is CI scheduling, not a change to
NOVOVM execution, economic rules, signatures, quorum or release authorization.

## 2026-11-02 correction: main-only development, explicit human branch approval

The former text incorrectly called a suggested temporary branch explicitly
authorized. The user has directed its correction: preserve all existing work and
concurrent main commits through a normal history-preserving merge, then develop
on main only. No new branch may be created without the human user's prior,
explicit, direct authorization. CI batching is not such authorization.
This mainline correction does not qualify the accumulated candidate for release,
performance or deployment. Existing main push checks remain enabled and failures
must be reported honestly; do not use skip markers or weaken any workflow.

| Boundary | Required evidence |
| --- | --- |
| Each code change | Local formatting, affected compilation and targeted tests. Signature, authorization, quorum and durable-state changes need their own positive and adversarial tests immediately. |
| A coherent feature boundary | Relevant regression groups, strict Clippy and cross-module integration; retain failures and fix causes, not assertions or production timeouts. |
| Final candidate | One planned full candidate CI on a frozen SHA, required independent workflows, Windows/Linux original-node recovery, matching artifacts and separately recorded installation/multi-host evidence. |

The batch comprises bounded pre-authentication ML-DSA handshake carriage;
transport/device/service/receipt producer and consumer migration; purpose
separation and downgrade rejection; compatible platform artifacts and offline
A-D shared-configuration validation. Existing UCA, consensus, registration,
custody, NovoRUDP/APFL and AOEM owners remain the only implementation path.
Do not claim individual codec/interface commits close the whole batch.

Actual trigger audit of all three workflows:

- `ci.yml`: push is limited to `main`; all PRs can trigger it. Ordinary batch
  branch pushes alone do not run it. Manual `profile=full` already exists.
- `native-auth-relation.yml`: push is limited to `main` plus its path filters;
  matching PRs and manual dispatch also run it.
- `mainline-nightly-soak.yml`: schedule/manual only, on its existing historical
  comparison owner; not product mainchain acceptance.
- Scheduled runs still exist on the default branch. Do not imply batching
  disables them or makes unvalidated branch commits eligible for deployment.

At freeze, fetch and incorporate current main without overwriting other work,
finish local validation, and record the exact candidate SHA. For the existing
workflow, the integration owner runs:

```powershell
gh workflow run ci.yml --repo novovm/supervm --ref main -f profile=full
```

Verify the run's actual `headSha` equals the frozen candidate; do not identify a
run only by a moving branch name. Run the independent host-rules workflow when
its contract/dependencies were affected; its success is not a business ZK proof.
Do not run these commands for every checkpoint. If CI fails, inspect the actual
failure and rerun the necessary corrected candidate; "once" is a planning goal,
not a prohibition on required verification after a fix.

After all required results pass, qualify only that reviewed main SHA for the
next delivery stage. A concurrent change that changes the candidate must be
integrated and revalidated, not force-pushed.
Current main protection was read-only checked: no required status checks or PR
review were configured; this does not waive the above delivery gate, and no
protection settings were changed.

**Main's automatic checks remain enabled.** A manual candidate run followed by
a main push can still produce a second run. This change removes repeated
intermediate runs; it does not promise exactly one cloud run, bypass main's
checks with skip markers, or introduce an unreviewed result-reuse controller.

### Baseline run observation, not whole-run signoff

On 2026-10-26 (Asia/Shanghai), run `40222837834` for `384afc4e` was still running.
The Linux `Integrated original tick and native SDK closure` and `NUAS
original-node process recovery and credited NOV respend` steps both succeeded.
Security and the Linux preserved-component job succeeded. The Windows
preserved-component job `121257995233` was cancelled with the explicit annotation
`The job has exceeded the maximum execution time of 15m0s`; ordinary tests had
passed, and the later release/include-ignored step was interrupted. This is not
an assertion failure or a passed job. It must be addressed before final batch
qualification; do not silently increase product protocol deadlines to fix a CI
job wall-clock budget. The current run is not cancelled/restarted by this branch
preparation, and neither the new branch nor D's package is deployment-qualified.

### Preserve coverage while separating lint/debug and release budgets

The completed Windows job's API timestamps and raw log identify a cumulative
job-budget failure, not a failing Rust assertion:

| Original phase | Observed duration in job `121257995233` |
| --- | --- |
| Checkout, pinned toolchain, boundary checks and fmt | About 68 seconds from job start |
| Workspace Clippy plus no-default-features host Clippy | 110 seconds |
| Entire debug workspace test step | 309 seconds, passed; includes 157 seconds of compilation |
| Platform AOEM fetch | 4 seconds |
| Release step | Started 8m11s after job start, leaving only 6m49s of the existing 15-minute budget |
| Release compilation | 4m35s, succeeded |
| Release tests before cancellation | AOEM 32/32 and host 414/414 passed, as did the five host integration targets (11 tests); network's 211-test suite was still running when the runner cancelled the job |

Sources: [job and its step timestamps](https://github.com/novovm/supervm/actions/runs/40222837834/job/121257995233)
and its raw job log. The unfinished network suite and later doc tests are NOT
counted as passed. This observation does not establish that the last displayed
network test was hung, or that transport/signature semantics caused the timeout.

The workflow now keeps two independent jobs, each with the original 15-minute
CI job limit and both Ubuntu/Windows platforms. Both depend on scope selection,
not on one another; their concurrency groups are distinct. Matrix fail-fast is
disabled so one platform failure does not cancel evidence from the other.

| Original coverage | Current owner on **each** platform |
| --- | --- |
| Original/preserved workspace boundary scripts, fmt, full all-targets strict Clippy, no-default-features host Clippy | `runtime_modules` (existing job name retained) |
| `cargo test --manifest-path runtime/Cargo.toml --workspace --locked` | `runtime_modules`; unchanged debug workspace command |
| Fetch the platform-specific AOEM library, set its explicit absolute test path | `runtime_modules_release` |
| `cargo test --manifest-path runtime/Cargo.toml --workspace --release --locked -- --include-ignored --test-threads=1` | `runtime_modules_release`; unchanged entire release workspace command, ordinary **and** ignored tests, integration targets and doc tests |

No package list or test-name filter is introduced: future workspace members
remain selected by `--workspace`. `--include-ignored` is not replaced by
`--ignored`. No product, consensus, recovery or network deadline changes.
Both jobs are required by `CI scope result`; failure, cancellation, missing
result or unexpected skip in either fails the selected full scope. Both remain
skipped for docs/security-only scope. The original root-product Rust job and
independent workflows are unchanged.

This removes the measured shared-budget contention; it does not claim fewer
total billed runner minutes or guarantee a future runner completes within its
budget. Separate runners repeat setup/dependency work. Local validation covers
the workflow, complete command/platform mapping and fail-closed result routing;
the final frozen candidate still needs an actual complete cloud run. This slice
does not dispatch or cancel CI and does not rebuild Rust locally.

## Reused checks, conservative selection

The existing `.github/workflows/ci.yml` still owns all existing test commands.
`scripts/ci_scope.py` replaces repository-presence-only detection with a complete
push-range / PR merge-base comparison. Only the exact reviewed prose files in
`PROSE_ONLY` select `docs`. They are not compiled or consumed by deployment gates
in the reviewed tree. Changes to the allowlist itself require full checks.

Do not generalize to `docs/**` or `*.md`: `docs/NOVORUDP_BOUNDARY.md` is read by
Rust `include_str!`; package/boundary scripts also consume other documentation.
New docs, scripts, configs, lockfiles, SDK binaries, code and any unknown path
select `full`. Renames are compared as deletion plus addition, so moving code
into an allowed prose path cannot hide the old code. Missing/invalid history,
malformed event data and empty/unknown comparisons select full checks; failure
of the router or its tests cannot produce a passing result.

This first slice intentionally does NOT attempt crate-level dependency pruning.
Product changes still run the complete existing checks, including historical
comparison cases and preserved runtime components. No assertions, tests or
security advisories are ignored or removed.

| Trigger | Selected scope |
| --- | --- |
| Push/PR touching only exact reviewed prose | Router tests + UTF-8/NUL prose validation + explicit scoped result; no product build |
| Any other push/PR, including CI changes | Complete existing CI suite |
| Daily `17 3 * * *` UTC (11:17 Asia/Shanghai) | Existing audit/deny security checks plus router/result |
| Weekly Sunday `47 3 * * 0` UTC (11:47 Asia/Shanghai) | Complete existing CI suite |
| Manual dispatch, default `profile=full` | Complete existing CI suite on the selected ref/SHA |
| Manual dispatch `profile=security` | Security only, explicitly not product qualification |

`native-auth-relation.yml` retains its existing independent path filters.
`mainline-nightly-soak.yml` is unchanged and remains historical comparison,
not a substitute for the active mainchain acceptance suite. The weekly profile
does not claim to execute every independent workflow or real multi-device tests.

## Cancellation and truthful status

Workflow-wide `cancel-in-progress` is removed. Only selected heavy jobs enter
their own ref/job/platform concurrency groups; a docs-only job is skipped before
that group and must not cancel a running code check. New automatic code checks
may supersede old automatic checks. Manual/scheduled runs use unique run IDs,
so later pushes do not cancel a pinned candidate run. A user can still cancel
an unnecessary run explicitly. No automatic candidate-run cancellation is added.

`CI scope result` requires routing success and the correct terminal result of
every selected job. Failure, cancellation or unexpected skipping is not success.
It also rejects inconsistent scope/output combinations. Docs/security-only
success explicitly says product tests were NOT EXECUTED; it is not full green,
deployment qualification, mobile acceptance or permission to activate assets.

Current remote `main` protection was inspected read-only: required_status_checks
was null. No protection or repository settings were changed. Existing job names
(including `Rust checks`) are retained. If required checks are configured later,
use the explicit scoped result with a separate full candidate acceptance policy;
do not use workflow-level path skipping that leaves required runs pending.

## Validation and limits

- All 13 local Python unit/fixture tests pass, covering reviewed prose, unknown/embedded docs,
  code/config/dependency changes, schedules, manual profiles, missing history,
  malformed payloads, missing product manifest, real Git rename/PR/push ranges,
  earlier code hidden behind a later prose commit, result failure/cancellation/missing
  jobs, and the retained debug/release workspace commands, platform matrices,
  independent budgets and required result dependencies.
- The original routing change's parsed old/new workflow comparison confirmed all
  78 non-router steps unchanged; the later budget split above relocates the same
  runtime release command and adds its independent runner setup. Actual historical
  ranges `41782197..e4018716` (four files) and
  `e4018716..91a9a587` (three files) both select docs.
- `actionlint` 1.7.12 validates the workflow (shellcheck/pyflakes disabled locally);
  downloaded Windows tool archive was checked against upstream release SHA-256.
- No Rust rebuild or TPS measurement is required to claim this routing change.
  Remote routing and cancellation behavior still require actual GitHub runs;
  local fixtures do not prove cloud scheduling. The CI-change commit itself must
  take the full route, not an artificially light one.

For a candidate, select Actions → CI → Run workflow → `full`, record the actual
head SHA, and verify its complete results. Continue independent development with
relevant local tests; do not treat unrelated cancellation as a product defect.
Known security, financial or consensus failures remain blockers for their affected
delivery, not something to hide by selecting a lighter profile.

## Actual GitHub routing evidence

Code commit `42464842` selected full checks in
[40118293960](https://github.com/novovm/supervm/actions/runs/40118293960).
The subsequent real handoff document commit `834c1c59` selected docs in
[40118394541](https://github.com/novovm/supervm/actions/runs/40118394541): success,
25 seconds from run start to final update, only scope selection and result jobs
executed, product/security jobs explicitly skipped. The earlier full code run
was still in progress after the docs run completed; it was not cancelled by
that push. This validates the automatic docs/code separation for this sample,
not manual/scheduled concurrency, product full green or any throughput gain.
