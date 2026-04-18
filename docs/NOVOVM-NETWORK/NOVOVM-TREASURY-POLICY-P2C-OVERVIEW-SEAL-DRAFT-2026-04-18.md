# NOVOVM Treasury Policy P2-C Overview Seal Draft (2026-04-18)

Status: SUPERSEDED by `NOVOVM-TREASURY-POLICY-P2C-SEAL-2026-04-18.md`

## Purpose

This draft consolidates all signed P2-C increments into one authoritative overview, so subsequent work can proceed without phase-boundary drift.

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
- `P2-C overall`: in progress

## Sealed scope in P2-C (current)

### 1. Policy object is versioned and source-aware

- `policy_version` is query-visible and journal-traceable.
- `policy_source` is normalized and stable (`config_path`, `governance_path`).
- Governance updates persist version/source into follow-up settlement facts.

### 2. Threshold state is executable

- `healthy`, `constrained`, `blocked` are enforced in clearing behavior.
- Clearing no longer runs as a best-effort path only; policy state now gates execution.

### 3. Constrained strategy is executable

- Supported enum is sealed:
  - `daily_volume_only`
  - `treasury_direct_only`
  - `blocked`
- Strategy filtering is applied before generic failure checks.
- Rejection semantics are stable and query-visible.

### 4. Cross-view policy context is now contract-shaped

Policy context is consistently visible across:

- receipt (`policy_meta`)
- last selected route
- candidate routes
- settlement summary
- settlement policy query
- risk summary
- settlement journal

## Frozen contract surface (P2-C)

### A. Frozen policy identity fields

- `policy_contract_id`
- `policy_version`
- `policy_source`
- `policy_threshold_state`
- `policy_constrained_strategy`

### B. Frozen strategy enum

- `daily_volume_only`
- `treasury_direct_only`
- `blocked`

### C. Frozen failure namespace

- `fee.quote.*`
- `fee.clearing.*`
- `fee.settlement.*`

## Validation baseline

The following remain the minimum sign-off gate:

- `cargo fmt --all --check`
- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
- `cargo deny check --disable-fetch` (warnings allowed per current policy)

## Explicit non-claims (still out of scope)

- Multi-hop routing
- Split-order clearing
- Automatic strategy tuning engine
- Complex financial product layer (staking/dividend/revenue products)

## Remaining boundary for P2-C overall

The unresolved P2-C tail should stay narrow:

1. Parameter-object stability and comparability hardening
2. Further constrained behavior differentiation (still explainable, no solver)
3. Full config/governance path homomorphism lock across all policy views

## Recommended external wording

`P2-C has established a policy execution contract that is versioned, source-aware, state-graded, strategy-driven, and cross-view traceable; P2-C overall remains in controlled tail hardening.`
