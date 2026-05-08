# NOVOVM Account Protocol Phase 3 / Cut 2 Seal (2026-04-20)

Status: FINAL SEAL (Phase 3 / Cut 2)  
Scope: extend the unified asset view to real `pledge` and `treasury exposure` sources while preserving `account_id` ownership, explicit `source`, and explainable `components`

## Objective

`Phase 3 / Cut 2` does one thing only:

`expand unified-asset-view sources while remaining aggregation-only, read-only, and non-ledger`

This cut does not introduce:

- a synthetic `staking` view
- a unified asset ledger
- `asset_root`
- a mapped-asset master protocol
- a privacy-subaccount asset space
- proof-structured asset state

## Established capabilities

The following are now established:

- `account_balance`
  - now returns explicit `components`
  - no longer compresses multiple sources into one opaque truth
- `account_assets`
  - now returns structured account-owned asset exposure output
- `pledges`
  - now present on the real read surface from existing `credit_vaults`
- `treasury_exposures`
  - now present on the real read surface from existing `treasury_settlement_journal`
- `ownership_subject`
  - remains fixed to `account_id`
- `source`
  - remains explicit on each aggregated result
- aggregation semantics
  - continue to read from existing real state sources only
  - do not introduce any new global asset state source

This upgrades the unified asset view from:

`account_id -> one balance number`

to:

`account_id -> a set of explainable asset-exposure components with source and classification`

## Current structured-output facts

The current read surface now exposes:

- `components`
  - `liquid_balance`
  - `pledge_locked_collateral`
  - `debt_outstanding`
  - `treasury_source_flow`
  - `treasury_settled_nov`
  - `treasury_reserve_bucket_exposure`
  - `treasury_fee_bucket_exposure`
  - `treasury_risk_buffer_exposure`
- `pledges`
  - sourced from existing `credit_vaults`
- `treasury_exposures`
  - sourced from existing `treasury_settlement_journal`

What is established here is:

`an explainable unified asset view`

not:

`a unified asset ledger`

## Explicitly not established

The following remain out of scope and are not claimed as complete:

- `staking`
- unified asset ledger
- `asset_root`
- mapped asset settlement protocol
- privacy subaccount asset space
- proof-structured asset state

Important note:

- there is currently no stable runtime `staking` state source that can be cleanly owned by `account_id`
- therefore this cut does not fake a `staking` view just to look more complete

## Current mainline path

The current product path is:

`novovm-node (bin) -> mainline_query -> unified_account_surface -> native_execution_store`

Current real read-surface method contract:

| Method | Current semantics | Status |
| --- | --- | --- |
| `account_balance` | query structured balance / collateral / debt / treasury-exposure view for a given `account_id` across existing sources | sealed (Cut 2) |
| `account_assets` | query structured asset, position, and treasury-exposure inventory for a given `account_id` | sealed (Cut 2) |

## Validation results (locally executed on 2026-04-20)

This cut is sealed against the following local executions:

- `cargo fmt --all`
- `cargo test -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
  - result: `supervm mainline gate passed`
  - result: `L1=100% L2=100% L3=100% L4=100% Overall=100%`

Real entry-path regression was also added for:

- `pledge + treasury exposure` on `account_balance / account_assets`
- `ownership_subject = account_id`
- explicit `source`
- explicit `components`
- source-distinguishable output even when one asset appears across multiple sources

## Recommended external wording

`Phase 3 / Cut 2 is complete: the unified asset view now includes pledge and treasury exposure from real sources, remains owned by account_id, keeps source traceability and explainable components, and still does not introduce a unified asset ledger.`
