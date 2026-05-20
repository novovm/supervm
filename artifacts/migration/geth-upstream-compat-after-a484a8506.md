# NOVOVM geth upstream compat after a484a8506

Date: 2026-05-20

Code-level compatibility: PASS

Merge candidate: YES

## Scope

This patch closes the minimal geth-facing compatibility gap introduced by go-ethereum `592209c0e..a484a8506` for the active external Ethereum gateway path in the current SUPERVM workspace.

Project wording:

- External brand and technical wording should use `NOVOVM`.
- `SUPERVM` is treated as the current repository/path/internal historical code name.
- NOVOVM/SUPERVM is the host system.
- EVM is a plugin capability, not the host identity.
- This patch is a geth-facing external compatibility patch, not a NOVOVM architecture rewrite.
- The route remains: externally standard Ethereum-compatible behavior, internally AOEM-oriented refactoring.

## Entrypoint Clarification

This patch does not redefine the NOVOVM host/node entrypoint.

The NOVOVM host/node binary is the configured `novovm-node` bin path:

```text
crates/novovm-node/src/bin/novovm-node.rs
```

EVM is treated as a NOVOVM plugin capability.

The active external Ethereum RPC / bridge edge for this compatibility patch is:

```text
crates/gateways/evm-gateway
```

The live `eth_*` RPC dispatch path touched by this patch is:

```text
crates/gateways/evm-gateway/src/main.rs
```

The historical / non-authoritative file that must stay untouched is:

```text
crates/novovm-node/src/main.rs
```

Therefore, `eth_baseFee` is implemented only on the live `evm-gateway` RPC dispatch path. The historical/non-authoritative `crates/novovm-node/src/main.rs` file is intentionally left untouched.

The phrase "live gateway RPC path" means the active external Ethereum RPC edge, not the NOVOVM host entrypoint and not a new EVM plugin architecture.

## Implemented

- `eth_baseFee` implemented on the live `evm-gateway` RPC dispatch path.
- `eth_baseFee` accepts no params; non-empty params return JSON-RPC invalid params (`-32602`).
- `eth_baseFee` returns a JSON-RPC hex quantity.
- `eth_baseFee` uses the existing `eth_feeHistory(1, "latest", [])` source of truth and returns `baseFeePerGas.last()`.
- Native `novovm-network` capability guard caps current support at `eth/70`.
- EVM gateway RLPx capability guard caps current support at `eth/69`.
- Geth/compatibility profile construction cannot accidentally advertise `eth/71`.
- BAL message codes `0x12` / `0x13` are classified as unsupported-safe.
- BAL unsupported-safe handling records `unsupported_eth71_bal_message` and does not panic the session loop.
- `balHash` serializer hook added with omit semantics when real BAL metadata is unavailable.
- Gateway and mainline block JSON omit `balHash` consistently when BAL metadata is unavailable.
- Explicit boundary guard added: `scripts/migration/assert_geth_upstream_compat_boundary.ps1`.

## Tests

Passed:

```powershell
cargo build -p novovm-evm-gateway
cargo test -p novovm-evm-gateway eth_base_fee --quiet
cargo test -p novovm-evm-gateway eth_query_block_by_hash_tx_by_block_index_and_logs_work --quiet
cargo test -p novovm-evm-gateway rlpx_gateway --quiet
cargo test -p novovm-network eth71_bal_message_codes_are_classified_as_unsupported_safe --quiet
cargo test -p novovm-network capabilities --quiet
cargo test -p novovm-node --lib --quiet
git diff --check
powershell -ExecutionPolicy Bypass -File scripts/migration/assert_geth_upstream_compat_boundary.ps1
```

Optional check:

- `cargo fmt --check` was attempted and reported a pre-existing formatting delta in `crates/novovmctl/src/commands/governance_stats.rs`; that file has no diff in this patch and was intentionally left untouched.

## Known Gaps

`balHash` is a known parity gap.

It is omitted until real BAL metadata is available.

No synthetic hash is produced.

Full `eth/71` / BAL support is not implemented in this patch.

`GetBlockAccessLists` / `BlockAccessLists` data service is not implemented in this patch.

## Runtime Notes

The previous gateway startup blocker from:

```text
artifacts/gateway/unified-account-router.rocksdb
```

