# Original node tick integration checkpoint

Recovery source: `3ed27becae0eb23c7f13a99e533e249277617d84`, parent
`1796346e233fc4471b677c41249301f13e7e3de4`, existing detached July recovery
worktree. This source is **not yet integrated into shared main**. Reports below
are recovery-build results, not claims about shared main or a release artifact.

The actual executable now enters the existing `native_execution_tick` loop.
Its original `UdpTransport` transfers receive ownership to the shared
authenticated object implementation; the same loop polls reused admission,
candidate execution, durable consensus and native query components. There is
no additional listener, raw-pending executor or progress-derived publication
on this configured path. The old resident bootstrap requires the explicit
historical comparison feature; it is rejected in the tested default binary.

## Evidence

- Windows debug, one machine, four independent original-node processes/stores.
- CPU native closure plus minority/signature-conflict/recovery: 2 passed,
  6.16 seconds for the combined functional test invocation.
- Complete AOEM algebraic backend, same two cases: 2 passed, 15.55 seconds.
- These are **test durations, not TPS or latency measurements**.
- `cpu-closure.json`, `cpu-minority.json`, `algebraic-closure.json`, and
  `algebraic-minority.json` preserve the original report values; formatting is
  normalized. No configuration files, test signing keys or databases copied.
- Closure tests use a nonvalidator single ingress, native submission/query on
  the same authenticated session, no HTTP listener, bad-signature rejection,
  5 success / 1 business-failure initial transactions and 2 successful
  post-restart transactions, fees/balances/nonce/receipts and identical heads.
- Minority cases retain 2-of-4 nonpublication and conflicting signature checks,
  then bring up the other validators and verify progress/restart. Not A/A/B
  split-vote, multi-machine or interrupted durable-write acceptance.
- Final regular pipeline suite: 341 passed, 42 ignored (17.59 seconds).
- Native structural/socket suite: 9 passed (0.25 seconds); native transport
  suite: 80 passed (1.24 seconds); new ingress policy tests: 3 passed (0.02s).
- Default binary build/check and non-strict Clippy completed. Existing warnings
  remain; the new channel constructor also triggers `too_many_arguments`.
  This is not a strict-Clippy or remote-CI pass.
- Initial compilation failed on a NodeId type-alias constructor and the nested
  test module path; both fixed and rerun, not suppressed.

## Artifact identity

| Artifact | SHA256 |
| --- | --- |
| original `novovm-node.exe` | `98a868efb7a74d269c97612d74921d8f4a739a5552c7515ded2f40e5fec1973f` |
| node library test executable | `13465ecf16cf1f45e5a555b1837cd7880ed5d123b77977661d1c706beb4a6f1a` |
| existing shared AOEM Windows core | `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947` |

AOEM source/package remains the existing `aab3f8bb` SDK; strict manifest
loading was enabled. The historical July SDK was not overwritten. Tests set
`AOEM_SEMANTIC_GRAPH_V2_WORKERS=4`; local test setting, not a new product default.

Reproduce from the recovery source with explicit current SDK and binary:

```powershell
cargo build --locked -p novovm-node --bin novovm-node
# Set NOVOVM_AOEM_TEST_LIBRARY, AOEM_DLL_MANIFEST_REQUIRED=1,
# AOEM_DLL_MANIFEST and NOVOVM_RESIDENT_NODE_BINARY to the matched artifacts.
# Set NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND=cpu_callbacks or aoem_integer_v7.
cargo test --locked -p novovm-node --lib native_only_ -- --ignored --test-threads=1 --nocapture
```

The fixture launches `NOVOVM_NODE_MODE=native_execution_tick`, an explicit
`NOVOVM_NATIVE_EXECUTION_CONFIG` with the same profile, and
`NOVOVM_NATIVE_EXECUTION_START=create|existing`. It does not set legacy Host
permission or fixed fixture transaction totals. The application pool remains
bounded and non-durable; transport DONE does not authorize publication.

## Not completed by this checkpoint

Shared-main activation, native bulk finalized-throughput comparison, successor
overlap, complete product scope, business validity proofs, Linux and real-network
release validation remain outstanding. Admission/pool selection still has
canonical-row materialization boundaries; retaining structured BatchPlan input
does not prove the entire original bulk path is zero-materialization. The
historical July ~hundred-thousand row/s fixture is unchanged and is not relabeled
as these functional tests' finalized transaction TPS. No new TPS claimed.
