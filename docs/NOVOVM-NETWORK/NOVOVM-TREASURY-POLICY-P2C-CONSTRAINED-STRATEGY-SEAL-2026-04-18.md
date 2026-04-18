# NOVOVM Treasury Policy P2-C Constrained Strategy Seal (2026-04-18)

## Status

- P2-C Stage 1: sealed
- P2-C Stage 2: sealed
- P2-C policy increment (Constrained Strategy): sealed

## Scope sealed in this round

1. Added `clearing_constrained_strategy` minimal strategy enum:
   - `daily_volume_only`
   - `treasury_direct_only`
   - `blocked`
2. Under `threshold_state=constrained`, route candidates are filtered by strategy first, then generic clearing guards are applied.
3. Added failure semantics:
   - `fee.clearing.constrained_blocked`
4. `policy_version / policy_source / threshold_state / clearing_constrained_strategy` are visible in policy and risk queries.

## Constrained behavior definition

- `daily_volume_only`:
  Clearing is allowed, but with tighter daily usage limits plus normal risk checks.
- `treasury_direct_only`:
  Only `TreasuryDirect` routes are admitted into candidate routes; reject when none is available.
- `blocked`:
  Clearing is rejected immediately with `fee.clearing.constrained_blocked`.

## Verified in this round

- `cargo fmt --all --check`
- `cargo check -p novovm-node -p novovm-protocol -p novovm-consensus`
- `cargo clippy -p novovm-node -p novovm-protocol -p novovm-consensus --all-targets -- -D warnings`
- `cargo test -p novovm-node --quiet`
- `cargo run -p novovm-node --bin supervm-mainline-gate`

All commands passed locally.

## Explicitly out of scope

- multi-hop routing
- split-order execution
- complex strategy expressions (combined policies / auto-tuning)
- financial extensions such as revenue sharing, staking, dividends

## Current conclusion

P2-C has moved from “policy exists” to “policy explicitly constrains clearing behavior via a queryable, testable strategy enum”.
