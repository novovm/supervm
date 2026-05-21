# PR Stack Summary After go-ethereum a484a8506

## Scope

This stack closes the NOVOVM EVM cleanup and compatibility work after the
go-ethereum `592209c0e..a484a8506` update.

It is organized as eight independent merge candidates. Each patch has a bounded
theme, its own validation, and explicit Not Claimed items. The stack is intended
to be reviewed as a set of small, auditable changes rather than one historical
bulk diff.

This stack does not redefine the NOVOVM host/node entrypoint, does not redefine
the EVM plugin architecture, and does not turn NOVOVM into a strategy-specific
product. External wording remains NOVOVM. `SuperVM` / `SUPERVM` is retained only
as repository/path/internal historical code name.

## Patch List

1. `fbbb5d3` - geth-facing external compatibility.
2. `dc66a56a` - remove strategy-specific txpool surface.
3. `7023c25d` - layered RLPx session canary diagnostics.
4. `85807d0` - UA router RocksDB isolation / diagnosis.
5. `e14496d` - eth/71 / BAL design-only plan.
6. `ec530501` - local geth RLPx canary evidence.
7. `2bab0317` - historical EVM acceptance wording cleanup.
8. `da8e330a` - chain-scoped Amsterdam / Prague gas validation.

## 1. fbbb5d3 - geth-facing external compatibility

Purpose:

Minimal external Ethereum/geth compatibility patch after go-ethereum a484a8506.

Includes:

- `eth_baseFee` on the live external Ethereum RPC edge.
- Explicit `eth_baseFee` regression test.
- eth/71 capability guard.
- BAL `0x12` / `0x13` unsupported-safe handling.
- `balHash` serializer hook with omit semantics.
- Boundary assertion script and migration report.

Not Claimed:

- No full eth/71 / BAL support.
- No real `balHash` metadata source.
- No public RLPx session readiness.
- No old UA RocksDB migration.
- No strategy-observation acceptance result.
- No new EVM plugin architecture.

Review Focus:

- `eth_baseFee` uses the feeHistory source of truth.
- No non-geth `evm_baseFee` alias exists.
- `crates/novovm-node/src/main.rs` is intentionally unchanged.
- Capability guard does not falsely advertise eth/71.

## 2. dc66a56a - remove strategy-specific txpool surface

Purpose:

Remove strategy-specific txpool surface from the active NOVOVM EVM gateway and
migration tooling.

Includes:

- Removal of router/selector-specific txpool classification.
- Removal of priority scoring tied to strategy surface.
- Removal of strategy observation/autopilot migration scripts.
- Generic upstream impact scan restored to session canary / exec proof checks.
- `assert_no_strategy_surface_in_supervm.ps1`.

Not Claimed:

- No RLPx public canary diagnosis.
- No UA RocksDB migration.
- No full eth/71 / BAL support.
- No strategy-observation acceptance result.

Review Focus:

- Active gateway remains a generic Ethereum txpool / RLPx / execution surface.
- Strategy-specific acceptance language is absent from active paths.

## 3. 7023c25d - layered RLPx session canary diagnostics

Purpose:

Add layered diagnostics for public RLPx session probing.

Includes:

- Local controlled geth peer canary path.
- Public discovery-only canary.
- Public discovered-peer session canary.
- Layered counters for TCP, auth, ack, Hello, Status, and readiness.
- Report: `artifacts/migration/rlpx-session-canary-after-a484a8506.md`.

Observed Short-Window Result:

- Public DNS ENR discovery completed.
- One public candidate was found.
- Public session stopped below auth ack.
- One sample showed disconnect reason `4 / too_many_peers`.
- Another candidate timed out at TCP.

Not Claimed:

- No local geth session pass in this patch.
- No public RLPx readiness pass.
- No EVM plugin architecture change.

Review Focus:

- Bootnode/discovery is not treated as eth session readiness.
- Below-auth-ack public failure is not misreported as missing Hello/Status
  implementation.

## 4. 85807d0 - UA router RocksDB isolation / diagnosis

Purpose:

Harden UnifiedAccountRouter RocksDB state handling so stale or corrupt historical
state does not pollute gateway or RLPx canary diagnosis.

Includes:

- UA router state envelope:
  `magic / schema_version / codec / payload_len / checksum / payload`.
- Explicit `unified_account_router_state_decode_failed` diagnostic.
- Legacy decode only under explicit migrate mode.
- Reset only under explicit reset mode.
- Quarantine for old state.
- Canary default isolated UA state path.
- UA router state tests and migration report.

Not Claimed:

- No EVM execution semantic change.
- No geth RPC compatibility change.
- No RLPx handshake change.
- No eth/71 / BAL support.
- No automatic deletion of historical UA state.

Review Focus:

- Normal startup does not silently fallback across incompatible schemas.
- Explicit migrate/reset boundaries are respected.

## 5. e14496d - eth/71 / BAL design-only plan

Purpose:

Document the full eth/71 / Block Access List support plan without implementing
it.

Includes:

- Current guard state.
- `EthWireVersion::V71` placement options.
- `GetBlockAccessLists` / `BlockAccessLists` message design.
- BAL RLP encode/decode and hash validation requirements.
- Real `block-access-list-hash` / `balHash` metadata source requirements.
- Block JSON `balHash` transition plan.
- Capability negotiation Go / No-Go conditions.
- Phased implementation plan.

Not Claimed:

- No eth/71 implementation.
- No BAL wire support enabled.
- No capability advertisement change.
- No RLPx handshake semantic change.
- No UA RocksDB change.
- No strategy-specific txpool surface.

