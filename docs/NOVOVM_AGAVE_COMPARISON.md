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

### Bounded fixture correction and comparison stop (2026-10-16)

Reference-only `FixedLoad::record` now uses a64KiB buffered manifest writer
and an incrementally checked retry-byte count. This removes direct per-row
file writes and the per-batch traversal of all cached transactions. Original
signature uniqueness, pre-queue timestamps,64MiB retry bound and explicit
completion flush/fsync remain. Buffered visibility can delay status observation;
do not mix old and new latency measurements without declaring this change.
Incomplete/aborted buffered manifests are not complete-load evidence.

Release client build passed24.66s. Added a65,536 signed-transfer manifest
regression (identity/order/completion/streaming visibility); it is NOT EXECUTED:
the crate test command began downloading a large additional development graph
and was stopped to honor the comparison time limit. Existing Python observer
tests remain separate from this Rust regression. No measured supply gain or
new chain TPS is claimed.

The inspected upstream generator cycles its payer pool and chooses receivers
relative to each batch. Two-payer alternating transfers are not the NOVOVM
1024-payer/shared-recipient workload, and increasing count alone does not fix
that. Matching the signed preparation boundary and account topology requires
additional fixture work. Per the user's explicit time bound, stop expanding
this reference fixture now: matched capacity comparison remains NOT OBTAINED.
Return development to NOVOVM's existing native mainline; do not infer either
chain's capacity or ranking from these supply-limited samples.

### 4,096-transfer supply diagnostic (not capacity acceptance)

Optional `AGAVE_REFERENCE_SEQUENTIAL_AMOUNTS=1` replaces the reference
generator's range sampling with bounded, unique amounts 1..4096. It requires
fixed-count mode; manifest duplicate rejection remains enabled. This changes
the reference workload, not validator rules, and is not an equivalent random
amount workload. The tracked patch is applied by the existing isolated builder.

Run: `target/reference-agave/v4.3.0/four-validator-knyata`, WSL four validators,
two funded payers, batch1, workers-pull16, exact-byte retry enabled, queue trace
disabled, no competing build. These alternating accounts have high contention;
they are NOT the NOVOVM shared-recipient workload. Release build passed; client
SHA256 `c5085edcf037e2a989821dcf715530e13e070f0bf6fc4836efb287ee1333b8b9`.

| Metric | target10,000/s | no target pacing |
| --- | ---: | ---: |
| Unique submitted / four-node finalized successes | 4096 / 4096 | 4096 / 4096 |
| Business failures / unconfirmed | 0 / 0 | 0 / 0 |
| First-to-last submission span | about16.53s | about16.29s |
| Supply rate across that span | about247.72/s | about251.42/s |
| First submission to last four-node observation | 44.978330678s | 48.722641247s |
| Finite-window successful tx/s, NOT capacity | 91.066074 | 84.067692 |
| P95 / P99 seconds | 32.370108 / 35.106443 | 38.907119 / 41.152064 |
| Exact-byte retransmissions | 440 | 935 |

Both observers completed without observation errors. Finality latency includes
the slowest of four HTTP status observations, not merely execution latency.
The second run follows the first on the same chain; it is a supply diagnostic,
not a controlled capacity A/B. Removing target pacing did NOT remove slow
generation/submission. Neither result is a Solana capacity limit or a matched
NOVOVM ranking. Retry counts do not establish network packet loss.

First-run independent finalized-block audit passed:4096 successes,20480000
lamport fees, equal receipts on all four nodes and balance/fee conservation.
Finalized slots67->73; common slot73 hash
`BMDSJmrmD1AqRntmLvkH4PWfX1oc31JY7PaAeWe5MBRv`; genesis
`EB7YdPpKf9PD6bYq3jG148gpM5t1BwW18eq9eD6V7deV`.
Second-run independent economics audit (from slot74) was NOT COMPLETED: the
bounded240s cluster window had exited before the audit, which received connection
refused. Its all-finalized observer result is retained, not upgraded to audited
economics or restart recovery. Logs, ledgers, manifests and timing JSON remain
in the run directory; private payer keys are not committed.

Remaining reference gap: identify generator/manifest/queue supply cost before
capacity testing. The fixture currently writes each manifest row directly to a
file and rescans cached retry lengths per batch; these are source-visible costs,
not yet proven explanations for the16s span. No NOVOVM/AOEM product path changed.
Do not indefinitely extend this fixture or count it as mainchain delivery.

