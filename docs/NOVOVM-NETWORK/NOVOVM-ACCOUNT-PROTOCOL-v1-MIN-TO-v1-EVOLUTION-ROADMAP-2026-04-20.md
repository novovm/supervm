# NOVOVM Account Protocol v1-min to v1 Evolution Roadmap (2026-04-20)

Status: AUTHORITATIVE ROADMAP (Frozen)  
Scope: the frozen order, activation conditions, and no-skip rules for evolving unified account from the current `v1-min` subject protocol toward a later `v1` account system

## Purpose

This document freezes the evolution order for unified account from `v1-min` toward a later `v1`.

It answers:

- what the next phase is allowed to do
- which capabilities must remain deferred
- which conditions must be satisfied before a larger freeze is allowed

The purpose is to prevent unified account from drifting back into a state where protocol claims outrun the runtime.

## Current baseline

The current public baseline is:

- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-v1-2026-04-20.md`

What is established now:

- `account_id` as the single canonical subject direction
- the real mainline entry path: `novovm-node -> mainline_query -> unified_account_surface`
- subject creation, binding, policy, nonce, audit, and routing
- the unified asset-view read surface: `account_balance / account_assets`
- `Cut A / KeyAlgo`
- `Cut C / ExecutionPolicy`

What is not established now:

- a unified asset ledger
- privacy subaccounts on the mainline path
- proof-root account trees
- a full mapped-asset settlement protocol

The correct current positioning therefore remains:

`Subject Protocol`

not:

`Full Account System`

## Global rules

Unified account evolution must follow these four rules:

1. Make the system run `account_id`-first before expanding account objects.
2. Build a unified asset view before deciding whether a unified asset ledger is needed.
3. Make privacy a controlled subspace of the main account before discussing a parallel privacy account system.
4. Introduce proof-root structures only after the runtime proves they are actually needed.

Additional frozen rule:

5. `AccountMode` is not a required mainline layer; in the capability-driven model, capability is expressed by `KeyAlgo + ExecutionPolicy` by default, and `AccountMode` may appear only as an optional label if a non-replaceable labeling need emerges.

## Frozen phase order

The frozen evolution order is:

1. `Phase 2`: execution strong binding
2. `Phase 3`: unified asset view
3. `Phase 4`: mapped-asset protocol
4. `Phase 5`: privacy subaccounts on the mainline path
5. `Phase 6`: proof-root account trees

Skipping phases is not allowed.

## Phase 2: execution strong binding

### Goal

Make the system start running `account_id`-first rather than continuing to derive account identity from addresses.

### Allowed in this phase

- new or modified execution entry points explicitly accept `account_id`
- fee ownership is explicitly bound to `account_id`
- nonce ownership is unified under the account subject
- new trace / receipt / audit fields prefer `account_id`

### Not allowed in this phase

- adding root fields to `NovoAccount`
- introducing a new unified account asset ledger
- mixing privacy subaccount structures into the main subject protocol object

### Exit conditions

This phase may end only when all of the following are true:

- new execution entry points no longer expand `address-first` semantics
- fee ownership is stably traceable to `account_id`
- nonce ownership is stably traceable to `account_id`
- `account_id` has become the default subject input for new execution capability

## Phase 3: unified asset view

### Goal

Make the system look like a unified account before turning it into a new ledger.

### Allowed in this phase

- `account_balance(account_id, asset_id)`
- `account_assets(account_id)`
- aggregation across existing execution, economic, reserve, locked, pledge, treasury-exposure, and later-realized real sources
- returning balances under `account_id` ownership

### Not allowed in this phase

- introducing `asset_root`
- rewriting current economic modules into a new ledger
- presenting the unified asset view as if a unified asset ledger already exists

### Exit conditions

This phase may end only when all of the following are true:

- major asset sources are stably aggregated by the unified asset view
- returned balances are stably owned by `account_id`
- the unified asset view is sealed and added to the public documentation surface

## Phase 4: mapped-asset protocol

### Goal

Converge external asset mapping into one main protocol rather than one chain-specific flow per integration.

### Allowed in this phase

- freezing main protocol objects for `nETH / nBTC / nUSDT` and similar assets
- freezing lock / proof / mint / burn / redeem main flows
- freezing custody / risk / audit boundaries for mapped assets

### Not allowed in this phase

- freezing mapped-asset protocol before the unified asset view is stable
- adding chain-specific side paths that bypass the main protocol
- mixing mapped-asset protocol and privacy-subaccount rollout into one step

### Exit conditions

This phase may end only when all of the following are true:

- the unified asset view is stable
- mapped-asset ownership and audit land stably on `account_id`
- at least one major mapped-asset family is sealed under the common protocol

## Phase 5: privacy subaccounts on the mainline path

### Goal

Introduce privacy as a controlled subspace under the main account rather than as a second parallel subject system.

### Allowed in this phase

- main-account deposit into privacy subaccounts
- privacy-internal transfers
- controlled withdrawal back to the main account
- minimal view-policy and audit-policy contracts

### Not allowed in this phase

- making privacy a second subject system beside the main account
- introducing privacy subaccounts before fee and nonce are account-owned
- using privacy subaccounts as a replacement for the subject protocol

### Exit conditions

This phase may end only when all of the following are true:

- `account_id` subject binding is stable
- fee and nonce are stably account-owned
- the unified asset view is established
- privacy inflow and outflow can be stably audited back to the main account

## Phase 6: proof-root account trees

### Goal

Move unified account into proof-root form only when the runtime actually needs a provable account structure.

### Allowed in this phase

- freezing `identity_root / key_root / asset_root / permission_root`
- freezing root update rules
- freezing root proof, sync, and consumer contracts

### Not allowed in this phase

- writing root fields into the public protocol before a real root-update mainline exists
- introducing proof-root account trees before a clear proof consumer exists
- using "we may need zk later" as a reason to freeze roots early

### Exit conditions

This phase may end only when all of the following are true:

- account-state update semantics are stable
- the root update path is single-source and verifiable
- proof consumers are explicit and real
- introducing roots will not create a second truth beside snapshot / audit runtime reality

## Activation conditions for four major capabilities

### 1) When `asset_root` may be introduced

Only when all of the following are true:

- the unified asset view is sealed
- major asset sources are stably account-owned
- the root update path is clear
- a real proof / sync / cross-domain consumer exists

### 2) When privacy subaccounts may enter the mainline

Only when all of the following are true:

- `account_id` subject binding is stable
- fee and nonce are account-owned
- the unified asset view exists
- audit boundaries for privacy in/out flows are stable

### 3) When mapped-asset protocol may be frozen

Only when all of the following are true:

- the unified asset view exists
- subject ownership is stably bound to `account_id`
- custody / risk / audit rules are ready to seal

### 4) When proof account trees may be introduced

Only when all of the following are true:

- the runtime has a real root-update requirement
- state update semantics are stable
- proof consumers are explicit
- roots will not create a second truth

## Current phase conclusion

Unified account should currently be treated as:

`v1-min complete, Phase 2 complete, Phase 3 / Cut 1 complete, Phase 3 / Cut 2 complete`

Additional current capability-layer conclusion:

- `Cut A / KeyAlgo`: implemented and sealed
- `Cut B / AccountMode`: not a required mainline layer, currently default `No-Go`
- `Cut C / ExecutionPolicy`: implemented and sealed (minimal execution-policy slice)

The correct state of the unified account / asset line is now:

`The unified-account mainline has completed the minimal production closure of the subject layer, asset-view layer, key-capability layer, and execution-policy layer; AccountMode / Cut B remains a non-core optional label layer with default No-Go; Phase 4 remains trigger-governed and is currently No-Go.`

This means the current mainline has entered a stable baseline:

- ready for long-term operation
- no default structural push
- remaining engineering work splits into business integration and trigger-governed expansion only

The current implementation gate for that phase is:

- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTB-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-SEAL-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-CUTC-EXECUTIONPOLICY-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE2-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT1-SEAL-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-CUT2-SEAL-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE3-IMPLEMENTATION-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-TRIGGER-CHECKLIST-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-MAPPED-ASSET-CONSTRAINT-DRAFT-2026-04-20.md`
- `docs/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE4-FAILURE-MODES-2026-04-20.md`

not:

- expanding account objects
- expanding roots
- expanding a unified asset ledger
- expanding privacy mainline implementation
- jumping straight to a unified asset ledger
- slipping `Cut B` into mainline
- using `Cut C` as a shortcut into `Phase 4`

## Recommended external wording

`The NOVOVM unified-account mainline has completed the minimal production closure of the subject layer, asset-view layer, key-capability layer, and execution-policy layer; AccountMode / Cut B remains a non-core optional label layer with default No-Go; Phase 4 remains trigger-governed and is currently No-Go. This line has entered a stable baseline for long-term operation and does not require further structural push by default.`
