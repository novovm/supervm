# NOVOVM Consensus Network E2E TPS 封盘（Linux persist + ops_wire_v1 + inmemory，2026-03-10）

## 本次封盘目标

- 补齐 Linux 版 `consensus + network + aoem` E2E TPS 文档。
- 固定 `persist + ops_wire_v1 + inmemory` 口径，不与旧 Linux 打包运行时结果混写。
- 使用当前 AOEM 仓库重新编译的 Linux core runtime 与 persist sidecar plugin 完成一次正式封盘。

## 固定口径（不可混写）

- path：`consensus + network + aoem (single-process multi-node simulation)`
- variant：`persist`
- d1_ingress_mode：`ops_wire_v1`
- d1_input_source：`tx_wire`
- d1_codec：`local_tx_wire_v1_write_u64le_v1`
- aoem_ingress_path：`ops_wire_v1`
- network_transport：`inmemory`
- 固定参数：
  - `txs_total=1,000,000`
  - `validators=4`
  - `batches=1,000`
  - `batch_size=1,000`
  - `repeat_count=1`

## 测试环境

- OS：Ubuntu 24.04.3 LTS（Noble Numbat）
- Kernel：Linux 6.17.0-14-generic（x86_64）
- CPU：AMD Ryzen AI MAX+ 395 w/ Radeon 8060S
- Rust：`rustc 1.94.0`
- Cargo：`cargo 1.94.0`
- PowerShell：`7.5.4`
- 报告生成时间（UTC）：`2026-03-10T16:48:25Z`

## 运行时说明

- 仓库内旧打包件 `artifacts/aoem-platform-build/linux/core/bin/libaoem_ffi.so` 仅导出 `aoem_execute_ops_v2`，不导出 `aoem_execute_ops_wire_v1`，因此不能直接用于本次 `ops_wire_v1` 封盘。
- 本次封盘使用当前 AOEM 仓库重新编译的 Linux core runtime，并暂存到 `artifacts/aoem-e2e-runtime/linux-core-current/bin/libaoem_ffi.so`。
- Persist sidecar plugin 来自 `/home/aoem-a3/WorksArea/AOEM/artifacts/aoem-persist-plugin/linux-current`。
- 当前封盘运行时已确认同时导出 `aoem_execute_ops_v2` 与 `aoem_execute_ops_wire_v1`。

## TPS / Latency

| metric | value |
|---|---:|
| wall_ms | 315.63 |
| consensus_network_e2e_tps p50 | 3,973,567.83 |
| consensus_network_e2e_tps p90 | 4,142,793.82 |
| consensus_network_e2e_tps p99 | 4,263,792.30 |
| consensus_network_e2e_latency_ms p50 | 0.25 |
| consensus_network_e2e_latency_ms p90 | 0.31 |
| consensus_network_e2e_latency_ms p99 | 0.43 |
| aoem_kernel_tps p50 | 10,869,565.22 |
| aoem_kernel_tps p90 | 11,904,761.90 |
| aoem_kernel_tps p99 | 12,345,679.01 |
| network_message_count | 6,000 |
| network_message_bytes | 840,000 |
| runtime_total_ms | 301.23 |
| tx_wire_load_ms | 26.98 |
| setup_ms | 4.66 |
| loop_total_ms | 269.59 |

## Wall Breakdown（consensus-network-e2e 进程内）

| stage | ms |
|---|---:|
| stage_batch_admission_ms | 0.09 |
| stage_ingress_pack_ms | 10.78 |
| stage_aoem_submit_ms | 109.30 |
| stage_proposal_build_ms | 17.57 |
| stage_proposal_broadcast_ms | 0.63 |
| stage_state_sync_ms | 2.26 |
| stage_follower_vote_ms | 45.36 |
| stage_qc_collect_ms | 1.61 |
| stage_commit_resync_ms | 72.41 |
| stage_other_ms | 9.57 |
| qc_poll_iters_total | 2,999 |

## 关键结论

1. Linux `persist + ops_wire_v1 + inmemory` E2E 已跑通，`1,000,000` 笔交易在单次封盘中完成，进程退出码为 `0`。
2. 本次 `consensus_network_e2e_tps` 为 `3.97M / 4.14M / 4.26M`（P50/P90/P99），对应 `aoem_kernel_tps` 为 `10.87M / 11.90M / 12.35M`。
3. `stage_aoem_submit_ms`（`109.30 ms`）与 `stage_commit_resync_ms`（`72.41 ms`）是本轮 wall 时间的主要组成部分。
4. 网络面本轮固定产生 `6,000` 条消息、`840,000` 字节消息负载，符合 `4` validator、`1,000` batch 的 inmemory 仿真规模。
5. 该封盘使用的是当前 AOEM 重编译运行时，不应与旧 Linux 打包件下无法执行 `ops_wire_v1` 的结果混写。

## 产物路径

- `artifacts/migration/consensus-network-e2e-tps-linux-2026-03-10/consensus-network-e2e-summary.json`
- `docs_CN/CONSENSUS/NOVOVM-CONSENSUS-NETWORK-E2E-TPS-RAW-Linux-2026-03-10.csv`
- `artifacts/migration/consensus-network-e2e-tps-linux-2026-03-10/consensus-network-e2e.stdout.log`
- `artifacts/migration/consensus-network-e2e-tps-linux-2026-03-10/consensus-network-e2e.stderr.log`

## 复现命令（Linux）

```bash
cd /home/aoem-a3/WorksArea/SUPERVM

NOVOVM_AOEM_ROOT="$PWD/artifacts/aoem-e2e-runtime/linux-core-current" \
pwsh -NoProfile -File scripts/migration/run_consensus_network_e2e_tps.ps1 \
  -RepoRoot "$PWD" \
  -OutputDir "$PWD/artifacts/migration/consensus-network-e2e-tps-linux-2026-03-10" \
  -DocOutputPath "$PWD/docs_CN/CONSENSUS/NOVOVM-CONSENSUS-NETWORK-E2E-TPS-SEAL-Linux-2026-03-10.md" \
  -RawCsvOutputPath "$PWD/docs_CN/CONSENSUS/NOVOVM-CONSENSUS-NETWORK-E2E-TPS-RAW-Linux-2026-03-10.csv" \
  -Txs 1000000 \
  -Accounts 100000 \
  -BatchSize 1000 \
  -Validators 4 \
  -MaxBatches 1000 \
  -AoemVariant persist \
  -NetworkTransport inmemory \
  -AoemPluginDir "/home/aoem-a3/WorksArea/AOEM/artifacts/aoem-persist-plugin/linux-current" \
  -D1IngressMode ops_wire_v1 \
  -BuildProfile release \
  -TimeoutSec 1200
```
