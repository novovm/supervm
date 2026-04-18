# NOVOVM Treasury Policy P2-C Seal (2026-04-18)

Status: SEALED (Authoritative)  
Supersedes: `NOVOVM-TREASURY-POLICY-P2C-OVERVIEW-SEAL-DRAFT-2026-04-18.md`  
Scope: P2-C Stage1 + Stage2 + constrained strategy + policy consolidation/closure hardening

## Purpose

This document is the final sealed reference for the P2-C treasury policy phase. It freezes completed scope, frozen contract surfaces, and explicit non-claims, so post-P2-C changes must be treated as new phase work.

## Phase status

- `P0`: accepted
- `P1-A`: accepted
- `P1-B`: accepted
- `P1-C`: accepted
- `P2-A`: accepted
- `P2-B1`: accepted
- `P2-B2`: accepted
- `P2-C Stage1`: accepted
- `P2-C Stage2`: accepted
- `P2-C constrained strategy`: accepted
- `P2-C policy increment round 2`: accepted
- `P2-C closure hardening`: accepted
- `P2-C overall`: in controlled tail hardening

## Sealed scope

### 1) Policy contract identity is fixed

- `policy_contract_id`
- `policy_version`
- `policy_source`
- `policy_threshold_state`
- `policy_constrained_strategy`

The above identity fields are stable and query-visible.

### 2) Policy source and version are persistent facts

- `policy_source` is normalized (`config_path`, `governance_path`).
- Governance updates persist into follow-up settlement facts.
- Version/source are journal-traceable and summary-visible.

### 3) Threshold-state behavior is executable

- `healthy`, `constrained`, `blocked` are executable states.
- State decides route candidacy and/or rejection behavior.
- Route constraints are evaluated before generic failure filtering.

### 4) Constrained strategy enum is sealed

Frozen strategy values:

- `daily_volume_only`
- `treasury_direct_only`
- `blocked`

### 5) Cross-view policy context is contract-shaped

Policy context is consistently visible across:

- receipt (`policy_meta`)
- last selected route
- candidate routes
- settlement summary
- settlement policy query
- risk summary
- settlement journal

### 6) Journal policy context is event-homomorphic

Policy context remains traceable for journal event classes, including accepted/settled/redeemed/rejected flows where applicable.

## Frozen failure namespace

- `fee.quote.*`
- `fee.clearing.*`
- `fee.settlement.*`

## Validation baseline

Minimum sign-off gate:

- `cargo fmt --all --check`
- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
- `cargo deny check --disable-fetch` (non-blocking warnings allowed by current policy)

## Explicit non-claims

Still out of scope:

- multi-hop routing
- split-order clearing
- automatic strategy tuning engine
- complex financial product layer (staking/dividend/revenue products)

## Controlled tail boundary (post-seal)

Remaining work after this seal should stay narrow:

1. parameter-object stability and comparability hardening
2. further constrained behavior differentiation (still explainable, no solver)
3. full config/governance path homomorphism lock across all policy views

## Recommended external wording

`P2-C has established a policy execution contract that is versioned, source-aware, state-graded, strategy-driven, and cross-view traceable.`

