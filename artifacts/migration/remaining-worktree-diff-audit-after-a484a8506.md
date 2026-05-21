# Remaining Worktree Diff Audit After a484a8506

## Scope

This is a diff audit only. No files were staged or committed.

The seven sealed commits remain out of scope:

- `fbbb5d3` geth-facing external compatibility.
- `dc66a56a` strategy-specific txpool surface cleanup.
- `7023c25d` layered RLPx session canary diagnostics.
- `85807d0` UA router RocksDB isolation / diagnosis hardening.
- `e14496d` eth/71 / BAL design-only plan.
- `ec530501` local geth RLPx canary evidence follow-up.
- `2bab0317` historical EVM acceptance wording cleanup.

## Remaining Worktree Files

Current unstaged worktree diff:

- `crates/gateways/evm-gateway/src/main.rs`
- `crates/gateways/evm-gateway/src/main_tests.rs`
- `crates/novovm-adapter-novovm/src/lib.rs`
- `crates/plugins/evm/core/src/lib.rs`

Diff size:

- `evm-gateway/src/main.rs`: 10 changed lines.
- `evm-gateway/src/main_tests.rs`: gateway regression coverage added after the
  initial audit.
- `novovm-adapter-novovm/src/lib.rs`: 5 changed lines.
- `plugins/evm/core/src/lib.rs`: 206 changed lines.

## File A: evm-gateway/src/main.rs

Theme: gateway gas / intrinsic validation call-site alignment.

Observed diff:

- Imports `estimate_intrinsic_gas_with_envelope_extras_for_chain_m0`.
- Keeps the old helper available for test-only import.
- Replaces two active call sites with the chain-aware helper:
  - estimate path around `eth_estimateGas` style intrinsic calculation.
  - send path intrinsic precheck before gateway ingress record creation.
- Passes `chain_id` into the core estimator.

Interpretation:

- This file is not the semantic source of the change.
- It adapts the external Ethereum RPC / bridge edge to the new core estimator.
- It does not touch `eth_baseFee`, fee history, RLPx, UA RocksDB, eth/71 guard,
  BAL fallback, or strategy-specific txpool surface.

Risk:

- Medium if committed without the core change, because the new function is
  defined in `novovm-adapter-evm-core`.
- Medium if gateway-specific regression tests do not cover the chain-scoped
  Amsterdam / Prague gas toggles.

## File B: novovm-adapter-novovm/src/lib.rs

Theme: NOVOVM adapter host gas estimate alignment.

Observed diff:

- Imports `estimate_intrinsic_gas_with_envelope_extras_for_chain_m0`.
- Updates `tx_intrinsic_gas_with_envelope_extras_v1` to pass `tx.chain_id`.

Interpretation:

- This is an adapter call-site alignment, not a new execution truth source.
- The adapter still delegates intrinsic gas semantics to
  `novovm-adapter-evm-core`.
- No direct AOEM FFI bypass, account-id remapping, asset source change, or write
  path expansion was observed in this diff.

Risk:

- Low-to-medium as a call-site patch when paired with the core change.
- Medium if committed alone, because it depends on the new chain-aware core
  helper.

## File C: plugins/evm/core/src/lib.rs

Theme: chain-scoped Amsterdam / Prague gas semantics.

Observed diff:

- Adds chain-scoped environment switches:
  - `NOVOVM_EVM_ENABLE_AMSTERDAM_GAS_RULES`
  - `NOVOVM_EVM_ENABLE_PRAGUE_FLOOR_GAS`
- Adds Amsterdam access-list intrinsic extra gas calculation.
- Adds a chain-aware intrinsic helper:
  - `estimate_intrinsic_gas_with_envelope_extras_for_chain_m0`
- Adds calldata floor gas helpers:
  - `estimate_calldata_floor_gas_m0`
  - `estimate_calldata_floor_gas_for_chain_m0`
- Updates `validate_tx_semantics_m0` to use the chain-aware intrinsic helper.
- Adds optional Prague floor-gas rejection when the chain-scoped gate is enabled.
- Adds regression tests for:
  - Amsterdam access-list intrinsic delta.
  - Amsterdam calldata/access-list floor gas token charging.
  - chain-scoped Amsterdam validation rejection.

