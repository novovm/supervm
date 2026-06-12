# NOVOVM EVM Gateway Unified Account Dedup Signoff

Date: 2026-06-13
Status: PASS

## Conclusion

`novovm-evm-gateway` is now restricted to EVM RPC adapter responsibilities.

Unified Account state, identity binding, policy, mapped assets, account balances, and account assets are exclusively owned by:

```text
novovm-node -> mainline_query -> unified_account_surface
```

The gateway is no longer a Unified Account source of truth.

## Product Boundary

```text
gateway surface = evm_rpc_adapter_only
unified account source of truth = novovm-node -> mainline_query -> unified_account_surface
```

Allowed gateway responsibilities:

- EVM-compatible JSON-RPC adapter surface.
- `eth_sendRawTransaction` / `eth_sendTransaction` ingress.
- Pending transaction consumer.
- Receipt, block, tx, call, gas, and filter query surfaces for the EVM product path.
- Read-only Unified Account preflight through mainline.

Forbidden gateway responsibilities:

- No Unified Account creation.
- No identity/persona binding writes.
- No policy writes.
- No mapped asset registration.
- No `account_balance` / `account_assets` source generation.
- No local UCA truth source.

## Implemented State

Gateway-local UCA state has been removed from the production path:

- `GatewayUaStoreBackend`: removed.
- gateway-local UCA store load/save path: removed.
- gateway-local UCA router write path: removed.
- `ua_*` write match branches in gateway runtime: removed.

The gateway keeps only read-only mainline checks needed by EVM ingress:

```text
eth_sendRawTransaction / eth_sendTransaction
    -> mainline ua_checkRoute readonly preflight
    -> EVM adapter submit path
```

`ua_checkRoute` is a read-only preflight and does not advance nonce.

`eth_getTransactionReceipt` lookup order is:

```text
gateway local index/runtime
    -> mainline fallback only on local miss
```

This preserves gateway product reads while keeping Unified Account authority in mainline.

## Locked Rules

These rules are now product boundaries:

- gateway must not reintroduce any Unified Account write entry.
- gateway must not reintroduce a local UCA store/router as source of truth.
- gateway must not independently create, bind, update, or aggregate UCA state.
- `account_balance` and `account_assets` must only come from mainline unified account surface.
- EVM transaction ingress must fail closed if mainline UCA readonly check is unavailable.
- Gateway PRs must not weaken `evm_rpc_adapter_only`.

## Verification

The gateway runtime/test isolation and product boundary were verified with:

```text
cargo check -p novovm-evm-gateway
cargo test -p novovm-evm-gateway
cargo test -p novovm-evm-gateway -- --nocapture --test-threads=1
cargo test -p novovm-evm-gateway mainline_only -- --nocapture
cargo test -p novovm-evm-gateway gateway_error_ -- --nocapture
cargo test -p novovm-evm-gateway ua_checkRoute -- --nocapture
```

Result:

```text
PASS
novovm-evm-gateway full suite passes in default parallel mode.
novovm-evm-gateway full suite passes in serial mode.
UCA boundary lock tests pass.
```

## Commit Chain

```text
88d2c19 Disable gateway unified account entry
35c4fb0 Deduplicate gateway unified account state
95e31d4 Stabilize gateway runtime test isolation
```

## Signoff

```text
Gateway UCA Dedup: PASS
Gateway Runtime Test Isolation: PASS
Gateway Product Surface: LOCKED
```

