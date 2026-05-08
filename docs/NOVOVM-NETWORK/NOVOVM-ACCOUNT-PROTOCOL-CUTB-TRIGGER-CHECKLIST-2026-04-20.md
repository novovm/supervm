# NOVOVM Account Protocol Cut B / AccountMode Trigger Checklist (2026-04-20)

Status: AUTHORITATIVE TRIGGER CHECKLIST (Cut B / AccountMode)  
Scope: define the only valid trigger conditions for introducing `AccountMode` as an optional label layer in rare cases; until those conditions are satisfied, unified account must remain in the state of `Cut A` sealed and `Cut B` default `No-Go`

## Purpose

This document answers one question only:

`When unified account is allowed to introduce AccountMode (Cut B) as an optional label layer.`

This document is not a Cut B design note and not a Cut B implementation plan.

More importantly:

`Cut B` is not a required layer in the current unified-account architecture.

Its role is to:

- turn `Cut B` from "something we may do later" into "something we do not do by default and may enter only when triggered"
- prevent premature entry into `Cut B` for reasons such as "cleaner interface" or "more elegant structure"
- preserve the current stable state of unified account:
  - `account_id` subject layer complete
  - `asset view` layer complete
  - `KeyAlgo / Cut A` implemented and sealed
  - mainline capability expressed by `KeyAlgo + ExecutionPolicy`

## ALL REQUIRED

`Cut B` may start only when all of the following are true:

### 1) `KeyAlgo / Cut A` is stably running

All of the following must be true:

- multi-algorithm binding metadata is stable on the real mainline path
- the minimal `secp256k1 / ed25519 / mldsa87` support has not polluted subject semantics
- `Cut A` has no compatibility issue requiring rollback
- `account_id` remains the only canonical subject

### 2) A real "default behavior" need exists

All of the following must be true:

- at least one real production scenario requires:
  - `the account needs a stable presentation label or control-plane hint`
- the demand comes from real product, operations, or control-plane behavior
- the reason is not merely "cleaner design" or "more unified interface"
- the label is not meant to participate in execution, asset, or security paths

### 3) The need cannot be expressed by `KeyAlgo` alone

All of the following must be true:

- the demand cannot be expressed by `KeyAlgo` and does not belong to `ExecutionPolicy`
- if `KeyAlgo` already expresses the need, `Cut B` must not start
- if `ExecutionPolicy` already expresses the need, `Cut B` must not start
- `AccountMode` must not become a rename layer for `KeyAlgo`
- `AccountMode` must not become a proxy layer for `ExecutionPolicy`

### 4) Existing phase constraints remain intact

All of the following must be true:

- no new global asset state source is introduced
- no unified asset ledger is touched
- no mapped-asset supply or settlement protocol is touched
- no privacy asset space is touched
- no supply / audit / ownership invariant is changed
- nothing violates `Phase 4` `Constraint Draft` or `Failure Modes`

## Explicit No-Go conditions

Any one of the following is enough to block `Cut B`:

- the change is driven only by "interface unification"
- the change is designed only for "possible future need"
- the current need is already expressible by `KeyAlgo`
- the current need is already expressible by `ExecutionPolicy`
- entering `Cut B` would require a new ledger or a new state source
- entering `Cut B` would change `account_id` semantics
- entering `Cut B` would make `KeyAlgo` indirectly trigger privacy, PQ, or hybrid behavior
- entering `Cut B` would make `AccountMode` participate in execution routing, asset semantics, or security constraints
- entering `Cut B` would affect the read-only aggregation boundary of `account_balance / account_assets`

## One-line veto rules for code review

The following lines may be used directly in code review:

- `This change introduces AccountMode without satisfying trigger conditions.`
- `This change attempts to encode behavior in AccountMode that belongs to KeyAlgo or ExecutionPolicy.`
- `This change advances Cut B without a real production requirement.`
- `This change turns AccountMode into a backdoor for privacy, PQ, or hybrid execution behavior.`
- `This change makes AccountMode a mainline semantic layer instead of an optional label.`

## Current conclusion

The formal current verdict for `Cut B / AccountMode` is:

`No-Go`

The reason is not that `Cut B` is forbidden forever, but that:

- `Cut A` is sealed
- there is no real label-type demand that cannot already be expressed by `KeyAlgo + ExecutionPolicy`
- the system should stay in its current stable stage of "frozen core + condition-triggered extension"

The correct current positioning of `Cut B` is therefore:

`an optional label layer, not a mainline semantic layer.`

## Recommended external wording

`Cut B / AccountMode has not entered implementation and is not a required mainline layer. It may start only when unified account faces a real label-type requirement that cannot be expressed by KeyAlgo or ExecutionPolicy and does not violate existing phase constraints.`
