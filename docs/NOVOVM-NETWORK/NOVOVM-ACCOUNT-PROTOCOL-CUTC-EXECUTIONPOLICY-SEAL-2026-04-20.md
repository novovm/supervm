# NOVOVM Account Protocol Cut C / ExecutionPolicy Seal (2026-04-20)

Status: FINAL SEAL (Cut C / ExecutionPolicy)  
Scope: make `KeyAlgo + ExecutionPolicy` a real execution gate without entering `AccountMode`, a unified asset ledger, the mapped-asset protocol, or a privacy asset ledger

## Objective

`Cut C` does one thing only:

`turn customer-declared execution policy into a non-bypassable rule under the single mainline resolve + enforcement path`

This cut does not introduce:

- `AccountMode`
- a full `Confidential` path
- a privacy asset ledger
- the mapped-asset protocol
- `asset_root`
- any automatic downgrade or silent fallback
- any semantic change to `account_balance / account_assets`

## Established capabilities

The following are now established:

- `execution_policy`
  - minimal enum:
    - `Standard`
    - `PqRequired`
    - `PrivacyRequired`
- single mainline resolve + enforcement
  - the only enforcement points are now:
    - `tx_ingress`
    - `mainline_query`
  - gateway / adapter do not grow a second policy semantics layer
- gateway pass-through only
  - gateway only carries and forwards `execution_policy`
  - gateway does not invent a second subject or policy truth
- `PqRequired`
  - strong enforcement is established:
    - `key_algo == mldsa87`
    - explicit rejection when unmet:
      - `ERR_PQ_REQUIRED_BUT_KEY_NOT_PQ`
- `PrivacyRequired`
  - strong enforcement is established:
    - privacy path must be available
    - execution must explicitly go through the privacy path
    - explicit rejection when unmet:
      - `ERR_PRIVACY_REQUIRED_BUT_PATH_NOT_AVAILABLE`
- no silent fallback
  - policy failure rejects explicitly; it does not downgrade to the public path
- audit visibility
  - `receipt`
  - `trace`
  - `audit`
  now expose:
    - `execution_policy`
    - `policy_enforced`
    - `policy_rejection_reason`
- real product semantics
  - `KeyAlgo + ExecutionPolicy` now genuinely determines whether execution is allowed
  - `account_id` remains the only canonical subject

What is established here is:

`the minimal closed loop of key capability + execution enforcement`

not:

`entry into account-mode or privacy-asset layers`

## Explicitly not established

The following remain out of scope and are not claimed as complete:

- `AccountMode`
- a full `Confidential` path
- a privacy asset ledger
- the mapped-asset protocol
- `asset_root`
- a unified asset ledger
- any automatic downgrade mechanism
- any write-path or ledger semantics for `account_balance / account_assets`

Important notes:

- `ExecutionPolicy` does not recompute or split `account_id`
- `ExecutionPolicy` does not change asset ownership
- `ExecutionPolicy` does not turn `account_assets / account_balance` into write paths
- what is established here is execution enforcement, not a new state-truth source

## Validation results (locally executed on 2026-04-20)

This cut is sealed against the following real local executions:

- `cargo fmt --all`
- `cargo clippy -p novovm-protocol --all-targets -- -D warnings`
- `cargo clippy -p novovm-adapter-api --all-targets -- -D warnings`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo clippy -p novovm-evm-gateway --all-targets -- -D warnings`
- `cargo test -p novovm-protocol`
- `cargo test -p novovm-adapter-api`
- `cargo test -p novovm-node`
- `cargo test -p novovm-evm-gateway`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
  - result: `supervm mainline gate passed`
  - result: `L1=100% L2=100% L3=100% L4=100% Overall=100%`

Minimal regressions were also added for:

- `ed25519 + Standard` succeeds
- `mldsa87 + PqRequired` succeeds
- `secp256k1 + PqRequired` rejects
- `mldsa87 + PrivacyRequired` succeeds when privacy path is available
- `mldsa87 + PrivacyRequired` rejects explicitly when privacy path is unavailable

## Recommended external wording

`Cut C is complete: KeyAlgo + ExecutionPolicy now truly determines whether execution is allowed, and failure paths reject explicitly and remain auditable.`
