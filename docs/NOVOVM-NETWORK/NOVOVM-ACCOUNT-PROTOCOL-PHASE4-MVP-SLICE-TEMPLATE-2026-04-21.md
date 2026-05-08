# NOVOVM Account Protocol Phase 4 MVP Slice Template (2026-04-21)

Status: AUTHORITATIVE EXECUTION TEMPLATE (Trigger-Activated)  
Scope: minimum internal validation slice that may run only after `Phase 4` trigger review returns `Go`

## Purpose

This template defines how to execute a `Phase 4` minimal slice after trigger approval.

It is not a feature roadmap and not an external-launch plan.

Default state remains unchanged:

`Phase 4 = No-Go unless trigger-approved.`

## Activation precondition

This template may be used only when all conditions below are true:

1. `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md` is approved as `Go`
2. Structural-change PR payload is complete under:
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`
3. Scope is internal validation only (no external release)

If any one is false, decision is:

`Do not start Phase 4 implementation slice.`

## Hard boundaries (must stay true)

- no new mapped-asset ledger
- no `asset_root` or new global root source
- no multi-chain framework
- no privacy-asset ledger space
- no write-path conversion for `account_balance / account_assets`
- canonical subject must remain `account_id`

## MVP slice declaration block (fill before coding)

Use this exact block in the PR description:

```text
Trigger checklist:
Trigger item IDs:
Decision request: Go

Slice scope:
- source chain:
- source asset:
- target asset representation:
- ownership subject: account_id

Storage statement:
- no new ledger:
- no new root:
- no new global asset truth source:

Flow:
- lock -> register -> visible-in-view -> burn -> release

Rollback boundary:
- code boundary:
- state boundary:
- abort condition:
```

## Mandatory invariants

The implementation must prove all of the following:

1. supply conservation over the tested slice
2. every mapped unit is traceable to `lock_id -> source_tx_hash`
3. no new global asset truth source is introduced
4. audit chain is continuous for register/burn/release
5. release requires prior burn state

## Minimum test set

1. valid lock proof -> register success -> visible in `account_assets`
2. duplicate lock proof rejected
3. invalid proof rejected
4. burn transitions `active -> burn_pending`
5. release without burn rejected
6. burn + release reaches `released` and is no longer active in view
7. audit trace resolves `account_id -> mapping_id -> lock_id -> source_tx_hash`

## Completion criteria

A slice is considered complete only when all are true:

- invariants pass
- required tests pass
- `cargo clippy` and existing unified-account gate remain green
- no hard boundary is violated

## Decision outputs

- `Continue in internal validation`: slice passed and boundaries preserved
- `No-Go rollback`: any invariant/boundary fails
- `Candidate for controlled expansion`: only after explicit new trigger review

## Related documents

- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MAPPED-ASSET-CONSTRAINT-DRAFT-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-FAILURE-MODES-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`
