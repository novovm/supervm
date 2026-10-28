# Actual asset-account correction — 2026-10-28

Status: corrected-account implementation; final validation in progress.
NOT a release, main integration, multi-host or restored-TPS signoff.

Baseline: `dev/mldsa65-identity-closure@8d92bbb1` plus the preserved uncommitted
performance/recovery work inventoried before this round. `main`, bundled AOEM,
deployment services and existing databases are unchanged.

The corrected native contract signs concrete payer/recipient accounts and NOV.
UCA remains identity association; account authority and replay state belong to
each stable asset account. Identity suspension/recovery/unbinding does not grant
or freeze asset authority. Management and explicitly configured recovery use the
same native candidate, fee/nonce, AOEM state and journal publication owners.

## Implemented on the original product path

- `AssetSpendIntentV2` / `SignedAssetSpendV2`, NUAS v2 and NUAPFL02 bind actual
  payer/recipient accounts and NOV, not UCA ids or unsigned address hints.
- Stable actual-account nonce; different accounts remain independent while all
  grants for one account share its replay state. Rotation does not move funds.
- Compact account authority replaces full identity/mapping reads in ordinary
  transfers: 10 logical declared keys for a distinct-account transfer (not a
  claim of 10 physical disk reads). Canonical missing-page checks remain.
- Account-authorized grant replacement, revocation, rotation, freeze and
  explicitly configured recovery execute in the original CPU/integer plan and
  candidate write set, with the same nonce/fee/journal authority. Missing
  recovery configuration fails closed. Identity actions grant no asset rights.
- Initialization separately approves identities, each asset manager and optional
  recovery authority. One identity can associate with multiple asset accounts.
- Original ingress, signature-fact reuse, bounded batch capture, pool, typed
  queries, receipts, archive replay, cold recovery and complete successor state
  consume the same contract. No replacement runtime/account store was added.
- Old NUAS v1, old body/query/archived packet versions are rejected, not guessed
  or reinterpreted. No production signature downgrade was introduced.

## Recorded local validation

| Check | Result |
| --- | --- |
| Final native pipeline defaults | 501 passed, 0 failed, 106 ignored |
| Actual ML-DSA CPU/integer authority/prefix equivalence | 2 passed |
| Early-height vote/ACK counterexamples | 4 passed |
| Complete successor regressions | 7 passed |
| Actual signed journal publication/respend/cold recovery | 1 passed |
| Actual initialization and protected-key cold reopen | 2 passed |
| Default parallel network tests | 875 passed, 0 failed, 11 ignored |
| Six affected packages, strict release Clippy/all targets | Passed |
| Genesis CLI normal checks | 10 passed, 0 failed, 1 ignored |
| Explicit real AOEM identity/manager/recovery approvals | 1 passed |
| Real UDP services: rotation/freeze/recovery/old-key and cross-account rejection | 1 passed, 13.85 s |
| Four original node processes, late start/interrupted catch-up/respend/cold | Passed, 129.26 s |

The process recovery report uses one host, four independent original processes
and databases, quorum 3/4. The missing validator's healthy peers advance 40
heights before its late authenticated startup. It is killed during incomplete
catch-up, restarts with its own database, catches up and spends received NOV.
No database copy, wallet replay into the laggard or HTTP transaction bypass.
See `four-process-recovery-report.json` and the corresponding log.

`native-management-report.json` separately covers four real original service
instances/sockets/databases IN ONE PROCESS: manager and spend-key replacement,
old-key rejection, freeze, restart, independent asset recovery, recovered-key
spend, replay idempotence and another cold restart. It does not prove management
publication crash injection or offline management catch-up.

Six pre-open store-guard counterexamples are included in the 501 tests. Missing,
wrong, oversized or redirected guards/paths reject before AOEM loading and leave
synthetic old-database bytes unchanged. The local guard only denies incompatible
opens; the original in-database format/domain validation remains mandatory.
Do not copy a new guard into an old database or call this a data migration.

The maximum 64-validator/32-account/8-grant configuration remains under the
unchanged 16 MiB text ceiling: 16,689,665 bytes. Canonical manifest: 709,378 bytes.
The first maximum-shape failure is retained in `genesis-cli-first.log`; compact
approval JSON fixes presentation size, not canonical signatures or wire limits.

Failed first runs are retained (`native-regressions-first.log`,
`strict-clippy.log`, `all-actual-mldsa.log`). Corrected fixtures do not remove
production assertions. The packaged-library test is a separate retained failure,
not silently substituted by the isolated candidate (below).

