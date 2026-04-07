# NOVOVM PowerShell Mainline Entry Removal Audit

Date: 2026-04-08

## Scope

This audit covers the five PowerShell scripts that still exist on the NOVOVM mainline path surface:

1. `scripts/novovm-up.ps1`
2. `scripts/novovm-node-rollout-control.ps1`
3. `scripts/novovm-node-lifecycle.ps1`
4. `scripts/novovm-node-rollout.ps1`
5. `scripts/novovm-prod-daemon.ps1`

## Current engineering conclusion

Mainline command authority and default command generation have already been collected to `novovmctl`.

These five `.ps1` files are no longer part of the NOVOVM mainline entry path. They now exist only as deprecated compatibility shells.

PowerShell mainline entry removal is therefore in the final cleanup stage, not in the mainline logic migration stage.

## Classification rules

### A. Immediate physical delete

All of the following must be true:

1. No mainline document or default generation path points to the script.
2. No active repo code path shells out to the script.
3. No compatibility retention requirement remains.

### B. Retain as deprecated compatibility shell

Use when:

1. Mainline entry authority is already removed.
2. The script still has compatibility value.
3. Repo-internal blockers still justify keeping the shell in place.

### C. Defer physical deletion

Use when:

1. Repo-internal blockers are already gone.
2. Physical deletion is waiting only on external dependency confirmation or a stability window.

## Audit result

### A. Immediate physical delete

Current batch: none.

Reason:

1. The five scripts have already been downgraded from mainline entry to deprecated compatibility shells.
2. Repo-internal blockers have been removed.
3. Immediate physical deletion still waits on external dependency confirmation and one stability window.

### B. Retain as deprecated compatibility shell

Current batch: none.

Reason:

1. Repo-internal live dependencies have been removed.
2. Repo-internal executable command examples for these shells have been removed.
3. Remaining retention is now purely external-dependency and stability-window based.

### C. Defer physical deletion

Current batch:

1. `scripts/novovm-up.ps1`
2. `scripts/novovm-node-rollout-control.ps1`
3. `scripts/novovm-node-lifecycle.ps1`
4. `scripts/novovm-node-rollout.ps1`
5. `scripts/novovm-prod-daemon.ps1`

Reason:

1. Repo-internal live dependencies have been removed.
2. Repo-internal executable command examples have been removed; remaining mentions are historical records, tasklists, audits, and compatibility statements.
3. Physical deletion now requires only external dependency confirmation and one stability window.

## First physical deletion batch

Current batch after external dependency confirmation:

1. `scripts/novovm-up.ps1`
2. `scripts/novovm-node-rollout-control.ps1`
3. `scripts/novovm-node-lifecycle.ps1`
4. `scripts/novovm-node-rollout.ps1`
5. `scripts/novovm-prod-daemon.ps1`

This is the first credible delete-candidate batch.

The correct state on 2026-04-08 is:

1. Mainline authority has been moved to `novovmctl`.
2. The five `.ps1` files have been downgraded to deprecated compatibility shells.
3. Repo-internal live dependencies have been removed.
4. Repo-internal executable command examples for all five shells have been removed.
5. The entire five-script set is now waiting only on external invocation confirmation and one stability window.

## Next cleanup actions

1. Confirm CI, deployment automation, and operator environments do not still call these five shells as primary entry.
2. Keep one stability window with `novovmctl` as the only documented/default entry.
3. If external invocation confirmation is clean, physically delete:
   - `scripts/novovm-up.ps1`
   - `scripts/novovm-node-rollout-control.ps1`
   - `scripts/novovm-node-lifecycle.ps1`
   - `scripts/novovm-node-rollout.ps1`
   - `scripts/novovm-prod-daemon.ps1`
4. Re-run this audit only if new local `.ps1` command surfaces are reintroduced.

## Formal audit conclusion

These five PowerShell entry shells are repo-clean and machine-local-clean, but physical retirement remains blocked on off-repo dependency confirmation.

## Final status line

`Rust policy core`: complete

`Rust runtime shell`: complete

`PowerShell mainline entry removal`: final cleanup stage in progress
