# NOVOVM Account Protocol v1 (2026-04-20)

Status: AUTHORITATIVE PROTOCOL (`v1-min`)  
Scope: the current effective account-subject protocol surface on the real `novovm-node` mainline product

## Purpose

This document freezes the minimal account-subject protocol surface for NOVOVM and describes only the current results that have already landed in the real mainline path.

It does not describe migration history, does not treat legacy entry points as primary external product material, and does not freeze an oversized account universe before the runtime is actually there.

## Design position

NOVOVM currently defines unified account as:

`a system-subject protocol`

rather than:

`a bundle of multi-chain wallets`

What is established now:

- one canonical subject direction: `account_id`
- one unified account entry path: real `novovm-node -> mainline_query -> unified_account_surface`
- one current capability surface for identity, binding, policy, nonce, audit, and routing
- one current unified asset-view read surface: `account_balance / account_assets`

What is not established in `v1-min`:

- proof-root account state trees
- a new account-native global asset ledger
- privacy subaccounts on the mainline path
- a full mapped-asset settlement protocol

## Final conclusion

Unified account has entered the real `novovm-node` mainline entry path and can now be published as the current minimal subject protocol for NOVOVM.

Current project state that can be stated externally:

- `real unified-account mainline entry`: established
- `subject direction`: `account_id` is canonical, `uca_id` remains a compatibility alias
- `account read/write/routing surface`: established on the real mainline path
- `unified-account gate`: passed on real entry-path tests
- `protocol freeze level`: `v1-min`

## Stable baseline conclusion

The unified-account mainline has now completed the following minimal production closure:

- `subject layer`: `account_id` as the single subject
- `asset-view layer`: `account_balance / account_assets`
- `key-capability layer`: `Cut A / KeyAlgo`
- `execution-policy layer`: `Cut C / ExecutionPolicy`

The authoritative engineering conclusion is therefore fixed as:

`The unified-account mainline has completed the minimal production closure of the subject layer, asset-view layer, key-capability layer, and execution-policy layer; AccountMode / Cut B remains a non-core optional label layer with default No-Go; Phase 4 remains trigger-governed and is currently No-Go.`

This means the current baseline is ready for long-term operation:

- the project should not reopen a structural redesign of unified account at this stage
- the project should not keep advancing new structure, ledger, or root layers by default
- the remaining engineering actions split into only two kinds: integrate and use the current capability surface; or wait for real trigger signals before deciding whether to open the minimal slice for `Cut B` or `Phase 4`

If reduced to one hard engineering line, the conclusion is:

`This line is now ready for long-term operation and does not require further structural push by default.`

## Current real mainline path

The current unified-account product path is:

`novovm-node (bin) -> mainline_query -> unified_account_surface -> UnifiedAccountRouter`

This means unified account no longer exists only on legacy `main.rs` or older public surfaces. It is now part of the real mainline product path.

Key wiring points:

- `crates/novovm-node/src/bin/novovm-node.rs`
- `crates/novovm-node/src/bin/supervm-mainline-query.rs`
- `crates/novovm-node/src/mainline_query.rs`
- `crates/novovm-node/src/unified_account_surface.rs`
- `crates/novovm-adapter-api/src/unified_account.rs`

## v1-min freeze scope

`v1-min` freezes only the following protocol surface:

1. `account_id` as the canonical subject direction
2. primary identity binding and primary key rotation
3. derived-address / persona binding and revocation
4. policy boundaries, permission boundaries, and routing decisions
5. nonce ownership and replay-protection semantics
6. audit events and traceable query surface
7. the unified-account method contract on the real mainline entry path

`v1-min` does not freeze:

- `identity_root / key_root / asset_root / permission_root`
- a new account-native asset ledger
- full privacy-subaccount semantics
- a full mapped-asset mint/burn settlement protocol
- a full `recover_account` lifecycle state machine

## Canonical subject rule

The current canonical-subject rule is:

- `account_id`: canonical subject wording
- `uca_id`: compatibility alias during transition
- address / persona: derived representation, not the final public subject definition

The transition direction is therefore:

`address-driven + account-attached`

toward:

`account-driven + address-derived`

## Current sealed method contract

The following unified-account methods are now on the real mainline entry path:

| Method | Current semantics | Status |
| --- | --- | --- |
| `ua_createUca` | create a unified account subject | sealed |
| `ua_rotatePrimaryKey` | rotate the primary key | sealed |
| `ua_setPolicy` | update account policy | sealed |
| `ua_bindPersona` | bind a persona / derived address | sealed |
| `ua_revokePersona` | revoke a persona / derived address | sealed |
| `ua_getBindingOwner` | query the owning subject of a persona | sealed |
| `ua_getAuditEvents` | query unified-account audit events | sealed |
| `ua_getAccount` | query subject account info | sealed |
| `ua_getPolicy` | query subject policy | sealed |
| `ua_listBindings` | query subject binding list | sealed |
| `ua_getNextNonce` | query the next available nonce | sealed |
| `ua_route` | make a subject-level routing decision under current policy | sealed |
| `account_balance` | query the unified asset view and structured asset-exposure components for the current `account_id` | sealed (Phase 3 / Cut 1, expanded in Cut 2) |
| `account_assets` | query the unified asset inventory / vault / treasury-exposure view for the current `account_id` | sealed (Phase 3 / Cut 1, expanded in Cut 2) |

