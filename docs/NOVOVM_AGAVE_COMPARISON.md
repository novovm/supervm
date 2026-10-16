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

## Four-process preparation (2026-10-16)

- Isolated build initially failed to find `libudev.h`; the extracted dependency
  was present. Added its include directory via `CPATH`, without modifying Agave
  source or Cargo.lock. Rebuild is still in progress; no binary success claim.
- `scripts/perf/run-agave-four-validator-reference.sh` creates fresh keys and a
  shared development genesis with four equal bootstrap stakes and BLS keys.
  It starts four independent, loopback-bound validators for a bounded window,
  terminates only its own children and retains all logs/ledgers. Bash syntax
  checks passed; actual startup has NOT YET been executed. This minimal genesis
  is not a claim of full official demo program coverage. Do not reuse its fresh
  genesis leader-start flag for recovery of existing validators.
- Before load: verify four expected vote identities with equal nonzero active
  stake, identical genesis hash, increasing finalized slots and an identical
  blockhash at a common non-skipped finalized slot. Four running processes or
  four gossip identities alone are insufficient.
- Official wrapper's external `solana-transaction-bench` is pinned at 0.1.0;
  downloaded crate SHA256:
  `9ab43a2e638a761a854954f3b579d19702a6d72da45dbd3ccc9fa31ede64d997`.
  Its reported `successfully_sent` is transport-worker accounting, NOT unique
  successful finalized transactions. Do not use it as the comparison numerator.
  Per-signature successful finalization and economic checks remain outstanding.
- Reference RPC is loopback observation/configuration only; it does not change
  the NOVOVM native-only product policy. No NOVOVM/AOEM runtime code was changed
  by this preparation. No new TPS result or production readiness is claimed.
