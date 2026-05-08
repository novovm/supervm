# NOVOVM Account Protocol Phase 3 Implementation Checklist (2026-04-20)

Status: AUTHORITATIVE CHECKLIST (Phase 3)  
Scope: implementation gate for unified asset view on the real mainline path

## Goal

`Phase 3` does one thing only:

`build the unified asset view`

This phase is not about creating a new unified asset ledger. It is about:

- making existing asset sources visible under `account_id` on the real mainline read path
- expanding aggregation coverage in a controlled way
- normalizing read-side semantics for balance, locked value, collateral, debt, and similar fields

## Cut order

1. `Cut 1`: establish the real mainline read surface
   - `account_balance`
   - `account_assets`
   - `ownership_subject = account_id`
   - aggregation from existing `native_execution_store`
2. `Cut 2`: expand view sources only
   - established: `pledge`
   - established: `treasury exposure`
   - only existing asset sources that already exist on the real mainline path may be added
   - read-side aggregation only, not a new ledger
   - not yet established: `staking`
3. `Cut 3`: normalize read-side semantics
   - unify `available / locked / collateral / debt / reserved / pending` view semantics
   - still a presentation contract, not a ledger redesign

## Dual-track boundary

This phase allows:

- input compatibility for `account_id / uca_id`
- multiple existing module sources behind the read surface
- subject-level aggregation across those sources

This phase does not allow:

- presenting aggregated view output as if a unified ledger already exists
- promoting temporary aggregation fields into ledger-commitment fields
- allowing non-`account_id` subjects back into canonical asset ownership semantics
- collapsing one asset from multiple sources into a source-less single truth

Every cut must include at least one “existing asset sources -> aggregated view output” sample that shows:

- which `account_id` is queried
- which existing sources are aggregated
- how each source maps into the unified output fields
- which `account_id` owns the final output

## Explicit prohibitions

The following are explicitly forbidden in this phase:

- adding a unified asset ledger
- introducing `asset_root`
- folding the mapped-asset master protocol into `Phase 3`
- folding privacy-subaccount asset space into `Phase 3`
- presenting proof-structured asset state as current product fact
- presenting a single-module balance view as if the unified asset system is already complete
- turning `account_assets / account_balance` into write paths or ledger entry points
- faking `staking` or any other asset view without a real runtime state source

## Merge gate

The merge gate is one sentence:

`Phase 3 may expand the unified asset view only; it must not smuggle in a unified asset ledger.`

Minimum PR acceptance must satisfy all of the following:

- the new or expanded asset-view method is owned by `account_id`
- aggregation sources are explicitly listed
- if one asset appears across multiple sources, the output remains source-distinguishable rather than collapsing into one opaque number
- the output is explicitly documented as a view, not a new ledger commitment
- `account_assets / account_balance` are explicitly preserved as read-only interfaces
- no `asset_root`
- no mapped asset settlement protocol
- no privacy-subaccount asset space

Recommended code-review wording:

`If a change cannot prove that it only aggregates existing asset sources under account_id ownership, or if it turns account_assets / account_balance into write paths, or if it implicitly introduces a new ledger, asset_root, mapped-asset master protocol, or privacy asset space, it must not merge into mainline.`
