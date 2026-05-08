# NOVOVM Account Protocol Phase 4 Mapped-Asset Constraint Draft (2026-04-20)

Status: AUTHORITATIVE CONSTRAINT DRAFT (Phase 4)  
Scope: frozen activation conditions, invariants, and anti-mixing rules before the mapped-asset master protocol may enter implementation

## Purpose

This document does not implement `Phase 4`. It answers three questions only:

- when `Phase 4` is allowed to start
- which invariants the mapped-asset master protocol must satisfy
- how to prevent bridge / mint / ledger / privacy from collapsing into one mixed layer

The purpose is to constrain `Phase 4` before implementation starts.

## Current prerequisite state

The current unified-account stage is:

- `v1-min`: complete
- `Phase 2`: complete
- `Phase 3 / Cut 1`: complete
- `Phase 3 / Cut 2`: complete

What is established now:

- `account_id` as the canonical subject
- account-first execution
- unified asset view on the real mainline read surface

What is not established now:

- unified asset ledger
- `asset_root`
- mapped-asset master protocol
- privacy-subaccount asset space
- proof-structured asset state

The correct `Phase 4` goal is therefore not:

`rebuild the asset system`

but:

`converge mapped assets into one master protocol`

## Activation conditions

`Phase 4` mainline implementation is allowed only when all of the following are true:

1. the unified asset view is sealed and already part of the public documentation surface
2. mapped-asset audit and ownership already land stably on `account_id`
3. `account_balance / account_assets` can already expose mapped-asset visibility in a stable way
4. custody / risk / audit boundaries can already be described and frozen independently
5. the mapped-asset master protocol does not require `asset_root` as a prerequisite

If any of these are false, `Phase 4` must not be treated as an implementation phase yet.

## Global hard constraint

`Phase 4` must not introduce a new global asset state source.

The only allowed state truths are:

- external lock proofs and their traceable references
- existing execution modules
- existing clearing modules
- already sealed treasury / risk / audit mainline modules

This means:

- `Phase 4` is not the starting point of a new ledger
- the mapped-asset protocol must not grow an extra global asset state source
- every new state transition must still trace back to either external lock truth or an existing mainline-module truth

## Core invariants

`Phase 4` must satisfy all of the following invariants:

### 1) Subject invariant

- mapped-asset ownership must be bound to `account_id`
- external addresses, bridge addresses, and custody addresses are not canonical asset owners
- audit, mint, burn, and redeem ownership must all trace back to `account_id`

### 2) One-mapping invariant

- each mapped asset must correspond to an explicit `source_chain + source_asset + proof_policy + custody_boundary`
- one mapped asset symbol must not hide multiple incompatible proof or custody semantics
- names such as `nETH / nBTC / nUSDT` must be protocol objects, not marketing aliases

### 3) Supply invariant

- mapped-asset supply changes must come only from protocol-defined `mint / burn / redeem` flows
- no writes may originate from `account_assets`, the view layer, or any read-only aggregation path
- temporary reporting fields must never become supply truth

### 4) Audit invariant

- lock / proof / mint / burn / redeem must form one traceable audit chain
- every mapped-asset state change must answer:
  - who owns it
  - why it exists
  - which proof justified it
  - which rule allows it to burn or redeem

### 5) Risk invariant

- custody policy, proof policy, redeemability, and risk boundary must be frozen independently
- risk controls must not be scattered across per-chain special scripts
- no single-chain special path may bypass the common protocol

## Explicit prohibitions

The following are explicitly forbidden in `Phase 4`:

- treating bridge implementation as the mapped-asset protocol itself
- turning mint logic directly into a unified asset ledger
- binding ledger redesign and mapped-asset protocol into one rollout
- mixing privacy-subaccount asset space with mapped-asset protocol in one step
- introducing `asset_root` as a prerequisite for entering `Phase 4`
- introducing a new global asset state source in `Phase 4`
- presenting a single-chain exception path as if the master protocol were already established

## Layering rule

`Phase 4` must preserve the following layering:

1. `bridge / custody`
   - lock, proof ingress, external references
2. `mapped asset protocol`
   - common protocol objects and main-flow contracts
3. `asset view`
   - display and aggregation only, never supply writes
4. `ledger / root`
   - not introduced in this phase

If one design change modifies three or more of these layers at once, it should be treated as a phase violation by default.

## Minimal protocol-object constraint

When `Phase 4` begins, it should freeze protocol-object semantics first, not ledger structure:

- `mapping_id`
- `source_chain`
- `source_asset`
- `proof_policy_id`
- `custody_policy_id`
- `target_asset_id`
- `redeemable`
- `mint_flow_contract`
- `burn_flow_contract`

These objects define protocol boundaries, not a future ledger commitment.

## Exit conditions

`Phase 4` may be sealed only when all of the following are true:

1. at least one major mapped-asset family runs under the common protocol
2. the unified audit chain is stable
3. ownership lands stably on `account_id`
4. the view layer can display mapped assets without becoming a write path
5. the protocol can be sealed without introducing `asset_root`

## Protection rule for Phase 3

Before `Phase 4` starts, `Phase 3` must remain protected:

- `account_assets / account_balance` are read-only
- they must not become write paths
- they must not become ledger entry points
- any attempt to write asset state back through the view layer is a design violation

## Recommended external wording

`Phase 4 currently exists only as a constraint draft, not as a completed capability. The only established asset-layer capability remains the unified asset view on the real mainline read surface; the mapped-asset master protocol has not yet entered mainline implementation.`

## Related document

- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-FAILURE-MODES-2026-04-20.md`
