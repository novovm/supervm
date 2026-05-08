# NOVOVM Account Protocol Phase 4 Failure Modes (2026-04-20)

Status: AUTHORITATIVE FAILURE-MODE LIST (Phase 4)  
Scope: hard rejection list, structural-failure list, and review rules for the mapped-asset master protocol before implementation begins

## 1. Purpose

This document is not a design draft and not a roadmap.

It does one thing only:

`freeze the most dangerous and most common Phase 4 failure modes into a formal rejection list.`

This document is for:

- code review
- design review
- PR rejection
- protecting `Phase 3` from being polluted by `Phase 4`

## 2. Red-line failures

The following failures are immediate rejection conditions:

1. A new global asset state source is introduced
   - `Phase 4` must not become the starting point of a new ledger
2. Mapped assets bypass `account_id` ownership
   - external, bridge, and custody addresses are not canonical asset owners
3. Addresses become fee / nonce / asset owners again
   - this directly breaks the completed `Phase 2`
4. `Phase 4` implicitly becomes a unified asset ledger
   - mapped-asset protocol is not a unified asset ledger
5. Lock proof and mint record are not traceable
   - the system cannot answer why mint happened or which proof justified it
6. Supply is not conserved
   - supply changes do not stay inside `mint / burn / redeem` main flows
7. The audit chain is broken
   - lock / proof / mint / burn / redeem do not form one traceable chain
8. Bridge / mint / ledger / privacy are mixed into one layer
   - any design owning three or more of these responsibilities is a default phase violation
9. `account_assets / account_balance` become write paths
   - the Phase 3 read surface must not become a ledger entry point
10. `asset_root` is pulled in as a prerequisite
   - this prematurely contaminates unfinished proof / ledger layers

## 3. Structural failures

The following failures may not explode immediately, but they guarantee future rework:

1. A view interface becomes a write path
   - even if it works short-term, the view layer becomes a hidden ledger
2. Custody boundary and risk boundary are not separated
   - custody rules and risk rules can no longer be frozen independently
3. Treasury / risk / audit mainline is bypassed
   - this creates a second truth
4. A mapped-asset object has no canonical source reference
   - the system can no longer answer where the asset came from
5. A fallback compatibility path becomes the default main path again
   - this drags the system back from `account-first` to `address-first`
6. A chain-specific special path lands first and is promised to be abstracted later
   - in practice the common protocol never converges
7. Aggregate fields, cache fields, or report fields are treated as supply truth
   - this silently upgrades the read layer into a state layer

## 4. Review checklist

Every `Phase 4` PR must answer all of the following:

1. Does it introduce a new state-truth source?
2. Does it weaken `account_id` as the canonical ownership subject?
3. Does it introduce a mint path that cannot be fully audited?
4. Does it turn `account_assets / account_balance` into write paths?
5. Does it make `Phase 4` depend on unfinished proof / account-tree work?
6. Does it mix bridge, mint, ledger, and privacy into one layer?
7. Does it bypass treasury / risk / audit mainline behavior?
8. Does it let a fallback compatibility path become the default main path again?

If any answer is unclear, the change must not merge into mainline.

## 5. One-line rejection rules

The following review lines are intended for direct use:

- `This change introduces a new global asset state source.`
- `This change makes Phase 4 a ledger entry point.`
- `This change breaks account_id as canonical ownership subject.`
- `This change creates an unauditable mint path.`
- `This change turns account_assets/account_balance into write paths.`
- `This change reintroduces address-owned asset semantics.`
- `This change mixes bridge, mint, ledger, and privacy into one layer.`
- `This change makes Phase 4 depend on asset_root or proof trees that are not yet in mainline.`

## Recommended usage

Before `Phase 4` starts, this document should be used together with:

- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MAPPED-ASSET-CONSTRAINT-DRAFT-2026-04-20.md`

That means:

`the positive boundary of Phase 4 is frozen by the constraint draft, and the negative failure surface is frozen by this failure-mode list.`
