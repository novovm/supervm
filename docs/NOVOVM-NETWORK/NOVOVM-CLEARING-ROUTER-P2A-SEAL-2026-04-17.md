# NOVOVM Clearing Router P2-A Seal (2026-04-17)

## Purpose

This document seals the completed scope of `P2-A` and freezes the current executable boundaries, so post-P2-A enhancements are not misread as already shipped.

## Phase status

- `P0`: accepted
- `P1-A`: accepted
- `P1-B`: accepted
- `P1-C`: accepted
- `P2-A`: accepted

## P2-A completed scope (code facts)

### Minimal multi-route clearing mainline is established

- `pay_asset != NOV` is no longer single-path clearing.
- Two minimum concurrent clearing sources:
  - `TreasuryDirect`
  - `StaticAmmPool`
- Router flow is fixed:
  - `quote_routes`
  - `select_best_route`
  - `execute_selected_route`

### Module split is implemented

- `clearing_types.rs`
- `liquidity_sources.rs`
- `clearing_router.rs`
- `treasury_settlement.rs`
- `tx_ingress.rs` (orchestration and wiring)

### Receipt and query wiring is implemented

- Receipt route metadata is visible:
  - `route_id`
  - `route_source`
  - `expected_nov_out`
  - `route_fee_ppm`
- Query surface includes:
  - `treasury.get_clearing_routes`
  - `treasury.get_last_clearing_route`
  - `nov_getTreasuryClearingSummary`

### Failure-code boundary remains stable

- `fee.quote.*` and `fee.clearing.*` stay separated.
- Current stable minimum `fee.clearing.*` set:
  - `route_unavailable`
  - `insufficient_liquidity`
  - `quote_expired`
  - `slippage_exceeded`
  - `max_pay_exceeded`

### Mainline validation and gate

- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node`
- `cargo run -p novovm-node --bin supervm-mainline-gate`

All checks passed locally; `P2-A` is acceptable.

## Not included in P2-A (explicit non-claims)

- Multi-hop routing
- Split routing
- Advanced AMM math models
- Global smart-best routing
- Full multi-source quote aggregation redesign

## Next-phase naming freeze

To avoid naming collisions, use:

- `P2-B1`: multi-source route/liquidity aggregation
- `P2-B2`: risk hardening (TTL / slippage / liquidity guard / global switch)
- `P2-C`: settlement policy / reserve strategy enhancements

## Stable external wording

`The NOV native payment clearing mainline has moved from single-path clearing to a minimum dual-track multi-route aggregator; multi-source aggregation and advanced risk controls are post-P2-A enhancements.`