### Exact-byte bounded retry closure

Pinned connection_worker.rs260-299 explicitly reconnects WITHOUT resending
the failed transaction batch. This establishes a transport contract limitation,
not the proven cause of a historical missing signature. Optional
`AGAVE_REFERENCE_RETRY=1` now retains at most64MiB of original signed bytes,
keeps the existing generator sender handles alive, observes all four local
validators, and resubmits through the SAME TPU queues when a status is absent.
At most8 retries per signature,1s intervals,60s overall completion deadline;
no re-signing, RPC submission, validator change or new transport. Status absence
may just be propagation lag: retry counts are not packet-loss counts. Original
manifest and first-submit timestamps remain unchanged. Mode is a reference
fixture option, not NOVOVM product behavior or an upstream Agave patch.

Actual `four-validator-ORo39N`:256 unique transfers,target100/s,batch1,two payers,
range1..1000000,queue trace OFF, no competing build. All256 finalized on four
nodes,0 business failures,0 missing,45 exact-byte retransmissions,1280000 lamport
fees; independent block audit confirms receipt equality and balance conservation.
Common slot68/hash `H4oMQMbNByPVfJGXAKUyT7KFcaX1bto6Xc9JzMHmMaRB`.
Elapsed31.473661185s; finite-window8.133785 successful tx/s, P95/P99
29.244413057/29.344047636s. These include low-rate supply and finality drain;
NOT capacity, NOT comparable with the65536 NOVOVM workload. All original records
remain in that ignored run (`retry.jsonl`, `client.log`, `timing.json`).
Release build passed25.25s; binary SHA256
`4943719d46ef2dd6822cd180b655a2b7433aef687aab96bfc8a2c5fddbb9afe1`.
Eight Python regression tests pass. Owned validators stopped after audit.
Historical253/255 failures remain recorded; this one success does not prove
which previous packets failed or sign off sustained reliable capacity.

### Bounded queue diagnosis

The previous missing signature was zero-based index31/256, not the tail. No
per-signature old queue trace exists, so its cause cannot be reconstructed.
Optional `AGAVE_REFERENCE_QUEUE_TRACE=1` now records scheduler receipt and
worker enqueue/no-worker-accepted on the ORIGINAL broadcaster; it changes no
routing, retries, quorum or validator code. Do not enable it for formal TPS.
Reproducible source patch: `scripts/perf/agave-queue-trace.patch`. Release build
passed25.86s; binary SHA256
`4efc87a82e9b17c1677df25f46f3febd31f7b3a9db366b6f0027a12512ebe9c3`.

Actual diagnostic run `four-validator-NZCNkf`: same256/count100/s/batch1/two
payers. All256 signatures reached scheduler and worker enqueue,0 no-worker
acceptance failures. At90s:253 four-node finalized successes,0 business
failures,3 unconfirmed,0 observation errors. TPS remains null; successful-subset
P95/P99=30.233594/30.292812s are not whole-workload latency or capacity evidence.
Raw client.log, trace.jsonl and timing.json remain in the ignored run. Eight
Python regression tests pass. Owned validators stopped after observation.

Pinned scheduler exits on producer channel close, then WorkersCache::shutdown
cancels outstanding sends (scheduler245-250,302; workers_cache314-316). This
is a source-level teardown risk, NOT proof of the missing signatures' cause.
The new trace narrows the current missing subset to AFTER worker enqueue;
enqueue is not QUIC delivery or validator acceptance. Further comparison needs
a bounded transport-completion/retry contract and must count the original exact
signatures, not freshly signed replacements. Do not turn this fixture diagnosis
into a Solana consensus rewrite or claim NOVOVM superiority from incomplete load.

### Concurrent finality timing (next checkpoint)

`measure-agave-fixed-load.py` starts before the fixed-count producer and queries
each signature on all four validators (bounded256/status request). Successful
latency ends at the slowest finalized HTTP response; business failures are
separate, unmatched signatures suppress TPS. This includes observer scheduling,
not pure consensus time. Economics still requires the original block audit.
Three timing unit tests plus five observer tests pass.

