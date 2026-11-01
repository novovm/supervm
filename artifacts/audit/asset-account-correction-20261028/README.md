# Actual asset-account correction — 2026-10-28

Status: corrected-account implementation and local native validation delivered;
affected network parallel fixture closure verified twice.
NOT a release, main integration, multi-host or restored-TPS signoff.

本轮结论：实际资产账户寻址、独立账户权限/nonce、管理换钥/冻结/资产恢复已经落到
原节点与原 journal，不再拿 UCA 身份代替银行卡。账户模型的本地验收与性能验收分开：
模型和已列恢复场景通过；吞吐仍未恢复约12K，严格同字节65,536 A/B及正式SDK/CI未通过。
本轮仅操作隔离夹具；没有激活部署、转换旧库、覆盖身份或更新正式随包运行库。

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

Final default-parallel network repeat initially failed: one pre-existing strict
3-second RLPx fanout test waited behind other tests' process-global session lock;
its poisoned ENV test lock cascaded to 14 more cases (860/15/11). The identical
single test passed in 0.27 s, which by itself was NOT acceptance. Its fixture is
now isolated in the SAME test executable's child process; the parent does not
hold or mutate shared ENV. Original 3 s/6 jobs/250 ms connect/1,000 ms tick limits
remain, with six real accepted auth requests, six auth-timeout failures and zero
connected peers checked. Outer 15 s watchdog only supervises child startup and
cleanup, not the worker limit. The complete default-parallel suite then passed
TWICE: 875/0/11 in 20.02 s and 20.13 s, and affected strict Clippy passed.

The production foreign-chain RLPx global session lock still spans blocking socket
reads. This is a known independent concurrency limitation, NOT fixed by isolating
the test and NOT established as the cause of native NOV TPS. The final follow-up
changes ONLY this cfg(test) fixture and evidence; native wire/economics/runtime
code and the measured c6810b49 binaries are unchanged. Renewed affected validation
is the network suite/Clippy above, not inherited whole-product or release approval.

Failed first runs are retained (`native-regressions-first.log`,
`strict-clippy.log`, `all-actual-mldsa.log`). Corrected fixtures do not remove
production assertions. The packaged-library test is a separate retained failure,
not silently substituted by the isolated candidate (below).

## Final native measurement at source checkpoint c6810b49

Actual final node: `03105a3c2d9b9231792ad1fc3372c48949edac7c32453c1c160f701b74136d9e`.
Client: `607c1fec00c7c821b5a925e069ec2498d658b48a0424e1de653b3634ad43eb34`.
The AOEM DLL and unchanged business backend are the same as the table below.

| Input | Signature backend | Successful TPS | P95 ms | P99 ms |
| --- | --- | ---: | ---: | ---: |
| Same 4,096 signed rows | CPU 1 | 1,860.04 | 1,067.159 | 1,086.461 |
| Same 4,096 signed rows | GPU-required | 709.37 | 3,724.015 | 4,135.800 |
| Same 4,096 signed rows | CPU 2 | 1,640.67 | 1,432.374 | 1,441.395 |
| Legal rolling 65,536 | CPU 1 | 1,286.51 | 158.684 | 187.240 |
| Legal rolling 65,536 | GPU-required | 1,186.42 | 162.064 | 198.969 |
| Legal rolling 65,536 | CPU 2 | 1,265.82 | 164.334 | 199.879 |

Every round passed four-node economic equality and cold recovery. Full results:
`paired-4096-final-report.json` and `rolling-65536-final-report.json`. Do not mix
these artifact identities or samples with the earlier checkpoint table below.
The final independent-process recovery also passed (121.29 s): healthy peers
advanced 40 heights; missing validator started after 106,252 ms, was killed at
durable height 1 while target-height-40 archive was absent, then restarted its own
store and caught up. All four ended at height 41 after recipient respend; cold
reopen and fee/nonce/authority/receipt checks passed. See
`four-process-final-report.json`.

One canonical signed transfer is 5,448 bytes in this corpus. The unchanged
393,216-byte canonical body budget admits at most 72 such rows; 65,536 rows need
at least 911 nonempty heights. Final rolling runs used 1,477/1,497/1,499 heights,
not the historical V3 64 heights. This is a proven amortization difference, NOT
proof it is the sole bottleneck. Rolling includes ~9.86–9.88 s subsequent signing,
~0.31–0.32 s structure preparation and ~1.27–1.41 s head queries; none is subtracted
to manufacture a faster TPS. Internal durable timestamps, wire traffic, memory
peaks and the exact cause of GPU/confirmation waiting were not measured here.

The strict same-byte 65,536 exit remains OPEN. The static signature window
`[1,257)` cannot cover the minimum required heights, and the rolling messages
explicitly differ across rounds. Neither protocol limits nor validity rules were
expanded. These results are useful legal-load evidence, not a substituted exit.

## Earlier performance checkpoints — retained negative samples

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
Remaining native transport/device/service ML-DSA migration is not delivered by
correcting the asset-account contract; transport still has a distinct legacy
signature boundary. Do not claim complete first-release PQ identity migration.

Raw command logs, including failed first runs and both successful network repeats,
are retained verbatim in `validation-logs.zip`; local `.log` files also remain.
No private key, database, phone state or signing journal is included in that zip.
All 49 ZIP entries were SHA-256 compared with their original logs. Bundle hash:
`26d28059a6d674de864017b29186401a5849c3f90ae1becae3887312842e0d75`.

Prior UCA-addressed results do not qualify this account model. Ordinary spending
requires already-approved initialized asset accounts; this round is not delivery
of dynamic asset-account onboarding, all identity-recovery flows or treasury
governance. Management publication crash/offline combinations remain untested.

Intermediate rebuilt node before test-only network formatting:
`936c30ca7cd17c0c64638e1ed9f4fc2f3f9660a797d04ec970bb5bb399eda36b`.
Client: `c54202db7373fb210a6ff07a35dbc56c1d2d0c6d9a374cac17808806325ef065`.
CLI: `ada18750291265c09f2a45079b8452ea9036a56c13ba5ccfae9e77eab4978f24`.
Libtest: `31eeeb5b6901cce5e975d19ca1f0b756c2da492e5bd551736c15307d0fecb834`.
The earlier 5d489 node's measurements above remain an explicitly separate
checkpoint; they do not qualify these final binaries by historical inheritance.

Known independent gates retained for this round: early-height vote custody,
network-test isolation, and qualification of a reproducible unified-semantic AOEM
artifact. An isolated local DLL does not replace the formal SDK or Linux runtime.