Interpretation:

- This is the semantic source of the remaining diff.
- The old helper `estimate_intrinsic_gas_with_envelope_extras_m0` remains
  available, preserving default callers and tests.
- Default behavior appears unchanged because the new Amsterdam / Prague gates
  default to disabled.

Risk:

- Medium-high because it touches EVM transaction validation and gas accounting.
- The env-var based tests mutate process-wide environment and should be watched
  for parallel-test flakiness if more env-sensitive tests are added.
- The `NOVOVM_EVM_ENABLE_PRAGUE_FLOOR_GAS` default currently follows
  `NOVOVM_EVM_ENABLE_AMSTERDAM_GAS_RULES`; this is a policy decision and should
  be explicitly accepted before commit.

## Proposed Patch Split

Recommended split:

```text
Patch A:
evm: add chain-scoped Amsterdam/Prague gas validation

Files:
- crates/plugins/evm/core/src/lib.rs
- crates/gateways/evm-gateway/src/main.rs
- crates/gateways/evm-gateway/src/main_tests.rs
- crates/novovm-adapter-novovm/src/lib.rs
```

Reason:

- All three files are one dependency chain.
- Gateway and adapter are call-site alignment.
- EVM core is the semantic implementation.
- Splitting these three files into separate commits would either create a compile
  dependency gap or leave callers on old semantics.

Do not split into independent Patch A/B/C unless the core helper is first merged
with compatibility shims and the call-site changes are deliberately deferred.

## Risk Classification

- `evm-gateway/src/main.rs`: medium, call-site only.
- `novovm-adapter-novovm/src/lib.rs`: low-to-medium, call-site only.
- `plugins/evm/core/src/lib.rs`: medium-high, semantic gas validation.
- Combined patch: medium-high, but coherent and testable as one gas/intrinsic
  patch.

## Required Tests

Already run during this audit:

- `cargo test -p novovm-adapter-evm-core amsterdam --quiet` passed.
- `cargo test -p novovm-adapter-evm-core intrinsic --quiet` passed.
- `cargo test -p novovm-adapter-evm-core --quiet` passed.
- `cargo check -p novovm-evm-gateway --quiet` passed.
- `cargo check -p novovm-adapter-novovm --quiet` passed.
- `cargo test -p novovm-adapter-novovm tx_intrinsic_gas --quiet` passed.
- `cargo test -p novovm-adapter-novovm --quiet` passed.
- `cargo test -p novovm-evm-gateway gas --quiet` passed.
- `cargo test -p novovm-evm-gateway intrinsic --quiet` passed.
- `cargo test -p novovm-evm-gateway chain_scoped --quiet` passed.
- `git diff --check` passed.

Gateway regression coverage added after the initial audit:

- `eth_estimate_gas_chain_scoped_amsterdam_access_list_intrinsic_gas`
  verifies that the gateway estimate path uses chain-scoped Amsterdam access-list
  intrinsic gas.
- `eth_send_raw_transaction_chain_scoped_prague_calldata_floor_gas` verifies that
  the gateway raw transaction path accepts the same tx with the chain-scoped
  Prague floor disabled and rejects it when the chain-scoped floor is enabled.

Recommended before commit:

- Re-run the validation matrix listed above after final staging.

## No-Go Conditions

Do not commit if:

- The patch is described as geth a484a8506 compatibility, RLPx, UA RocksDB, or
  eth/71 / BAL support.
- Gateway or adapter changes are committed without the core helper they depend
  on.
- The Amsterdam / Prague gas gates are enabled by default without explicit
  product acceptance.
- The Prague floor-gas defaulting to Amsterdam gas rules is not accepted.
- Env-var tests become flaky under parallel execution.
- No gateway-level regression covers the new chain-scoped behavior.
- The patch reintroduces strategy-specific txpool surface or acceptance wording.

## Recommended Next Action

Treat the three remaining dirty files as one coherent gas/intrinsic patch:

```text
evm: add chain-scoped Amsterdam Prague gas validation
```

Targeted gateway regression coverage has now been added. The next step can be a
staged diff audit for the coherent gas/intrinsic patch.

Do not stage or commit this audit report as part of the gas/intrinsic patch
unless a documentation/audit commit is desired separately.
