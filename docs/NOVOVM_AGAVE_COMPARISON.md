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

## Actual native TPU smoke (2026-10-16)

`solana-transaction-bench`0.1.0 release build passed in5m12s, binary SHA256
`9966a39206d00f2ecf93b197f95e47650a6bca38db4eb31462597c818dd1f6d7`.
Build uses the published Cargo.lock (client dependencies4.0.0-rc.0), not the
validator4.3.0 lock. The extracted manifest required an empty `[workspace]`
table to isolate it from SUPERVM; no Rust source or dependency change. Build
entry is `scripts/perf/build-agave-tpu-reference.sh`; archive hash checked.

Fresh run `target/reference-agave/v4.3.0/four-validator-Q8PVm6`:
two pre-funded payers, one transfer instruction, batch1, one generator,
target10tx/s for2s, amount range1..100000 lamports, CU budget600, WS leader
tracking, default fanout1. **No staked identity, mock RPC, or validator exemption**.
Funding used RPC confirmed before load; workload used actual TPU QUIC.
Generator and scheduler exited successfully. Four-node finalized audit found
21 unique successful workload signatures,0 failures, fees105000 lamports,
matching per-signature slot/block/error/fee/pre/post balances and conservation.
Votes and funding transactions are excluded by exact workload payer identities.
All four finalized slots advanced35->40, common blockhash
`CqyZBKDKgzXmTBBXLtoxzcnii5VsyVGdhMGTAF9gweAz` at40, genesis
`BfbgyayMHFUDFxQdPhGfhA8p1BEHAHmyKZRjNpBDnwxx`.

This is a **low-rate TPU functionality sample, NOT capacity TPS**. No full
submission manifest, per-transaction latency, unlanded count or matched NOVOVM
load exists yet. Do not divide21 by2 and present it as capacity. The observer's
two tests cover success/failure separation, vote exclusion, and rejection of
unequal receipts, bad conservation, duplicate signatures and empty results.

Preserved unsuccessful preparation in `four-validator-r6om9m`: finalized
funding completed10/16 before the360s cluster window ended; tool saved partial
test keys, moved from root into that ignored run as `partial-payers.json`.
Two-payer/batch10 initial TPU sample formed self-transfers and is not normal
transfer evidence; its finality audit missed the shutdown window. Resume kept
keys/ledgers/towers and omitted the fresh-genesis leader-start flag. Initial
entrypoint IP-echo startup race was fixed by waiting for its TCP listener; an
attempted `--advertised-ip` was unsupported and removed. Resume later converged
to finalized343 but did not advance in the observed windows; the next TPU
scheduler did not exit and was stopped. **Cold recovery remains NOT PASSED**;
no lock reset or safety bypass was used. This comparison fixture issue does
not establish a Solana production defect or NOVOVM performance advantage.

## Actual four-process result (2026-10-16; supersedes preparation status below)

Pinned release build completed in 15m23s. Four independent validators started
from a new shared genesis; the four pinned Core BPF programs are loaded from
the clean reference source's embedded artifacts, not downloaded as latest.
Syntax/CLI checks passed. Equal active stake was 999997717120 lamports each;
all four finalized slots advanced 41 -> 47 and returned the same blockhash at47.
The early check failed while finalized genesis still classified all validators
as delinquent; the later actual progress check passed without changing criteria.

A real signed 1 SOL transfer finalized at slot110 on all four nodes: error null,
fee5000 lamports and identical pre/post balances. **This transfer used RPC**:
the CLI accepts `--use-tpu-client` but `cli/src/wallet.rs:1066-1077` still calls
`rpc_client.send_transaction_with_config` / spinner-and-config. The flag is only
declared in clap_app.rs; do NOT claim native TPU verification from this command.
No NOVOVM ingress policy changed. Native TPU load and successful finalized TPS,
matched NOVOVM comparison, and restart recovery are still NOT EXECUTED.

[Recorded observations and exact binaries](../artifacts/audit/agave-four-validator-20261016/observed.json).
Local logs and ledgers remain under the ignored run path recorded there; keys
are not committed. `check-agave-four-validator-reference.py` only performs
read-only genesis/stake/finalized-block checks, and never reports TPS.
The first runner's SIGINT shutdown was ignored by background children. After
checking the four owned PIDs, SIGTERM stopped them and the runner exited0.
The runner now uses SIGTERM; ledgers/logs are retained. This is not a graceful
shutdown or cold-recovery acceptance claim.

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
