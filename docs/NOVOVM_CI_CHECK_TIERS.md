# CI scope and candidate validation

2026-10-25. Source baseline: `91a9a587`. This is CI scheduling, not a change to
NOVOVM execution, economic rules, signatures, quorum or release authorization.

## 2026-10-26 user-approved batch development

Use the single integration branch `dev/mldsa65-identity-closure`, based on
`384afc4e`, for the unified ML-DSA-65 identity batch. This is the user's explicit
exception to the earlier main-only development instruction; it does not permit
an architectural fork. Intermediate commits remain reviewable and are pushed
promptly for C/D coordination, but do not open a PR for each internal slice.

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
gh workflow run ci.yml --repo novovm/supervm --ref dev/mldsa65-identity-closure -f profile=full
```

Verify the run's actual `headSha` equals the frozen candidate; do not identify a
run only by a moving branch name. Run the independent host-rules workflow when
its contract/dependencies were affected; its success is not a business ZK proof.
Do not run these commands for every checkpoint. If CI fails, inspect the actual
failure and rerun the necessary corrected candidate; "once" is a planning goal,
not a prohibition on required verification after a fix.

After all required results pass, integrate only the reviewed candidate into
main, preferably fast-forward if shared history permits. A concurrent change
that changes the candidate must be integrated and revalidated, not force-pushed.
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

- All 11 local Python unit/fixture tests pass, covering reviewed prose, unknown/embedded docs,
  code/config/dependency changes, schedules, manual profiles, missing history,
  malformed payloads, missing product manifest, real Git rename/PR/push ranges,
  earlier code hidden behind a later prose commit, and result failure/cancellation.
- Parsed old/new workflow comparison confirms all 78 non-router steps are
  unchanged. Actual ranges `41782197..e4018716` (four files) and
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
