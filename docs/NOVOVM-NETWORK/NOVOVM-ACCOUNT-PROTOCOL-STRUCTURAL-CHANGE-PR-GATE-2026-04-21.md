# NOVOVM Account Protocol Structural-Change PR Gate (2026-04-21)

Status: AUTHORITATIVE GOVERNANCE RULE  
Scope: unified-account structural changes on the real `novovm-node` mainline path

## Core rule

Any PR that changes unified-account structure must explicitly map to at least one approved trigger checklist item; otherwise the default decision is `Reject (No-Go)`.

Fixed governance direction:

`push requires reason; no push needs no explanation`

## What counts as a structural change

A PR is structural if it changes any of the following:

- canonical subject semantics (`account_id`, compatibility alias rules)
- protocol method contract or router method surface
- source of account asset-truth (asset-view source, new ledger source, root source)
- enforcement semantics of `KeyAlgo`, `ExecutionPolicy`, or future `AccountMode`
- phase boundary for `Cut B` or `Phase 4`

The following are not structural by default:

- bug fixes with no protocol semantic change
- test-only updates
- docs-only wording updates
- refactors that do not change behavior

## Mandatory PR payload for structural changes

A structural PR must include all fields below:

1. Trigger checklist reference:
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
   - or `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
2. Trigger item IDs and evidence links (tests, metrics, incidents, business requirement)
3. Minimal irreversible slice definition and rollback boundary
4. Explicit decision request (`Go` or `No-Go`)
5. For `Phase 4` structural changes only: completed slice declaration using
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`

If any field is missing, review outcome is `Reject (No-Go)` by default.

## Meta-governance lock (anti-drift rule)

Any PR that modifies any of the governance control documents below must include a higher-level governance proof chain:

- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`

Required proof chain:

1. Governance proposal reference
2. Governance vote evidence
3. Governance execute/activation evidence

If any one item is missing, review outcome is `Reject (No-Go)` by default.

## Review decision policy

- trigger not met: `Reject (No-Go)`
- trigger met but slice boundary unclear: `Reject (No-Go)`
- trigger met + bounded slice + evidence complete: eligible for `Go` review (not auto-approval)

## Non-structural PR declaration

If a PR does not change unified-account structure, the author should state:

`No unified-account structural change in this PR. Trigger checklist not required.`

## Authoritative references

- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-v1-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
- `docs/CURRENT-AUTHORITATIVE-ENTRYPOINT-2026-04-17.md`
