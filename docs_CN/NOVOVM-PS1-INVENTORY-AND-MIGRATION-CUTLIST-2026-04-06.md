# NOVOVM PS1 Inventory and Migration Cutlist 2026-04-06

## Scope

Total `.ps1`: `111`

- `scripts/*.ps1` mainline roots: `18`
- `scripts/aoem/*.ps1`: `5`
- `scripts/migration/*.ps1`: `88`

## Rule

- Normal main path: Rust `novovm-rollout-policy`
- PowerShell: startup shell, ops shell, env injection, audit, emergency fallback
- Legacy compatibility is allowed only as a thin wrapper
- Duplicate real logic should not remain in `.ps1`

## A. Keep as mainline shells

- `scripts/novovm-up.ps1`
- `scripts/novovm-prod-daemon.ps1`
- `scripts/novovm-node-rollout.ps1`
- `scripts/novovm-node-rollout-control.ps1`
- `scripts/novovm-node-lifecycle.ps1`
- `scripts/novovm-ua-prod-store.ps1`
- `scripts/novovm-l1l4-settlement-cycle.ps1`
- `scripts/novovm-l1l4-reconcile.ps1`
- `scripts/novovm-l1l4-reconcile-daemon.ps1`
- `scripts/novovm-l1l4-real-broadcast.ps1`
- `scripts/novovm-l1l4-payout-execute.ps1`
- `scripts/novovm-l1l4-external-confirm.ps1`
- `scripts/novovm-l1l4-auto-payout.ps1`
- `scripts/novovm-generate-role-matrix.ps1`

## B. Thin wrappers now sealed to Rust

- `scripts/novovm-rollout-decision-dashboard-export.ps1` -> `novovm-rollout-policy rollout decision-dashboard-export`
- `scripts/novovm-rollout-decision-dashboard-consumer.ps1` -> `novovm-rollout-policy rollout decision-dashboard-consumer`
- `scripts/novovm-overlay-relay-health-refresh.ps1` -> `novovm-rollout-policy overlay relay-health-refresh`
- `scripts/novovm-overlay-relay-discovery-merge.ps1` -> `novovm-rollout-policy overlay relay-discovery-merge`

## C. Keep as AOEM build/package shells

- `scripts/aoem/build_aoem_manifest.ps1`
- `scripts/aoem/build_aoem_variants_current_os.ps1`
- `scripts/aoem/package_aoem_beta08.ps1`
- `scripts/aoem/sync_aoem_fullmax_bundle.ps1`
- `scripts/aoem/verify_aoem_binary.ps1`

## D. Freeze as migration/history scripts, not current policy-brain migration targets

- `scripts/migration/*`

These `88` files are release gates, migration gates, snapshot/report helpers, canaries, historical import/export helpers, and one-shot acceptance automation. They are not the current mainline strategy-core migration target. Keep frozen unless a specific script is reactivated into the mainline runtime path.

## E. Delete candidates

- None yet

Delete only after a separate external-usage audit confirms the file is not a required shell and not a compatibility surface.

## 2026-04-06 batch cleanup mode

### Batch A: mainline root-script thin-wrapper conversion

Completed in the current round:

- `scripts/novovm-rollout-decision-dashboard-export.ps1`
- `scripts/novovm-rollout-decision-dashboard-consumer.ps1`
- `scripts/novovm-overlay-relay-health-refresh.ps1`
- `scripts/novovm-overlay-relay-discovery-merge.ps1`

These files are no longer independent PowerShell implementations. They are compatibility shells only.

### Batch B: legacy bin retirement audit baseline

Tracked in:

- `docs_CN/NOVOVM-LEGACY-BIN-RETIREMENT-AUDIT-2026-04-06.md`

Policy:

- Green: delete
- Yellow: keep thin wrapper
- Red: hold

### Batch C: migration/history isolation

- `scripts/migration/*` is frozen as a history asset pool.
- `scripts/migration/README.md` is the directory-level rule file.
- These files are excluded from current mainline strategy-core cleanup unless explicitly reactivated.
