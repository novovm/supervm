# NOVOVM Account Protocol Cut A / KeyAlgo Seal (2026-04-20)

Status: FINAL SEAL (Cut A / KeyAlgo)  
Scope: add `KeyAlgo` metadata, proof-of-possession verification, and audit visibility to unified account without entering `AccountMode`, `ExecutionPolicy`, or any asset-layer change

## Objective

`Cut A` does one thing only:

`make unified account aware of which key algorithm is bound, and complete the declare -> verify -> bind loop on create/rotate`

This cut does not introduce:

- `AccountMode`
- `ExecutionPolicy`
- a multi-algorithm primary-key migration state machine
- privacy-account behavior
- a unified-account-level "default post-quantum" mode
- any asset-layer change

## Established capabilities

The following are now established:

- `primary_key_binding`
  - unified account now has explicit primary-key-binding metadata
- `UcaKeyAlgo`
  - minimal supported set:
    - `secp256k1`
    - `ed25519`
    - `mldsa87`
- `UcaKeyProofType`
  - minimal proof type:
    - `signature_v1`
- `UcaPrimaryKeyBinding`
  - minimal binding object:
    - `key_algo`
    - `public_key`
    - `proof_type`
    - `proof_payload`
    - `verified_at`
- `ua_createUca`
  - now supports `declare -> verify -> bind`
- `ua_rotatePrimaryKey`
  - now supports `declare -> verify -> bind`
- audit and query visibility
  - `ua_getAccount` exposes `primary_key_binding`
  - audit events expose `key_algo`
- unchanged subject semantics
  - `account_id` remains the only canonical subject
  - `key_algo` does not recompute or split `account_id`

What is established here is:

`minimal closed-loop key-algorithm metadata + proof verification + audit visibility`

not:

`entry into account-mode or execution-policy layers`

## Explicitly not established

The following remain out of scope and are not claimed as complete:

- `AccountMode`
- `ExecutionPolicy`
- a multi-algorithm primary-key migration state machine
- privacy-account capability
- unified-account-level "default post-quantum"
- any asset-layer change
- any semantic change to `account_balance / account_assets`

Important note:

- `mldsa87` is not fake support
- what is established is:
  - if AOEM verification is available, `mldsa87` proof can pass for real
  - if AOEM verification is unavailable, the request is explicitly rejected
- privacy semantics are not attached to `key_algo`

## Current mainline path

The current real path for `Cut A` is:

`novovm-node (bin) -> mainline_query -> unified_account_surface -> UnifiedAccountRouter`

Current method contract:

| Method | Current semantics | Status |
| --- | --- | --- |
| `ua_createUca` | may accept `key_algo + public_key + proof_type + proof_payload`, verify them, then bind them to the account | sealed (Cut A) |
| `ua_rotatePrimaryKey` | may accept `key_algo + public_key + proof_type + proof_payload`, verify them, then rotate the primary key | sealed (Cut A) |
| `ua_getAccount` | exposes `primary_key_binding` and `key_algo` visibility | sealed (Cut A) |
| `ua_getAuditEvents` | exposes `key_algo` visibility in audit output | sealed (Cut A) |

## Validation results (locally executed on 2026-04-20)

This cut is sealed against the following local executions:

- `cargo fmt --all`
- `cargo test -p novovm-adapter-api`
- `cargo test -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `powershell -ExecutionPolicy Bypass -File scripts/migration/run_unified_account_gate.ps1`
  - result: `16/16` passed
- `cargo run -p novovm-node --bin supervm-mainline-gate`
  - result: `supervm mainline gate passed`
  - result: `L1=100% L2=100% L3=100% L4=100% Overall=100%`

Minimal regressions were also added for:

- successful `ed25519` key binding
- successful `secp256k1` key binding
- invalid proof rejection without polluting account-subject state
- successful `mldsa87` key rotation when AOEM is available

## Recommended external wording

`Cut A is complete: unified account now has the minimal closed loop of key-algorithm metadata, proof-of-possession verification, and audit visibility, but it has not yet entered the account-mode or execution-policy layers.`
