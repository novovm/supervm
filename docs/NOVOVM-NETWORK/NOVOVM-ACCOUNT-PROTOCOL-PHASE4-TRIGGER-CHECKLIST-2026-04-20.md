# NOVOVM Account Protocol Phase 4 Trigger Checklist (2026-04-20)

Status: AUTHORITATIVE TRIGGER CHECKLIST (Phase 4)  
Scope: the single decision gate that determines whether `Phase 4` may move from frozen constraints into mainline implementation

## Purpose

This document is not a design draft, not a roadmap, and not an implementation task list.

It does one thing only:

`freeze when Phase 4 is allowed to begin implementation.`

That means starting `Phase 4` no longer depends on narrative judgment. It depends on explicit, reviewable trigger conditions.

## Current state

The current unified account / asset line is:

- `Phase 2`: complete
- `Phase 3 / Cut 1`: complete
- `Phase 3 / Cut 2`: complete
- `Phase 4`: constraints and failure modes are frozen; implementation has not started

Therefore the default conclusion remains:

`Phase 4 does not start.`

Only if this checklist is fully satisfied may `Phase 4` move into implementation.

## Single trigger rule

`Phase 4` may enter mainline implementation only when all of the following are true:

1. At least one real mapped-asset runtime source exists
   - not a placeholder object
   - not a documentation object
   - not a synthetic test-only source
2. That source can be stably owned by `account_id`
   - ownership must not fall back to external, bridge, or custody addresses
3. No new global asset state source is required
   - `Phase 4` must not become the start of a new ledger
4. The current `Phase 3` view is sufficient to expose the source
   - the source must be representable in `account_balance / account_assets` with ownership, classification, and source
5. Custody / risk / audit boundaries can be described and frozen independently
   - they must not collapse into one implementation block
6. The supply invariant can be validated through one main flow
   - the system must be able to explain `lock / proof / mint / burn / redeem`
7. The implementation does not depend on unfinished layers:
   - `asset_root`
   - unified asset ledger
   - privacy-subaccount asset space
   - proof-root account tree
8. The slice does not violate any frozen guardrail
   - it does not violate the Phase 4 constraint draft
   - it does not trigger any Phase 4 failure mode

If any one of these is false, the conclusion is:

`Phase 4 must not start.`

## Required evidence

Any attempt to open `Phase 4` must provide at least the following evidence:

1. Real-source statement
   - what the source object is
   - why it is a runtime truth source
2. Ownership statement
   - why final ownership lands on `account_id`
3. View-carrier statement
   - how the source is represented read-only in `account_balance / account_assets`
4. Invariant statement
   - how supply is conserved
   - how the audit chain remains continuous
5. Layering statement
   - bridge / custody
   - mapped asset protocol
   - asset view
   - ledger / root
   must remain separate

If any of these evidence items is missing, the phase must remain frozen.

## Immediate no-go conditions

Even if other conditions appear satisfied, `Phase 4` must not start if any of the following is true:

1. It requires a new global asset state source
2. It requires addresses to become canonical owners again
3. It requires `account_assets / account_balance` to become write paths
4. It requires bridge / mint / ledger / privacy to be mixed into one layer
5. It requires `asset_root` before it can work
6. It cannot provide a complete audit chain
7. It cannot demonstrate the supply invariant

## Review questions

Before approving `Phase 4` implementation, the review must answer:

1. Is the new source a real runtime source or a synthetic source created to push the phase forward?
2. Is the final asset owner `account_id` or some address object?
3. Can the current `Phase 3` view carry it in a read-only way?
4. Does this implementation implicitly grow a new ledger?
5. Does this implementation bypass treasury / risk / audit mainline behavior?
6. Does this implementation depend on unfinished root / privacy / proof layers?

If any answer is unclear, the default decision is `No-Go`.

## Final decision

This document allows only two outputs:

- `Go`: all conditions satisfied, a minimal Phase 4 implementation slice may start
- `No-Go`: any condition unsatisfied, remain in Phase 3 plus frozen Phase 4 constraints

The current conclusion is:

`No-Go`

## Related documents

- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MAPPED-ASSET-CONSTRAINT-DRAFT-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-FAILURE-MODES-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
