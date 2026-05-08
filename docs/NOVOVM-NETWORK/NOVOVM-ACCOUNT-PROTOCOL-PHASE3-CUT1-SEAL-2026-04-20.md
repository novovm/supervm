# NOVOVM Account Protocol Phase 3 / Cut 1 Seal (2026-04-20)

Status: FINAL SEAL (Phase 3 / Cut 1)  
Scope: unified asset view on the real mainline read surface, aggregated under `account_id`

## Objective

`Phase 3 / Cut 1` does one thing only:

`land the unified asset view on the real mainline read surface`

This cut does not introduce:

- a unified asset ledger
- `asset_root`
- a mapped-asset master protocol
- a privacy-subaccount asset space
- proof-structured asset state

## Established capabilities

The following are now established:

- `account_balance`
  - live on the real `novovm-node -> mainline_query -> unified_account_surface` read path
- `account_assets`
  - live on the real `novovm-node -> mainline_query -> unified_account_surface` read path
- `ownership_subject`
  - fixed to `account_id`
- data sources
  - existing `native_execution_store.account_asset_balances`
  - existing `native_execution_store.credit_vaults`
- aggregation semantics
  - subject-level aggregation over existing asset sources, without introducing a new ledger

This establishes:

`a mainline-grade, account-owned, aggregated asset visibility surface`

not:

`a new unified asset ledger`

## Explicitly not established

The following remain out of scope and are not claimed as complete:

- unified asset ledger
- `asset_root`
- mapped asset settlement protocol
- privacy subaccount asset space
- proof-structured asset state

What is established here is:

`a unified asset view`

not:

`a complete unified asset system`

## Current mainline path

The current product path is:

`novovm-node (bin) -> mainline_query -> unified_account_surface -> native_execution_store`

Current real read-surface method contract:

| Method | Current semantics | Status |
| --- | --- | --- |
| `account_balance` | query aggregated balance / collateral / debt view for a given `account_id` | sealed |
| `account_assets` | query aggregated asset and vault inventory for a given `account_id` | sealed |

## Validation results (locally executed on 2026-04-20)

This cut is sealed against the following local executions:

- `cargo fmt --all`
- `cargo test -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
  - result: `supervm mainline gate passed`
  - result: `L1=100% L2=100% L3=100% L4=100% Overall=100%`

Real entry-path regression was also added for:

- `account_balance`
- `account_assets`
- `ownership_subject = account_id`
- aggregation from existing `native_execution_store` sources

## Recommended external wording

`Phase 3 / Cut 1 is complete: the unified asset view is now live on the real mainline read surface, and asset ownership is aggregated under account_id without introducing a unified asset ledger.`