Review Focus:

- Design does not imply current support.
- eth/71 remains guarded until metadata, validation, fixtures, and tests exist.

## 6. ec530501 - local geth RLPx canary evidence

Purpose:

Record the `LocalGethEnode` control-group evidence skipped in `7023c25d`.

Observed:

- Local geth was built from `D:\WEB3_AI\go-ethereum` at `a484a8506`.
- Local geth listened at `127.0.0.1:30333`.
- TCP connect -> RLPx auth ack -> Hello -> Status -> eth/69 -> ready.
- Gateway observed `status_received remote_chain_id=1 negotiated_eth=69`.
- `ready_count = 1`.

Interpretation:

Local controlled geth session passes. The prior public below-auth-ack result is
therefore better classified as public peer selection, remote policy, or endpoint
reachability, not as an EVM plugin Hello/Status capability gap.

Not Claimed:

- No public RLPx session readiness pass.
- No `eth_baseFee` / `balHash` / eth/71 guard change.
- No full eth/71 / BAL implementation.

Review Focus:

- Local geth evidence is not overclaimed as public readiness.

## 7. 2bab0317 - historical EVM acceptance wording cleanup

Purpose:

Clean non-active historical materials so they do not misstate active NOVOVM EVM
acceptance or supported capabilities.

Includes:

- `ARCHIVED / NON-ACTIVE` labels for historical strategy research.
- Tightened NOVOVM / SUPERVM naming boundaries.
- Legacy daemon wording cleanup.
- WEB30 / SVM2026 source-era / archived labels.
- `assert_non_active_history_surface_boundary.ps1`.
- Cleanup report.

Not Claimed:

- No EVM implementation change.
- No RLPx implementation change.
- No UA RocksDB change.
- No eth/71 / BAL implementation.
- No strategy-specific txpool surface.
- No public RLPx readiness claim.

Review Focus:

- Historical materials are preserved but cannot be mistaken for active
  acceptance.
- `SUPERVM` remains internal/path/historical wording only.

## 8. da8e330a - chain-scoped Amsterdam / Prague gas validation

Purpose:

Close the remaining coherent gas/intrinsic worktree diff by making Amsterdam and
Prague intrinsic gas validation chain-scoped across EVM core, gateway, and
adapter.

Includes:

- Amsterdam access-list intrinsic gas validation behind chain-scoped env gating.
- Prague calldata floor gas validation behind chain-scoped env gating.
- Gateway `eth_estimateGas` path uses chain-aware intrinsic helper.
- Gateway `eth_sendRawTransaction` validates Prague calldata floor by chain.
- NOVOVM adapter host gas estimate uses `tx.chain_id` with the same core helper.
- Explicit gateway regression tests:
  - `eth_estimate_gas_chain_scoped_amsterdam_access_list_intrinsic_gas`.
  - `eth_send_raw_transaction_chain_scoped_prague_calldata_floor_gas`.
- Remaining-worktree diff audit report.

Not Claimed:

- No geth a484a8506 external compatibility changes.
- No `eth_baseFee` / `balHash` / eth/71 guard changes.
- No RLPx handshake changes.
- No UA RocksDB changes.
- No strategy-specific txpool surface.
- No full eth/71 / BAL implementation.

Review Focus:

- Core/gateway/adapter form one coherent gas/intrinsic semantic chain.
- Gateway tests directly prove chain-scoped Amsterdam/Prague toggle behavior.

## Validation Summary

Across the stack:

- geth upstream compatibility boundary guard passed.
- Strategy-surface guard passed.
- Non-active history surface guard passed.
- Relevant package-level cargo checks/tests passed per patch.
- Staged and post-commit diff audits passed per patch.
- Final worktree after `da8e330a` was clean.

Known caveat:

- Full workspace `cargo fmt --check` may still be affected by unrelated existing
  formatting deltas outside these patch scopes, where noted in individual patch
  records. Package-level fmt was used where appropriate.

## Overall Not Claimed

This stack does not claim:

- Full eth/71 / BAL support.
- Public RLPx session readiness.
- A new EVM plugin architecture.
- Strategy-specific txpool / M[E]V acceptance.
- U[n]iswap observation as NOVOVM EVM acceptance.
- Automatic deletion of historical UA RocksDB state.
- A product-level change to NOVOVM branding or host/node entrypoint.

## Recommended Review Order

1. Review `fbbb5d3` first because it fixes the direct geth-facing compatibility
   gap.
2. Review `dc66a56a` next because it removes strategy-specific surfaces from
   active paths.
3. Review `7023c25d` and `ec530501` together as RLPx diagnostic evidence.
4. Review `85807d0` independently as gateway persistence hardening.
5. Review `e14496d` as design-only. Do not evaluate it as implementation.
6. Review `2bab0317` as documentation/governance wording cleanup.
7. Review `da8e330a` last because it closes the prior dangling gas/intrinsic
   worktree diff.

## Merge Strategy

- Keep the eight patches as independent merge candidates.
- Avoid squashing them into one historical cleanup commit.
- If one patch needs follow-up, fix it in that patch's scope or add a narrow
  follow-up. Do not reopen unrelated sealed patch boundaries.
- Do not combine future eth/71 / BAL implementation, public peer selection
  improvement, or UA persistence work with this stack summary.

## Final Stack State

```text
Patch stack: 8 independent merge candidates
Worktree after da8e330a: clean
This summary: documentation only
```
