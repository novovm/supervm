# NOVOVM Account Protocol KeyAlgo / AccountMode / ExecutionPolicy Layering Rule (2026-04-20)

Status: AUTHORITATIVE LAYERING RULE  
Scope: freeze the mainline separation between subject identity, key algorithm class, and execution policy, and define `AccountMode` explicitly as an optional label layer so that basic crypto, post-quantum crypto, and privacy do not become three parallel account systems

## Purpose

This document answers one question only:

`How unified account should carry basic crypto, post-quantum crypto, and privacy without splitting the account subject.`

This document is not a full implementation note, and it does not claim that all of these layering objects already exist as first-class runtime objects on the current mainline path.

It freezes:

- what the subject is
- what the cryptographic algorithm class is
- what the execution-routing policy is
- under which conditions `AccountMode` may exist only as an optional label

## Core rule

The unified-account rule is:

`account_id answers only "who", not "which cryptography" and not "how to execute".`

That means:

- `account_id` is the unified subject
- `key_algo` is the key/public-key algorithm class
- `execution_policy` is the execution-routing and visibility policy
- `account_mode`, if it ever exists, is optional metadata only, not a source of capability truth

Therefore there are not three parallel subject systems:

- "basic account"
- "post-quantum account"
- "privacy account"

There is:

`one subject + bound capability + policy routing`

## Three mainline layers + one optional label

### 1) Subject layer: `account_id`

The subject layer defines only:

- who owns the account
- who owns the bindings
- who owns nonce / fee / audit semantics

The subject layer does not change because of:

- `secp256k1`
- `ed25519`
- `mldsa87`
- privacy keys
- privacy execution policies

### 2) Key-algorithm layer: `key_algo`

`key_algo` answers only:

`Which cryptographic algorithm the bound key belongs to.`

Minimal recommended set:

```rust
pub enum KeyAlgo {
    Secp256k1,
    Ed25519,
    Mldsa87,
}
```

This layer covers:

- public-key / signature classification
- algorithm compatibility checks
- algorithm validity checks

This layer does not directly determine:

- whether the account is a privacy account
- whether execution must take a privacy path

### 3) Execution-policy layer: `execution_policy`

`execution_policy` answers only:

`Which route and visibility requirement execution must follow.`

Minimal recommended set:

```rust
pub enum ExecutionPolicy {
    Standard,
    PqRequired,
    PrivacyRequired,
    Confidential,
}
```

This layer covers:

- route selection
- privacy / confidentiality requirements
- hard constraints on execution behavior

This layer must not rewrite:

- the subject definition
- the cryptographic fact of the key

### 4) Optional label layer: `account_mode` (non-mainline)

`account_mode` is not a required layer in the current capability-driven model.

If it is ever introduced, it may answer only:

`What label or control-plane hint this account should present.`

It may belong only to:

- UI labeling
- control-plane hints
- non-authoritative metadata

It explicitly does not belong to:

- subject definition
- key-algorithm fact
- execution-routing truth
- asset ownership or privacy truth

If it is ever introduced, even the minimal enum remains illustrative metadata only, not a mainline semantic layer:

```rust
pub enum AccountMode {
    Basic,
    PostQuantum,
    Privacy,
    Hybrid,
}
```

## Required flow

This design cannot stop at a classification label. It must be:

`declare -> validate -> bind -> route`

That means:

1. the user declares:
   - `key_algo`
   - `execution_policy`
2. the system validates:
   - the provided public key / signature material actually matches `key_algo`
3. the system binds:
   - the capability to the account subject
4. the system routes:
   - execution using `execution_policy`

If `account_mode` ever exists, it may be attached only after binding as a label and must not participate in mainline routing truth.

The following pseudo-classification is forbidden:

`the user claims to be post-quantum -> the system accepts the claim without validating the key material`

## Why privacy must not live only on `key_algo`

Post-quantum is:

`a cryptographic algorithm property`

Privacy is:

`an execution-mode + record-visibility property`

Therefore:

- post-quantum belongs primarily to `key_algo`
- privacy belongs primarily to `execution_policy`
- `account_mode` may be, at most, a non-authoritative label

Otherwise the system mixes:

- algorithm class
- visibility policy
- routing policy
- presentation label

into one layer.

## Minimal binding shape

The recommended expression is to bind these concepts without raising them into the subject layer:

```rust
pub struct AccountKeyBinding {
    pub account_id: [u8; 32],
    pub key_id: [u8; 32],
    pub key_algo: KeyAlgo,
    pub execution_policy: ExecutionPolicy,
    pub account_mode_hint: Option<AccountMode>,
    pub public_key: Vec<u8>,
}
```

The key point is:

`account_id is the subject; the other fields are capability-layer fields.`

## Current facts and current non-claims

The following are already true:

- the unified account subject protocol is on the real mainline path
- subject binding, policy, nonce, and audit are established
- the governance extension layer already exposes a controlled `mldsa87 external vote` path

The following are not yet claimed as complete:

- the unified-account primary-key layer fully implements `key_algo / account_mode / execution_policy` as first-class public runtime objects
- unified account already fully supports basic + optional PQ + privacy account capabilities end to end

The only minimal slice that has entered mainline implementation and is now sealed is:

- `Cut A / KeyAlgo`
  - `KeyAlgo` metadata is present
  - proof-of-possession validation is present
  - query and audit visibility is present
- `Cut C / ExecutionPolicy`
  - the minimal enum is present:
    - `Standard`
    - `PqRequired`
    - `PrivacyRequired`
  - single mainline resolve + enforcement is present
  - explicit rejection paths and audit visibility are present
  - silent fallback is explicitly forbidden

The following have not entered implementation yet:

- `Cut B / AccountMode` (default `No-Go`, and not a required mainline layer)
- execution-policy slices beyond `Cut C`
  - a full `Confidential` path
  - any policy behavior that touches ledgers, privacy asset spaces, or new state-truth sources

The formal current positioning of `Cut B` is:

`an optional label layer, not a mainline semantic layer.`

Unless a real need appears that cannot be expressed by `KeyAlgo + ExecutionPolicy`, `AccountMode` must not be introduced.

Therefore this document freezes:

`the correct layering direction`

not:

`the claim that all related implementation is already complete`

## Recommended external wording

`Unified account does not split into separate basic, post-quantum, and privacy subject systems; mainline capability is determined by bound key algorithm and execution policy. account_id remains the subject; Cut A / KeyAlgo and Cut C / ExecutionPolicy are now implemented, while AccountMode remains an optional label rather than a mainline semantic layer.`