The initial wall-time experiment in `four-validator-E3DJkC` detected a1.3127s
wall/monotonic discrepancy; its timing numbers were suppressed despite32/32
success and160000 lamports fees. A256-transaction preparation using an amount
range of1e9 hit its30s generation deadline: upstream `choose_multiple` scans
the range. This is a generator cost, not a validator capacity measurement.
Both failures remain in the ignored run; no throughput is signed off.

Producer timestamps and observer now both use Linux CLOCK_MONOTONIC on the
same host; wall clock is diagnostic only. Updated release built successfully
in1m03s; binary SHA256
`a2e782d0320a1bde048f32166b140c89e0720cc5e7014af8904274cfe1a23f9b`.
Actual run `four-validator-5GqLem` uses256 transactions,target100/s,batch1,
two funded payers,amount range1..1000000,ordinary TPU QUIC, no parallel build.
At common finalized slot46 the block audit found255 successes,0 failures,
1275000 lamports fees and four-node receipt/conservation agreement. One exact
submitted signature remained absent; successful generation is not delivery.
Final timing report is retained as `timing.json` alongside `timed.jsonl`.
At the90s observer deadline:255 successful,0 business failures,1 unconfirmed,
no RPC observation errors. Successful-subset P95=27.774618459s,
P99=27.841233342s; these are NOT whole-workload percentiles. TPS remains null.
This low-supply, two-account test is not Solana's maximum throughput and cannot
be compared to NOVOVM's65536-transaction workload. Next isolate the missing
signature's queue/shutdown path before any high-load capacity claim.
No matched NOVOVM capacity comparison or NOVOVM runtime change is claimed.

### Submission coverage checkpoint

Fixed-count producer is now wired into the existing official generator via
`scripts/perf/agave-fixed-load.patch` and `agave_fixed_load.rs`; validators,
TPU scheduler and business instructions are unchanged. Build script applies
the checked patch or verifies its reverse. Set `AGAVE_REFERENCE_COUNT` and
`AGAVE_REFERENCE_MANIFEST` (new `.jsonl` path) together. Each decoded signed
transaction is recorded immediately BEFORE the original client queue attempt;
this is not proof of enqueue or delivery. Duplicate signatures, incomplete
generation and deadlines fail; a completion marker is required by the observer.
Serialization/file output and generation are instrumentation costs, not excluded
pre-signing work; this mode is NOT yet a matched NOVOVM performance fixture.

Actual run `target/reference-agave/v4.3.0/four-validator-1zYZvL`:
32 transactions, target10/s, batch1, two funded distinct payers, ordinary TPU
QUIC identity. Generator and scheduler exited0. The first audit preceded
finalization and rejected zero workload receipts; the later audit verified
32/32 finalized successes,0 failures,0 missing,160000 lamports fees, identical
four-node receipts and fee/balance conservation. Common finalized slot143,
hash `7edtRPN9hdVeFiounVGSxZkuomWuMGzYjVThLq8BySns`, genesis
`B6jW5PrFopQLmjdR55N6EcJnftMKXZ3NkAkKJNvtHyHx`. Manifest is
`attempts-now.jsonl` in that retained ignored run; keys are not published.
All five observer unit tests pass. Four owned validators were stopped with
SIGTERM after observation; no recovery claim. Release compilation passed in
3m29s; editing the wrapper during that invocation caused a subsequent shell
line-offset error, not a compiler error. A redundant rebuild was stopped;
the successful fixed-count run used the completed binary. No formal TPS was
measured while compiling. Concurrent per-signature finality timestamps and a
matched large workload remain pending; no NOVOVM runtime or AOEM changes.

The observer now accepts `--manifest FILE`, containing
`{"signatures":["base58 signature", "..."]}`. It rejects empty, duplicate,
malformed and non-64-byte signatures, and finalized workload signatures absent
from the manifest. It reports exact unconfirmed signatures separately from
finalized business failures; absence at observation does not prove permanent
loss. Four local unit tests pass, including rejection cases. No new live load
was run for this checkpoint; no TPS or latency result is added.

Source inspection of the pinned tool confirms that `send_batch` measures queue
insertion, while `BackpressuredBroadcaster::send_to_workers` may return `Ok(())`
even when no leader queue accepted the batch. Neither is finality evidence.
Remaining work is an actual fixed-count manifest producer at the existing TPU
submission boundary, first-submit timing and concurrent four-node observation.
The optional manifest check alone does not complete that producer or benchmark.

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
