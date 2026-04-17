# NOVOVM Observability P2-D Seal (2026-04-18)

Status: SEALED (Authoritative)  
Scope: Execution Trace + Metrics Summary + Debug Query contract for native clearing/policy/settlement path

## Purpose

This document seals the P2-D observability phase and freezes the query/trace contract that now drives production diagnostics and P3 activation decisions.

P2-D is a visibility layer. It does not change clearing semantics, routing semantics, or policy semantics.

## Phase status

- `P0`: sealed
- `P1`: sealed
- `P2-A`: accepted
- `P2-B1`: accepted
- `P2-B2`: accepted
- `P2-C`: FINAL seal
- `P2-D`: accepted and sealed

## Sealed scope (code facts)

### 1) Execution trace contract is implemented

Frozen trace objects:

- `NovExecutionTraceV1`
- `NovTraceQuotePhaseV1`
- `NovTraceRoutingPhaseV1`
- `NovTraceClearingPhaseV1`
- `NovTraceSettlementPhaseV1`

Trace captures quote, candidate/selected routes, clearing result, settlement result, policy context, and final failure code for each execution attempt.

### 2) Trace persistence is implemented

Frozen runtime persistence contract:

- `last_execution_trace`
- `execution_traces_by_tx`
- `execution_trace_order`
- bounded retention by `NOV_EXECUTION_TRACE_MAX_ENTRIES_V1`

### 3) Native debug query methods are implemented

Frozen native methods under `treasury` module:

- `get_last_execution_trace`
- `get_execution_trace_by_tx`
- `get_clearing_metrics_summary`
- `get_policy_metrics_summary`

### 4) External `nov_*` wrappers are implemented

Frozen external wrappers:

- `nov_getExecutionTrace`
- `nov_getTreasuryClearingMetricsSummary`
- `nov_getTreasuryPolicyMetricsSummary`

### 5) Metrics summary contract is implemented

Clearing summary includes stable counters such as:

- `total_clearing_attempts`
- `successful_clearings`
- `failed_clearings`
- `route_source_hits`
- `route_source_failures`
- `selection_reason_hits`
- `failure_counts`

Policy summary includes stable counters such as:

- `policy_contract_id`
- `policy_source`
- `threshold_state`
- `constrained_strategy`
- `threshold_state_hits`
- `constrained_strategy_hits`
- `policy_event_state_hits`

## Frozen non-semantic boundary

P2-D frozen behavior:

- observability records and exposes execution facts
- observability does not alter policy/routing execution logic
- observability does not auto-enable any P3 routing mode

## Validation baseline

- `cargo fmt --all --check`
- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
- `cargo deny check --disable-fetch` (non-blocking warnings allowed by current policy)

## Explicit non-claims

Still out of scope:

- multi-hop routing enablement
- split-order routing enablement
- automatic strategy tuning engine
- any implicit change to sealed P2-C policy contract

## P3 decision boundary (post-seal)

P3 activation should be evidence-driven from sealed P2-D metrics, not assumption-driven.  
At minimum, route hit/failure distribution and policy state distribution must justify expansion.

## Recommended external wording

`P2-D has established a sealed observability contract (trace + metrics + debug query) for native clearing and policy paths; P3 activation is evidence-driven.`

