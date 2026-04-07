# NOVOVM PowerShell Entry External Confirmation Checklist

Date: 2026-04-08

## Purpose

This checklist closes the gap between:

1. repo-clean and machine-local-clean confirmation
2. off-repo dependency confirmation
3. final physical retirement of the five deprecated PowerShell entry shells

This checklist is part of `PowerShell mainline entry removal`.

It is not a new feature track.

## Current verified state

The following has already been confirmed on the current repository and current machine:

1. Mainline command authority has moved to `novovmctl`.
2. Repo-internal live dependencies on the five PowerShell entry shells have been removed.
3. Repo-internal executable command examples for the five PowerShell entry shells have been removed.
4. Machine-local scheduled tasks do not reference these five PowerShell entry shells.
5. Machine-local startup folders do not reference these five PowerShell entry shells.
6. No references were found in the local `D:\WEB3_AI` search scope outside the repository.

## Retirement target set

The retirement target set is fixed:

1. `scripts/novovm-up.ps1`
2. `scripts/novovm-node-rollout-control.ps1`
3. `scripts/novovm-node-lifecycle.ps1`
4. `scripts/novovm-node-rollout.ps1`
5. `scripts/novovm-prod-daemon.ps1`

## Required off-repo confirmation domains

The following domains must be checked before physical deletion:

1. CI runners and pipeline machines
2. Deployment hosts and release automation
3. Operations workstations and scheduled admin jobs
4. Off-repo operator manuals / local runbooks / copy-pasted shell snippets

## Confirmation table

| Domain | Required check | Status |
| --- | --- | --- |
| CI runners | No pipeline step calls any of the five `.ps1` files | Pending |
| Deployment hosts | No deploy script or host-local wrapper calls any of the five `.ps1` files | Pending |
| Ops workstations | No scheduled/manual command still uses any of the five `.ps1` files as primary entry | Pending |
| Off-repo manuals | No off-repo SOP/runbook still instructs operators to use any of the five `.ps1` files | Pending |

## Stability window gate

Before physical deletion, hold one stability window under the following rules:

1. Mainline commands are executed only through `novovmctl`.
2. No new `.ps1` executable examples are reintroduced into repo docs.
3. No incident or fallback forces reactivation of the PowerShell entry shells.

## Deletion authorization rule

Physical retirement of the five PowerShell entry shells is authorized only when:

1. All four off-repo confirmation domains are marked complete.
2. The stability window passes without reintroducing PowerShell entry usage.

## Final action

When the authorization rule is satisfied, delete the following as one batch:

1. `scripts/novovm-up.ps1`
2. `scripts/novovm-node-rollout-control.ps1`
3. `scripts/novovm-node-lifecycle.ps1`
4. `scripts/novovm-node-rollout.ps1`
5. `scripts/novovm-prod-daemon.ps1`

Do not retire them one by one.

Batch retirement is required to keep the mainline entry surface consistent.

## Final project state after deletion

1. `Rust policy core`: complete
2. `Rust runtime shell`: complete
3. `PowerShell mainline entry removal`: complete
