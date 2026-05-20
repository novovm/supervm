# NOVOVM RLPx Local Geth Canary After a484a8506

Date: 2026-05-21

Status: local controlled geth evidence follow-up complete.

Result: PASS.

## Scope

This report supplements the sealed layered RLPx diagnosis patch:

```text
7023c25d evm: add layered rlpx session canary diagnostics
```

The earlier layered diagnosis had `local controlled geth peer` marked as skipped
because `LocalGethEnode` was not supplied. This follow-up supplies a local geth
enode and records the local controlled session result.

This is evidence capture only. It does not change RLPx handshake semantics,
geth-facing RPC compatibility, BAL guard behavior, UA RocksDB handling, or
NOVOVM plugin architecture.

## Input

Local geth source:

```text
D:\WEB3_AI\go-ethereum
commit: a484a8506
```

Temporary geth build:

```text
D:\Temp\novovm-geth-a484a8506\geth.exe
Geth/v1.17.4-unstable-a484a850-20260519/windows-amd64/go1.24.0
```

The temporary Go toolchain and module cache were kept outside the SUPERVM
working tree.

Local geth was started with an isolated datadir under:

```text
artifacts/migration/state/local-geth-canary-1779313167910
```

The final local geth run used a non-dev mainnet genesis configuration because
geth `--dev` mode explicitly disables networking and reports `tcp=0`.

## LocalGethEnode

The local controlled geth peer advertised:

```text
enode://5255fa0ebfda95bf0c49f3220f14a7fce98fea34ed665511779b6be0c021a67e1f6862b1758c30fe57afe50ff76c59d38c48e50cab0c4cc486974c3103aed5c0@127.0.0.1:30333?discport=0
```

geth log evidence:

```text
Started P2P networking self="enode://...@127.0.0.1:30333?discport=0"
```

## Observed Result

Local controlled geth session: PASS.

Observed gateway log evidence:

```text
rlpx stage tcp_connected
rlpx stage auth_sent
rlpx stage ack_received
rlpx stage hello_sent
rlpx stage hello_received
rlpx stage status_received remote_chain_id=1 negotiated_eth=69
rlpx stage status_sent
rlpx stage ready
```

The local geth remote advertised:

```text
remote_name=Geth/v1.17.4-unstable-a484a850-20260519/windows-amd64/go1.24.0
remote_caps=eth/69,eth/70,eth/71,snap/1
```

The NOVOVM gateway side advertised current gateway capability:

```text
caps=eth/66,eth/67,eth/68,eth/69
```

The selected shared capability was:

```text
eth/69
```

This is expected for the current gateway guard state and does not advertise
`eth/71`.

## Layered Counters

Generated summary:

```text
artifacts/migration/rlpx-local-geth-canary-after-a484a8506-summary.json
```

Local layer counters:

```text
local_geth_enode_supplied = true
tcp_connect_attempt_count = 3
tcp_connect_success_count = 1
tcp_connect_fail_count = 2
rlpx_auth_sent_count = 1
rlpx_auth_ack_seen_count = 1
rlpx_auth_timeout_count = 0
rlpx_disconnect_before_ack_count = 0
hello_sent_count = 1
hello_seen_count = 1
status_sent_count = 1
status_seen_count = 1
ready_count = 1
disconnect_reason_code = none
remote_endpoint = 127.0.0.1:30333
selected_eth_capability = eth/69
```

The two TCP failures are the gateway peer selector probing fallback addresses
`127.0.0.1:30303` and `127.0.0.1:30304`; the supplied geth enode endpoint
`127.0.0.1:30333` reached ready.

Reporting caveat:

- The generated JSON field `selected_eth_capability` remained empty.
- The gateway stderr log records `negotiated_eth=69` at `status_received`.
- This report treats the log line as the capability evidence for this run.

## Public Comparison

The public discovered-peer session in the same short window remained below auth
ack:

```text
tcp_connect_success_count = 1
rlpx_auth_sent_count = 1
rlpx_auth_ack_seen_count = 0
ready_count = 0
disconnect_reason_code = 4 / too_many_peers
```

Interpretation:

- Local controlled geth proves the local RLPx path can reach auth ack, Hello,
  Status, selected capability, and ready.
- The prior public finding remains classified as public peer selection, remote
  peer policy, or endpoint reachability.
- It is not evidence that the NOVOVM EVM plugin lacks Hello/Status handling.
- It is not evidence of an `eth_baseFee`, `balHash`, or eth/71 guard regression.

## Logs

Relevant logs:

```text
artifacts/migration/logs/local-geth-1779313167910.stderr.log
artifacts/migration/logs/rlpx-layered-local-controlled-geth-peer-1779313169738.stderr.log
```

## Not Claimed

- no protocol fix
- no RLPx handshake semantic change
- no geth-facing RPC compatibility change
- no `eth_baseFee` change
- no `balHash` behavior change
- no `eth/71` capability advertisement
- no full eth/71 or BAL implementation
- no UA RocksDB change
- no strategy-specific txpool surface
- no NOVOVM plugin architecture rewrite

## Diff Audit

This evidence follow-up adds only:

```text
artifacts/migration/rlpx-local-geth-canary-after-a484a8506.md
```

The generated JSON summary remains an ignored local artifact:

```text
artifacts/migration/rlpx-local-geth-canary-after-a484a8506-summary.json
```

The existing unrelated worktree changes remain out of scope:

```text
crates/gateways/evm-gateway/src/main.rs
crates/novovm-adapter-novovm/src/lib.rs
crates/plugins/evm/core/src/lib.rs
```

## Merge Note

This patch records a local controlled geth RLPx canary follow-up for NOVOVM.

The local geth peer was built from go-ethereum `a484a8506` and reached:

```text
TCP -> RLPx auth ack -> Hello -> Status -> eth/69 -> ready
```

The result fills the local controlled geth evidence gap left by `7023c25d`.
It does not change protocol semantics or expand current `eth/71` support.