## Current protocol facts

### 1) Subject, binding, and uniqueness

The current unified-account surface establishes:

- subject creation
- persona uniqueness constraints
- conflict rejection on duplicate binding
- revocation and cooldown-bound rebind behavior

### 2) Signature-domain and nonce rules

The current unified-account surface establishes:

- signature-domain isolation
- chain-aware domain constraints
- nonce replay rejection
- nonce reverse-order rejection

### 3) Permission and policy boundaries

The current unified-account surface establishes:

- delegate / session-key permission boundaries
- expired session-key rejection
- policy-driven routing decisions
- Type4 mode boundaries

### 4) Audit and persistence

The current unified-account surface establishes:

- unified-account snapshot persistence
- audit-sink persistence
- real entry-path audit queries

This means the current surface is not just callable. It is:

`persisted + auditable + traceable as a real subject surface`

### 5) Unified asset view (read-only)

The current unified-account surface now establishes:

- `account_balance`
- `account_assets`
- `ownership_subject = account_id`
- mainline-grade asset visibility aggregated from existing asset sources
- `components`
- `pledges`
- `treasury_exposures`
- explicit `source`

What this establishes is:

`a read and aggregation contract`

not:

`a new unified asset ledger`

What is still not established:

- `staking`
- unified asset ledger
- `asset_root`
- mapped asset settlement protocol
- privacy subaccount asset space

### 6) Key capability and execution enforcement

The current unified-account surface now establishes:

- `Cut A / KeyAlgo`
  - `key_algo` metadata
  - proof-of-possession verification
  - query and audit visibility
- `Cut C / ExecutionPolicy`
  - minimal enum:
    - `Standard`
    - `PqRequired`
    - `PrivacyRequired`
  - single mainline resolve + enforcement
  - explicit rejection paths for `PqRequired / PrivacyRequired`
  - policy visibility in `receipt / trace / audit`

What this establishes is:

`KeyAlgo + ExecutionPolicy now truly determines whether execution is allowed`

What is still not established:

- `AccountMode`
- a full `Confidential` path
- any asset-ledger or state-truth behavior triggered by `ExecutionPolicy`

## What this document does not claim

This document does not claim that the following are complete:

- proof-root account state trees
- a unified account-native asset ledger
- privacy subaccounts on the real mainline path
- a fully sealed recovery lifecycle
- a complete unified asset system just because read-side asset views now exist

What `v1-min` establishes is:

`a unified subject protocol`

not:

`a complete account universe`

## Validation baseline (locally executed on 2026-04-20)

The current mainline account-subject claim is based on these local executions:

- `cargo fmt --all`
- `cargo check -p novovm-node`
- `cargo test -p novovm-node unified_account_gate_ua_g -- --nocapture`
  - result: `16 passed; 0 failed`
- `scripts/migration/run_unified_account_gate.ps1`
  - result: `pass: True`
  - result: `passed_cases: 16/16`

This gate now runs against real mainline entry-path tests rather than legacy `main.rs`-only tests.

## Reading order

For external readers, the current unified-account capability should be read in this order:

1. system overview: `docs_CN/NOVOVM-NETWORK/NOVOVM-CURRENT-SYSTEM-ARCHITECTURE-2026-04-19.md`
2. current account-subject protocol: this document
3. native execution, economic, and governance entry seals:
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-ECONOMIC-USER-SURFACE-SEAL-2026-04-18.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-USER-SURFACE-SEAL-2026-04-18.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-GOVERNANCE-MLDSA87-EXTERNAL-VOTE-SEAL-2026-04-18.md`
4. evolution order:
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-v1-MIN-TO-v1-EVOLUTION-ROADMAP-2026-04-20.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTA-KEYALGO-SEAL-2026-04-20.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTA-KEYALGO-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-SEAL-2026-04-20.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
5. current unified-asset-view seal and gate:
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT1-SEAL-2026-04-20.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT2-SEAL-2026-04-20.md`
   - `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-IMPLEMENTATION-CHECKLIST-2026-04-20.md`

This keeps the public reading path focused on current subject protocol, current entry points, current boundaries, and current verification results.

## Recommended external wording

`NOVOVM Account Protocol v1-min establishes unified account as the current subject protocol on the real novovm-node mainline: account entry, subject binding, policy, nonce, audit, read-side unified asset views, and the minimal KeyAlgo / ExecutionPolicy execution closure are live on the real product path, while a unified asset ledger, asset_root, privacy subaccounts, and proof-root account trees are not yet claimed as complete.`
