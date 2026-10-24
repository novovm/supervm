# NUAS original-node process recovery

Input: `000e221e`. Windows x86_64, one host, four independent original
`novovm-node` processes and databases. Isolated test NOV only; no mainnet
activation, mobile deployment or TPS qualification.

## Reuse and change boundary

The existing `native_product_rpc::ProductNodes` process fixture, original native
application client, typed queries and `native_product_recovery` archive inspection
are reused. The new `native_product_uca_recovery` test submits genuine ML-DSA-65
NUAS intents through the existing NovoRUDP/APFL input. There is no in-process
validator, HTTP observer, wallet replay to the laggard, database copy, new runtime,
consensus rule, storage authority or production fault hook.

The shared test client uses the already-defined NUAS byte budget when the node
configuration has UCA initialization; V3 fixture limits remain unchanged. Existing
recovery helpers become visible to the sibling test only. Process stdout/stderr
now have per-launch names, preserving every interrupted/restarted launch rather
than overwriting the previous `existing` run.

## Acceptance and evidence

Windows release validation passed. `process-second.log` and `process-report.json`
record the final test (132.50 seconds; functional fault-scenario duration, NOT TPS):

- Three original node processes published forty heights while validator 0 (highest
  peer identity) was absent. Every height used a real ML-DSA-65 signed spend sent
  to one healthy native application entrance; no synthetic QC or direct injection.
- The missing validator started after 107,180 ms. It was killed at durable height
  1; its stopped original DB had no height-40 archive. The saved decision had three
  verified signatures. Original-DB restart replayed the missing history.
- The already-connected validator was independently stopped again. Bob spent 40
  of the 4,000 test units actually received; restart caught up without the wallet
  replaying its transactions to that validator. All four finished at height 41.
- Alice/Bob balances: 994,240 / 3,915; remaining budgets: 994,200 / 999,915;
  stable nonces: 40 / 1. Forty-one successful transactions charged 45 units each.
  Fees totaled 1,845: reserve 1,271, fee bucket 369, risk buffer 205. Balances plus
  fees equal the original 1,000,000 isolated test units; budgets are not added to
  supply. All typed views and ordered receipts match across four nodes.
- Original signing outbox bytes, configuration and validator key files remain
  unchanged. All four original databases cold-reopened with the same root,
  receipts and economic views. Every stopped store's canonical fee/policy pages,
  window and diagnostics matched the ordered oracle.

`native-regression.log`: 413 passed, 0 failed, 67 explicitly ignored.
`v3-native-process.log`: existing native V3 genesis/transactions/queries/cold
recovery passed (one explicitly selected process test). `clippy.log`: strict
release node lib/tests Clippy passed. Workspace fmt and workflow YAML parsing
passed; CI scope routing tests: 11 passed. Remote Linux/CI execution is separate.

`process-first.log` and `failed-fixture-window.json` retain the first failure:
the new fixture requested a 999-height intent validity window, exceeding the
existing 256-height wire bound. It failed before any transaction publication.
Only the fixture expiry was corrected to height 100; protocol bounds are unchanged.

The fee oracle uses the existing ordered `quote_and_settle`, original configured
policy and actual candidate timestamp, rather than treating receipt charges as
their own expected result. After stopping the nodes, every original fee/policy
record page (including absent tails) is compared against that oracle at the
verified final root. These are assertion-only reads, never ledger installation.

The bounded scenario requires three healthy validators to publish forty heights
with the fourth absent, late authentication after the initial handshake interval,
a kill during catch-up with the target archive still absent on disk, original-DB
resume, recipient respend during another independent outage, four-node typed
economic equality and cold reopen. Original configuration and signing outbox must
be retained. The final report and raw logs will record actual results and failures.

Asset authorization is ML-DSA-65. The existing fixed fixture validator and
transport identities are still Ed25519; this is not full post-quantum consensus.
Typed query results are authenticated node observations, not state/validity proofs.

CI adds one explicitly selected ignored process test with an enumeration guard.
Failure artifacts allow only reports and stdout/stderr; no configurations, private
keys or databases are uploaded. The existing correctness gates remain enabled.

## Artifact identity and remaining boundaries

- Rust 1.94.0, `x86_64-pc-windows-msvc`, release, `cpu_callbacks` fixture backend.
- Fresh original node SHA256:
  `1c12b640842b1db10c362943069f134fc7a76c3dee7a43b82cfb4ccc43401d33`.
- Final test executable SHA256:
  `602d5f438b6c9eb975dd4868719109b6d17a4ff22dc29bd5f7d928f7489ca8eb`.
- Shipped AOEM SHA256:
  `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.

No production runtime algorithm was changed in this cut. This closes the NUAS
independent-process recovery gap on Windows, not all crash combinations: precise
storage commit-point power loss, combined missing-body/A/A/B on NUAS, opposite
missing-peer ordering for this new asset profile, multi-host/Linux qualification,
dynamic authorization changes, funded service settlement, mobile activation,
mainnet distribution, full PQ signatures, ZK and new TPS remain separate work.
Prior V3 opposite-order/A/A/B evidence is retained, not relabeled as NUAS evidence.
