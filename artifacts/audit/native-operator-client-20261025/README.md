# Native operator consumer and A–D handoff

Input: `c82bbdc551f925af4bd3c395b19cebc319f00026` (main, clean and equal to
origin/main before this round). During validation, fast-forwarded the two
nonoverlapping mobile handoff documentation commits through `ec0b7c78`.
This is a thin consumer of the original
NativeApplicationClient, not a new transport, signer, executor or account store.

## Change and reuse

`novovm-native-client` calls the existing native SDK for Status, typed binary
query and pre-signed NUAPFL01 submission. Original session/sequence correlation,
NUAS decoding, bounded send/receive and node authentication remain authoritative.
No HTTP, automatic signing, grant generation or database access. The 64-row /
512-KiB operator input budget is not a protocol or throughput ceiling.
Transport delivery is explicitly not finality; node replies are observations,
not independent QC or ZK proofs. Monetary u128 values are rendered as decimal
strings, avoiding the generic JSON Value u64 limit; wire types are unchanged.

The existing NUAS process-recovery fixture invokes the actual CLI executable
for initial Status, UCA state query and its first signed transaction. Its other
transactions and all finality, replay, catch-up, respend and cold-state checks
stay on the existing fixture. A missing CLI binary fails instead of substituting
a library call. The subprocess uses the CLI's own bounded network loop, not a
separate parent-enforced subprocess deadline.

CI builds this binary beside the original node and runs the existing gate. The
existing Linux packager includes the CLI and both operator/handoff documents.
No additional independent workflow or relaxed check was introduced.

## Validation status

Windows x64 release results:

- Operator boundary tests: 4 passed (included in the next total, not additive).
- Native pipeline: 420 passed, 68 explicitly ignored; ignored tests not claimed.
- NUAS real child-node/CLI recovery: 1 passed in 132.84 s. Three healthy nodes
  reached h40; absent node started after 107.450 s; killed at durable h1 while
  h40 archive was absent; original DB resumed; credited NOV respend and four
  nodes at h41 verified. Final balances 994240/3915, budgets 994200/999915,
  nonces 40/1, total fees 1845; roots, receipts, fee records and cold reopen equal.
- Existing V3 native process closure: 1 passed in 1.55 s.
- Strict lib/tests/client release Clippy (`-D warnings`) and workspace fmt passed.

The real node executable is the retained `c82bbdc5` baseline (business and node
runtime unchanged here); CLI and test executable were rebuilt from this round's
source. This proves the new consumer works with the existing node, not a new
node performance qualification. CI will build both executables from the delivered
commit. See [original process report](uca-recovery-report.json), `operator_client`
for actual CLI observations and submission. Source/test outputs are preserved;
test keys, signed input files, configs, databases and signing journals are not uploaded.

| Executable/runtime | SHA-256 |
| --- | --- |
| Existing release novovm-node.exe | `1c12b640842b1db10c362943069f134fc7a76c3dee7a43b82cfb4ccc43401d33` |
| New release novovm-native-client.exe | `99dd4bdf323297abc33feb57bdbd84ffb41db469a4a448ed3dc27b89a28ccc82` |
| New release lib test executable | `dcbff7579a43f80829d164f1f99790942939ff0159a82df708b6e424b2f251f8` |
| Bundled Windows core AOEM DLL | `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947` |

Build with `cargo build -p novovm-node --bin novovm-native-client --release --locked`
and `cargo test -p novovm-node --lib --bin novovm-native-client --release --locked --no-run`.
Tests used `NOVOVM_AOEM_TEST_LIBRARY` pointing to the bundled Windows core,
`NOVOVM_RESIDENT_NODE_BINARY` pointing to the explicit node above,
`NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND=cpu_callbacks`,
`NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=0`, and one test thread. Filters/outputs:
`native_operator_`, `native_pipeline`, `native_uca_process_ --ignored`,
`native_only_genesis_transactions_queries_and_cold_recovery --ignored`.

Initial build exposed two unused must-use poll results in the adapter; corrected.
Independent review identified full-width JSON conversion risk and an invalid
response test fixture (mismatched query/answer); both corrected before final test.

Package safety: 25 assertions passed; build-failure/missing-artifact checks passed.
CI conservative scope routing: 11 tests passed.
No actual Linux package build/install was performed on this Windows host.

## Device scope

User assigned A (this host), B (Aliyun), C (phone-debug laptop), D (Linux AI host)
as validator candidates. E/F phones are not validators; G (MEV 5090 host) is a
reserve only. A inventory: Windows 11 x64, Ultra 9 275HX, 24 logical processors,
63.43 GiB reported RAM, 13.72 GiB D: free at the preparation checkpoint.
No remote deployment, security-group edits, service replacement or chain reset.
See [machine handoff](../../../docs/NOVOVM_MULTI_HOST_NUAS_HANDOFF.md).

## Not signed off

Real multi-host operation, Linux installation/systemd startup, new NUAS TPS,
production genesis/economic distribution, dynamic authorization lifecycle,
mobile custody/runtime, full-chain PQ and execution validity proofs remain
separate. This client does not generate approved allocation or signing inputs.
The earlier `c82bbdc5` CI run `40129230557` succeeded, but is not this change's CI.
The earlier first-query Linux startup-delay root cause remains unproven.
