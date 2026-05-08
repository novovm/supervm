# NOVOVM Account Protocol Cut A / KeyAlgo Implementation Checklist (2026-04-20)

Status: AUTHORITATIVE CHECKLIST (Cut A / KeyAlgo)  
Scope: implementation gate for the minimal `KeyAlgo` slice in unified account; later changes may expand key metadata, verification, and audit visibility only, without drifting into `Cut B / Cut C`

## Goal

`Cut A` does one thing only:

`add key-algorithm metadata, proof-of-possession verification, and audit visibility to unified account`

This cut is not about:

- creating a new account-subject category
- introducing the account-mode layer
- introducing the execution-policy layer
- introducing privacy-account behavior
- introducing any asset-ledger or asset-view change

## Currently sealed scope

The current sealed implementation scope of `Cut A` is:

- `primary_key_binding`
- `UcaKeyAlgo`
- `UcaKeyProofType`
- `UcaPrimaryKeyBinding`
- `ua_createUca` with the `declare -> verify -> bind` loop
- `ua_rotatePrimaryKey` with the `declare -> verify -> bind` loop
- minimal support for `secp256k1 / ed25519 / mldsa87`
- `key_algo` visibility in `ua_getAccount` and audit events

## Dual-track boundary

This cut allows:

- continued compatibility for legacy `primary_key_ref` input
- continued legacy create/rotate paths when no `KeyAlgo` metadata is provided
- real validation and metadata binding when `KeyAlgo` metadata is provided

This cut does not allow:

- recomputing or splitting `account_id` because of `key_algo`
- treating `key_algo` as a unified-account subject category
- attaching privacy semantics directly to `key_algo`
- changing `account_balance / account_assets`
- introducing any new state-truth source under the name of `KeyAlgo`

Every expansion of `KeyAlgo` support must include at least one `declare -> verify -> bind -> query/audit visible` sample showing:

- the declared `key_algo`
- the public key and proof type
- whether validation passed
- how the binding appears in `ua_getAccount`
- how the binding appears in audit output

## Explicit prohibitions

The following are explicitly forbidden in this cut:

- introducing `AccountMode`
- introducing `ExecutionPolicy`
- turning `KeyAlgo` into a subject-splitting condition
- attaching privacy semantics to `KeyAlgo`
- making `mldsa87` the mandatory default account path
- introducing a full multi-algorithm primary-key migration state machine
- changing `account_balance / account_assets`
- introducing a unified asset ledger, mapped-asset protocol, privacy asset space, or proof-root account tree

## Merge gate

The merge gate is one sentence:

`KeyAlgo describes only which algorithm is bound; it must not determine account_id, asset ownership, privacy semantics, or unified-account subject splitting.`

Minimum PR acceptance must satisfy all of the following:

- `key_algo` appears only as binding metadata and validation input
- `account_id` is not recomputed and not split
- `ua_createUca / ua_rotatePrimaryKey` preserve the `declare -> verify -> bind` loop
- failed binding does not pollute account-subject state
- `ua_getAccount` and audit events expose `key_algo`
- no `AccountMode`
- no `ExecutionPolicy`
- no asset-layer change

Recommended code-review wording:

`If a change attempts to use KeyAlgo to alter account_id, asset ownership, privacy semantics, or unified-account subject splitting, or if it slips in AccountMode, ExecutionPolicy, privacy-account behavior, a unified asset ledger, a mapped-asset protocol, or a proof-root account tree, it must not merge into mainline.`

Long-term guardrail:

`Any change that uses KeyAlgo to directly trigger PostQuantum / Privacy / Hybrid account behavior is treated as an unauthorized jump into Cut B / Cut C.`
