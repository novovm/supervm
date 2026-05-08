## Summary / 变更摘要

Describe the user-facing or protocol-facing outcome of this PR.

请说明本 PR 的对外效果或协议效果。

## Scope / 影响范围

- [ ] Code
- [ ] Docs
- [ ] Tests
- [ ] CI or workflow

## Unified Account Structural-Change Gate / 统一账户结构变更守门

Mark exactly one option:

- [ ] No unified-account structural change in this PR. Trigger checklist not required.
- [ ] This PR includes unified-account structural change. Trigger payload below is complete.

For structural changes, all fields below are required:

1. Trigger checklist reference:
   - [ ] `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
   - [ ] `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
   - [ ] `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
   - [ ] `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
2. Trigger item ID(s):
3. Evidence link(s) (tests, metrics, incident, business requirement):
4. Minimal irreversible slice and rollback boundary:
5. Decision request:
   - [ ] `Go`
   - [ ] `No-Go`
6. If this is a `Phase 4` structural change, include completed template:
   - [ ] `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
   - [ ] `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`

## Governance Lock Changes / 守门规则变更

Mark exactly one option:

- [ ] This PR does not modify Trigger Checklist / MVP Slice Template / PR Gate.
- [ ] This PR modifies governance control documents and includes full governance proof chain.

If the second option is selected, all fields below are required:

1. Governance proposal reference:
2. Governance vote evidence:
3. Governance execute/activation evidence:

Governance default:

`Missing trigger payload => Reject (No-Go) by default.`
`Missing governance proof chain for governance-control changes => Reject (No-Go) by default.`

Authoritative rule reference:

- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-STRUCTURAL-CHANGE-PR-GATE-2026-04-21.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MVP-SLICE-TEMPLATE-2026-04-21.md`