## Performance checkpoints — not a restored-performance claim

Measured node SHA-256:
`5d4897e4025f1ba4db44e53979d5a3bec5cfd3ec434a2f54e1f7aaf744862317`.
Actual isolated AOEM DLL:
`8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671`.
Business execution is `cpu_callbacks` in every row; only signature backend
changes. CPU/GPU/CPU denotes separate measurement rounds, not data shuttling
inside the GPU cryptographic pipeline. Auto and the shipped library are unchanged.

| Input | Signature backend | Successful TPS | P95 ms | P99 ms |
| --- | --- | ---: | ---: | ---: |
| Exact same 4,096 signed rows | CPU 1 | 1,763.97 | 1,291.027 | 1,310.069 |
| Exact same 4,096 signed rows | GPU-required | 652.45 | 3,764.046 | 4,166.727 |
| Exact same 4,096 signed rows | CPU 2 | 1,773.03 | 1,075.905 | 1,128.850 |
| Legal rolling 65,536 | CPU 1 | 1,148.23 | 177.533 | 209.920 |
| Legal rolling 65,536 | GPU-required | 1,067.24 | 182.497 | 224.072 |
| Legal rolling 65,536 | CPU 2 | 1,129.08 | 181.077 | 213.174 |

All six rows pass complete four-node receipt, balance, fee, authority, nonce and
cold-recovery checks. Preserve the negative GPU results. These results do not
establish performance improvement or restore the historical ~12K.

The 4,096 comparison replays byte-identical signatures, domain and initial state.
Actual candidates have at most 72 rows under the unchanged 384 KiB body budget.
The rolling workload keeps 256 transactions per wave and signs legal height
windows, without enlarging MAX_SPEND_WINDOW. Its one unpaused timing wall
includes subsequent signing, preparation and between-wave common-head queries.
It compares equivalent business intents, NOT byte-identical later-wave messages.
Do not directly derive an algorithm-only loss from historical V3 65,536 results.
Consequently this rolling run does NOT satisfy a strict byte-identical 65,536-row
CPU/GPU acceptance condition. That performance gate remains open; the smaller
4,096-row experiment is the completed exact-byte comparison, not a substitute.

TPS ends when all four clients observe every durable receipt. Latency starts at
each transaction's first send attempt. These are not internal durable timestamps.
Physical wire bytes/retransmissions and peak memory were not measured. The
full reports are `paired-4096-report.json` and `rolling-65536-report.json`; raw
observations and isolated fixture databases remain at their report paths, not in
the public source bundle. No deployment keys/custody are used by these fixtures.

## Independent artifact/release blockers

The shipped Windows DLL `31306037...15e8947` and Linux SO
`4d06f36a...538309` lack `aoem_compute_result_take_v1`. The isolated 8779 DLL
has it, but its AOEM implementation is not yet a frozen, reproducible upstream
commit with qualified Windows/Linux artifacts. The real shipped-library test
still rejects with `AOEM ML-DSA batch verification capability unavailable`;
see `shipped-library-gate.log`. Do not remove it or restore the old ABI fallback.

The existing full CI uses shipped artifacts, not this local DLL. A final
qualified candidate CI is therefore BLOCKED, not green, pending upstream
artifact qualification and an explicitly approved SDK update. No full CI was
triggered for these intermediate changes. C must consume the v2 shared contract;
D must not package an unqualified or mixed candidate. Multi-host, mobile,
privacy/proof, final economic allocations and formal genesis remain separate.

Final guard/lifecycle regression and binary identities will be appended after
their last build. Prior UCA-addressed results do not qualify this account model.

Final rebuilt node before the last process/performance rerun:
`936c30ca7cd17c0c64638e1ed9f4fc2f3f9660a797d04ec970bb5bb399eda36b`.
Client: `c54202db7373fb210a6ff07a35dbc56c1d2d0c6d9a374cac17808806325ef065`.
CLI: `ada18750291265c09f2a45079b8452ea9036a56c13ba5ccfae9e77eab4978f24`.
Libtest: `31eeeb5b6901cce5e975d19ca1f0b756c2da492e5bd551736c15307d0fecb834`.
The earlier 5d489 node's measurements above remain an explicitly separate
checkpoint; they do not qualify these final binaries by historical inheritance.

Known independent gates retained for this round: early-height vote custody,
network-test isolation, and qualification of a reproducible unified-semantic AOEM
artifact. An isolated local DLL does not replace the formal SDK or Linux runtime.
