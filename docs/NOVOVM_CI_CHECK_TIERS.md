# CI scope and candidate validation

2026-10-25. Source baseline: `91a9a587`. This is CI scheduling, not a change to
NOVOVM execution, economic rules, signatures, quorum or release authorization.

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
