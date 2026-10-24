# NUAS process readiness: bounded timeout recovery

Input: `826d9b73fe56b9904972d0cf7f0b3729e6c3fb31`, clean main, fetched equal to origin/main.

## Confirmed failure and scope

CI [40124666359](https://github.com/novovm/supervm/actions/runs/40124666359)
failed in NUAS original-node process recovery, before submitting any transaction.
The report records `completed=false`, empty `offline_progress`, and a first
`native query timeout: Status` after 5,063 ms. Raw Linux failure log/report are
preserved alongside this document. The node startup stdout/stderr were empty.
This does not establish why Linux initialization/status delivery took longer,
nor prove a consensus or economic failure. The aggregate scope job correctly
failed because the selected Rust check failed.

The test's original 90-second readiness budget was defeated by the first
strict five-second query. This patch changes only the existing test harness:

- Keep ordinary asset/receipt queries strict at five seconds.
- During readiness/recovery, retry only a typed query timeout, after the existing
  `NativeApplicationClient::abandon_query` releases read correlation. Existing
  bounded transport retains admitted bytes; no transactions are discarded.
- All peers/attempts share the original 90-second deadline, with each attempt
  capped at five seconds or remaining time, whichever is smaller.
- Poll tracked child liveness inside each query wait. Child exit, transport,
  malformed response and projection errors still fail immediately.
- Use this at initial readiness, restarted-node height waits and the NUAS late
  starter. Publication, balances, budgets, nonce, fee, root and crash assertions
  remain unchanged. No production timer, protocol, AOEM, signature or quorum
  change; no alternative execution path.

## Regression design

Three deterministic tests cover a timeout followed by success, shared total
deadline exhaustion (including another peer), and fatal-error propagation
without relying on matching error strings.

A real-process delayed-start test intentionally leaves the original NovoRUDP
client without a server for the first five-second request, then starts the
original node and obtains its genesis status using the same client. It is part
of the existing `native_only_` CI filter with an explicit enumeration assertion.

## Local artifact identity

Windows x86_64, release tests, shipped AOEM core, CPU callbacks, legacy Host
execution explicitly disabled. Production source/binary and DLL unchanged by
this test-only patch:

- Node SHA-256: `1c12b640842b1db10c362943069f134fc7a76c3dee7a43b82cfb4ccc43401d33`
- AOEM SHA-256: `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`
- New lib-test SHA-256: `cf46485cd78f61b4d2fc410da101503a198620b357f4eaf476656e608d6ac6a4`

Three deterministic readiness regressions passed. The real delayed-start test
passed with two attempts in 5,111 ms (test wall time 5.12 s). See the corresponding
raw logs. Full NUAS four-process recovery passed in 132.66 s: three healthy
nodes published 40 heights; missing node 0 started after 107,430 ms and was
killed at durable height 1 with height 40 absent and three-signature QC. Original
database restart caught up; credited funds were spent again and all four nodes
reached height 41. Full cold reopen, fee oracle, budgets, nonces, balances,
receipts and preserved signing outbox/config/key assertions passed.

Final balances: 994240 / 3915; budgets: 994200 / 999915; nonces: 40 / 1.
Forty-one fees of 45 total 1845, with reserve/fee/risk buckets 1271 / 369 / 205.
These are isolated test base units, not mainnet allocations or a throughput run.
See `nuas-process-report.json` and per-launch process logs.

Additional local results: 416 native pipeline regressions passed, 68 explicitly
ignored (the new delayed-start and NUAS process cases were run separately);
original V3 native transaction/query/cold-recovery process test passed (1.58 s).
CI routing's 11 tests and workflow YAML parsing passed.
Strict `cargo clippy -p novovm-node --lib --tests --release --locked -- -D warnings`
and `cargo fmt --all -- --check` passed. A read-only independent diff review
found no blocking issue; it did not constitute another runtime test.

Reproduce with the existing explicit library/node environment and CPU backend:
`cargo test -p novovm-node --lib --release --locked native_status_readiness`,
then the ignored `native_only_status_readiness_survives_first_query_timeout`
and `native_uca_process_` filters, each with `--nocapture --test-threads=1`.

Linux CI, real multi-host deployment, mainnet activation and TPS are separate
acceptance boundaries; no such results are claimed by this harness repair.