was bypassed by using an isolated state path.

The original local RocksDB state still has an unresolved schema / stale-state / corruption / migration issue and is tracked separately as a UnifiedAccountRouter persistence migration or isolation task.

This is not classified as an EVM execution, RPC semantic, `eth_baseFee`, `balHash`, or RLPx capability regression.

One public probing run did not observe an RLPx auth ack.

Therefore that run did not proceed far enough to observe Hello, Status, or eth capability negotiation.

Observed public probing result:

- `auth_sent_count = 4`
- `ack_seen_count = 0`
- `ready_count = 0`
- `disconnected_count = 4`
- `total = 8`
- `reachable = 4`

This does not mean the NOVOVM EVM plugin lacks RLPx Hello/Status implementation, and it does not redefine the EVM plugin architecture.

The result should be tracked as a public RLPx probing / peer-selection / network-egress / bootnode-vs-session-peer diagnostic item, not as a regression in `eth_baseFee`, `balHash`, BAL unsupported-safe handling, or the EVM plugin route.

Uniswap-named scripts are MEV / txpool diagnostic tooling present in the repository.

They are not part of SUPERVM / NOVOVM EVM plugin acceptance criteria.

No claim is made for that observation window, and the previous invocation is not retained as a SUPERVM / NOVOVM EVM acceptance result.

## Not Claimed

- Full `eth/71` / BAL support.
- Real `balHash` metadata source.
- Public RLPx session readiness.
- Old UnifiedAccountRouter RocksDB state migration.
- MEV / Uniswap observation result.
- New EVM plugin architecture.

## Next Independent Tasks

These are explicitly not part of this merge candidate:

- RLPx public canary layered diagnosis.
- UnifiedAccountRouter RocksDB migration / isolation hardening.
- Any MEV / Uniswap observation work.

## Diff Audit

本轮新增/修改内容:

- `crates/gateways/evm-gateway/src/main.rs`
- `crates/gateways/evm-gateway/src/main_tests.rs`
- `crates/gateways/evm-gateway/src/rpc_error_http.rs`
- `crates/gateways/evm-gateway/src/rpc_eth_query_helpers.rs`
- `crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs`
- `crates/novovm-network/src/eth_fullnode.rs`
- `crates/novovm-network/src/eth_rlpx.rs`
- `crates/novovm-network/src/transport.rs`
- `crates/novovm-node/src/mainline_query.rs`
- `scripts/migration/assert_geth_upstream_compat_boundary.ps1`
- `artifacts/migration/geth-upstream-compat-after-a484a8506.md`

工作区原本已有本地修改:

- `crates/novovm-adapter-novovm/src/lib.rs`
- `crates/plugins/evm/core/src/lib.rs`

明确未修改内容:

- `crates/novovm-node/src/main.rs` has no diff.
- No `evm_baseFee` / `evm_base_fee` alias exists under `crates`.
- No full `eth/71` / BAL implementation is claimed.
- No UA RocksDB migration code is included.
- No Uniswap / MEV observation is treated as SUPERVM / NOVOVM EVM acceptance.

## Merge Note

This patch closes the minimal geth-facing compatibility gap introduced by go-ethereum `592209c0e..a484a8506` for the active external Ethereum gateway path.

It does not redefine the NOVOVM host/node entrypoint. The NOVOVM host remains the configured `novovm-node` binary, while EVM remains a NOVOVM plugin capability. The `evm-gateway` path is the active external Ethereum RPC/bridge edge where `eth_baseFee` compatibility is required.

Implemented:

- `eth_baseFee` on the live `evm-gateway` RPC dispatch path.
- Explicit `eth_baseFee` regression coverage.
- `eth/71` capability guard.
- BAL `0x12` / `0x13` unsupported-safe handling.
- `balHash` serializer hook with omit semantics when no real BAL metadata exists.

Not claimed:

- Full `eth/71` / BAL support.
- Real `balHash` metadata source.
- Public RLPx session readiness.
- Old UnifiedAccountRouter RocksDB state migration.
- MEV / Uniswap observation result.
- New EVM plugin architecture.

`crates/novovm-node/src/main.rs` is intentionally left unchanged because it is not the authoritative `novovm-node` binary entrypoint in the current Cargo layout.
