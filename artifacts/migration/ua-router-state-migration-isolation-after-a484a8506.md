# UA Router State Migration Isolation After a484a8506

Status: gateway / UnifiedAccountRouter persistence hardening patch.

## Scope

This patch handles stale, legacy, or corrupt UnifiedAccountRouter gateway state at:

```text
artifacts/gateway/unified-account-router.rocksdb
```

It is not an EVM execution semantic change, geth RPC compatibility change, RLPx session fix, or eth/71/BAL implementation.

## Problem

The gateway can be blocked during startup when the local UnifiedAccountRouter RocksDB state was written by an older schema or contains bytes that no longer match the current router type.

Using an isolated state path lets canaries continue, but the original state still needs a clear product behavior:

```text
diagnose
isolate
explicitly migrate
explicitly reset
never silently delete or silently reinterpret historical state
```

## Implemented

- Added a stable UA router state envelope with:
  - magic
  - schema_version
  - codec
  - payload_len
  - checksum
  - payload
- Normal startup now accepts only the envelope format.
- Legacy state is detected and reported as migration-required instead of being silently decoded.
- Explicit legacy migration is gated by:

```text
NOVOVM_GATEWAY_UA_STORE_MIGRATE_LEGACY=1
```

- Explicit reset/quarantine is gated by:

```text
NOVOVM_GATEWAY_UA_STORE_RESET=1
NOVOVM_GATEWAY_UA_STORE_QUARANTINE_DIR=<optional path>
```

- Reset moves the existing state into a quarantine directory before returning an empty router.
- Decode failures now include:
  - `unified_account_router_state_decode_failed`
  - path
  - backend
  - classification
  - decode_attempts
  - safe_action
  - detail
- `run_evm_eth_plugin_session_canary.ps1` now uses an isolated gateway state directory by default.

## Classification

Failure classification uses:

```text
schema_mismatch
stale_state
corrupt_state
unsupported_version
```

Known legacy state is classified as `schema_mismatch` during normal startup, with an explicit migration action in the diagnostic.

## State Format

New state is written as:

```text
magic: NVUAENV1
schema_version: 1
codec: bincode
payload_len: u64
checksum: sha256(domain || schema_version || codec || payload)
payload: bincode(UnifiedAccountRouter)
```

The previous simple bincode envelope remains readable only in explicit migration mode.

## Canary Isolation

Canaries should not reuse:

```text
artifacts/gateway/unified-account-router.rocksdb
```

The plugin session canary now writes gateway state under:

```text
artifacts/gateway/canary/eth-plugin-session/<run_id>/
```

The layered RLPx canary from the previous diagnostic patch already writes under:

```text
artifacts/migration/state/rlpx-layered-canary-<run_id>/
```

## Migration / Reset Semantics

Normal startup:

```text
envelope decode only
legacy state detected -> fail with migration-required diagnostic
corrupt/stale state -> fail with classified diagnostic
no automatic reset
no automatic deletion
```

Explicit migration:

```text
NOVOVM_GATEWAY_UA_STORE_MIGRATE_LEGACY=1
legacy decode is attempted
on success, state is rewritten using the new envelope
on failure, startup fails with classified diagnostic
```

Explicit reset:

```text
NOVOVM_GATEWAY_UA_STORE_RESET=1
existing state is moved to quarantine
gateway starts with an empty router
```

## Tests

Validation targets:

```text
cargo test -p novovm-evm-gateway ua_router --quiet
cargo test -p novovm-evm-gateway plugin_session --quiet
cargo check -p novovm-evm-gateway --quiet
cargo fmt -p novovm-evm-gateway --check
```

Boundary guards:

```text
scripts/migration/assert_geth_upstream_compat_boundary.ps1
scripts/migration/assert_no_strategy_surface_in_supervm.ps1
```

## Not Claimed

- no EVM execution semantic change
- no geth RPC compatibility change
- no RLPx session readiness change
- no eth/71 or BAL support
- no strategy txpool surface
- no automatic deletion of historical UA state

## Diff Audit

Expected patch scope:

```text
crates/gateways/evm-gateway/src/main.rs
crates/gateways/evm-gateway/src/main_tests.rs
scripts/migration/run_evm_eth_plugin_session_canary.ps1
artifacts/migration/ua-router-state-migration-isolation-after-a484a8506.md
```

Pre-existing worktree changes remain out of scope and must not be mixed into this commit.

## Merge Note

This patch hardens the NOVOVM gateway UnifiedAccountRouter persistence path so stale or legacy local state cannot be mistaken for an EVM, geth compatibility, or RLPx regression. Historical state now requires explicit migration or explicit quarantine/reset, and canary runs use isolated state paths by default.
