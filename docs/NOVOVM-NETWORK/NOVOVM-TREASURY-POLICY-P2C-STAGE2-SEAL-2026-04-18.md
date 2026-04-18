# NOVOVM Treasury Policy P2-C Stage2 Seal (2026-04-18)

## Purpose

This document seals `P2-C Stage2` and freezes the policy-layer boundaries that are now executable on mainline.

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
- `P2-C overall`: in progress

## P2-C Stage2 completed scope (code facts)

### 1. Policy version is now first-class and query-visible

- `policy_version` is exposed in policy and settlement summaries.
- Journal entries carry `policy_version` for settlement traceability.
- Governance policy apply updates runtime policy version with regression guard.

### 2. Policy source is now normalized and query-visible

- Active policy source is exposed as `policy_source`.
- Source path distinction is stable:
  - `config_path`
  - `governance_path`
- Legacy `default` source values are normalized to `config_path` for compatibility.

### 3. Threshold-state grading is executable and observable

Clearing risk state is now explicitly graded and query-visible:

- `healthy`
- `constrained`
- `blocked`

Behavior differences are enforced:

- `healthy`: normal non-NOV clearing path can proceed.
- `constrained`: non-NOV clearing applies constrained slippage gate.
- `blocked`: non-NOV clearing is rejected by policy gate.

### 4. Governance-disabled precedence remains strict

- Governance-disabled rejection remains ahead of authorization checks.
- Stage2 work does not relax this production boundary.

### 5. Risk query and failure summary remain stable

- `treasury.get_clearing_risk_summary` exposes:
  - `policy_version`
  - `policy_source`
  - `current_threshold_state`
  - `last_trigger`
  - `failure_summary`

## Validation matrix (local)

- `cargo fmt --all --check`
- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node --quiet`
- `cargo run -p novovm-node --bin supervm-mainline-gate`

All checks passed locally for this seal.

## Explicit non-claims (still out of scope)

- Advanced financial policy automation
- Revenue distribution products
- Staking/dividend mechanics
- Multi-hop or split-route clearing logic

## Stable wording

`P2-C Stage2 establishes a versioned and source-aware treasury policy layer with executable threshold-state behavior; P2-C overall remains in progress for further strategy-level policy evolution.`
