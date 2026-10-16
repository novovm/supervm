# Agave comparison checkpoint (2026-10-16)

Purpose: measure comparable successful finalized transfers, then reuse applicable
pipeline mechanisms in the ONE existing NOVOVM/AOEM path. No new product engine.

- NOVOVM baseline: `ed3ab7af`; native single-entry/four-process 65,536 transfers.
- Reference: official Agave v4.3.0, commit
  `825efd18292aff6ffcf9daa0f7612f21b3531a72`.
- Official Linux release SHA256:
  `c97289a8abb1d0efb497d8b5cb285baabd9b7f8ea6647f5d145c5dc8ff3611e8`.
- WSL2 Ubuntu24.04, 24 logical CPUs, 47GiB visible memory. D: free about67GiB.
  Virtual filesystem free capacity is NOT host free capacity.
- Release `solana-test-validator --version` verified4.3.0; isolated local
  smoke started and RPC reported finalized slot3; process subsequently stopped.
  Single node only, no transfer throughput or four-validator claim.
- Release bundle lacks agave-validator/genesis/gossip and transaction load tool.
  Pinned source and isolated Rust1.97.1/protobuf3.21.12/libudev dependencies are
  under ignored `target/reference-agave`; no sibling repo/system installation.
  Build entry: `bash scripts/perf/build-agave-reference.sh` from WSL login shell.
  Source-linux is LF checkout; the first Windows checkout is reference-only.
  Build started; multi-node executables and load test NOT YET VERIFIED.

Do not reuse old bench-tps assumptions: v4.3.0 wrapper
`multinode-demo/txs-bench.sh` uses external `solana-transaction-bench`, which
must be separately pinned/audited. `runtime/src/bank.rs::commit_transactions`
counts processed transactions including failures; root-bank non-vote counters
are not unique successful workload receipts. Finalized signatures and economic
results must be independently checked. Do not replace four-process comparison
with in-process LocalCluster or single test-validator numbers.

Source comparison targets: banking_stage transaction_scheduler greedy_scheduler
and scheduler_common use conflict-aware, queued/in-flight cost-balanced workers;
banking_stage uses bounded work/result queues and byte/compute batch limits.
These are source capabilities, not measured gains. Compare independent accounts,
shared recipient, and dependent same-account transfers separately. Match hardware,
resource budget, native ingress, success accounting, sustained supply and recovery;
document protocol finality differences rather than pretending identical semantics.
Current finalized TPS comparison: NOT EXECUTED. No change to production policies.
